//! The Changes tab: grouped file lists over a uniform list, then the commit
//! box at the bottom of the rail. Groups: Conflicts / Staged / Changes /
//! Untracked — each a header row (chevron, name, count, stage-all affordance)
//! then file rows with a status mark and hover actions.

use gpui::{AnyElement, Context, Div, Styled, div, prelude::*, uniform_list};
use wackdepot_core::status::FileChange;

use crate::icons::icon;
use crate::shell::Shell;
use crate::state::{Bucket, Selected};
use crate::theme::Theme;

/// One row in the flattened changes list model.
#[derive(Clone)]
enum ChangeRow {
    Header {
        bucket: Bucket,
        title: &'static str,
        count: usize,
    },
    File {
        bucket: Bucket,
        index: usize,
        path: String,
        orig: Option<String>,
        kind_letter: char,
        both_sides: bool,
    },
}

fn bucket_title(b: Bucket) -> &'static str {
    match b {
        Bucket::Conflicted => "Conflicted",
        Bucket::Staged => "Staged",
        Bucket::Unstaged => "Changes",
        Bucket::Untracked => "Untracked",
    }
}

impl Shell {
    /// Flattened row model in display order.
    fn change_rows(&self) -> Vec<ChangeRow> {
        let mut rows = Vec::new();
        for bucket in [
            Bucket::Conflicted,
            Bucket::Staged,
            Bucket::Unstaged,
            Bucket::Untracked,
        ] {
            let files = self.bucket_list(bucket);
            if files.is_empty() {
                continue;
            }
            let collapsed = self.collapsed.contains(bucket_title(bucket));
            rows.push(ChangeRow::Header {
                bucket,
                title: bucket_title(bucket),
                count: files.len(),
            });
            if collapsed {
                continue;
            }
            for (index, f) in files.iter().enumerate() {
                rows.push(ChangeRow::File {
                    bucket,
                    index,
                    path: f.path.clone(),
                    orig: f.orig_path.clone(),
                    kind_letter: kind_letter(f),
                    both_sides: f.is_staged() && f.is_unstaged(),
                });
            }
        }
        rows
    }

    pub fn changes_view(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        if self.active.is_none() {
            return self.empty_rail(theme, "Pick a repository up top.");
        }
        let rows = self.change_rows();
        let body: Div = if rows.is_empty() {
            if self.repo.status.is_none() {
                self.empty_rail(theme, "Reading the repository…")
            } else if self
                .repo
                .status
                .as_ref()
                .is_some_and(|s| s.merge_in_progress)
            {
                self.empty_rail(theme, "All conflicts resolved.")
            } else {
                self.empty_rail(theme, "No changes — the working tree is clean.")
            }
        } else {
            let selected = self.selected.clone();
            let collapsed_keys: Vec<&'static str> = self.collapsed.iter().copied().collect();
            let _ = collapsed_keys;
            div().flex_1().min_h_0().child(
                uniform_list(
                    "changes",
                    rows.len(),
                    cx.processor(move |this, range, _window, cx| {
                        let theme = this.theme(_window);
                        let mut items = Vec::new();
                        for ix in range {
                            items.push(
                                this.change_row(cx, &theme, &rows[ix], &selected)
                                    .into_any_element(),
                            );
                        }
                        items
                    }),
                )
                .h_full(),
            )
        };

        let commit_box = self.commit_box(cx, theme);
        let merging = self
            .repo
            .status
            .as_ref()
            .is_some_and(|s| s.merge_in_progress);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(body)
            .when(merging, |el| el.child(self.merge_banner(cx, theme)))
            .child(commit_box)
    }

