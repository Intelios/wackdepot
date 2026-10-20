//! The rebuilt WackDepot shell: window, backdrop, routing, and the regions
//! composed over the glass — titlebar, left column, content card, overlays.

pub mod actions;
pub mod backdrop;
pub mod overlays;
pub mod prefs;
pub mod window;

use gpui::{
    App, AppContext as _, Context, Entity, ExternalPaths, FocusHandle, FontWeight, IntoElement,
    ParentElement, Render, Styled, Window, div, prelude::*, px,
};

use crate::components::text_input;
use crate::icons;
use crate::model::{RepoEvent, RepoModel, Tab};
use crate::motion::{EASE, Motion, POP};
use crate::theme::{self, ActiveTheme, metrics};
use crate::views::diff::{self, DiffCard};
use crate::views::settings::{SettingsEvent, SettingsNav, SettingsPage};
use crate::views::sidebar::{self, SelectNext, SelectPrev, SidebarView};
use crate::views::titlebar::{TitlePopover, TitlebarView};
use crate::views::welcome::Welcome;

use actions::*;
use backdrop::Backdrop;
use overlays::Overlays;

/// Open the main window and run the rebuilt app.
pub fn run() {
    gpui_platform::application()
        .with_assets(icons::IconAssets)
        .run(|cx: &mut App| {
            prefs::init(cx);
            crate::github::init(cx);
            let input = prefs::theme_input(prefs::get(cx));
            theme::init(cx, &input);
            actions::register(cx);
            text_input::bind_keys(cx);
            sidebar::bind_keys(cx);
            diff::bind_keys(cx);
            backdrop::sync_reduce_motion(cx);
            // The syntax set takes a moment to load; do it off the main thread.
            std::thread::spawn(wackdepot_syntax::warm);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            cx.open_window(window::options(cx), |window, cx| {
                cx.new(|cx| Workspace::new(window, cx))
            })
            .expect("failed to open the WackDepot window");
            cx.activate(true);
        });
}

/// Which page fills the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Repo,
    Settings,
}

/// Root view of the window.
pub struct Workspace {
    /// The root's focus: app actions and menu items dispatch through it
    /// whenever no field is focused.
    focus: FocusHandle,
    model: Entity<RepoModel>,
    overlays: Entity<Overlays>,
    titlebar: Entity<TitlebarView>,
    sidebar: Entity<SidebarView>,
    diff: Entity<DiffCard>,
    pulls: Entity<crate::model::pulls::PullRequestModel>,
    pull_workspace: Entity<crate::views::pulls::PullWorkspace>,
    settings: Entity<SettingsPage>,
    route: Route,
    /// The column's edge is being dragged.
    resizing: bool,
    backdrop: Backdrop,
    motion: Motion,
    sidebar_width: f32,
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let backdrop = Backdrop::new(window, prefs::get(cx));
        cx.observe_window_activation(window, |ws: &mut Workspace, window, cx| {
            backdrop::sync_reduce_motion(cx);
            ws.backdrop.set_active(window.is_window_active(), window);
            ws.pulls.update(cx, |m, cx| {
                m.foreground = window.is_window_active();
                if m.foreground {
                    m.refresh(cx);
                }
            });
            cx.notify();
        })
        .detach();
        window::observe_geometry(window, cx);

