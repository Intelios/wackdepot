//! The content card: one sheet in WackCode's anatomy — a lighter head band
//! with an accent mark, a dotted seam, and the diff on the well. Unified or
//! split, syntax-coloured, word-level emphasis, hunk actions, and
//! line-level staging by clicking or dragging the gutter.

mod text;

use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, ListAlignment, ListState, MouseButton,
    ParentElement, Render, Styled, StyledText, Window, div, list, prelude::*, px,
};
use wackdepot_core::diff::{DiffFileStatus, DiffLine, DiffLineKind, FileDiff};
use wackdepot_core::stage::LineSel;

use crate::app::overlays::{Overlays, Pending};
use crate::app::prefs;
use crate::components::button::{Variant, button, icon_button};
use crate::components::frost::frosted;
use crate::components::mark::{accent_mark, status_mark};
use crate::components::misc::empty_state;
use crate::components::pill_track::pill_track;
use crate::components::seam::seam;
use crate::components::spinner::orbit;
use crate::model::{
    Bucket, ConfirmAction, DiffDoc, DiffState, DocKind, RepoEvent, RepoModel, Selection,
    same_target,
};
use crate::theme::{ActiveTheme, Theme, metrics};
use crate::views::time::long_age;

const CODE_H: f32 = 22.0;
const HUNK_H: f32 = 36.0;
const GUTTER_W: f32 = 46.0;
/// A commit body shows this many lines before a "Show more" toggle.
const BODY_LINES: usize = 5;
/// ~BODY_LINES of wrapped text — a single hard line can be long too.
const BODY_CHARS: usize = 380;

