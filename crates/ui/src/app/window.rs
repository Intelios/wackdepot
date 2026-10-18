//! Window options and saved geometry.

use gpui::{
    App, Bounds, Context, TitlebarOptions, Window, WindowBounds, WindowOptions, point, px, size,
};

use crate::settings::WindowGeo;
use crate::theme::metrics;

pub fn options(cx: &App) -> WindowOptions {
    let s = super::prefs::get(cx);
    let bounds = match s.window {
        Some(g) => Bounds {
            origin: point(px(g.x), px(g.y)),
            size: size(px(g.w), px(g.h)),
        },
        None => Bounds::centered(None, size(px(1180.), px(760.)), cx),
    };
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(900.), px(560.))),
        // Hidden-inset chrome: traffic lights centred in the 38px titlebar,
        // our own strip drags the window.
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px((metrics::TITLEBAR_H - 10.) / 2.))),
        }),
        app_owns_titlebar_drag: true,
        window_background: super::backdrop::initial_appearance(s),
        app_id: Some("com.wackdepot.app".into()),
        ..Default::default()
    }
}

/// Save the window frame whenever it moves or resizes (ignoring sub-point
/// jitter).
pub fn observe_geometry<T: 'static>(window: &mut Window, cx: &mut Context<T>) {
    cx.observe_window_bounds(window, |_, window, cx| {
        let b = window.bounds();
        let geo = WindowGeo {
            x: b.origin.x.into(),
            y: b.origin.y.into(),
            w: b.size.width.into(),
            h: b.size.height.into(),
        };
        let changed = super::prefs::get(cx).window.is_none_or(|g| {
            (g.x - geo.x).abs() > 1.
                || (g.y - geo.y).abs() > 1.
                || (g.w - geo.w).abs() > 1.
                || (g.h - geo.h).abs() > 1.
        });
        if changed {
            super::prefs::update(cx, |s| s.window = Some(geo));
        }
    })
    .detach();
}
