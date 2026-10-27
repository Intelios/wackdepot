//! The left column on the glass: the Changes·History track, the banner for
//! a half-finished merge/rebase/pick, the list, and (on Changes) the
//! commit composer.

mod changes;
pub mod composer;
mod graph;
mod history;

use std::collections::{HashMap, HashSet};

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, IntoElement, ParentElement, Pixels, Point,
    Render, ScrollHandle, Styled, UniformListScrollHandle, Window, actions, div, px,
};
use wackdepot_core::status::Operation;

use crate::app::overlays::Overlays;
use crate::components::button::button;
use crate::components::misc::{Tone, banner};
use crate::components::pill_track::pill_track;
use crate::model::{Bucket, ConfirmAction, RepoEvent, RepoModel, Tab};
use crate::motion::Motion;
use crate::theme::ActiveTheme;

use composer::Composer;

actions!(
    sidebar,
    [
        SelectPrev,
        SelectNext,
        ToggleInclude,
        DiscardSelected,
        SelectAll
    ]
);

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("up", SelectPrev, Some("FileList")),
        gpui::KeyBinding::new("down", SelectNext, Some("FileList")),
        // The shell binds them too, so arrows move the list before it has
        // been clicked. Focused fields and the commit file strip bind their
        // own arrows at a deeper context and win.
        gpui::KeyBinding::new("up", SelectPrev, Some("Workspace")),
        gpui::KeyBinding::new("down", SelectNext, Some("Workspace")),
        gpui::KeyBinding::new("space", ToggleInclude, Some("FileList")),
        gpui::KeyBinding::new("cmd-backspace", DiscardSelected, Some("FileList")),
        gpui::KeyBinding::new("cmd-a", SelectAll, Some("FileList")),
    ]);
}

/// Something right-clicked in the column.
#[derive(Debug, Clone)]
enum ContextTarget {
    File { path: String, bucket: Bucket },
    Commit { sha: String },
}

/// Identity of a Changes row across refreshes (what FLIP matches on).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RowKey {
    Group(Bucket),
    File(String, Bucket),
}

pub struct SidebarView {
    model: Entity<RepoModel>,
    overlays: Entity<Overlays>,
    pub composer: Entity<Composer>,
    pull_list: Entity<crate::views::pulls::PullList>,
    focus: FocusHandle,
    motion: Motion,
    scroll: ScrollHandle,
    history_scroll: UniformListScrollHandle,
    collapsed: HashSet<Bucket>,
    /// Where each row sat last layout — the "first" of FLIP.
    prev_y: HashMap<RowKey, f32>,
    /// The row the cursor is over — drives the mounted hover pill (a
    /// `group_hover` reveal can't work: an occluding child un-hovers the
    /// group, and an invisible pill would still eat clicks).
    hot: Option<RowKey>,
    /// The first layout for this repo has happened (later rows animate in).
    seen: bool,
    context: Option<(Point<Pixels>, ContextTarget)>,
}