    /// Yellow rail banner while a merge is in progress.
    fn merge_banner(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        div()
            .flex_none()
            .mx_3()
            .mb_2()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(theme.warning.opacity(0.12))
            .border_1()
            .border_color(theme.warning.opacity(0.4))
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(theme.text)
                    .text_ellipsis()
                    .child("Merging — resolve, then commit."),
            )
            .child(
                div()
                    .id("abort-merge")
                    .flex_none()
                    .h_6()
                    .px_2()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(theme.warning)
                    .cursor_pointer()
                    .occlude()
                    .hover(|s| s.bg(theme.warning.opacity(0.15)))
                    .child("Abort")
                    .on_click(cx.listener(|this, _, _, cx| this.ask_abort_merge(cx))),
            )
    }

    fn empty_rail(&self, theme: &Theme, text: &str) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .text_sm()
            .text_color(theme.text_dim)
            .child(text.to_string())
    }

    fn change_row(
        &self,
        cx: &mut Context<Self>,
        theme: &Theme,
        row: &ChangeRow,
        selected: &Option<Selected>,
    ) -> AnyElement {
        let el: AnyElement = match row {
            ChangeRow::Header {
                bucket,
                title,
                count,
            } => {
                let collapsed = self.collapsed.contains(*title);
                let bucket = *bucket;
                let title_static = *title;
                div()
                    .id(("chg-hdr", index_of_bucket(bucket) as u64))
                    .h_7()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(theme.text_dim)
                    .child(
                        icon(if collapsed {
                            "chevron-right"
                        } else {
                            "chevron-down"
                        })
                        .size_3(),
                    )
                    .child(title_static)
                    .child(div().text_color(theme.text_faint).child(format!("{count}")))
                    .child(div().flex_1())
                    .when(*count > 0 && !collapsed, |el| {
                        el.child(
                            div()
                                .id(("chg-hdr-btn", index_of_bucket(bucket) as u64))
                                .text_color(theme.accent)
                                .hover(|s| s.text_color(theme.accent))
                                .child(if bucket == Bucket::Staged {
                                    "Unstage all"
                                } else {
                                    "Stage all"
                                })
                                .occlude()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.stage_bucket(bucket, cx);
                                    cx.stop_propagation();
                                })),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.insert(title_static) {
                            this.collapsed.remove(title_static);
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            }
            ChangeRow::File {
                bucket,
                index,
                path,
                orig,
                kind_letter,
                both_sides,
            } => {
                let selected_here = matches!(
                    selected,
                    Some(Selected::File {
                        bucket: b,
                        index: i
                    }) if *b == *bucket && *i == *index
                );
                let bucket = *bucket;
                let index = *index;
                let discard_path = path.clone();
                let is_untracked = bucket == Bucket::Untracked;
                let file_name = path.rsplit('/').next().unwrap_or(path).to_string();
                let dir = path
                    .rsplit_once('/')
                    .map(|(d, _)| d.to_string())
                    .unwrap_or_default();
                let mark_color = match kind_letter {
                    'A' => theme.success,
                    'D' => theme.danger,
                    'R' => theme.accent,
                    'U' => theme.warning,
                    _ => theme.text_dim,
                };
                let mut el = div()
                    .id((
                        "chg",
                        index as u64 + index_of_bucket(bucket) as u64 * 10_000,
                    ))
                    .h_7()
                    .px_3()
                    .mx_1()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .text_sm()
                    .text_color(if selected_here {
                        theme.text
                    } else {
                        theme.text_soft
                    });
                if selected_here {
                    el = el.bg(theme.accent.opacity(0.16));
                } else {
                    el = el.hover(|s| s.bg(theme.accent.opacity(0.08)));
                }
                el.child(
                    div()
                        .w_4()
                        .h_4()
                        .flex_none()
                        .rounded_full()
                        .border_1()
                        .border_color(mark_color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(mark_color)
                        .text_xs()
                        .child(kind_letter.to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .flex()
                        .items_baseline()
                        .gap_1()
                        .child(file_name)
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_faint)
                                .text_ellipsis()
                                .child(if *both_sides {
                                    format!("{dir} ·also staged")
                                } else if let Some(o) = orig {
                                    format!("← {o}")
                                } else {
                                    dir
                                }),
                        ),
                )
                .child(
                    // Hover actions: stage/unstage + discard.
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id((
                                    "stg",
                                    index as u64 + index_of_bucket(bucket) as u64 * 10_000,
                                ))
                                .w_5()
                                .h_5()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .text_color(theme.accent)
                                .hover(|s| s.bg(theme.accent.opacity(0.15)))
                                .occlude()
                                .child(
                                    icon(if bucket == Bucket::Staged {
                                        "x"
                                    } else {
                                        "check"
                                    })
                                    .size_3(),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_stage(bucket, index, cx);
                                    cx.stop_propagation();
                                })),
                        )
                        .child(
                            div()
                                .id((
                                    "dis",
                                    index as u64 + index_of_bucket(bucket) as u64 * 10_000,
                                ))
                                .w_5()
                                .h_5()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .text_color(theme.danger)
                                .hover(|s| s.bg(theme.danger.opacity(0.15)))
                                .occlude()
                                .child(icon("trash").size_3().text_color(theme.danger))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.ask_discard_file(discard_path.clone(), is_untracked, cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = Some(Selected::File { bucket, index });
                    this.refresh_selected_diff(cx);
                    cx.notify();
                }))
                .into_any_element()
            }
        };
        el
    }

    /// The commit composer at the rail's foot.
    fn commit_box(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        let staged_n = self
            .repo
            .status
            .as_ref()
            .map(|s| s.staged.len())
            .unwrap_or(0);
        let can = self.can_commit(cx);
        let branch = self.branch_label();

        let undo = if self.tip_is_unpushed {
            Some(
                div()
                    .id("undo-commit")
                    .text_xs()
                    .text_color(theme.text_dim)
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.text))
                    .child("Undo last commit")
                    .on_click(cx.listener(|this, _, _, cx| this.undo_commit(cx))),
            )
        } else {
            None
        };

        div()
            .flex_none()
            .p_3()
            .pt_2()
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_xs().text_color(theme.text_dim).child(format!(
                "{staged_n} staged · {} {}",
                self.author_name,
                if self.author_name.is_empty() {
                    ""
                } else {
                    "will commit"
                }
            )))
            .child(
                div()
                    .rounded_md()
                    .bg(theme.input)
                    .border_1()
                    .border_color(theme.border)
                    .px_2()
                    .py_1()
                    .text_sm()
                    .child(self.summary.clone()),
            )
            .child(
                div()
                    .rounded_md()
                    .bg(theme.input)
                    .border_1()
                    .border_color(theme.border)
                    .px_2()
                    .py_1()
                    .text_sm()
                    .child(self.description.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("commit-btn")
                            .flex_1()
                            .h_7()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_sm()
                            .bg(if can { theme.accent } else { theme.elevated })
                            .text_color(if can {
                                theme.on_accent
                            } else {
                                theme.text_faint
                            })
                            .when(can, |el| el.cursor_pointer())
                            .child(format!("Commit {staged_n} to {branch}"))
                            .on_click(cx.listener(|this, _, _, cx| this.do_commit(cx))),
                    )
                    .children(undo),
            )
    }
}

fn kind_letter(f: &FileChange) -> char {
    use wackdepot_core::status::ChangeKind::*;
    match f.kind {
        Added => 'A',
        Deleted => 'D',
        Renamed => 'R',
        Copied => 'C',
        TypeChanged => 'T',
        Unmerged => 'U',
        Modified => 'M',
    }
}

fn index_of_bucket(b: Bucket) -> usize {
    match b {
        Bucket::Conflicted => 0,
        Bucket::Staged => 1,
        Bucket::Unstaged => 2,
        Bucket::Untracked => 3,
    }
}
