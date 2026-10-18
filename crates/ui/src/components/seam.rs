//! WackCode's signature dotted accent line (`2px dotted --wc-accent-line`).
//! gpui has no dotted borders, so the dots are painted.

use gpui::{
    App, Bounds, IntoElement, ParentElement, Pixels, Styled, Window, canvas, div, fill, point, px,
    size,
};

use crate::theme::ActiveTheme;

const DOT: f32 = 2.0;
const STEP: f32 = 4.0;

/// A horizontal dotted seam filling its width (2px tall).
pub fn seam() -> impl IntoElement {
    div().w_full().h(px(DOT)).flex_none().child(dots(true))
}

/// A horizontal trail that grows to fill a row (group headings).
pub fn trail() -> impl IntoElement {
    div().flex_1().min_w(px(8.)).h(px(DOT)).child(dots(true))
}

/// A vertical dotted rail.
pub fn rail() -> impl IntoElement {
    div().w(px(DOT)).h_full().flex_none().child(dots(false))
}

fn dots(horizontal: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, cx: &mut App| {
            let color = cx.theme().accent_line;
            let len = if horizontal {
                bounds.size.width
            } else {
                bounds.size.height
            };
            let n = ((len / px(STEP)) as usize).max(1);
            for i in 0..n {
                let o = px(i as f32 * STEP);
                let at = if horizontal {
                    point(bounds.left() + o, bounds.top())
                } else {
                    point(bounds.left(), bounds.top() + o)
                };
                window.paint_quad(
                    fill(Bounds::new(at, size(px(DOT), px(DOT))), color).corner_radii(px(1.)),
                );
            }
        },
    )
    .size_full()
}

/// A dotted circle of `diameter` (the empty-state orbit).
pub fn dotted_ring(diameter: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, cx: &mut App| {
            let color = cx.theme().accent_line;
            let r = diameter / 2.0 - 1.0;
            let n = ((std::f32::consts::TAU * r) / STEP).floor().max(8.0) as usize;
            let c = bounds.center();
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let at = point(c.x + px(r * a.cos() - 1.0), c.y + px(r * a.sin() - 1.0));
                window.paint_quad(
                    fill(Bounds::new(at, size(px(DOT), px(DOT))), color).corner_radii(px(1.)),
                );
            }
        },
    )
    .size(px(diameter))
}
