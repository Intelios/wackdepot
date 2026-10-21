//! The Changes list: groups (Conflicts, Staged, Changes, Untracked) with a
//! count and a dotted trail, then file rows as pills. Rows are absolutely
//! positioned and keyed by path, so when a file moves group it glides there
//! (FLIP) instead of jumping.

use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window, div, prelude::*, px,
};

use super::{
    ContextTarget, DiscardSelected, RowKey, SelectNext, SelectPrev, SidebarView, ToggleInclude,
};
use wackdepot_core::stage::ConflictSide;

use crate::components::button::icon_button;
use crate::components::mark::{Check, checkbox, status_mark};
use crate::components::menu::{menu_row, menu_separator, popover};
use crate::components::misc::{empty_state, rolling_count};
use crate::components::seam::trail;
use crate::icons::icon;
use crate::model::{Bucket, ConfirmAction, Selection};
use crate::motion::{EASE, id, spring};
use crate::theme::{ActiveTheme, metrics};

struct Row {
    key: RowKey,
    y: f32,
    h: f32,
}

/// The pseudo-bucket standing in for the merged Changes group in
/// `RowKey::Group` and `collapsed` — the real buckets merged into it.
const CHANGES_GROUP: Bucket = Bucket::Unstaged;

impl SidebarView {
    /// Every row's key and position, top to bottom.
    fn changes_layout(&self, cx: &Context<Self>) -> (Vec<Row>, f32) {
        let model = self.model.read(cx);
        let conflicts = model.snap.bucket(Bucket::Conflicted).len();
        let mut rows = Vec::new();
        let mut y = 0.0;
        let group = |b: Bucket,
                     files: &[(Bucket, wackdepot_core::status::FileChange)],
                     rows: &mut Vec<Row>,
                     y: &mut f32| {
            if files.is_empty() {
                return;
            }
            if !rows.is_empty() {
                *y += 6.0;
            }
            rows.push(Row {
                key: RowKey::Group(b),
                y: *y,
                h: metrics::GROUP_H,
            });
            *y += metrics::GROUP_H;
            if self.collapsed.contains(&b) {
                return;
            }
            for (fb, f) in files {
                rows.push(Row {
                    key: RowKey::File(f.path.clone(), *fb),
                    y: *y,
                    h: metrics::ROW_H,
                });
                *y += metrics::ROW_H;
            }
        };
        if conflicts > 0 {
            let list: Vec<(Bucket, wackdepot_core::status::FileChange)> = model
                .snap
                .bucket(Bucket::Conflicted)
                .iter()
                .map(|f| (Bucket::Conflicted, f.clone()))
                .collect();
            group(Bucket::Conflicted, &list, &mut rows, &mut y);
        }
        group(CHANGES_GROUP, &model.commit_files(), &mut rows, &mut y);
        (rows, y)
    }

    /// The files in display order (for keyboard movement).
    fn visible_files(&self, cx: &Context<Self>) -> Vec<(String, Bucket)> {
        let model = self.model.read(cx);
        let mut out: Vec<(String, Bucket)> = Vec::new();
        if !self.collapsed.contains(&Bucket::Conflicted) {
            out.extend(
                model
                    .snap
                    .bucket(Bucket::Conflicted)
                    .iter()
                    .map(|f| (f.path.clone(), Bucket::Conflicted)),
            );
        }
        if !self.collapsed.contains(&CHANGES_GROUP) {
            out.extend(
                model
                    .commit_files()
                    .into_iter()
                    .map(|(b, f)| (f.path.clone(), b)),
            );
        }
        out
    }

