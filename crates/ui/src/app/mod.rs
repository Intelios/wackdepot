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
use crate::motion::Motion;
use crate::theme::{self, ActiveTheme, metrics};
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
            let input = prefs::theme_input(prefs::get(cx));
            theme::init(cx, &input);
            actions::register(cx);
            text_input::bind_keys(cx);
            backdrop::sync_reduce_motion(cx);
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
    route: Route,
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
            cx.notify();
        })
        .detach();
        window::observe_geometry(window, cx);

        let model = cx.new(RepoModel::new);
        let overlays = cx.new(|_| Overlays::new(model.clone()));
        let titlebar = cx.new(|cx| TitlebarView::new(model.clone(), overlays.clone(), cx));
        cx.subscribe(&model, |ws: &mut Workspace, _, e: &RepoEvent, cx| match e {
            RepoEvent::Notice(n) => ws.overlays.update(cx, |o, cx| o.toast(n.clone(), cx)),
            RepoEvent::Confirm(a) => ws
                .overlays
                .update(cx, |o, cx| o.confirm(a.clone(), None, cx)),
            RepoEvent::Opened => {
                ws.route = Route::Repo;
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
        Self {
            focus,
            model,
            overlays,
            titlebar,
            route: Route::Repo,
            backdrop,
            motion: Motion::default(),
            sidebar_width,
        }
    }

    fn on_drop(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        for p in paths.paths() {
            let p = p.clone();
            self.model.update(cx, |m, cx| m.open(p, cx));
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.overlays.update(cx, |o, cx| o.dismiss(cx)) {
            return;
        }
        if self.titlebar.update(cx, |t, cx| t.close(cx)) {
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
        cx.notify();
    }

    fn repo_body(&mut self, mix: f32, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = cx.theme().clone();
        let tab = self.model.read(cx).tab;
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
                    .p(px(10.))
                    .text_color(t.text_dim)
                    .child(format!("{tab:?}")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(metrics::RADIUS_CARD))
                    .bg(t.bg)
                    .border_1()
                    .border_color(t.border_bright.opacity(0.7)),
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

        let body = if self.route == Route::Settings {
            div()
                .flex_1()
                .rounded(px(metrics::RADIUS_CARD))
                .bg(t.bg)
                .border_1()
                .border_color(t.border_bright.opacity(0.7))
                .flex()
                .items_center()
                .justify_center()
                .text_color(t.text_dim)
                .child("Settings")
                .into_any_element()
        } else if has_repo {
            self.repo_body(mix, cx)
        } else {
            div()
                .flex_1()
                .rounded(px(metrics::RADIUS_CARD))
                .bg(t.bg)
                .border_1()
                .border_color(t.border_bright.opacity(0.7))
                .child(Welcome {
                    model: self.model.clone(),
                    overlays: self.overlays.clone(),
                })
                .into_any_element()
        };

        let root = div()
            .id("workspace")
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
            .on_action(cx.listener(|ws, _: &Fetch, _, cx| ws.model.update(cx, |m, cx| m.fetch(cx))))
            .on_action(cx.listener(|ws, _: &Pull, _, cx| ws.model.update(cx, |m, cx| m.pull(cx))))
            .on_action(cx.listener(|ws, _: &Push, _, cx| ws.model.update(cx, |m, cx| m.push(cx))))
            .on_action(
                cx.listener(|ws, _: &StashAll, _, cx| ws.model.update(cx, |m, cx| m.stash_all(cx))),
            )
            .on_action(cx.listener(|ws, _: &Refresh, _, cx| {
                ws.model.update(cx, |m, cx| m.request_refresh(cx))
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