/// One row of the rendered diff.
#[derive(Debug, Clone, Copy)]
enum Row {
    Hunk {
        hunk: usize,
    },
    Line {
        hunk: usize,
        line: usize,
    },
    /// Split view: the old line on the left, the new one on the right.
    Pair {
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
}

pub struct DiffCard {
    model: Entity<RepoModel>,
    overlays: Entity<Overlays>,
    doc: Option<Arc<DiffDoc>>,
    split: bool,
    rows: Rc<Vec<Row>>,
    list: ListState,
    /// Lines picked for partial staging, as (hunk, line).
    sel: BTreeSet<(usize, usize)>,
    /// Dragging across the gutter: `Some(true)` adds lines, `Some(false)`
    /// removes them.
    drag: Option<bool>,
    /// SHA of the commit whose body is expanded past its clamp.
    body_expanded: Option<String>,
}

impl DiffCard {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: Entity<Overlays>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&model, |this: &mut Self, model, e: &RepoEvent, cx| {
            if matches!(e, RepoEvent::Diff) {
                let doc = match &model.read(cx).diff {
                    DiffState::Ready(d) => Some(d.clone()),
                    _ => None,
                };
                this.set_doc(doc, cx);
            } else if matches!(e, RepoEvent::Busy | RepoEvent::Selection) {
                cx.notify();
            }
        })
        .detach();
        let split = prefs::get(cx).diff_split;
        Self {
            model,
            overlays,
            doc: None,
            split,
            rows: Rc::new(Vec::new()),
            list: ListState::new(0, ListAlignment::Top, px(600.)),
            sel: BTreeSet::new(),
            drag: None,
            body_expanded: None,
        }
    }

    fn set_doc(&mut self, doc: Option<Arc<DiffDoc>>, cx: &mut Context<Self>) {
        let same_target = match (&self.doc, &doc) {
            (Some(a), Some(b)) => a.selection == b.selection,
            _ => false,
        };
        // A refreshed diff for the same file keeps the scroll position; the
        // line pick is dropped because line numbers may have shifted.
        self.doc = doc;
        self.sel.clear();
        self.rebuild(same_target);
        cx.notify();
    }

    /// The split preference changed (View › Split Diff).
    pub fn sync_prefs(&mut self, cx: &mut Context<Self>) {
        let split = prefs::get(cx).diff_split;
        if split != self.split {
            self.split = split;
            self.rebuild(false);
            cx.notify();
        }
    }

    fn rebuild(&mut self, keep_scroll: bool) {
        let rows = match self.doc.as_ref().and_then(|d| d.file.as_ref()) {
            Some(f) => build_rows(f, self.split),
            None => Vec::new(),
        };
        if keep_scroll && rows.len() == self.rows.len() {
            self.rows = Rc::new(rows);
            return;
        }
        self.list.reset(rows.len());
        self.rows = Rc::new(rows);
    }

    fn file(&self) -> Option<&FileDiff> {
        self.doc.as_ref()?.file.as_ref()
    }

    fn bucket(&self) -> Option<Bucket> {
        match self.doc.as_ref()?.kind {
            DocKind::Working { bucket } => Some(bucket),
            DocKind::Commit { .. } => None,
        }
    }

    fn line(&self, hunk: usize, line: usize) -> Option<&DiffLine> {
        self.file()?.hunks.get(hunk)?.lines.get(line)
    }

    fn changeable(&self, hunk: usize, line: usize) -> bool {
        self.doc.as_ref().is_some_and(|d| d.stageable())
            && self
                .line(hunk, line)
                .is_some_and(|l| matches!(l.kind, DiffLineKind::Added | DiffLineKind::Removed))
    }

    fn gutter_down(&mut self, hunk: usize, line: usize, cx: &mut Context<Self>) {
        if !self.changeable(hunk, line) {
            return;
        }
        let add = !self.sel.contains(&(hunk, line));
        self.apply_pick(hunk, line, add);
        self.drag = Some(add);
        cx.notify();
    }

    fn gutter_enter(&mut self, hunk: usize, line: usize, cx: &mut Context<Self>) {
        if let Some(add) = self.drag
            && self.changeable(hunk, line)
        {
            self.apply_pick(hunk, line, add);
            cx.notify();
        }
    }

    fn apply_pick(&mut self, hunk: usize, line: usize, add: bool) {
        if add {
            self.sel.insert((hunk, line));
        } else {
            self.sel.remove(&(hunk, line));
        }
    }

    fn stage_picked(&mut self, cx: &mut Context<Self>) {
        let (Some(file), Some(bucket)) = (self.file().cloned(), self.bucket()) else {
            return;
        };
        let sel = LineSel(std::mem::take(&mut self.sel));
        let unstage = bucket == Bucket::Staged;
        self.model
            .update(cx, |m, cx| m.stage_lines(file, sel, unstage, cx));
        cx.notify();
    }

    fn discard_picked(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.file().cloned() else {
            return;
        };
        let count = self.sel.len();
        let path = file.path().to_string();
        let sel = LineSel(std::mem::take(&mut self.sel));
        self.overlays.update(cx, |o, cx| {
            o.confirm(
                ConfirmAction::DiscardLines { path, count },
                Some(Pending::Lines(Box::new(file), sel)),
                cx,
            )
        });
        cx.notify();
    }

    fn hunk_action(&mut self, hunk: usize, discard: bool, cx: &mut Context<Self>) {
        let (Some(file), Some(bucket)) = (self.file().cloned(), self.bucket()) else {
            return;
        };
        let Some(h) = file.hunks.get(hunk).cloned() else {
            return;
        };
        if discard {
            let path = file.path().to_string();
            self.overlays.update(cx, |o, cx| {
                o.confirm(
                    ConfirmAction::DiscardHunk { path, hunk },
                    Some(Pending::Hunk(Box::new(file), Box::new(h))),
                    cx,
                )
            });
        } else if bucket == Bucket::Staged {
            self.model.update(cx, |m, cx| m.unstage_hunk(file, h, cx));
        } else {
            self.model.update(cx, |m, cx| m.stage_hunk(file, h, cx));
        }
    }

    // ── Rows ───────────────────────────────────────────────────────────

    fn render_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(ix).copied() else {
            return div().into_any_element();
        };
        match row {
            Row::Hunk { hunk } => self.hunk_row(hunk, cx),
            Row::Line { hunk, line } => self.unified_row(hunk, line, cx),
            Row::Pair { hunk, left, right } => self.pair_row(hunk, left, right, cx),
        }
    }

    fn hunk_row(&mut self, hunk: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let Some(h) = self.file().and_then(|f| f.hunks.get(hunk)) else {
            return div().into_any_element();
        };
        // "@@ -52,7 +52,9 @@ fn gutter" → the range, then the context.
        let (range, context) = match h.header.rfind("@@") {
            Some(i) if i > 2 => (
                h.header[..i + 2].to_string(),
                h.header[i + 2..].trim().to_string(),
            ),
            _ => (h.header.clone(), String::new()),
        };
        let stageable = self.doc.as_ref().is_some_and(|d| d.stageable());
        let bucket = self.bucket();
        let entity = cx.entity();
        let mut actions = div().flex().items_center().gap(px(4.));
        if stageable {
            let e = entity.clone();
            let label = if bucket == Some(Bucket::Staged) {
                "Unstage hunk"
            } else {
                "Stage hunk"
            };
            actions = actions.child(button(("hunk-stage", hunk), label).small().on_click(
                move |_, _, cx| {
                    e.update(cx, |d, cx| d.hunk_action(hunk, false, cx));
                },
            ));
            if bucket != Some(Bucket::Staged) {
                let e = entity.clone();
                actions = actions.child(
                    button(("hunk-discard", hunk), "Discard")
                        .small()
                        .danger()
                        .on_click(move |_, _, cx| {
                            e.update(cx, |d, cx| d.hunk_action(hunk, true, cx));
                        }),
                );
            }
        }
        div()
            .h(px(HUNK_H))
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .when(hunk > 0, |el| el.border_t_1().border_color(t.border))
            .child(
                div()
                    .flex_none()
                    .px(px(9.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(t.sem.diff_hunk_bg.opacity(0.35))
                    .font_family(t.mono.clone())
                    .text_size(px(11.))
                    .text_color(t.sem.diff_hunk)
                    .child(range),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(t.mono.clone())
                    .text_size(px(11.5))
                    .text_color(t.text_faint)
                    .child(context),
            )
            .child(actions)
            .into_any_element()
    }

    fn gutter(
        &self,
        n: Option<u32>,
        picked: bool,
        hunk: usize,
        line: usize,
        id: (&'static str, usize),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        let changeable = self.changeable(hunk, line);
        let mut g = div()
            .id(id)
            .w(px(GUTTER_W))
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .pr(px(10.))
            .text_size(px(11.))
            .text_color(if picked { t.accent } else { t.text_faint })
            .child(n.map(|n| n.to_string()).unwrap_or_default());
        if picked {
            g = g.bg(t.accent.opacity(0.10)).font_weight(FontWeight::BOLD);
        }
        if changeable {
            g = g
                .cursor_pointer()
                .hover(move |s| s.text_color(t.accent))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |d, _, _, cx| d.gutter_down(hunk, line, cx)),
                )
                .on_mouse_move(cx.listener(move |d, _, _, cx| d.gutter_enter(hunk, line, cx)));
        }
        g
    }

    fn code(&self, l: &DiffLine, hunk: usize, line: usize, t: &Theme) -> impl IntoElement {
        let doc = self.doc.as_ref();
        let spans = doc.and_then(|d| d.spans(l));
        let words = doc
            .and_then(|d| d.words.get(hunk))
            .and_then(|w| w.get(line))
            .and_then(|w| w.as_deref());
        let word_bg = if l.kind == DiffLineKind::Removed {
            t.sem.diff_del_word
        } else {
            t.sem.diff_add_word
        };
        let (shown, hl) = text::styled_line(t, &l.text, spans, words, word_bg);
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(if l.kind == DiffLineKind::NoNewline {
                t.text_faint
            } else {
                t.code_text
            })
            .child(StyledText::new(shown).with_highlights(hl))
    }

    fn unified_row(&mut self, hunk: usize, line: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let Some(l) = self.line(hunk, line).cloned() else {
            return div().into_any_element();
        };
        let picked = self.sel.contains(&(hunk, line));
        let (bg, edge, sigil) = row_tone(&t, l.kind);
        let bg = if picked {
            bg.map(|b| b.opacity((b.a * 2.2).min(0.5)))
        } else {
            bg
        };
        let key = hunk * 100_000 + line;
        let mut row = div()
            .h(px(CODE_H))
            .w_full()
            .relative()
            .flex()
            .items_center()
            .font_family(t.mono.clone())
            .text_size(px(metrics::TEXT_CODE))
            .child(self.gutter(l.old_lineno, picked, hunk, line, ("go", key), cx))
            .child(self.gutter(l.new_lineno, picked, hunk, line, ("gn", key), cx))
            .child(
                div()
                    .w(px(18.))
                    .flex_none()
                    .text_color(edge.unwrap_or(t.text_faint))
                    .child(sigil),
            )
            .child(self.code(&l, hunk, line, &t));
        if let Some(bg) = bg {
            row = row.bg(bg);
        }
        if let Some(edge) = edge {
            row = row.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(if picked { t.accent } else { edge }),
            );
        }
        row.into_any_element()
    }

    fn pair_row(
        &mut self,
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.theme().clone();
        let key = hunk * 100_000 + left.or(right).unwrap_or(0);
        let half = |idx: Option<usize>, old_side: bool, cx: &mut Context<Self>| -> AnyElement {
            let Some(li) = idx else {
                return div()
                    .flex_1()
                    .h_full()
                    .bg(t.sem.black.opacity(0.12))
                    .into_any_element();
            };
            let Some(l) = self.line(hunk, li).cloned() else {
                return div().flex_1().into_any_element();
            };
            let picked = self.sel.contains(&(hunk, li));
            let (bg, edge, _) = row_tone(&t, l.kind);
            let n = if old_side { l.old_lineno } else { l.new_lineno };
            let mut h = div()
                .flex_1()
                .min_w_0()
                .h_full()
                .relative()
                .flex()
                .items_center()
                .child(self.gutter(
                    n,
                    picked,
                    hunk,
                    li,
                    (if old_side { "sl" } else { "sr" }, key),
                    cx,
                ))
                .child(self.code(&l, hunk, li, &t));
            if let Some(bg) = bg {
                h = h.bg(if picked {
                    bg.opacity((bg.a * 2.2).min(0.5))
                } else {
                    bg
                });
            }
            if let Some(edge) = edge {
                h = h.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(if picked { t.accent } else { edge }),
                );
            }
            h.into_any_element()
        };
        let l = half(left, true, cx);
        let r = half(right, false, cx);
        div()
            .h(px(CODE_H))
            .w_full()
            .flex()
            .font_family(t.mono.clone())
            .text_size(px(metrics::TEXT_CODE))
            .child(l)
            .child(div().w(px(1.)).h_full().bg(t.border))
            .child(r)
            .into_any_element()
    }

    // ── Head bands ─────────────────────────────────────────────────────

    fn view_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ws = prefs::get(cx).ignore_whitespace;
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                icon_button("ws-toggle", "whitespace")
                    .variant(if ws { Variant::Lit } else { Variant::Ghost })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(crate::app::actions::ToggleWhitespace), cx)
                    }),
            )
            .child(
                pill_track("diff-mode")
                    .height(26.)
                    .item("Unified")
                    .item("Split")
                    .selected(usize::from(self.split))
                    .on_select(|i, window, cx| {
                        if (i == 1) != prefs::get(cx).diff_split {
                            window.dispatch_action(
                                Box::new(crate::app::actions::ToggleSplitDiff),
                                cx,
                            );
                        }
                    }),
            )
    }

    fn file_head(&self, file: &FileDiff, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let path = file.path().to_string();
        let (dir, name) = match path.rfind('/') {
            Some(i) => (path[..=i].to_string(), path[i + 1..].to_string()),
            None => (String::new(), path.clone()),
        };
        let note = match file.status {
            DiffFileStatus::Added => Some("New file".to_string()),
            DiffFileStatus::Deleted => Some("Deleted".to_string()),
            DiffFileStatus::Renamed => file.old_path.as_ref().map(|o| format!("Renamed from {o}")),
            DiffFileStatus::Binary => Some("Binary".to_string()),
            DiffFileStatus::Modified => None,
        };
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .h(px(46.))
            .px(px(12.))
            .bg(t.surface)
            .child(accent_mark("file-code"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(t.mono.clone())
                            .text_size(px(12.5))
                            .child(
                                div()
                                    .flex()
                                    .child(div().text_color(t.text_dim).child(dir))
                                    .child(div().text_color(t.text).child(name)),
                            ),
                    )
                    .children(note.map(|n| {
                        div()
                            .flex_none()
                            .px(px(8.))
                            .h(px(19.))
                            .flex()
                            .items_center()
                            .rounded_full()
                            .bg(t.pill())
                            .text_size(px(11.))
                            .text_color(t.text_soft)
                            .child(n)
                    })),
            )
            .child(self.view_controls(cx))
    }

    fn commit_head(&self, doc: &DiffDoc, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let DocKind::Commit { entry, files } = &doc.kind else {
            return div().into_any_element();
        };
        let (added, removed) = files.iter().fold((0, 0), |(a, r), f| {
            (a + f.added.unwrap_or(0), r + f.removed.unwrap_or(0))
        });
        let sha = entry.sha.clone();
        div()
            .flex()
            .items_start()
            .gap(px(12.))
            .px(px(14.))
            .py(px(12.))
            .bg(t.surface)
            .child(div().pt(px(2.)).child(accent_mark("commit").size(28.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(metrics::TEXT_TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(entry.subject.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(t.text_dim)
                            .child(entry.author_name.clone())
                            .child("·")
                            .child(long_age(&entry.authored_at))
                            .child("·")
                            .child(div().font_family(t.mono.clone()).child(entry.short.clone()))
                            .child("·")
                            .child(format!(
                                "{} file{}",
                                files.len(),
                                if files.len() == 1 { "" } else { "s" }
                            ))
                            .child(div().text_color(t.sem.diff_add).child(format!("+{added}")))
                            .child(
                                div()
                                    .text_color(t.sem.diff_del)
                                    .child(format!("−{removed}")),
                            ),
                    )
                    .when(!entry.body.trim().is_empty(), |el| {
                        let body = entry.body.trim().to_string();
                        let long = body.lines().count() > BODY_LINES || body.len() > BODY_CHARS;
                        let expanded = self.body_expanded.as_deref() == Some(entry.sha.as_str());
                        let mut text = div()
                            .pt(px(4.))
                            .text_size(px(12.5))
                            .text_color(t.text_soft)
                            .child(body);
                        if long && !expanded {
                            text = text
                                .overflow_hidden()
                                .text_ellipsis()
                                .line_clamp(BODY_LINES);
                        }
                        el.child(text).when(long, |el| {
                            let sha = entry.sha.clone();
                            let ent = cx.entity();
                            el.child(
                                div().flex().pt(px(4.)).child(
                                    button(
                                        "body-toggle",
                                        if expanded { "Show less" } else { "Show more" },
                                    )
                                    .small()
                                    .ghost()
                                    .on_click(
                                        move |_, _, cx| {
                                            ent.update(cx, |d, cx| {
                                                d.body_expanded =
                                                    if expanded { None } else { Some(sha.clone()) };
                                                cx.notify();
                                            })
                                        },
                                    ),
                                ),
                            )
                        })
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon_button("copy-sha", "copy").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(sha.clone()));
                    }))
                    .child(self.view_controls(cx)),
            )
            .into_any_element()
    }

    fn file_strip(&self, doc: &DiffDoc, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let DocKind::Commit { entry, files } = &doc.kind else {
            return div().into_any_element();
        };
        // The live selection drives the highlight so a click lights its row
        // before the new diff lands; otherwise the file the doc is showing
        // stays lit (it may still be on screen while the next doc loads).
        let current = match &self.model.read(cx).selection {
            Some(Selection::Commit { sha, file: Some(f) }) if *sha == entry.sha => Some(f.clone()),
            _ => match &doc.selection {
                Selection::Commit { file, .. } => file.clone(),
                _ => None,
            },
        };
        let mut col = div()
            .id("commit-files")
            .w(px(240.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(t.border)
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(1.));
        for (i, f) in files.iter().enumerate() {
            let selected = current.as_deref() == Some(f.path.as_str());
            let letter = match f.status {
                DiffFileStatus::Added => 'A',
                DiffFileStatus::Deleted => 'D',
                DiffFileStatus::Renamed => 'R',
                _ => 'M',
            };
            let name = f.path.rsplit('/').next().unwrap_or(&f.path).to_string();
            let (sha, path) = (entry.sha.clone(), f.path.clone());
            let m = self.model.clone();
            let mut row = div()
                .id(("cf", i))
                .h(px(30.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .rounded_full()
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let sel = Selection::Commit {
                        sha: sha.clone(),
                        file: Some(path.clone()),
                    };
                    m.update(cx, |m, cx| m.select(Some(sel), cx));
                })
                .child(status_mark(letter).size(18.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(t.mono.clone())
                        .text_size(px(12.))
                        .child(name),
                )
                .children(f.added.filter(|a| *a > 0).map(|a| {
                    div()
                        .text_size(px(10.5))
                        .text_color(t.sem.diff_add)
                        .child(format!("+{a}"))
                }))
                .children(f.removed.filter(|r| *r > 0).map(|r| {
                    div()
                        .text_size(px(10.5))
                        .text_color(t.sem.diff_del)
                        .child(format!("−{r}"))
                }));
            if selected {
                row = row.bg(t.selected_fill()).shadow(vec![t.rim(t.accent_line)]);
            } else {
                row = row.hover(|s| s.bg(t.hover()));
            }
            col = col.child(row);
        }
        col.into_any_element()
    }

    /// The diff well. While a different selection's diff loads, the last
    /// diff stays on screen dimmed under a veil (blocking stray clicks)
    /// with the orbit — the card's chrome doesn't collapse to a spinner.
    fn body_pane(&self, stale: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let body = self.body(cx);
        if !stale {
            return body;
        }
        div()
            .size_full()
            .relative()
            .child(body)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(t.well.opacity(0.55))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(orbit("diff-swap").size(18.)),
            )
            .into_any_element()
    }

    fn body(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let Some(file) = self.file() else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state("commit", "This commit has no file changes"))
                .into_any_element();
        };
        if file.is_binary {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    empty_state("file", "Binary file")
                        .body("There's no text diff to show for this file."),
                )
                .into_any_element();
        }
        if file.hunks.is_empty() {
            let body = if self.doc.as_ref().is_some_and(|d| d.ignore_whitespace) {
                "Only whitespace changed, and whitespace changes are hidden (View › Hide Whitespace Changes)."
            } else {
                "The content is the same — only the file's mode or name changed."
            };
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state("check", "No text changes").body(body))
                .into_any_element();
        }
        let entity = cx.entity();
        div()
            .size_full()
            .bg(t.well)
            .child(
                list(self.list.clone(), move |ix, _window, cx| {
                    entity.update(cx, |d, cx| d.render_row(ix, cx))
                })
                .size_full(),
            )
            .into_any_element()
    }

    fn pick_pill(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if self.sel.is_empty() {
            return None;
        }
        let t = cx.theme().clone();
        let n = self.sel.len();
        let staged = self.bucket() == Some(Bucket::Staged);
        let entity = cx.entity();
        let (e1, e2, e3) = (entity.clone(), entity.clone(), entity.clone());
        let mut pill = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(40.))
            .pl(px(14.))
            .pr(px(6.))
            .rounded_full()
            .bg(t.frost())
            .border_1()
            .border_color(t.border_bright.opacity(0.8))
            .shadow(t.shadow_pop())
            .text_size(px(12.5))
            .child(format!(
                "{n} line{} selected",
                if n == 1 { "" } else { "s" }
            ))
            .child(
                button(
                    "pick-stage",
                    if staged {
                        "Unstage lines"
                    } else {
                        "Stage lines"
                    },
                )
                .small()
                .primary()
                .on_click(move |_, _, cx| e1.update(cx, |d, cx| d.stage_picked(cx))),
            );
        if !staged {
            pill = pill.child(
                button("pick-discard", "Discard")
                    .small()
                    .danger()
                    .on_click(move |_, _, cx| e2.update(cx, |d, cx| d.discard_picked(cx))),
            );
        }
        pill = pill.child(
            icon_button("pick-clear", "close")
                .small()
                .on_click(move |_, _, cx| {
                    e3.update(cx, |d, cx| {
                        d.sel.clear();
                        cx.notify();
                    })
                }),
        );
        Some(
            div()
                .absolute()
                .bottom(px(18.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(frosted(999.0, metrics::FROST_BLUR, pill)),
        )
    }
}