        let model = cx.new(RepoModel::new);
        let overlays = cx.new(|_| Overlays::new(model.clone()));
        let session = crate::github::session(cx);
        let pulls =
            cx.new(|cx| crate::model::pulls::PullRequestModel::new(model.clone(), session, cx));
        let pull_workspace = cx
            .new(|cx| crate::views::pulls::PullWorkspace::new(pulls.clone(), overlays.clone(), cx));
        let titlebar =
            cx.new(|cx| TitlebarView::new(model.clone(), overlays.clone(), pulls.clone(), cx));
        let sidebar =
            cx.new(|cx| SidebarView::new(model.clone(), overlays.clone(), pulls.clone(), cx));
        let diff = cx.new(|cx| DiffCard::new(model.clone(), overlays.clone(), cx));
        let settings = cx.new(SettingsPage::new);
        cx.subscribe_in(
            &settings,
            window,
            |ws: &mut Workspace, _, e: &SettingsEvent, window, cx| match e {
                SettingsEvent::Backdrop => {
                    let s = prefs::get(cx).clone();
                    ws.backdrop.reconfigure(window, &s);
                    cx.notify();
                }
                SettingsEvent::Close => {
                    ws.route = Route::Repo;
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(&model, |ws: &mut Workspace, _, e: &RepoEvent, cx| match e {
            RepoEvent::Notice(n) => ws.overlays.update(cx, |o, cx| o.toast(n.clone(), cx)),
            RepoEvent::Confirm(a) => ws
                .overlays
                .update(cx, |o, cx| o.confirm(a.clone(), None, cx)),
            RepoEvent::Opened => {
                ws.route = Route::Repo;
                ws.rebuild_in();
                cx.notify();
            }
            _ => cx.notify(),
        })
        .detach();

        let sidebar_width = prefs::get(cx)
            .sidebar_width
            .unwrap_or(metrics::SIDEBAR_W)
            .clamp(metrics::SIDEBAR_MIN, metrics::SIDEBAR_MAX);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // A focused element can vanish while it holds focus (switching tabs
        // hides the commit file strip, closing Settings drops its page).
        // With nothing focused, keys go nowhere, so hand focus to the root.
        cx.on_focus_lost(window, |ws, window, cx| ws.focus.focus(window, cx))
            .detach();
        let mut ws = Self {
            focus,
            model,
            overlays,
            titlebar,
            sidebar,
            diff,
            pulls,
            pull_workspace,
            settings,
            route: Route::Repo,
            resizing: false,
            backdrop,
            motion: Motion::default(),
            sidebar_width,
        };
        ws.rebuild_in();
        ws
    }

    /// The shell assembles itself: the column glides in from the left, the
    /// card rises a beat later (WackCode's rebuild-in).
    fn rebuild_in(&mut self) {
        self.motion
            .restart(("col-x", 0), -28.0, 0.0, 420.0, 0.0, POP);
        self.motion
            .restart(("col-o", 0), 0.0, 1.0, 260.0, 0.0, EASE);
        self.motion
            .restart(("card-y", 0), 26.0, 0.0, 460.0, 120.0, POP);
        self.motion
            .restart(("card-o", 0), 0.0, 1.0, 300.0, 120.0, EASE);
    }

    fn on_drop(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        for p in paths.paths() {
            let p = p.clone();
            self.model.update(cx, |m, cx| m.open(p, cx));
        }
    }

    /// Arrow keys: when nothing focused owns them — no field, no commit file
    /// strip, no dialog, no popover — they walk the list on the left.
    /// Text fields and the strip bind their own, deeper, arrows and win.
    fn step_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.route != Route::Repo || self.model.read(cx).repo.is_none() {
            return;
        }
        if self.titlebar.read(cx).open.is_some() {
            return;
        }
        self.sidebar.update(cx, |s, cx| {
            if s.step(delta, window, cx) {
                // Focus follows the list, so Space and ⌘⌫ work right after.
                s.focus_list(window, cx);
            }
        });
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.overlays.update(cx, |o, cx| o.dismiss(cx)) {
            return;
        }
        if self.titlebar.update(cx, |t, cx| t.close(cx)) {
            return;
        }
        if self.pull_workspace.update(cx, |p, cx| p.dismiss(cx)) {
            return;
        }
        if self.route == Route::Settings {
            self.route = Route::Repo;
            cx.notify();
        }
    }

    fn toggle_pref(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut crate::settings::Settings),
    ) {
        prefs::update(cx, f);
        self.model.update(cx, |m, cx| m.load_diff(cx));
        self.diff.update(cx, |d, cx| d.sync_prefs(cx));
        self.pull_workspace.update(cx, |p, cx| p.sync_prefs(cx));
        cx.notify();
    }

