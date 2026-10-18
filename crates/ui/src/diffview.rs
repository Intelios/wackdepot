//! The right-pane diff: virtualized rows — file/hunk headers and diff lines —
//! with hunk-level stage/unstage/discard affordances on hover. Commit
//! selections render one section per file in that commit.

use gpui::{AnyElement, Context, Div, Styled, div, prelude::*, px, uniform_list};
use wackdepot_core::diff::{DiffFileStatus, DiffLineKind, Hunk};

use crate::icons::icon;
use crate::shell::Shell;
use crate::state::Bucket;
use crate::theme::Theme;

const ROW_H: f32 = 20.0;

#[derive(Clone)]
enum DiffRow {
    FileHeader {
        path: String,
        status: DiffFileStatus,
    },
    HunkHeader {
        file_ix: usize,
        hunk_ix: usize,
        header: String,
    },
    Line {
        file_ix: usize,
        hunk_ix: usize,
        line_ix: usize,
    },
    Binary {
        path: String,
    },
}

impl Shell {
    /// Flattened row model for the current diff selection.
    fn diff_rows(&self) -> Vec<DiffRow> {
        let mut rows = Vec::new();
        if let Some(sel) = &self.selected_diff {
            if sel.commit.is_some() {
                for (file_ix, f) in sel.all.iter().enumerate() {
                    push_file_rows(&mut rows, f, file_ix);
                }
            } else {
                push_file_rows(&mut rows, &sel.file, 0);
            }
        }
        rows
    }