/// Row background, edge colour and sigil for a line kind.
fn row_tone(
    t: &Theme,
    kind: DiffLineKind,
) -> (Option<gpui::Hsla>, Option<gpui::Hsla>, &'static str) {
    match kind {
        DiffLineKind::Added => (Some(t.sem.diff_add_bg), Some(t.sem.diff_add), "+"),
        DiffLineKind::Removed => (Some(t.sem.diff_del_bg), Some(t.sem.diff_del), "−"),
        DiffLineKind::Context | DiffLineKind::NoNewline => (None, None, ""),
    }
}

/// Rows for a file diff: a band per hunk, then its lines (unified) or
/// paired old/new lines (split; a run of removals beside the run of
/// additions that follows it).
fn build_rows(file: &FileDiff, split: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    for (hi, h) in file.hunks.iter().enumerate() {
        rows.push(Row::Hunk { hunk: hi });
        if !split {
            rows.extend((0..h.lines.len()).map(|li| Row::Line { hunk: hi, line: li }));
            continue;
        }
        let lines = &h.lines;
        let mut i = 0;
        while i < lines.len() {
            match lines[i].kind {
                DiffLineKind::Context => {
                    rows.push(Row::Pair {
                        hunk: hi,
                        left: Some(i),
                        right: Some(i),
                    });
                    i += 1;
                }
                DiffLineKind::NoNewline => i += 1,
                _ => {
                    let mut dels = Vec::new();
                    while i < lines.len()
                        && matches!(
                            lines[i].kind,
                            DiffLineKind::Removed | DiffLineKind::NoNewline
                        )
                    {
                        if lines[i].kind == DiffLineKind::Removed {
                            dels.push(i);
                        }
                        i += 1;
                    }
                    let mut adds = Vec::new();
                    while i < lines.len()
                        && matches!(lines[i].kind, DiffLineKind::Added | DiffLineKind::NoNewline)
                    {
                        if lines[i].kind == DiffLineKind::Added {
                            adds.push(i);
                        }
                        i += 1;
                    }
                    for k in 0..dels.len().max(adds.len()) {
                        rows.push(Row::Pair {
                            hunk: hi,
                            left: dels.get(k).copied(),
                            right: adds.get(k).copied(),
                        });
                    }
                }
            }
        }
    }
    rows
}

