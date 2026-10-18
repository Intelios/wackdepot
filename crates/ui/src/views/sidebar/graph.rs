//! Painting one History row's graph lanes.

use gpui::{
    App, Bounds, Hsla, IntoElement, PathBuilder, Pixels, Styled, Window, canvas, point, px,
};
use wackdepot_core::graph::{Edge, GraphRow};

/// Column pitch and the cap on drawn columns.
pub const LANE_W: f32 = 14.0;
pub const MAX_LANES: usize = 6;
const PAD: f32 = 10.0;

/// Gutter width for `lanes` columns (rows share the widest so subjects align).
pub fn gutter_width(lanes: usize) -> f32 {
    PAD + lanes.clamp(1, MAX_LANES) as f32 * LANE_W
}

/// A canvas drawing `row`'s edges and node. `head` rings the node in the
/// accent (the checked-out commit).
pub fn paint_row(
    row: GraphRow,
    lanes: usize,
    colors: [Hsla; 6],
    accent: Hsla,
    bg: Hsla,
    head: bool,
    merge: bool,
) -> impl IntoElement {
    let width = gutter_width(lanes);
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            let x = |lane: usize| {
                bounds.left()
                    + px(PAD / 2.0 + LANE_W * lane.min(MAX_LANES - 1) as f32 + LANE_W / 2.0)
            };
            let (top, bottom) = (bounds.top(), bounds.bottom());
            let mid = top + bounds.size.height / 2.0;
            // The first line (the checked-out branch) wears the accent.
            let color = |c: usize| {
                if c == 0 {
                    accent
                } else {
                    colors[(c - 1) % colors.len()]
                }
            };
            let node = point(x(row.node), mid);
            let stroke = |from: gpui::Point<Pixels>,
                          to: gpui::Point<Pixels>,
                          c: Hsla,
                          window: &mut Window| {
                let mut p = PathBuilder::stroke(px(1.6));
                p.move_to(from);
                if from.x == to.x {
                    p.line_to(to);
                } else {
                    // A soft S-bend between columns.
                    let my = (from.y + to.y) / 2.0;
                    p.cubic_bezier_to(to, point(from.x, my), point(to.x, my));
                }
                if let Ok(path) = p.build() {
                    window.paint_path(path, c);
                }
            };
            for e in &row.edges {
                match *e {
                    Edge::Pass { lane, color: c } => stroke(
                        point(x(lane), top),
                        point(x(lane), bottom),
                        color(c),
                        window,
                    ),
                    Edge::In { lane, color: c } => {
                        stroke(point(x(lane), top), node, color(c), window)
                    }
                    Edge::Out { lane, color: c } => {
                        stroke(node, point(x(lane), bottom), color(c), window)
                    }
                    Edge::Shift { from, to, color: c } => {
                        stroke(point(x(from), top), point(x(to), bottom), color(c), window)
                    }
                }
            }
            let r = if merge { 3.5 } else { 4.5 };
            let ring = if head { accent } else { color(row.color) };
            window.paint_quad(
                gpui::fill(
                    Bounds::new(
                        point(node.x - px(r + 1.5), node.y - px(r + 1.5)),
                        gpui::size(px(2. * r + 3.), px(2. * r + 3.)),
                    ),
                    bg,
                )
                .corner_radii(px(r + 1.5)),
            );
            window.paint_quad(
                gpui::fill(
                    Bounds::new(
                        point(node.x - px(r), node.y - px(r)),
                        gpui::size(px(2. * r), px(2. * r)),
                    ),
                    if merge { bg } else { ring },
                )
                .corner_radii(px(r))
                .border_widths(px(if merge { 1.6 } else { 0. }))
                .border_color(ring),
            );
        },
    )
    .h_full()
    .w(px(width))
}