    pub(super) fn step_file(&mut self, delta: isize, cx: &mut Context<Self>) {
        let files = self.visible_files(cx);
        if files.is_empty() {
            return;
        }
        let current = match &self.model.read(cx).selection {
            Some(Selection::File { path, bucket }) => {
                files.iter().position(|(p, b)| p == path && b == bucket)
            }
            _ => None,
        };
        let next = match current {
            Some(i) => (i as isize + delta).clamp(0, files.len() as isize - 1) as usize,
            None => 0,
        };
        let (path, bucket) = files[next].clone();
        self.model.update(cx, |m, cx| {
            m.select(Some(Selection::File { path, bucket }), cx)
        });
        self.scroll_selected_into_view(cx);
    }

    fn scroll_selected_into_view(&self, cx: &Context<Self>) {
        let Some(Selection::File { path, bucket }) = self.model.read(cx).selection.clone() else {
            return;
        };
        let (rows, _) = self.changes_layout(cx);
        let Some(row) = rows
            .iter()
            .find(|r| r.key == RowKey::File(path.clone(), bucket))
        else {
            return;
        };
        let view_h = f32::from(self.scroll.bounds().size.height);
        let top = -f32::from(self.scroll.offset().y);
        if row.y < top {
            self.scroll.set_offset(gpui::point(px(0.), px(-row.y)));
        } else if row.y + row.h > top + view_h && view_h > 0.0 {
            self.scroll
                .set_offset(gpui::point(px(0.), px(-(row.y + row.h - view_h))));
        }
    }

