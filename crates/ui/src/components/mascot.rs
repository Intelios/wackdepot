//! The WackDepot mascot — a placeholder blob until Jack draws the real one.
//! Its poses are the animation hooks the app calls: idle (a slow bob and a
//! blink every few seconds), celebrate (a hop), ponder (a lean and a look).

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled,
    Window, div, px,
};

use crate::theme::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pose {
    #[default]
    Idle,
    Celebrate,
    Ponder,
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

impl RenderOnce for Mascot {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.size;
        let pose = self.pose;
        let period = match pose {
            Pose::Idle => 3600,
            Pose::Celebrate => 900,
            Pose::Ponder => 2400,
        };
        div()
            .size(px(s))
            .flex_none()
            .relative()
            .child(div().size_full().with_animation(
                self.id,
                Animation::new(Duration::from_millis(period)).repeat(),
                move |el, phase| {
                    let tau = std::f32::consts::TAU;
                    let (dy, dx) = match pose {
                        Pose::Idle => ((phase * tau).sin() * s * 0.03, 0.0),
                        Pose::Celebrate => {
                            (-(phase * std::f32::consts::PI).sin().abs() * s * 0.16, 0.0)
                        }
                        Pose::Ponder => ((phase * tau).sin() * s * 0.015, s * 0.03),
                    };
                    // A quick blink once per idle cycle.
                    let blink = pose == Pose::Idle && (0.62..0.66).contains(&phase);
                    let eye_h = if blink { s * 0.025 } else { s * 0.13 };
                    let look = if pose == Pose::Ponder { s * 0.035 } else { 0.0 };
                    let eye = |x: f32| {
                        div()
                            .absolute()
                            .left(px(x + look))
                            .top(px(s * 0.36 + (s * 0.13 - eye_h) / 2.0))
                            .w(px(s * 0.1))
                            .h(px(eye_h))
                            .rounded_full()
                            .bg(t.on_accent)
                    };
                    el.child(
                        div()
                            .absolute()
                            .left(px(dx))
                            .top(px(dy + s * 0.04))
                            .size(px(s * 0.92))
                            .rounded(px(s * 0.38))
                            .bg(t.accent)
                            .shadow(t.accent_glow())
                            .child(eye(s * 0.28))
                            .child(eye(s * 0.54))
                            .child(
                                div()
                                    .absolute()
                                    .left(px(s * 0.38 + look))
                                    .top(px(s * 0.6))
                                    .w(px(s * 0.16))
                                    .h(px(if pose == Pose::Celebrate {
                                        s * 0.08
                                    } else {
                                        s * 0.035
                                    }))
                                    .rounded_full()
                                    .bg(t.on_accent.opacity(0.75)),
                            ),
                    )
                },
            ))
    }
}
