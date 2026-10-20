//! The 38px titlebar on the glass: repo pill, branch pill, the sync
//! split-pill with live progress, and the more menu. Each pill owns its
//! frosted popover.

use gpui::{
    Anchor, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, div, prelude::*, px,
};
use wackdepot_core::stash::StashAction;

use crate::app::overlays::Overlays;
use crate::app::prefs;
use crate::components::button::{Variant, button, icon_button};
use crate::components::drag::drag_strip;
use crate::components::mark::monogram;
use crate::components::menu::{menu_heading, menu_row, menu_separator, popover};
use crate::components::misc::kbd;
use crate::components::pill_track::pill_track;
use crate::components::spinner::orbit;
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::icons::icon;
use crate::model::{Beat, ConfirmAction, NoticeKind, RepoEvent, RepoModel};
use crate::motion::{GLIDE, Motion};
use crate::theme::{ActiveTheme, metrics};
use crate::views::dialogs::NameKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlePopover {
    Repos,
    Branches,
    Sync,
    More,
}

pub struct TitlebarView {
    model: Entity<RepoModel>,
    overlays: Entity<Overlays>,
    pulls: Entity<crate::model::pulls::PullRequestModel>,
    pub open: Option<TitlePopover>,
    repo_filter: Entity<TextInput>,
    branch_filter: Entity<TextInput>,
    new_branch: Entity<TextInput>,
    /// Branches · Pull requests.
    branch_tab: usize,
    motion: Motion,
}

