//! History: commits newest-first with compact graph lanes, unpushed marks,
//! and a context menu for revert, cherry-pick, branch, tag and copy.

use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window, div, prelude::*, px, uniform_list,
};

use super::graph::{gutter_width, paint_row};
use super::{ContextTarget, SelectNext, SelectPrev, SidebarView};
use crate::components::menu::{Popover, menu_row, menu_separator};
use crate::components::misc::empty_state;
use crate::model::Selection;
use crate::theme::ActiveTheme;
use crate::views::dialogs::NameKind;
use crate::views::time::short_age;

const ROW_H: f32 = 50.0;

impl SidebarView {
    pub(super) fn step_history(&mut self, delta: isize, cx: &mut Context<Self>) {
        let snap = self.model.read(cx).snap.clone();
        if snap.log.is_empty() {
            return;
        }
        let current = match &self.model.read(cx).selection {
            Some(Selection::Commit { sha, .. }) => snap.log.iter().position(|c| &c.sha == sha),
            _ => None,
        };
        let next = match current {
            Some(i) => (i as isize + delta).clamp(0, snap.log.len() as isize - 1) as usize,
            None => 0,
        };
        let sha = snap.log[next].sha.clone();
        self.model.update(cx, |m, cx| {
            m.select(Some(Selection::Commit { sha, file: None }), cx)
        });
        self.history_scroll
            .scroll_to_item(next, gpui::ScrollStrategy::Nearest);
    }

    pub(super) fn history_list(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let snap = self.model.read(cx).snap.clone();
        if self.model.read(cx).loaded && snap.log.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    empty_state("commit", "No commits yet")
                        .body("Your first commit will start the history."),
                )
                .into_any_element();
        }
        let entity = cx.entity();
        let count = snap.log.len();
        div()
            .id("history")
            .key_context("FileList")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| {
                this.step(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                this.step(1, cx);
            }))
            .flex_1()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus_list(window, cx)),
            )
            .child(
                uniform_list("history-list", count, move |range, window, cx| {
                    entity.update(cx, |this, cx| {
                        range
                            .map(|i| this.commit_row(i, window, cx))
                            .collect::<Vec<_>>()
                    })
                })
                .track_scroll(&self.history_scroll)
                .size_full(),
            )
            .into_any_element()
    }

    fn commit_row(&mut self, i: usize, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let snap = model.snap.clone();
        let Some(c) = snap.log.get(i).cloned() else {
            return div().into_any_element();
        };
        let selected =
            matches!(&model.selection, Some(Selection::Commit { sha, .. }) if *sha == c.sha);
        // With nothing pushed anywhere yet, marking every commit is noise.
        let some_pushed = snap.unpushed.len() < snap.log.len();
        let unpushed = some_pushed && snap.unpushed.contains(&c.sha);
        let lanes = snap.graph.iter().map(|g| g.width).max().unwrap_or(1);
        let graph = snap.graph.get(i).cloned();
        let sha = c.sha.clone();
        let sha2 = c.sha.clone();

        let mut row = div()
            .id(("commit", i))
            .h(px(ROW_H))
            .w_full()
            .mx(px(2.))
            .flex()
            .items_center()
            .pr(px(10.))
            .rounded(px(14.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus_list(window, cx);
                let sel = Selection::Commit {
                    sha: sha.clone(),
                    file: None,
                };
                this.model.update(cx, |m, cx| m.select(Some(sel), cx));
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.context = Some((e.position, ContextTarget::Commit { sha: sha2.clone() }));
                    cx.notify();
                }),
            );
        if let Some(g) = graph {
            let w = gutter_width(lanes);
            row = row.child(div().h_full().w(px(w)).flex_none().child(paint_row(
                g,
                lanes,
                t.sem.lanes,
                t.accent,
                t.sidebar,
                i == 0,
                c.parents.len() > 1,
            )));
        } else {
            row = row.pl(px(10.));
        }
        let meta = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(t.text_dim)
            .child(div().truncate().child(c.author_name.clone()))
            .child(div().text_color(t.text_faint).child("·"))
            .child(div().flex_none().child(short_age(&c.authored_at)))
            .child(
                div()
                    .flex_none()
                    .font_family(t.mono.clone())
                    .text_color(t.text_faint)
                    .child(c.short.clone()),
            )
            .when(unpushed, |el| {
                el.child(
                    div()
                        .flex_none()
                        .size(px(16.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(t.selected_fill())
                        .child(
                            crate::icons::icon("arrow-up")
                                .size(px(10.))
                                .text_color(t.accent),
                        ),
                )
            });
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .pl(px(4.))
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.5))
                        .font_weight(if selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if c.parents.len() > 1 {
                            t.text_soft
                        } else {
                            t.text
                        })
                        .child(c.subject.clone()),
                )
                .child(meta),
        );
        if selected {
            row = row.bg(t.selected_fill()).shadow(vec![t.rim(t.accent_line)]);
        } else {
            row = row.hover(|s| s.bg(t.hover()));
        }
        row.into_any_element()
    }

    pub(super) fn commit_menu(
        &self,
        pop: Popover,
        sha: String,
        close: impl Fn(&mut App) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Popover {
        let snap = self.model.read(cx).snap.clone();
        let short = snap
            .log
            .iter()
            .find(|c| c.sha == sha)
            .map(|c| c.short.clone())
            .unwrap_or_else(|| sha[..7.min(sha.len())].to_string());
        let (m1, m2) = (self.model.clone(), self.model.clone());
        let (o1, o2) = (self.overlays.clone(), self.overlays.clone());
        let (s1, s2, s3, s4, s5) = (
            sha.clone(),
            sha.clone(),
            sha.clone(),
            sha.clone(),
            sha.clone(),
        );
        let (sh1, sh2) = (short.clone(), short.clone());
        let (c1, c2, c3, c4, c5) = (
            close.clone(),
            close.clone(),
            close.clone(),
            close.clone(),
            close.clone(),
        );
        pop.child(
            menu_row("ctx-copy-sha", "Copy SHA")
                .icon("copy")
                .hint(short)
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(s1.clone()));
                    c1(cx);
                }),
        )
        .child(menu_separator(cx))
        .child(
            menu_row("ctx-branch", "Branch from here…")
                .icon("branch")
                .on_click(move |_, window, cx| {
                    o1.update(cx, |o, cx| {
                        o.open_name(
                            NameKind::BranchAt {
                                sha: s2.clone(),
                                short: sh1.clone(),
                            },
                            window,
                            cx,
                        )
                    });
                    c2(cx);
                }),
        )
        .child(
            menu_row("ctx-tag", "Create tag…")
                .icon("tag")
                .on_click(move |_, window, cx| {
                    o2.update(cx, |o, cx| {
                        o.open_name(
                            NameKind::Tag {
                                sha: s3.clone(),
                                short: sh2.clone(),
                            },
                            window,
                            cx,
                        )
                    });
                    c3(cx);
                }),
        )
        .child(menu_separator(cx))
        .child(
            menu_row("ctx-revert", "Revert this commit")
                .icon("undo")
                .on_click(move |_, _, cx| {
                    m1.update(cx, |m, cx| m.revert(s4.clone(), cx));
                    c4(cx);
                }),
        )
        .child(
            menu_row("ctx-pick", "Cherry-pick onto this branch")
                .icon("cherry")
                .on_click(move |_, _, cx| {
                    m2.update(cx, |m, cx| m.cherry_pick(s5.clone(), cx));
                    c5(cx);
                }),
        )
        .child(
            menu_row("ctx-reset", "Reset branch to here")
                .icon("rebase")
                .hint("soon")
                .disabled(true),
        )
    }
}
