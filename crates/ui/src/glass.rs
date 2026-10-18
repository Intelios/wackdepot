//! Glass surfaces shared by controls: each is a vertical gradient (lit from
//! above), a hairline rim, an inner top highlight, and a soft drop. Ported
//! from zeron's crates/ui glass.rs (MIT), tuned to WackDepot tokens.
//! Some helpers sit idle until the controls that use them land (M2+).
#![allow(dead_code)]

use gpui::{
    Background, Bounds, BoxShadow, Corners, Hsla, Pixels, Styled, Window, hsla, linear_color_stop,
    linear_gradient, point, px,
};

use crate::theme::Theme;

fn shadow(color: Hsla, y: f32, blur: f32, spread: f32, inset: bool) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset,
    }
}

fn white(alpha: f32) -> Hsla {
    hsla(0.0, 0.0, 1.0, alpha)
}

fn black(alpha: f32) -> Hsla {
    hsla(0.0, 0.0, 0.0, alpha)
}

fn vertical(top: Hsla, bottom: Hsla) -> Background {
    linear_gradient(
        180.0,
        linear_color_stop(top, 0.0),
        linear_color_stop(bottom, 1.0),
    )
}

/// A lighter tint of `color` for the lit top of a gradient.
fn lift(color: Hsla, amount: f32) -> Hsla {
    mix(color, white(color.a), amount)
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    a.blend(b.opacity(t * b.a))
}

/// One glass surface, applicable to an element or paintable in a canvas.
pub(crate) struct Plate {
    background: Background,
    rim: Hsla,
    shadows: Vec<BoxShadow>,
}

impl Plate {
    pub(crate) fn apply<E: Styled>(self, el: E) -> E {
        el.bg(self.background)
            .border_1()
            .border_color(self.rim)
            .shadow(self.shadows)
    }

    /// Paints the plate the way a styled element would: drops beneath the
    /// fill, the rim on it, and inset light above it.
    pub(crate) fn paint(&self, window: &mut Window, bounds: Bounds<Pixels>, radius: f32) {
        let corners = Corners::all(px(radius));
        let (inset, drop): (Vec<_>, Vec<_>) = self.shadows.iter().cloned().partition(|s| s.inset);
        window.paint_drop_shadows(bounds, corners, &drop);
        window.paint_quad(gpui::quad(
            bounds,
            corners,
            self.background,
            px(1.0),
            self.rim,
            gpui::BorderStyle::default(),
        ));
        window.paint_inset_shadows(bounds, corners, &inset);
    }
}

/// Neutral glass. `t` fades the whole treatment in.
pub(crate) fn light_plate(theme: &Theme, t: f32) -> Plate {
    if theme.is_dark() {
        return Plate {
            background: vertical(white(0.08 * t), white(0.05 * t)),
            rim: white(0.09 * t),
            shadows: vec![
                shadow(white(0.07 * t), 1.0, 0.0, 0.0, true),
                shadow(black(0.16 * t), 1.0, 2.0, 0.0, false),
            ],
        };
    }
    Plate {
        background: vertical(black(0.075 * t), black(0.04 * t)),
        rim: black(0.08 * t),
        shadows: vec![
            shadow(black(0.08 * t), 1.0, 2.0, 0.0, true),
            shadow(white(0.55 * t), -1.0, 0.0, 0.0, true),
        ],
    }
}

/// Accent glass. `t` fades it in over the resting control; `glow` (0–1)
/// spreads its coloured shadow.
pub(crate) fn accent_plate(theme: &Theme, t: f32, glow: f32) -> Plate {
    let dark = theme.is_dark();
    let base = theme.accent;
    let top = lift(base, if dark { 0.06 } else { 0.22 });
    let rim = if dark {
        lift(base, 0.35).opacity(0.35)
    } else {
        mix(base, black(1.0), 0.14)
    };
    let highlight = white(if dark { 0.12 } else { 0.32 });
    let ring = white(if dark { 0.0 } else { 0.22 });
    let halo = base.opacity((if dark { 0.0 } else { 0.14 } + 0.3 * glow) * t);
    Plate {
        background: vertical(top.opacity(t), base.opacity(t)),
        rim: rim.opacity(t),
        shadows: vec![
            shadow(highlight.opacity(t), 1.0, 0.0, 0.0, true),
            shadow(ring.opacity(t), 0.0, 0.0, 1.0, true),
            shadow(halo, 2.0 + 2.0 * glow, 6.0 + 14.0 * glow, 0.0, false),
        ],
    }
}

/// The opaque handle: a solid light plate in every appearance, so it stays
/// solid over translucent dark glass.
pub(crate) fn thumb_plate(theme: &Theme) -> Plate {
    let dark = theme.is_dark();
    let mut shadows = vec![
        shadow(white(0.8), 1.0, 0.0, 0.0, true),
        shadow(black(if dark { 0.22 } else { 0.14 }), 1.0, 2.0, 0.0, false),
    ];
    if !dark {
        shadows.push(shadow(black(0.06), 2.0, 6.0, 0.0, false));
    }
    Plate {
        background: vertical(hsla(0.0, 0.0, 1.0, 1.0), hsla(0.0, 0.0, 0.965, 1.0)),
        rim: black(if dark { 0.12 } else { 0.11 }),
        shadows,
    }
}

pub(crate) fn light<E: Styled>(el: E, theme: &Theme, t: f32) -> E {
    light_plate(theme, t).apply(el)
}

#[allow(dead_code)]
pub(crate) fn thumb<E: Styled>(el: E, theme: &Theme) -> E {
    thumb_plate(theme).apply(el)
}