impl TitlebarView {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: Entity<Overlays>,
        pulls: Entity<crate::model::pulls::PullRequestModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&pulls, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&model, |this: &mut Self, _, e: &RepoEvent, cx| match e {
            RepoEvent::Beat(Beat::Pushed) => {
                // The comet: it leaves the sync pill and the ↑ count rolls to 0.
                this.motion
                    .restart(("comet", 0), 0.0, 1.0, 900.0, 0.0, GLIDE);
                cx.notify();
            }
            RepoEvent::Snapshot | RepoEvent::Busy | RepoEvent::Opened => cx.notify(),
            _ => {}
        })
        .detach();
        let repo_filter = cx.new(|cx| TextInput::new("Filter repositories", cx));
        let branch_filter = cx.new(|cx| TextInput::new("Filter branches", cx));
        let new_branch = cx.new(|cx| TextInput::new("New branch name", cx));
        for input in [&repo_filter, &branch_filter] {
            cx.subscribe(input, |_, _, _: &TextInputEvent, cx| cx.notify())
                .detach();
        }
        cx.subscribe(&new_branch, |this: &mut Self, _, e: &TextInputEvent, cx| {
            if *e == TextInputEvent::Submit {
                this.create_branch(cx);
            }
        })
        .detach();
        Self {
            model,
            overlays,
            pulls,
            open: None,
            repo_filter,
            branch_filter,
            new_branch,
            branch_tab: 0,
            motion: Motion::default(),
        }
    }

    pub fn toggle(&mut self, which: TitlePopover, window: &mut Window, cx: &mut Context<Self>) {
        self.open = if self.open == Some(which) {
            None
        } else {
            Some(which)
        };
        match self.open {
            Some(TitlePopover::Repos) => {
                self.repo_filter.update(cx, |i, cx| {
                    i.clear(cx);
                    i.focus(window, cx);
                });
            }
            Some(TitlePopover::Branches) => {
                self.branch_filter.update(cx, |i, cx| {
                    i.clear(cx);
                    i.focus(window, cx);
                });
            }
            _ => {}
        }
        cx.notify();
    }

    /// Open the branch picker with the new-branch field focused.
    pub fn new_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = Some(TitlePopover::Branches);
        self.branch_tab = 0;
        self.new_branch.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.open.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    fn create_branch(&mut self, cx: &mut Context<Self>) {
        let name = self.new_branch.read(cx).content().trim().to_string();
        if name.is_empty() {
            return;
        }
        self.model
            .update(cx, |m, cx| m.create_branch(name, String::new(), cx));
        self.new_branch.update(cx, |i, cx| i.clear(cx));
        self.open = None;
        cx.notify();
    }

    // ── Pills ──────────────────────────────────────────────────────────

    fn pill(id: &'static str, open: bool, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let t = cx.theme().clone();
        div()
            .id(id)
            .relative()
            .flex()
            .items_center()
            .gap(px(7.))
            .h(px(26.))
            .px(px(9.))
            .rounded_full()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.text)
            .bg(if open { t.pill_hover() } else { t.pill() })
            .shadow(vec![t.rim(t.pill())])
            .cursor_pointer()
            .occlude()
            .hover(move |s| s.bg(t.pill_hover()))
    }

    fn repo_pill(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let name = model.name().unwrap_or_else(|| "No repository".into());
        let has_repo = model.repo.is_some();
        let open = self.open == Some(TitlePopover::Repos);
        let mut pill = Self::pill("repo-pill", open, cx)
            .child(monogram(name.clone()).size(18.).current(has_repo))
            .child(div().max_w(px(180.)).truncate().child(name))
            .child(icon("chevron-down").size(px(12.)).text_color(t.text_dim))
            .on_click(
                cx.listener(|this, _, window, cx| this.toggle(TitlePopover::Repos, window, cx)),
            );
        if open {
            pill = pill.child(self.repo_picker(window, cx));
        }
        pill
    }

    fn branch_pill(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let label = model.branch_label();
        let (ahead, behind) = model
            .snap
            .status
            .as_ref()
            .map(|s| (s.ahead, s.behind))
            .unwrap_or((0, 0));
        let _ = model;
        let open = self.open == Some(TitlePopover::Branches);
        let mut pill = Self::pill("branch-pill", open, cx)
            .child(icon("branch").size(px(14.)).text_color(t.text_dim))
            .child(
                div()
                    .max_w(px(220.))
                    .truncate()
                    .font_family(t.mono.clone())
                    .text_size(px(12.5))
                    .child(label),
            );
        if ahead > 0 {
            pill = pill.child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.accent_text)
                    .child(format!("↑{ahead}")),
            );
        }
        if behind > 0 {
            pill = pill.child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_soft)
                    .child(format!("↓{behind}")),
            );
        }
        pill = pill
            .child(icon("chevron-down").size(px(12.)).text_color(t.text_dim))
            .on_click(
                cx.listener(|this, _, window, cx| this.toggle(TitlePopover::Branches, window, cx)),
            );
        if open {
            pill = pill.child(self.branch_picker(window, cx));
        }
        pill
    }

    /// The sync split-pill: the primary action (by state) with live
    /// progress, and a chevron into the sync menu.
    fn sync_pill(
        &self,
        window: &mut Window,
        comet: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let status = model.snap.status.as_ref();
        let (ahead, behind) = status.map(|s| (s.ahead, s.behind)).unwrap_or((0, 0));
        let unpublished = status.is_some_and(|s| s.upstream.is_none() && s.branch.is_some());
        let busy = model.busy;
        let progress = model.progress.clone();

        let (glyph, label, lit): (&'static str, SharedString, bool) = match busy {
            Some(op) if op.is_sync() => {
                let pct = progress
                    .as_ref()
                    .and_then(|p| p.percent)
                    .map(|p| format!(" {p}%"))
                    .unwrap_or_default();
                ("fetch", format!("{}{pct}", op.label()).into(), true)
            }
            None if comet < 1.0 => ("check", "Pushed".into(), true),
            _ if unpublished => ("cloud", "Publish branch".into(), true),
            _ if behind > 0 => ("pull", format!("Pull {behind}").into(), true),
            _ if ahead > 0 => ("push", format!("Push {ahead}").into(), true),
            _ => ("fetch", "Fetch".into(), false),
        };
        let syncing = busy.is_some_and(|op| op.is_sync());
        let (bg, fg, rim) = if lit {
            (t.selected_fill(), t.accent_text, t.accent_line)
        } else {
            (t.pill(), t.text_soft, t.pill())
        };
        let open = self.open == Some(TitlePopover::Sync);
        let mut main = div()
            .id("sync-main")
            .flex()
            .items_center()
            .gap(px(7.))
            .h_full()
            .pl(px(11.))
            .pr(px(9.))
            .text_color(fg)
            .cursor_pointer()
            .hover(|s| s.text_color(t.text))
            .on_click(cx.listener(|this, _, _, cx| {
                this.model.update(cx, |m, cx| m.sync(cx));
            }));
        main = if syncing {
            main.child(orbit("sync-orbit").size(14.))
        } else {
            main.child(icon(glyph).size(px(15.)).text_color(fg))
        };
        main = main.child(div().whitespace_nowrap().child(label));
        let chevron = div()
            .id("sync-chevron")
            .flex()
            .items_center()
            .h_full()
            .pl(px(6.))
            .pr(px(8.))
            .border_l_1()
            .border_color(rim)
            .cursor_pointer()
            .text_color(fg)
            .child(icon("chevron-down").size(px(12.)))
            .on_click(
                cx.listener(|this, _, window, cx| this.toggle(TitlePopover::Sync, window, cx)),
            );
        let mut pill = div()
            .id("sync-pill")
            .relative()
            .flex()
            .items_center()
            .h(px(26.))
            .rounded_full()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .bg(bg)
            .shadow(vec![t.rim(rim)])
            .occlude()
            .child(main)
            .child(chevron);
        if comet < 1.0 {
            // A lead bead and its fading trail arc up out of the pill.
            let fade = if comet < 0.7 {
                1.0
            } else {
                (1.0 - (comet - 0.7) / 0.3).max(0.0)
            };
            for k in 0..5 {
                let p = (comet - k as f32 * 0.05).max(0.0);
                let size = 6.0 - k as f32;
                pill = pill.child(
                    div()
                        .absolute()
                        .left(px(14.0 + p * 70.0 - size / 2.0))
                        .top(px(13.0 - p * 34.0 - size / 2.0))
                        .size(px(size))
                        .rounded_full()
                        .bg(t.accent.opacity(fade * (1.0 - k as f32 * 0.18))),
                );
            }
        }
        if open {
            pill = pill.child(self.sync_menu(window, cx));
        }
        pill
    }

    fn more_button(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open == Some(TitlePopover::More);
        let mut wrap = div().relative().child(
            icon_button("more-btn", "more")
                .variant(if open {
                    Variant::Secondary
                } else {
                    Variant::Ghost
                })
                .on_click(
                    cx.listener(|this, _, window, cx| this.toggle(TitlePopover::More, window, cx)),
                ),
        );
        if open {
            wrap = wrap.child(self.more_menu(window, cx));
        }
        wrap
    }

    // ── Popovers ───────────────────────────────────────────────────────

    fn dismiss_handler(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut gpui::App) + 'static {
        let this = cx.entity().downgrade();
        move |_, cx| {
            if let Some(this) = this.upgrade() {
                this.update(cx, |t, cx| {
                    t.open = None;
                    cx.notify();
                });
            }
        }
    }

    fn filter_row(input: &Entity<TextInput>, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(34.))
            .px(px(10.))
            .mb(px(4.))
            .rounded(px(10.))
            .bg(t.recessed())
            .text_size(px(13.))
            .child(icon("search").size(px(14.)).text_color(t.text_dim))
            .child(div().flex_1().child(input.clone()))
    }

    fn repo_picker(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let filter = self.repo_filter.read(cx).content().to_lowercase();
        let current = self.model.read(cx).repo.clone();
        let repos = prefs::get(cx).repos.clone();
        let model = self.model.clone();
        let overlays = self.overlays.clone();
        let this = cx.entity().downgrade();
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for (i, path) in repos.iter().enumerate() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !filter.is_empty() && !name.to_lowercase().contains(&filter) {
                continue;
            }
            let is_current = current.as_ref() == Some(path);
            let (m1, m2) = (model.clone(), model.clone());
            let (p1, p2) = (path.clone(), path.clone());
            let close = this.clone();
            let forget = icon_button(("forget", i), "close")
                .small()
                .on_click(move |_, _, cx| {
                    m2.update(cx, |m, cx| m.forget(&p2, cx));
                });
            rows.push(
                menu_row(("repo", i), name.clone())
                    .lead(monogram(name).size(20.).current(is_current))
                    .selected(is_current)
                    .trailing(
                        div()
                            .opacity(0.)
                            .group_hover("repo-row", |s| s.opacity(1.))
                            .child(forget),
                    )
                    .on_click(move |_, _, cx| {
                        m1.update(cx, |m, cx| m.open(p1.clone(), cx));
                        if let Some(t) = close.upgrade() {
                            t.update(cx, |t, cx| {
                                t.open = None;
                                cx.notify();
                            });
                        }
                    })
                    .into_any_element(),
            );
        }
        let add = {
            let (m, c) = (model.clone(), this.clone());
            menu_row("repo-add", "Add existing repository…")
                .icon("folder-plus")
                .trailing(kbd("⌘O", cx))
                .on_click(move |_, _, cx| {
                    m.update(cx, |m, cx| m.add_existing(cx));
                    if let Some(t) = c.upgrade() {
                        t.update(cx, |t, cx| t.close(cx));
                    }
                })
        };
        let clone = {
            let (o, c) = (overlays.clone(), this.clone());
            menu_row("repo-clone", "Clone repository…")
                .icon("cloud")
                .trailing(kbd("⇧⌘O", cx))
                .on_click(move |_, window, cx| {
                    o.update(cx, |o, cx| o.open_clone(window, cx));
                    if let Some(t) = c.upgrade() {
                        t.update(cx, |t, cx| t.close(cx));
                    }
                })
        };
        let create = {
            let (o, c) = (overlays.clone(), this.clone());
            menu_row("repo-new", "Create new repository…")
                .icon("plus")
                .trailing(kbd("⌘N", cx))
                .on_click(move |_, window, cx| {
                    o.update(cx, |o, cx| o.open_new_repo(window, cx));
                    if let Some(t) = c.upgrade() {
                        t.update(cx, |t, cx| t.close(cx));
                    }
                })
        };
        let empty = rows.is_empty();
        popover("repo-popover")
            .width(320.)
            .on_dismiss(self.dismiss_handler(cx))
            .child(Self::filter_row(&self.repo_filter, cx))
            .child(menu_heading("Repositories", cx))
            .child(
                div()
                    .id("repo-list")
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows.into_iter().map(|r| div().group("repo-row").child(r)))
                    .when(empty, |el| {
                        el.child(
                            div()
                                .px(px(9.))
                                .py(px(8.))
                                .text_size(px(12.5))
                                .text_color(t.text_dim)
                                .child("No repositories match."),
                        )
                    }),
            )
            .child(menu_separator(cx))
            .child(add)
            .child(clone)
            .child(create)
    }

    fn branch_picker(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.clone();
        let overlays = self.overlays.clone();
        let this = cx.entity().downgrade();
        let tab = self.branch_tab;
        let tabs = {
            let this = this.clone();
            pill_track("branch-tabs")
                .equal()
                .item("Branches")
                .item("Pull requests")
                .selected(tab)
                .on_select(move |i, _, cx| {
                    if let Some(t) = this.upgrade() {
                        t.update(cx, |t, cx| {
                            t.branch_tab = i;
                            cx.notify();
                        });
                    }
                })
        };
        let mut pop = popover("branch-popover")
            .width(360.)
            .on_dismiss(self.dismiss_handler(cx))
            .child(div().mb(px(6.)).child(tabs));
        if tab == 1 {
            let state = self.pulls.read(cx);
            let loading = state.loading;
            let connected = state.session.read(cx).user.is_some();
            let rows = state
                .list
                .iter()
                .take(8)
                .enumerate()
                .map(|(i, pull)| {
                    let number = pull.number;
                    let pulls = self.pulls.clone();
                    let titlebar = cx.entity();
                    menu_row(("quick-pr", i), format!("#{number} {}", pull.title))
                        .icon("pull-request")
                        .on_click(move |_, _, cx| {
                            pulls.update(cx, |m, cx| m.select(number, cx));
                            titlebar.update(cx, |t, cx| {
                                t.close(cx);
                            });
                        })
                })
                .collect::<Vec<_>>();
            let pulls = self.pulls.clone();
            let git = self.model.clone();
            let titlebar = cx.entity();
            return pop
                .child(
                    div()
                        .id("quick-pr-list")
                        .max_h(px(340.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .children(rows)
                        .when(state.list.is_empty(), |el| {
                            el.child(
                                div()
                                    .p(px(14.))
                                    .text_size(px(12.))
                                    .text_color(t.text_dim)
                                    .child(if loading {
                                        "Loading pull requests…"
                                    } else if connected {
                                        "No pull requests in the current view."
                                    } else {
                                        "Connect GitHub to browse pull requests."
                                    }),
                            )
                        }),
                )
                .child(menu_separator(cx))
                .child(
                    menu_row("all-pull-requests", "Open Pull Requests")
                        .icon("pull-request")
                        .trailing(kbd("⌘3", cx))
                        .on_click(move |_, _, cx| {
                            git.update(cx, |m, cx| m.set_tab(crate::model::Tab::PullRequests, cx));
                            pulls.update(cx, |m, cx| m.refresh(cx));
                            titlebar.update(cx, |t, cx| {
                                t.close(cx);
                            });
                        }),
                );
        }
        let filter = self.branch_filter.read(cx).content().to_lowercase();
        let branches = self.model.read(cx).snap.branches.clone();
        // Merging needs a quiet lane: no op in flight and commits to land on.
        // A plain fn so every on_click closure can copy it freely.
        fn merge_guard(
            preselect: Option<String>,
            window: &mut Window,
            cx: &mut gpui::App,
            model: &Entity<RepoModel>,
            overlays: &Entity<Overlays>,
        ) {
            if let Some(reason) = model.read(cx).merge_blocker() {
                model.update(cx, |m, cx| m.notice(reason, NoticeKind::Error, cx));
            } else {
                overlays.update(cx, |o, cx| o.open_merge(preselect, window, cx));
            }
        }
        let mut rows = Vec::new();
        for (i, b) in branches.iter().enumerate() {
            if !filter.is_empty() && !b.name.to_lowercase().contains(&filter) {
                continue;
            }
            let name = b.name.clone();
            let mut row = menu_row(("branch", i), b.name.clone())
                .mono()
                .icon(if b.is_current { "check" } else { "branch" })
                .selected(b.is_current);
            if b.is_current {
                row = row.hint("current");
            } else {
                let (m, n, c) = (model.clone(), name.clone(), this.clone());
                row = row.on_click(move |_, _, cx| {
                    m.update(cx, |m, cx| m.checkout(n.clone(), cx));
                    if let Some(t) = c.upgrade() {
                        t.update(cx, |t, cx| t.close(cx));
                    }
                });
            }
            let rename = {
                let (o, n, c) = (overlays.clone(), name.clone(), this.clone());
                icon_button(("rename", i), "pencil")
                    .small()
                    .on_click(move |_, window, cx| {
                        o.update(cx, |o, cx| {
                            o.open_name(NameKind::RenameBranch { old: n.clone() }, window, cx)
                        });
                        if let Some(t) = c.upgrade() {
                            t.update(cx, |t, cx| t.close(cx));
                        }
                    })
            };
            let mut actions = div()
                .flex()
                .gap(px(2.))
                .opacity(0.)
                .group_hover("branch-row", |s| s.opacity(1.))
                .child(rename);
            if !b.is_current {
                let (o, m, n, c) = (overlays.clone(), model.clone(), name.clone(), this.clone());
                actions = actions.child(icon_button(("merge", i), "merge").small().on_click(
                    move |_, window, cx| {
                        merge_guard(Some(n.clone()), window, cx, &m, &o);
                        if let Some(t) = c.upgrade() {
                            t.update(cx, |t, cx| t.close(cx));
                        }
                    },
                ));
                let (o, n, c) = (overlays.clone(), name.clone(), this.clone());
                actions = actions.child(icon_button(("delete", i), "trash").small().on_click(
                    move |_, _, cx| {
                        o.update(cx, |o, cx| {
                            o.confirm(ConfirmAction::DeleteBranch { name: n.clone() }, None, cx)
                        });
                        if let Some(t) = c.upgrade() {
                            t.update(cx, |t, cx| t.close(cx));
                        }
                    },
                ));
            }
            rows.push(
                div()
                    .group("branch-row")
                    .child(row.trailing(actions))
                    .into_any_element(),
            );
        }
        let create = {
            let this = this.clone();
            button("branch-create", "Create")
                .small()
                .primary()
                .on_click(move |_, _, cx| {
                    if let Some(t) = this.upgrade() {
                        t.update(cx, |t, cx| t.create_branch(cx));
                    }
                })
        };
        let empty = rows.is_empty();
        pop = pop
            .child(Self::filter_row(&self.branch_filter, cx))
            .child(menu_heading("Local branches", cx))
            .child(
                div()
                    .id("branch-list")
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows)
                    .when(empty, |el| {
                        el.child(
                            div()
                                .px(px(9.))
                                .py(px(8.))
                                .text_size(px(12.5))
                                .text_color(t.text_dim)
                                .child("No branches match."),
                        )
                    }),
            )
            .child(menu_separator(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(4.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(32.))
                            .px(px(10.))
                            .rounded_full()
                            .bg(t.input)
                            .border_1()
                            .border_color(t.border)
                            .text_size(px(12.5))
                            .child(icon("plus").size(px(13.)).text_color(t.text_dim))
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(t.mono.clone())
                                    .child(self.new_branch.clone()),
                            ),
                    )
                    .child(create),
            )
            .child(menu_separator(cx))
            .child({
                let (o, m, c) = (overlays.clone(), model.clone(), this.clone());
                let label = format!("Merge into {}…", self.model.read(cx).branch_label());
                menu_row("branch-merge", label)
                    .icon("merge")
                    .on_click(move |_, window, cx| {
                        merge_guard(None, window, cx, &m, &o);
                        if let Some(t) = c.upgrade() {
                            t.update(cx, |t, cx| t.close(cx));
                        }
                    })
            })
            .child(
                div()
                    .px(px(10.))
                    .pb(px(2.))
                    .text_size(px(11.))
                    .text_color(t.text_faint)
                    .child(format!(
                        "Branches from {}.",
                        self.model.read(cx).branch_label()
                    )),
            );
        pop
    }

    fn sync_menu(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.clone();
        let overlays = self.overlays.clone();
        let this = cx.entity().downgrade();
        let snap = self.model.read(cx).snap.clone();
        let item = |id: &'static str,
                    label: &'static str,
                    glyph: &'static str,
                    keys: &'static str,
                    f: fn(&mut RepoModel, &mut Context<RepoModel>),
                    cx: &mut Context<Self>| {
            let (m, c) = (model.clone(), this.clone());
            menu_row(id, label)
                .icon(glyph)
                .trailing(kbd(keys, cx))
                .on_click(move |_, _, cx| {
                    m.update(cx, f);
                    if let Some(t) = c.upgrade() {
                        t.update(cx, |t, cx| t.close(cx));
                    }
                })
        };
        let mut pop = popover("sync-popover")
            .width(320.)
            .corner(Anchor::TopRight)
            .on_dismiss(self.dismiss_handler(cx))
            .child(item(
                "sync-fetch",
                "Fetch",
                "fetch",
                "⇧⌘T",
                RepoModel::fetch,
                cx,
            ))
            .child(item(
                "sync-pull",
                "Pull",
                "pull",
                "⇧⌘P",
                RepoModel::pull,
                cx,
            ))
            .child(item("sync-push", "Push", "push", "⌘P", RepoModel::push, cx))
            .child(menu_separator(cx))
            .child(item(
                "sync-stash",
                "Stash all changes",
                "stash",
                "⌥⌘S",
                RepoModel::stash_all,
                cx,
            ));
        if !snap.stashes.is_empty() {
            pop = pop.child(menu_heading(
                format!("Stashes · {}", snap.stashes.len()),
                cx,
            ));
            for (i, s) in snap.stashes.iter().enumerate().take(8) {
                let label = s
                    .message
                    .trim_start_matches("WIP on ")
                    .trim_start_matches("On ")
                    .to_string();
                let (m1, m2) = (model.clone(), model.clone());
                let (sha1, sha2, sha3) = (s.sha.clone(), s.sha.clone(), s.sha.clone());
                let o = overlays.clone();
                let lbl = label.clone();
                let actions =
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            button(("stash-pop", i), "Pop")
                                .small()
                                .on_click(move |_, _, cx| {
                                    m1.update(cx, |m, cx| {
                                        m.stash_action(sha1.clone(), StashAction::Pop, cx)
                                    })
                                }),
                        )
                        .child(button(("stash-apply", i), "Apply").small().on_click(
                            move |_, _, cx| {
                                m2.update(cx, |m, cx| {
                                    m.stash_action(sha2.clone(), StashAction::Apply, cx)
                                })
                            },
                        ))
                        .child(icon_button(("stash-drop", i), "trash").small().on_click(
                            move |_, _, cx| {
                                o.update(cx, |o, cx| {
                                    o.confirm(
                                        ConfirmAction::DropStash {
                                            sha: sha3.clone(),
                                            label: lbl.clone(),
                                        },
                                        None,
                                        cx,
                                    )
                                });
                            },
                        ));
                pop = pop.child(
                    menu_row(("stash", i), label)
                        .icon("stash")
                        .trailing(actions),
                );
            }
        }
        pop.child(menu_separator(cx))
            .child(item(
                "open-editor",
                "Open in Editor",
                "external",
                "⇧⌘A",
                RepoModel::open_in_editor,
                cx,
            ))
            .child(item(
                "open-terminal",
                "Open in Terminal",
                "terminal",
                "⌃`",
                RepoModel::open_in_terminal,
                cx,
            ))
            .child(item(
                "open-finder",
                "Show in Finder",
                "folder",
                "⇧⌘F",
                RepoModel::open_in_finder,
                cx,
            ))
    }

    fn more_menu(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.clone();
        let overlays = self.overlays.clone();
        let this = cx.entity().downgrade();
        let snap = self.model.read(cx).snap.clone();
        let repo = self.model.read(cx).repo.clone();
        let close = move |c: &gpui::WeakEntity<Self>, cx: &mut gpui::App| {
            if let Some(t) = c.upgrade() {
                t.update(cx, |t, cx| t.close(cx));
            }
        };
        let mut pop = popover("more-popover")
            .width(280.)
            .corner(Anchor::TopRight)
            .on_dismiss(self.dismiss_handler(cx));
        {
            let c = this.clone();
            pop = pop.child(
                menu_row("more-settings", "Settings…")
                    .icon("settings")
                    .trailing(kbd("⌘,", cx))
                    .on_click(move |_, window, cx| {
                        window.dispatch_action(Box::new(crate::app::actions::OpenSettings), cx);
                        close(&c, cx);
                    }),
            );
        }
        {
            let (m, c) = (model.clone(), this.clone());
            pop = pop.child(
                menu_row("more-refresh", "Refresh")
                    .icon("refresh")
                    .trailing(kbd("⌘R", cx))
                    .on_click(move |_, _, cx| {
                        m.update(cx, |m, cx| m.request_refresh(cx));
                        close(&c, cx);
                    }),
            );
        }
        if repo.is_some() {
            pop = pop.child(menu_separator(cx));
            if snap.tip_unpushed {
                let (o, c) = (overlays.clone(), this.clone());
                pop = pop.child(
                    menu_row("more-undo", "Undo last commit")
                        .icon("undo")
                        .on_click(move |_, _, cx| {
                            o.update(cx, |o, cx| o.confirm(ConfirmAction::UndoCommit, None, cx));
                            close(&c, cx);
                        }),
                );
            }
            let has_changes = !snap.bucket(crate::model::Bucket::Unstaged).is_empty()
                || !snap.bucket(crate::model::Bucket::Untracked).is_empty();
            let (o, c) = (overlays.clone(), this.clone());
            pop = pop.child(
                menu_row("more-discard", "Discard all changes…")
                    .icon("trash")
                    .danger()
                    .disabled(!has_changes)
                    .on_click(move |_, _, cx| {
                        o.update(cx, |o, cx| o.confirm(ConfirmAction::DiscardAll, None, cx));
                        close(&c, cx);
                    }),
            );
            if let Some(path) = repo {
                let (m, c) = (model.clone(), this.clone());
                pop = pop.child(
                    menu_row("more-forget", "Remove from list")
                        .icon("close")
                        .on_click(move |_, _, cx| {
                            m.update(cx, |m, cx| m.forget(&path, cx));
                            close(&c, cx);
                        }),
                );
            }
        }
        pop
    }
}

