//! The WackDepot mascot: the Commit Buddy of the logo, drawn live so it can
//! move. Its poses are the animation hooks the app calls: idle (a slow bob
//! and a blink), celebrate (a hop and a wider grin), ponder (a lean and a
//! look). A pose plays for a while and then settles: a looping animation
//! redraws the window every frame, so it never loops forever.
//!
//! Geometry comes from the mark in `design/logo/source/sym.py`: a 152 × 144
//! squircle body, 20 × 44 eye pills 16 apart, a half-disc smile, and eyes set
//! 8 units forward of centre. Here one logo unit is `size * 0.92 / 152` px.

use std::time::{Duration, Instant};

use gpui::{
    Animation, AnimationExt, App, Bounds, Div, ElementId, Hsla, IntoElement, ParentElement,
    PathBuilder, Pixels, RenderOnce, Styled, Window, canvas, div, point, px,
};

use crate::theme::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pose {
    #[default]
    Idle,
    Celebrate,
    Ponder,
}

impl Pose {
    fn period(self) -> u64 {
        match self {
            Pose::Idle => 3600,
            Pose::Celebrate => 900,
            Pose::Ponder => 2400,
        }
    }

    fn play_time(self) -> Duration {
        match self {
            Pose::Idle => Duration::from_secs(11),
            Pose::Celebrate => Duration::from_millis(1800),
            Pose::Ponder => Duration::from_secs(7),
        }
    }
}

#[derive(IntoElement)]
pub struct Mascot {
    id: ElementId,
    size: f32,
    pose: Pose,
}

pub fn mascot(id: impl Into<ElementId>) -> Mascot {
    Mascot {
        id: id.into(),
        size: 64.0,
        pose: Pose::Idle,
    }
}

impl Mascot {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }

    pub fn pose(mut self, p: Pose) -> Self {
        self.pose = p;
        self
    }
}

/// The body's outline in logo units (152 × 144): a squircle, 64-unit corner
/// run with 44-unit handles. A quarter circle through the same points cuts
/// tighter, so this cannot be a `rounded(px)` quad.
fn body_path(bounds: Bounds<Pixels>, k: f32) -> Option<gpui::Path<Pixels>> {
    let at = |x: f32, y: f32| point(bounds.left() + px(x * k), bounds.top() + px(y * k));
    let mut b = PathBuilder::fill();
    b.move_to(at(64., 0.));
    b.line_to(at(88., 0.));
    b.cubic_bezier_to(at(152., 64.), at(108., 0.), at(152., 20.));
    b.line_to(at(152., 80.));
    b.cubic_bezier_to(at(88., 144.), at(152., 124.), at(132., 144.));
    b.line_to(at(64., 144.));
    b.cubic_bezier_to(at(0., 80.), at(20., 144.), at(0., 124.));
    b.line_to(at(0., 64.));
    b.cubic_bezier_to(at(64., 0.), at(0., 20.), at(20., 0.));
    b.close();
    b.build().ok()
}

/// The smile: a half-disc, flat side up, filling `bounds`. Painted because
/// gpui clamps corner radii to half the box, so a bottom-rounded div cannot
/// make one.
fn smile_path(bounds: Bounds<Pixels>) -> Option<gpui::Path<Pixels>> {
    let r = bounds.size.width / 2.0;
    let c = r * 0.5523; // a quarter circle as one cubic
    let at = |x: Pixels, y: Pixels| point(bounds.left() + x, bounds.top() + y);
    let mut b = PathBuilder::fill();
    b.move_to(at(px(0.), px(0.)));
    b.line_to(at(r * 2.0, px(0.)));
    b.cubic_bezier_to(at(r, r), at(r * 2.0, c), at(r + c, r));
    b.cubic_bezier_to(at(px(0.), px(0.)), at(r - c, r), at(px(0.), c));
    b.close();
    b.build().ok()
}

/// One frame of `pose` at `phase` (0–1 through its cycle).
fn frame(
    el: Div,
    s: f32,
    pose: Pose,
    phase: f32,
    body: Hsla,
    ink: Hsla,
    glow: Vec<gpui::BoxShadow>,
) -> Div {
    let tau = std::f32::consts::TAU;
    let (dy, dx) = match pose {
        Pose::Idle => ((phase * tau).sin() * s * 0.03, 0.0),
        Pose::Celebrate => (-(phase * std::f32::consts::PI).sin().abs() * s * 0.16, 0.0),
        Pose::Ponder => ((phase * tau).sin() * s * 0.015, s * 0.03),
    };
    // A quick blink once per idle cycle.
    let blink = pose == Pose::Idle && (0.62..0.66).contains(&phase);
    // Logo units: the body is 152 × 144 with its centre at (76, 72).
    let k = s * 0.92 / 152.0;
    // The mark's eyes sit 8 units forward of centre; a ponder looks further.
    let look = if pose == Pose::Ponder { 14.0 } else { 8.0 };
    let (eye_w, eye_h) = (20.0, if blink { 6.0 } else { 44.0 });
    let eye_top = 72.0 - 34.0 + (44.0 - eye_h) / 2.0;
    let eye = |x: f32| {
        div()
            .absolute()
            .left(px((x + look) * k))
            .top(px(eye_top * k))
            .w(px(eye_w * k))
            .h(px(eye_h * k))
            .rounded_full()
            .bg(ink)
    };
    let grin = if pose == Pose::Celebrate { 26.0 } else { 22.0 };
    let (pad_x, pad_y) = ((s - 152.0 * k) / 2.0, (s - 144.0 * k) / 2.0);
    // The div is the body's underlay and carries the glow; the canvas paints
    // the squircle over it, so its corners are the mark's, not a circle's.
    el.child(
        div()
            .absolute()
            .left(px(dx + pad_x))
            .top(px(dy + pad_y))
            .w(px(152.0 * k))
            .h(px(144.0 * k))
            .rounded(px(64.0 * k))
            .bg(body)
            .shadow(glow)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                        if let Some(path) = body_path(bounds, k) {
                            window.paint_path(path, body);
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(eye(48.0))
            .child(eye(84.0))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                        if let Some(path) = smile_path(bounds) {
                            window.paint_path(path, ink);
                        }
                    },
                )
                .absolute()
                .left(px((76.0 - grin + look) * k))
                .top(px(94.0 * k))
                .w(px(2.0 * grin * k))
                .h(px(grin * k)),
            ),
    )
}

impl RenderOnce for Mascot {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let (s, pose) = (self.size, self.pose);
        let born = window.use_keyed_state(self.id.clone(), cx, |_, _| Instant::now());
        let resting = cx.reduce_motion() || born.read(cx).elapsed() > pose.play_time();
        let (body, ink, glow) = (t.accent, t.on_accent, t.accent_glow());
        let content = if resting {
            frame(div().size_full(), s, pose, 0.25, body, ink, glow).into_any_element()
        } else {
            div()
                .size_full()
                .with_animation(
                    self.id,
                    Animation::new(Duration::from_millis(pose.period())).repeat(),
                    move |el, phase| frame(el, s, pose, phase, body, ink, glow.clone()),
                )
                .into_any_element()
        };
        div().size(px(s)).flex_none().relative().child(content)
    }
}
