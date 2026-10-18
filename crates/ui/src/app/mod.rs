//! The rebuilt WackDepot shell: window, backdrop, routing, and the regions
//! composed over the glass — titlebar, left column, content card, overlays.

pub mod actions;
pub mod backdrop;
pub mod prefs;
pub mod window;

use gpui::{
    App, AppContext as _, Context, FontWeight, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};

use crate::components::drag::drag_strip;
use crate::icons;
use crate::motion::Motion;
use crate::theme::{self, ActiveTheme, metrics};

use backdrop::Backdrop;

/// Open the main window and run the rebuilt app.
pub fn run() {
    gpui_platform::application()
        .with_assets(icons::IconAssets)
        .run(|cx: &mut App| {
            prefs::init(cx);
            let input = prefs::theme_input(prefs::get(cx));
            theme::init(cx, &input);
            actions::register(cx);
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

/// Root view of the window.
pub struct Workspace {
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
        let sidebar_width = prefs::get(cx)
            .sidebar_width
            .unwrap_or(metrics::SIDEBAR_W)
            .clamp(metrics::SIDEBAR_MIN, metrics::SIDEBAR_MAX);
        Self {
            backdrop,
            motion: Motion::default(),
            sidebar_width,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.motion.begin(cx);
        let mix = self.backdrop.mix(&mut self.motion);
        if self.backdrop.needs_opaque(mix) {
            cx.defer_in(window, |ws, window, _| ws.backdrop.go_opaque(window));
        }
        let theme = cx.theme().clone();

        let titlebar = div()
            .h(px(metrics::TITLEBAR_H))
            .w_full()
            .flex_none()
            .child(drag_strip("titlebar", div()));

        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .gap(px(metrics::GAP))
            .px(px(metrics::GAP))
            .pb(px(metrics::GAP))
            .child(
                div()
                    .w(px(self.sidebar_width))
                    .flex_none()
                    .rounded(px(metrics::RADIUS_CARD))
                    .bg(theme.column(mix)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(metrics::RADIUS_CARD))
                    .bg(theme.bg)
                    .border_1()
                    .border_color(theme.border_bright.opacity(0.7))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_dim)
                    .child("WackDepot"),
            );

        let root = div()
            .size_full()
            .bg(theme.shell(mix))
            .font_family(theme::fonts::SANS)
            .text_size(px(metrics::TEXT_BASE))
            .font_weight(FontWeight::NORMAL)
            .text_color(theme.text)
            .flex()
            .flex_col()
            .child(titlebar)
            .child(body);
        self.motion.end(window);
        root
    }
}