impl Render for TitlebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.motion.begin(cx);
        let comet = self.motion.value(("comet", 0), 1.0);
        self.motion.end(window);
        let has_repo = self.model.read(cx).repo.is_some();
        let busy_other = self.model.read(cx).busy.filter(|b| !b.is_sync());
        let t = cx.theme().clone();
        let mut left = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(metrics::TRAFFIC_INSET))
            .child(self.repo_pill(window, cx));
        if has_repo {
            left = left.child(self.branch_pill(window, cx));
            if let Some(pull) = self.pulls.read(cx).current.clone() {
                let number = pull.number;
                let state = self.pulls.read(cx).current_checks;
                let model = self.pulls.clone();
                left = left.child(
                    button(
                        "current-pr",
                        format!(
                            "{} #{number}",
                            match state {
                                wackdepot_github::CheckState::Passing => "✓",
                                wackdepot_github::CheckState::Failing => "×",
                                wackdepot_github::CheckState::Pending => "◌",
                                wackdepot_github::CheckState::None => "PR",
                            }
                        ),
                    )
                    .small()
                    .on_click(move |_, _, cx| model.update(cx, |m, cx| m.select(number, cx))),
                );
            }
        }
        if let Some(op) = busy_other {
            left = left.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .px(px(6.))
                    .text_size(px(12.))
                    .text_color(t.text_dim)
                    .child(orbit("busy-orbit").size(13.))
                    .child(format!("{}…", op.label())),
            );
        }
        let mut right = div().flex().items_center().gap(px(6.)).pr(px(10.));
        if has_repo {
            right = right.child(self.sync_pill(window, comet, cx));
        }
        right = right.child(self.more_button(window, cx));
        drag_strip(
            "titlebar",
            // 3px top padding centres the pills on the traffic lights.
            div()
                .flex()
                .items_center()
                .justify_between()
                .pt(px(3.))
                .child(left)
                .child(right),
        )
    }
}