impl SidebarView {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: Entity<Overlays>,
        pulls: Entity<crate::model::pulls::PullRequestModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| Composer::new(model.clone(), cx));
        let pull_list = cx.new(|cx| crate::views::pulls::PullList::new(pulls.clone(), cx));
        cx.subscribe(&model, |this: &mut Self, _, e: &RepoEvent, cx| match e {
            RepoEvent::Opened => {
                this.prev_y.clear();
                this.seen = false;
                this.context = None;
                this.hot = None;
                cx.notify();
            }
            RepoEvent::Snapshot | RepoEvent::Selection | RepoEvent::Busy => cx.notify(),
            _ => {}
        })
        .detach();
        Self {
            model,
            overlays,
            composer,
            pull_list,
            focus: cx.focus_handle(),
            motion: Motion::default(),
            scroll: ScrollHandle::new(),
            history_scroll: UniformListScrollHandle::new(),
            collapsed: HashSet::new(),
            prev_y: HashMap::new(),
            hot: None,
            seen: false,
            context: None,
        }
    }

    pub fn focus_list(&self, window: &mut Window, cx: &mut gpui::App) {
        self.focus.focus(window, cx);
    }

    /// Arrow keys: move the selected row of whichever list is showing.
    /// Returns whether the key belonged to a list — the workspace then parks
    /// focus here so Space and ⌘⌫ work straight after.
    pub fn step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // A popover over the column, or a dialog on top of everything, owns
        // the keyboard; stepping the list behind it would be invisible.
        if self.context.is_some() || self.overlays.read(cx).is_open() {
            return false;
        }
        match self.model.read(cx).tab {
            Tab::Changes => self.step_file(delta, cx),
            Tab::History => self.step_history(delta, cx),
            Tab::PullRequests => self
                .pull_list
                .update(cx, |list, cx| list.step(delta, window, cx)),
        }
        true
    }

    /// The merge / rebase / cherry-pick / revert in progress, if any.
    fn operation_banner(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let snap = self.model.read(cx).snap.clone();
        let op = snap.status.as_ref()?.operation?;
        let conflicts = snap.bucket(Bucket::Conflicted).len();
        let what = match op {
            Operation::Merge => "merge",
            Operation::Rebase => "rebase",
            Operation::CherryPick => "cherry-pick",
            Operation::Revert => "revert",
            Operation::Squash => "squash",
        };
        let title = match op {
            Operation::Merge => "Merging",
            Operation::Rebase => "Rebasing",
            Operation::CherryPick => "Cherry-picking",
            Operation::Revert => "Reverting",
            Operation::Squash => "Squashing",
        };
        let body = if conflicts > 0 {
            format!(
                "Resolve {conflicts} conflict{}, mark {} resolved, then continue.",
                if conflicts == 1 { "" } else { "s" },
                if conflicts == 1 { "it" } else { "them" }
            )
        } else if op == Operation::Squash {
            "Changes are staged — continue to commit them.".to_string()
        } else {
            "Everything's resolved — continue when you're ready.".to_string()
        };
        let (m, o) = (self.model.clone(), self.overlays.clone());
        let mut b = banner(Tone::Warning, title).body(body);
        if conflicts > 0 {
            let o2 = o.clone();
            b = b.action(
                button("op-resolve", "Resolve…")
                    .small()
                    .on_click(move |_, _, cx| o2.update(cx, |o, cx| o.open_conflicts(cx))),
            );
        }
        b = b
            .action(
                button("op-continue", "Continue")
                    .small()
                    .lit(conflicts == 0)
                    .disabled(conflicts > 0)
                    .on_click(move |_, _, cx| m.update(cx, |m, cx| m.continue_operation(op, cx))),
            )
            .action(
                button("op-abort", "Abort")
                    .small()
                    .danger()
                    .on_click(move |_, _, cx| {
                        o.update(cx, |o, cx| {
                            o.confirm(ConfirmAction::AbortOperation { what }, None, cx)
                        });
                    }),
            );
        Some(b)
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.motion.begin(cx);
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let tab = model.tab;
        let changed = model.snap.changed_count();
        let m = self.model.clone();
        let tabs = pill_track("tabs")
            .equal()
            .height(30.)
            .item_count("Changes", changed)
            .item("History")
            .item("Pull Requests")
            .selected(tab.index())
            .on_select(move |i, _, cx| {
                let tab = match i {
                    0 => Tab::Changes,
                    1 => Tab::History,
                    _ => Tab::PullRequests,
                };
                m.update(cx, |m, cx| m.set_tab(tab, cx));
            });
        let list = match tab {
            Tab::Changes => self.changes_list(window, cx).into_any_element(),
            Tab::History => self.history_list(window, cx).into_any_element(),
            Tab::PullRequests => self.pull_list.clone().into_any_element(),
        };
        let banner = self.operation_banner(cx);
        let mut col = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(8.))
            .child(tabs)
            .children(banner)
            .child(list);
        if tab == Tab::Changes {
            col = col.child(self.composer.clone());
        }
        if let Some(menu) = self.context_menu(cx) {
            col = col.child(menu);
        }
        self.motion.end(window);
        let _ = t;
        col
    }
}