    fn toggle_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(Selection::File { path, bucket }) = self.model.read(cx).selection.clone() {
            self.model
                .update(cx, |m, cx| m.toggle_include(path, bucket, cx));
        }
    }

    pub(super) fn ask_discard(&mut self, path: String, bucket: Bucket, cx: &mut Context<Self>) {
        let action = if bucket == Bucket::Untracked {
            ConfirmAction::TrashFile { path }
        } else {
            // Any tracked row — even one only staged — drops everything.
            ConfirmAction::DiscardFile { path }
        };
        self.overlays
            .update(cx, |o, cx| o.confirm(action, None, cx));
    }

    fn discard_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(Selection::File { path, bucket }) = self.model.read(cx).selection.clone()
            && bucket != Bucket::Conflicted
        {
            self.ask_discard(path, bucket, cx);
        }
    }

    pub(super) fn changes_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let model = self.model.read(cx);
        let snap = model.snap.clone();
        let loaded = model.loaded;
        let selection = model.selection.clone();

        if loaded && snap.changed_count() == 0 {
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    empty_state("check", "No changes")
                        .body("The working tree is clean. Edits you make show up here.")
                        .art(crate::components::mascot::mascot("clean-mascot").size(56.)),
                )
                .into_any_element();
        }

        let commit_files = self.model.read(cx).commit_files();
        let (rows, total) = self.changes_layout(cx);
        // FLIP: rows that moved since the last layout start where they were.
        let mut moved_from: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        for (k, y) in &self.prev_y {
            if let RowKey::File(p, _) = k
                && !rows.iter().any(|r| &r.key == k)
            {
                moved_from.insert(p.clone(), *y);
            }
        }
        for r in &rows {
            let old = self.prev_y.get(&r.key).copied().or_else(|| match &r.key {
                RowKey::File(p, _) => moved_from.get(p).copied(),
                _ => None,
            });
            if let Some(old) = old
                && (old - r.y).abs() > 0.5
            {
                self.motion
                    .kick(("dy", id(&r.key)), old - r.y, 0.0, spring::FLY);
            }
        }
        let first_layout = !self.seen && loaded;

        // Only rows near the viewport are built.
        let view_h = f32::from(self.scroll.bounds().size.height);
        let top = -f32::from(self.scroll.offset().y);
        let (lo, hi) = if view_h > 0.0 {
            (top - 300.0, top + view_h + 300.0)
        } else {
            (0.0, 2400.0)
        };

        let mut children: Vec<AnyElement> = Vec::new();
        for (i, r) in rows.iter().enumerate() {
            let dy = self.motion.spring(("dy", id(&r.key)), 0.0, spring::FLY);
            let y = r.y + dy;
            if y + r.h < lo || y > hi {
                continue;
            }
            let fresh = !self.prev_y.contains_key(&r.key)
                && !moved_from.contains_key(match &r.key {
                    RowKey::File(p, _) => p.as_str(),
                    RowKey::Group(_) => "",
                });
            let delay = if first_layout {
                (i.min(10) as f32) * 24.0
            } else {
                0.0
            };
            let (opacity, dx) = if fresh && (self.seen || first_layout) {
                (
                    self.motion
                        .tween_from(("in", id(&r.key)), 0.0, 1.0, 200.0, delay, EASE),
                    self.motion
                        .tween_from(("inx", id(&r.key)), 8.0, 0.0, 240.0, delay, EASE),
                )
            } else {
                (
                    self.motion.tween(("in", id(&r.key)), 1.0, 1.0, EASE),
                    self.motion.tween(("inx", id(&r.key)), 0.0, 1.0, EASE),
                )
            };
            let el = match &r.key {
                RowKey::Group(b) => {
                    let n = if *b == CHANGES_GROUP {
                        commit_files.len()
                    } else {
                        snap.bucket(*b).len()
                    };
                    self.group_row(*b, n, cx).into_any_element()
                }
                RowKey::File(path, b) => {
                    let selected = selection
                        == Some(Selection::File {
                            path: path.clone(),
                            bucket: *b,
                        });
                    let change = if *b == Bucket::Conflicted {
                        snap.bucket(Bucket::Conflicted)
                            .iter()
                            .find(|f| &f.path == path)
                            .cloned()
                    } else {
                        commit_files
                            .iter()
                            .find(|(_, f)| &f.path == path)
                            .map(|(_, f)| f.clone())
                    };
                    match change {
                        Some(change) => self
                            .file_row(path, *b, &change, selected, i, cx)
                            .into_any_element(),
                        None => div().into_any_element(),
                    }
                }
            };
            children.push(
                div()
                    .absolute()
                    .left(px(dx))
                    .right(px(-dx))
                    .top(px(y))
                    .h(px(r.h))
                    .opacity(opacity.clamp(0.0, 1.0))
                    .child(el)
                    .into_any_element(),
            );
        }
        if loaded {
            self.prev_y = rows.iter().map(|r| (r.key.clone(), r.y)).collect();
            self.seen = true;
        }

        let _ = window;
        div()
            .id("changes-scroll")
            .key_context("FileList")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectPrev, window, cx| {
                this.step(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectNext, window, cx| {
                this.step(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleInclude, _, cx| this.toggle_selected(cx)))
            .on_action(cx.listener(|this, _: &DiscardSelected, _, cx| this.discard_selected(cx)))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus_list(window, cx)),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(total.max(1.0)))
                    .children(children),
            )
            .into_any_element()
    }

    fn group_row(&self, b: Bucket, n: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let collapsed = self.collapsed.contains(&b);
        let model = self.model.read(cx);
        let title = b.title();
        // The merged group's master checkbox: off when nothing will
        // commit, on when everything does, dash between.
        let master = if b == CHANGES_GROUP {
            let partials = model.partials();
            let included = model.included_count();
            let partial = model
                .commit_files()
                .iter()
                .any(|(_, f)| !model.excluded.contains(&f.path) && partials.contains(&f.path));
            let state = if included == 0 {
                Check::Off
            } else if included < n || partial {
                Check::Mixed
            } else {
                Check::On
            };
            let m = self.model.clone();
            Some(
                checkbox(("group-check", b as usize), state).on_click(move |_, _, cx| {
                    m.update(cx, |m, cx| {
                        // Anything less than "all in" means "in".
                        m.set_all_included(state != Check::On, cx)
                    })
                }),
            )
        } else {
            None
        };
        let m = self.model.clone();
        let hot = self.hot == Some(RowKey::Group(b));
        let key = RowKey::Group(b);
        div()
            .id(("group", b as usize))
            .size_full()
            .flex()
            .items_center()
            .gap(px(7.))
            .px(px(8.))
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hot = Some(key.clone());
                } else if this.hot == Some(key.clone()) {
                    this.hot = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !this.collapsed.remove(&b) {
                    this.collapsed.insert(b);
                }
                cx.notify();
            }))
            .child(
                icon(if collapsed { "chevron" } else { "chevron-down" })
                    .size(px(11.))
                    .text_color(t.text_faint),
            )
            .children(master)
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if b == Bucket::Conflicted {
                        t.sem.warning
                    } else {
                        t.text_dim
                    })
                    .child(title),
            )
            .child(rolling_count(("group-count", b as usize), n, cx))
            .child(trail())
            .when(b == Bucket::Conflicted && hot, |el| {
                // The only bulk act left: marking every conflict resolved.
                el.child(
                    div()
                        .id("group-act")
                        .flex_none()
                        .px(px(8.))
                        .h(px(22.))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.text_dim)
                        .hover(|s| s.bg(t.hover()).text_color(t.text))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            m.update(cx, |m, cx| m.stage_group(b, cx));
                        })
                        .child("Mark all resolved"),
                )
            })
    }

    fn file_row(
        &self,
        path: &str,
        b: Bucket,
        change: &wackdepot_core::status::FileChange,
        selected: bool,
        i: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let excluded = model.excluded.contains(path);
        let partial = model.partials().contains(path);
        let check = if b == Bucket::Conflicted || excluded {
            Check::Off
        } else if partial {
            Check::Mixed
        } else {
            Check::On
        };
        let letter = match b {
            Bucket::Untracked => '?',
            Bucket::Conflicted => 'U',
            Bucket::Staged => change.x,
            _ => change.y,
        };
        let letter = if letter == 'T' { 'M' } else { letter };
        let (dir, name) = match path.rfind('/') {
            Some(i) => (&path[..=i], &path[i + 1..]),
            None => ("", path),
        };
        let stat = model.snap.line_stat(b, path);
        let (p1, p_check, p3) = (path.to_string(), path.to_string(), path.to_string());
        let m = self.model.clone();
        let entity = cx.entity();

        // The hover pill — mounted only while the row is hovered, so it
        // can't flicker (no group_hover) nor eat clicks while invisible.
        let mut actions = div()
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(2.))
            .rounded_full()
            .bg(t.elevated)
            .shadow(t.shadow_pop());
        if b == Bucket::Conflicted {
            actions =
                actions.child(icon_button(("resolve", i), "check").small().on_click(
                    move |_, _, cx| m.update(cx, |m, cx| m.toggle_stage(p1.clone(), b, cx)),
                ));
        } else {
            // A conflicted path can't be "discarded" (`git restore` refuses
            // unmerged paths) — its outs are ours/theirs/resolve instead.
            let e = entity.clone();
            let p2 = path.to_string();
            actions = actions.child(icon_button(("discard", i), "trash").small().on_click(
                move |_, _, cx| {
                    e.update(cx, |s, cx| s.ask_discard(p2.clone(), b, cx));
                },
            ));
        }

        let hot = self.hot.as_ref() == Some(&RowKey::File(path.to_string(), b));
        let row_key = RowKey::File(path.to_string(), b);
        let m2 = self.model.clone();
        let mut row = div()
            .id(("file", i))
            .mx(px(2.))
            .h(px(metrics::ROW_H - 2.))
            .mt(px(1.))
            .flex()
            .items_center()
            .gap(px(7.))
            .px(px(9.))
            .rounded_full()
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    if this.hot.as_ref() != Some(&row_key) {
                        this.hot = Some(row_key.clone());
                        cx.notify();
                    }
                } else if this.hot.as_ref() == Some(&row_key) {
                    this.hot = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.focus_list(window, cx);
                let sel = Selection::File {
                    path: p3.clone(),
                    bucket: b,
                };
                this.model.update(cx, |m, cx| m.select(Some(sel), cx));
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let path = path.to_string();
                    move |this, e: &MouseDownEvent, _, cx| {
                        this.context = Some((
                            e.position,
                            ContextTarget::File {
                                path: path.clone(),
                                bucket: b,
                            },
                        ));
                        cx.notify();
                    }
                }),
            )
            .child(
                checkbox(("check", i), check)
                    .disabled(b == Bucket::Conflicted)
                    .on_click(move |_, _, cx| {
                        m2.update(cx, |m, cx| m.toggle_include(p_check.clone(), b, cx))
                    }),
            )
            .child(status_mark(letter))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .font_family(t.mono.clone())
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .child(
                        // The folder truncates from its start so the name stays.
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .justify_end()
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(t.text_faint)
                                    .child(dir.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(if selected { t.text } else { t.text_body })
                            .child(name.to_string()),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .min_w(px(48.))
                    .h_full()
                    .child(
                        div()
                            .flex()
                            .gap(px(5.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .opacity(if hot { 0. } else { 1. })
                            .children(stat.and_then(|(a, _)| {
                                (a > 0).then(|| {
                                    div().text_color(t.sem.diff_add).child(format!("+{a}"))
                                })
                            }))
                            .children(stat.and_then(|(_, d)| {
                                (d > 0).then(|| {
                                    div().text_color(t.sem.diff_del).child(format!("−{d}"))
                                })
                            })),
                    )
                    .when(hot, move |el| {
                        el.child(div().absolute().right(px(-4.)).child(actions))
                    }),
            );
        if selected {
            row = row.bg(t.selected_fill()).shadow(vec![t.rim(t.accent_line)]);
        } else {
            row = row.hover(|s| s.bg(t.hover()));
        }
        row
    }

    /// The right-click menu for a file or commit row.
    pub(super) fn context_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (at, target) = self.context.clone()?;
        let this = cx.entity().downgrade();
        let close = {
            let this = this.clone();
            move |cx: &mut gpui::App| {
                if let Some(t) = this.upgrade() {
                    t.update(cx, |s, cx| {
                        s.context = None;
                        cx.notify();
                    });
                }
            }
        };
        let dismiss = {
            let close = close.clone();
            move |_: &mut Window, cx: &mut gpui::App| close(cx)
        };
        let m = self.model.clone();
        let pop = popover("context").width(240.).at(at).on_dismiss(dismiss);
        let pop = match target {
            ContextTarget::File { path, bucket } => {
                // The checkbox's word in a menu: conflicted files resolve,
                // everything else flips its include flag.
                let out = self.model.read(cx).excluded.contains(&path);
                let (label, icon_name) = match bucket {
                    Bucket::Conflicted => ("Mark resolved", "check"),
                    _ if out => ("Include in commit", "check"),
                    _ => ("Exclude from commit", "minus"),
                };
                let (m1, m2, m3) = (m.clone(), m.clone(), m.clone());
                let (pa, pb, pc, pd, pe) = (
                    path.clone(),
                    path.clone(),
                    path.clone(),
                    path.clone(),
                    path.clone(),
                );
                let (c1, c2, c3, c4, c5) = (
                    close.clone(),
                    close.clone(),
                    close.clone(),
                    close.clone(),
                    close.clone(),
                );
                let e = cx.entity();
                let mut pop = pop.child(menu_row("ctx-include", label).icon(icon_name).on_click(
                    move |_, _, cx| {
                        m1.update(cx, |m, cx| m.toggle_include(pa.clone(), bucket, cx));
                        c1(cx);
                    },
                ));
                if bucket == Bucket::Conflicted {
                    // The wholesale outs for an unmerged path. Porcelain's
                    // XY tells which side deleted it — resolving that way
                    // is `git rm`, not a checkout.
                    let change = self
                        .model
                        .read(cx)
                        .snap
                        .bucket(Bucket::Conflicted)
                        .iter()
                        .find(|f| f.path == path)
                        .cloned();
                    let (ours, theirs) = change
                        .map(|f| (f.x != 'D', f.y != 'D'))
                        .unwrap_or((true, true));
                    let mut sides: Vec<(&'static str, &'static str, ConflictSide, bool)> =
                        Vec::new();
                    if ours {
                        sides.push(("ctx-ours", "Use our version", ConflictSide::Ours, false));
                    }
                    if theirs {
                        sides.push((
                            "ctx-theirs",
                            "Use their version",
                            ConflictSide::Theirs,
                            false,
                        ));
                    }
                    if !ours || !theirs {
                        sides.push(if theirs {
                            ("ctx-del", "Keep deleted", ConflictSide::Ours, true)
                        } else {
                            ("ctx-del", "Delete file", ConflictSide::Theirs, true)
                        });
                    }
                    for (i, (id, text, side, delete)) in sides.into_iter().enumerate() {
                        let (o, p, c) = (self.overlays.clone(), path.clone(), close.clone());
                        pop = pop.child(
                            menu_row((id, i), text)
                                .icon(if delete { "trash" } else { "branch" })
                                .on_click(move |_, _, cx| {
                                    o.update(cx, |o, cx| {
                                        o.confirm(
                                            ConfirmAction::ResolveConflict {
                                                path: p.clone(),
                                                side,
                                                delete,
                                            },
                                            None,
                                            cx,
                                        )
                                    });
                                    c(cx);
                                }),
                        );
                    }
                } else {
                    pop = pop.child(
                        menu_row("ctx-discard", "Discard changes…")
                            .icon("trash")
                            .danger()
                            .on_click(move |_, _, cx| {
                                e.update(cx, |s, cx| s.ask_discard(pb.clone(), bucket, cx));
                                c2(cx);
                            }),
                    );
                }
                if bucket == Bucket::Untracked {
                    // Ignoring only makes sense untracked: on a tracked
                    // file a gitignore line silently does nothing.
                    let (m4, m5, pf, px) = (m.clone(), m.clone(), path.clone(), path.clone());
                    let (c6, c7) = (close.clone(), close.clone());
                    pop = pop.child(menu_separator(cx)).child(
                        menu_row("ctx-ignore", "Ignore file")
                            .icon("eye-off")
                            .on_click(move |_, _, cx| {
                                m4.update(cx, |m, cx| m.ignore_path(pf.clone(), false, cx));
                                c6(cx);
                            }),
                    );
                    if let Some(pattern) = wackdepot_core::gitignore::extension_pattern(&px) {
                        let label = format!("Ignore all {} files", pattern.trim_start_matches('*'));
                        pop =
                            pop.child(menu_row("ctx-ignore-ext", label).icon("eye-off").on_click(
                                move |_, _, cx| {
                                    m5.update(cx, |m, cx| m.ignore_path(px.clone(), true, cx));
                                    c7(cx);
                                },
                            ));
                    }
                }
                pop.child(menu_separator(cx))
                    .child(menu_row("ctx-copy", "Copy path").icon("copy").on_click(
                        move |_, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(pc.clone()));
                            c3(cx);
                        },
                    ))
                    .child(
                        menu_row("ctx-reveal", "Reveal in Finder")
                            .icon("folder")
                            .on_click(move |_, _, cx| {
                                m2.update(cx, |m, cx| m.reveal(&pd, cx));
                                c4(cx);
                            }),
                    )
                    .child(
                        menu_row("ctx-editor", "Open in Editor")
                            .icon("external")
                            .on_click(move |_, _, cx| {
                                // The file itself — where conflicts are
                                // resolved — not the repo root.
                                m3.update(cx, |m, cx| m.open_file_in_editor(&pe, cx));
                                c5(cx);
                            }),
                    )
            }
            ContextTarget::Commit { sha } => self.commit_menu(pop, sha, close, cx),
        };
        Some(pop.into_any_element())
    }
}