impl Render for DiffCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let loading = matches!(model.diff, DiffState::Loading);
        let error = match &model.diff {
            DiffState::Error(e) => Some(e.clone()),
            _ => None,
        };
        let has_selection = model.selection.is_some();
        let mut card = div().size_full().relative().flex().flex_col().on_mouse_up(
            MouseButton::Left,
            cx.listener(|d, _, _, cx| {
                d.drag = None;
                cx.notify();
            }),
        );
        // The kept doc is stale once the selection moved on to a different
        // target: its body veils while the new diff loads.
        let stale = match (&model.selection, self.doc.as_ref()) {
            (Some(sel), Some(doc)) => !same_target(&doc.selection, sel),
            _ => false,
        };
        let Some(doc) = self.doc.clone() else {
            let inner = if let Some(e) = error {
                empty_state("alert", "Couldn't load this diff")
                    .body(e)
                    .into_any_element()
            } else if loading && has_selection {
                orbit("diff-loading").size(18.).into_any_element()
            } else {
                empty_state("file-code", "Pick a file to see its changes")
                    .body("Select a change on the left, or a commit in History.")
                    .into_any_element()
            };
            return card
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(inner),
                )
                .into_any_element();
        };
        match &doc.kind {
            DocKind::Working { .. } => {
                if let Some(file) = doc.file.as_ref() {
                    card = card.child(self.file_head(file, cx));
                }
                card = card
                    .child(seam())
                    .child(div().flex_1().min_h_0().child(self.body_pane(stale, cx)));
            }
            DocKind::Commit { .. } => {
                card = card.child(self.commit_head(&doc, cx)).child(seam()).child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(self.file_strip(&doc, cx))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .child(self.body_pane(stale, cx)),
                        ),
                );
            }
        }
        if doc.ignore_whitespace && matches!(doc.kind, DocKind::Working { .. }) {
            card = card.child(
                div()
                    .absolute()
                    .top(px(52.))
                    .right(px(14.))
                    .px(px(10.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(t.sem.warning_bg)
                    .text_size(px(11.))
                    .text_color(t.sem.warning)
                    .child("Whitespace hidden — staging lines is off"),
            );
        }
        if !stale {
            card = card.children(self.pick_pill(cx));
        }
        card.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wackdepot_core::diff::parse_diff;

    #[test]
    fn split_pairs_runs_side_by_side() {
        let patch =
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,4 +1,4 @@\n ctx\n-a\n-b\n+A\n tail\n";
        let f = &parse_diff(patch)[0];
        let rows = build_rows(f, true);
        // hunk band, ctx pair, (a|A), (b|·), tail pair
        assert_eq!(rows.len(), 5);
        assert!(matches!(
            rows[2],
            Row::Pair {
                left: Some(1),
                right: Some(3),
                ..
            }
        ));
        assert!(matches!(
            rows[3],
            Row::Pair {
                left: Some(2),
                right: None,
                ..
            }
        ));
        assert_eq!(build_rows(f, false).len(), 6);
    }
}