    fn repo_body(&mut self, mix: f32, enter: [f32; 4], cx: &mut Context<Self>) -> gpui::AnyElement {
        let [col_x, col_o, card_y, card_o] = enter;
        let t = cx.theme().clone();
        let resizing = self.resizing;
        let grab = div()
            .id("resizer")
            .w(px(metrics::GAP))
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_col_resize()
            .group("resizer")
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.resizing = true;
                    cx.notify();
                }),
            )
            .on_click(cx.listener(|ws, e: &gpui::ClickEvent, _, cx| {
                // Double-click puts the column back to its default width.
                if e.click_count() == 2 {
                    ws.sidebar_width = metrics::SIDEBAR_W;
                    prefs::update(cx, |s| s.sidebar_width = None);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .w(px(4.))
                    .h(px(44.))
                    .rounded_full()
                    .bg(t.accent.opacity(0.75))
                    .opacity(if resizing { 1. } else { 0. })
                    .group_hover("resizer", |s| s.opacity(1.)),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                div()
                    .w(px(self.sidebar_width))
                    .flex_none()
                    .relative()
                    .left(px(col_x))
                    .opacity(col_o.clamp(0.0, 1.0))
                    .rounded(px(metrics::RADIUS_CARD))
                    .bg(t.column(mix))
                    .child(self.sidebar.clone()),
            )
            .child(grab)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .top(px(card_y))
                    .opacity(card_o.clamp(0.0, 1.0))
                    .rounded(px(metrics::RADIUS_CARD))
                    .bg(t.bg)
                    .border_1()
                    .border_color(t.border_bright.opacity(0.7))
                    .overflow_hidden()
                    .child(if self.model.read(cx).tab == Tab::PullRequests {
                        self.pull_workspace.clone().into_any_element()
                    } else {
                        self.diff.clone().into_any_element()
                    }),
            )
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.motion.begin(cx);
        let mix = self.backdrop.mix(&mut self.motion);
        if self.backdrop.needs_opaque(mix) {
            cx.defer_in(window, |ws, window, _| ws.backdrop.go_opaque(window));
        }
        // A closed dialog or popover takes its focused field with it; hand
        // focus back to the root so shortcuts and menus keep working.
        if window.focused(cx).is_none() {
            cx.defer_in(window, |ws, window, cx| ws.focus.focus(window, cx));
        }
        let t = cx.theme().clone();
        let has_repo = self.model.read(cx).repo.is_some();
        let enter = [
            self.motion.value(("col-x", 0), 0.0),
            self.motion.value(("col-o", 0), 1.0),
            self.motion.value(("card-y", 0), 0.0),
            self.motion.value(("card-o", 0), 1.0),
        ];

        let body = if self.route == Route::Settings {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(px(metrics::GAP))
                .child(
                    div()
                        .w(px(self.sidebar_width))
                        .flex_none()
                        .rounded(px(metrics::RADIUS_CARD))
                        .bg(t.column(mix))
                        .child(SettingsNav {
                            page: self.settings.clone(),
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .rounded(px(metrics::RADIUS_CARD))
                        .bg(t.bg)
                        .border_1()
                        .border_color(t.border_bright.opacity(0.7))
                        .overflow_hidden()
                        .child(self.settings.clone()),
                )
                .into_any_element()
        } else if has_repo {
            self.repo_body(mix, enter, cx)
        } else {
            div()
                .flex_1()
                .rounded(px(metrics::RADIUS_CARD))
                .bg(t.bg)
                .border_1()
                .border_color(t.border_bright.opacity(0.7))
                .relative()
                .top(px(enter[2]))
                .opacity(enter[3].clamp(0.0, 1.0))
                .child(Welcome {
                    model: self.model.clone(),
                    overlays: self.overlays.clone(),
                })
                .into_any_element()
        };

        let root = div()
            .id("workspace")
            .key_context("Workspace")
            .relative()
            .size_full()
            .bg(t.shell(mix))
            .font_family(theme::fonts::SANS)
            .text_size(px(metrics::TEXT_BASE))
            .font_weight(FontWeight::NORMAL)
            .text_color(t.text)
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_drop(cx.listener(|ws, paths: &ExternalPaths, _, cx| ws.on_drop(paths, cx)))
            .on_mouse_move(cx.listener(|ws, e: &gpui::MouseMoveEvent, _, cx| {
                if ws.resizing {
                    if e.pressed_button != Some(gpui::MouseButton::Left) {
                        ws.resizing = false;
                        let w = ws.sidebar_width;
                        prefs::update(cx, |s| s.sidebar_width = Some(w));
                    } else {
                        let x = f32::from(e.position.x) - metrics::GAP;
                        ws.sidebar_width = x.clamp(metrics::SIDEBAR_MIN, metrics::SIDEBAR_MAX);
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    if ws.resizing {
                        ws.resizing = false;
                        let w = ws.sidebar_width;
                        prefs::update(cx, |s| s.sidebar_width = Some(w));
                        cx.notify();
                    }
                }),
            )
            .on_action(cx.listener(|ws, _: &Commit, _, cx| {
                if ws.route != Route::Repo || ws.model.read(cx).tab != Tab::Changes {
                    return;
                }
                let composer = ws.sidebar.read(cx).composer.clone();
                composer.update(cx, |c, cx| c.commit(cx));
            }))
            .on_action(cx.listener(|ws, _: &Dismiss, _, cx| ws.dismiss(cx)))
            .on_action(cx.listener(|ws, _: &OpenSettings, _, cx| {
                ws.route = if ws.route == Route::Settings {
                    Route::Repo
                } else {
                    Route::Settings
                };
                cx.notify();
            }))
            .on_action(cx.listener(|ws, _: &AddRepository, _, cx| {
                ws.model.update(cx, |m, cx| m.add_existing(cx))
            }))
            .on_action(cx.listener(|ws, _: &CloneRepository, window, cx| {
                ws.overlays.update(cx, |o, cx| o.open_clone(window, cx))
            }))
            .on_action(cx.listener(|ws, _: &NewRepository, window, cx| {
                ws.overlays.update(cx, |o, cx| o.open_new_repo(window, cx))
            }))
            .on_action(cx.listener(|ws, _: &ShowChanges, _, cx| {
                ws.model.update(cx, |m, cx| m.set_tab(Tab::Changes, cx))
            }))
            .on_action(cx.listener(|ws, _: &ShowHistory, _, cx| {
                ws.model.update(cx, |m, cx| m.set_tab(Tab::History, cx))
            }))
            .on_action(cx.listener(|ws, _: &ShowPullRequests, _, cx| {
                ws.route = Route::Repo;
                ws.model
                    .update(cx, |m, cx| m.set_tab(Tab::PullRequests, cx));
                ws.pulls.update(cx, |m, cx| m.refresh(cx));
            }))
            .on_action(
                cx.listener(|ws, _: &SelectPrev, window, cx| ws.step_selection(-1, window, cx)),
            )
            .on_action(
                cx.listener(|ws, _: &SelectNext, window, cx| ws.step_selection(1, window, cx)),
            )
            .on_action(cx.listener(|ws, _: &ShowRepositories, window, cx| {
                ws.titlebar
                    .update(cx, |t, cx| t.toggle(TitlePopover::Repos, window, cx))
            }))
            .on_action(cx.listener(|ws, _: &ShowBranches, window, cx| {
                ws.titlebar
                    .update(cx, |t, cx| t.toggle(TitlePopover::Branches, window, cx))
            }))
            .on_action(cx.listener(|ws, _: &NewBranch, window, cx| {
                ws.titlebar.update(cx, |t, cx| t.new_branch(window, cx))
            }))
            .on_action(cx.listener(|ws, _: &MergeIntoCurrent, window, cx| {
                if let Some(reason) = ws.model.read(cx).merge_blocker() {
                    ws.model.update(cx, |m, cx| {
                        m.notice(reason, crate::model::NoticeKind::Error, cx)
                    });
                } else {
                    ws.overlays
                        .update(cx, |o, cx| o.open_merge(None, window, cx));
                }
            }))
            .on_action(cx.listener(|ws, _: &Fetch, _, cx| ws.model.update(cx, |m, cx| m.fetch(cx))))
            .on_action(cx.listener(|ws, _: &Pull, _, cx| ws.model.update(cx, |m, cx| m.pull(cx))))
            .on_action(cx.listener(|ws, _: &Push, _, cx| ws.model.update(cx, |m, cx| m.push(cx))))
            .on_action(
                cx.listener(|ws, _: &StashAll, _, cx| ws.model.update(cx, |m, cx| m.stash_all(cx))),
            )
            .on_action(cx.listener(|ws, _: &Refresh, _, cx| {
                ws.model.update(cx, |m, cx| m.request_refresh(cx));
                ws.pulls.update(cx, |m, cx| m.refresh(cx));
            }))
            .on_action(cx.listener(|ws, _: &OpenInEditor, _, cx| {
                ws.model.update(cx, |m, cx| m.open_in_editor(cx))
            }))
            .on_action(cx.listener(|ws, _: &OpenInTerminal, _, cx| {
                ws.model.update(cx, |m, cx| m.open_in_terminal(cx))
            }))
            .on_action(cx.listener(|ws, _: &ShowInFinder, _, cx| {
                ws.model.update(cx, |m, cx| m.open_in_finder(cx))
            }))
            .on_action(cx.listener(|ws, _: &ViewOnGitHub, _, cx| {
                ws.model.update(cx, |m, cx| {
                    m.open_on_github(crate::model::GitHubTarget::Repository, cx)
                })
            }))
            .on_action(cx.listener(|ws, _: &ToggleSplitDiff, _, cx| {
                ws.toggle_pref(cx, |s| s.diff_split = !s.diff_split)
            }))
            .on_action(cx.listener(|ws, _: &ToggleWhitespace, _, cx| {
                ws.toggle_pref(cx, |s| s.ignore_whitespace = !s.ignore_whitespace)
            }))
            .child(
                div()
                    .h(px(metrics::TITLEBAR_H))
                    .w_full()
                    .flex_none()
                    .child(self.titlebar.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .px(px(metrics::GAP))
                    .pb(px(metrics::GAP))
                    .child(body),
            )
            .child(self.overlays.clone());
        self.motion.end(window);
        root
    }
}