    pub fn diff_view(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        let Some(sel) = &self.selected_diff else {
            return self.empty_content(theme, "Pick a file to see its changes.");
        };
        let rows = self.diff_rows();
        if rows.is_empty() {
            return self.empty_content(theme, "Nothing to show here.");
        }
        let header = self.diff_header(cx, theme, sel);
        let list = uniform_list(
            "diff",
            rows.len(),
            cx.processor(move |this, range, window, cx| {
                let theme = this.theme(window);
                let mut items: Vec<AnyElement> = Vec::new();
                for ix in range {
                    items.push(this.diff_row(cx, &theme, &rows[ix]).into_any_element());
                }
                items
            }),
        )
        .h_full();

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(list))
    }

    fn empty_content(&self, theme: &Theme, text: &str) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .text_color(theme.text_dim)
            .child(icon("commit").size_10().text_color(theme.text_faint))
            .child(div().text_sm().child(text.to_string()))
    }

    fn diff_header(
        &self,
        cx: &mut Context<Self>,
        theme: &Theme,
        sel: &crate::shell::SelectedDiff,
    ) -> Div {
        let title = if let Some(c) = &sel.commit {
            format!("{} — {}", c.short, c.subject)
        } else {
            sel.file.path().to_string()
        };
        let show_stage = !sel.commit.is_some();
        div()
            .flex_none()
            .h_9()
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_sm()
                    .text_color(theme.text)
                    .child(title),
            )
            .when(show_stage, |el| {
                let label = if self.ignore_ws {
                    "Ignore whitespace: on"
                } else {
                    "Ignore whitespace"
                };
                el.child(
                    div()
                        .id("ws-toggle")
                        .text_xs()
                        .text_color(if self.ignore_ws {
                            theme.accent
                        } else {
                            theme.text_dim
                        })
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme.text))
                        .child(label)
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ignore_ws = !this.ignore_ws;
                            this.refresh_selected_diff(cx);
                            cx.notify();
                        })),
                )
            })
    }

    fn diff_row(&self, cx: &mut Context<Self>, theme: &Theme, row: &DiffRow) -> Div {
        match row {
            DiffRow::FileHeader { path, status } => {
                let letter = match status {
                    DiffFileStatus::Added => "A",
                    DiffFileStatus::Deleted => "D",
                    DiffFileStatus::Renamed => "R",
                    DiffFileStatus::Binary => "B",
                    DiffFileStatus::Modified => "M",
                };
                div()
                    .h(px(ROW_H + 4.))
                    .px_3()
                    .mt_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .w_4()
                            .h_4()
                            .flex_none()
                            .rounded_sm()
                            .bg(theme.elevated)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(theme.text_dim)
                            .child(letter),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_soft)
                            .text_ellipsis()
                            .child(path.clone()),
                    )
            }
            DiffRow::HunkHeader {
                file_ix,
                hunk_ix,
                header,
            } => {
                let sel_file = self.selected_diff.as_ref().and_then(|s| {
                    if s.commit.is_some() {
                        s.all.get(*file_ix).cloned()
                    } else {
                        Some(s.file.clone())
                    }
                });
                let hunk = self
                    .selected_diff
                    .as_ref()
                    .and_then(|s| {
                        if s.commit.is_some() {
                            s.all.get(*file_ix).and_then(|f| f.hunks.get(*hunk_ix))
                        } else {
                            s.file.hunks.get(*hunk_ix)
                        }
                    })
                    .cloned();
                let bucket = self
                    .selected_diff
                    .as_ref()
                    .map(|s| s.bucket)
                    .unwrap_or(Bucket::Unstaged);
                let actions_enabled = self
                    .selected_diff
                    .as_ref()
                    .is_some_and(|s| s.commit.is_none());

                let mut el = div()
                    .h(px(ROW_H + 2.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(theme.diff_hunk_bg)
                    .text_xs()
                    .font_family("SF Mono")
                    .text_color(theme.diff_hunk_text);
                el = el.child(div().flex_1().text_ellipsis().child(header.clone()));
                if actions_enabled && let (Some(file), Some(hunk)) = (sel_file, hunk) {
                    el = el.child(self.hunk_actions(cx, theme, &file, &hunk, bucket));
                }
                el
            }
            DiffRow::Line {
                file_ix,
                hunk_ix,
                line_ix,
            } => {
                let line = self.selected_diff.as_ref().and_then(|s| {
                    if s.commit.is_some() {
                        s.all
                            .get(*file_ix)
                            .and_then(|f| f.hunks.get(*hunk_ix))
                            .and_then(|h| h.lines.get(*line_ix))
                    } else {
                        s.file
                            .hunks
                            .get(*hunk_ix)
                            .and_then(|h| h.lines.get(*line_ix))
                    }
                });
                let Some(line) = line else {
                    return div().h(px(ROW_H));
                };
                let (bg, sigil, color) = match line.kind {
                    DiffLineKind::Added => (theme.diff_add_bg, "+", theme.diff_add_text),
                    DiffLineKind::Removed => (theme.diff_del_bg, "-", theme.diff_del_text),
                    DiffLineKind::Context => (gpui::hsla(0., 0., 0., 0.), " ", theme.text_soft),
                    DiffLineKind::NoNewline => (gpui::hsla(0., 0., 0., 0.), " ", theme.text_faint),
                };
                div()
                    .h(px(ROW_H))
                    .bg(bg)
                    .flex()
                    .items_center()
                    .font_family("SF Mono")
                    .text_xs()
                    .child(
                        div()
                            .w_10()
                            .flex_none()
                            .text_right()
                            .px_1()
                            .text_color(theme.text_faint)
                            .child(line.old_lineno.map(|n| n.to_string()).unwrap_or_default()),
                    )
                    .child(
                        div()
                            .w_10()
                            .flex_none()
                            .text_right()
                            .px_1()
                            .text_color(theme.text_faint)
                            .child(line.new_lineno.map(|n| n.to_string()).unwrap_or_default()),
                    )
                    .child(
                        div()
                            .w_4()
                            .flex_none()
                            .text_center()
                            .text_color(color)
                            .child(sigil),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(if matches!(line.kind, DiffLineKind::NoNewline) {
                                theme.text_faint
                            } else {
                                color
                            })
                            .whitespace_nowrap()
                            .child(line.text.clone()),
                    )
            }
            DiffRow::Binary { path } => div()
                .h(px(60.))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.text_dim)
                .child(format!("{path} — binary file, nothing to diff")),
        }
    }

    /// Per-hunk actions shown in the hunk header, per the file's bucket.
    fn hunk_actions(
        &self,
        cx: &mut Context<Self>,
        theme: &Theme,
        file: &wackdepot_core::diff::FileDiff,
        hunk: &Hunk,
        bucket: Bucket,
    ) -> Div {
        type HunkClick = Box<dyn Fn(&mut Shell, &mut Context<Shell>)>;
        let mk = |id: (&'static str, u64), label: &'static str, color: gpui::Hsla, f: HunkClick| {
            div()
                .id(id)
                .h_5()
                .px_2()
                .rounded_full()
                .text_xs()
                .text_color(color)
                .cursor_pointer()
                .hover(|s| s.bg(color.opacity(0.12)))
                .occlude()
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    f(this, cx);
                    cx.stop_propagation();
                }))
        };
        let file1 = file.clone();
        let file2 = file.clone();
        let h1 = hunk.clone();
        let h2 = hunk.clone();
        let key = (hunk.new_start as u64) << 32 | hunk.old_start as u64;
        match bucket {
            Bucket::Staged => div().flex().gap_1().child(mk(
                ("unstage-hunk", key),
                "Unstage",
                theme.accent,
                Box::new(move |this, cx| this.unstage_hunk(file1.clone(), h1.clone(), cx)),
            )),
            _ => div()
                .flex()
                .gap_1()
                .child(mk(
                    ("stage-hunk", key),
                    "Stage",
                    theme.accent,
                    Box::new(move |this, cx| this.stage_hunk(file1.clone(), h1.clone(), cx)),
                ))
                .child(mk(
                    ("discard-hunk", key),
                    "Discard",
                    theme.danger,
                    Box::new(move |this, cx| this.discard_hunk(file2.clone(), h2.clone(), cx)),
                )),
        }
    }
}

fn push_file_rows(rows: &mut Vec<DiffRow>, f: &wackdepot_core::diff::FileDiff, file_ix: usize) {
    rows.push(DiffRow::FileHeader {
        path: f.path().to_string(),
        status: f.status,
    });
    if f.is_binary {
        rows.push(DiffRow::Binary {
            path: f.path().to_string(),
        });
        return;
    }
    for (hunk_ix, h) in f.hunks.iter().enumerate() {
        rows.push(DiffRow::HunkHeader {
            file_ix,
            hunk_ix,
            header: h.header.clone(),
        });
        for line_ix in 0..h.lines.len() {
            rows.push(DiffRow::Line {
                file_ix,
                hunk_ix,
                line_ix,
            });
        }
    }
}
