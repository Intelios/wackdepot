//! WackCode's bead-trail working mark: three accent beads orbiting.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled,
    Window, div, px,
};

use crate::theme::ActiveTheme;

#[derive(IntoElement)]
pub struct Orbit {
    id: ElementId,
    size: f32,
}

pub fn orbit(id: impl Into<ElementId>) -> Orbit {
    Orbit {
        id: id.into(),
        size: 14.0,
    }
}

impl Orbit {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
}

impl RenderOnce for Orbit {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = cx.theme().accent;
        let size = self.size;
        let r = size * 0.36;
        let beads = [(1.6, 1.0), (1.2, 0.6), (0.85, 0.32)];
        div()
            .size(px(size))
            .flex_none()
            .relative()
            .child(div().size_full().with_animation(
                self.id,
                Animation::new(Duration::from_millis(900)).repeat(),
                move |el, t| {
                    let mut el = el;
                    for (i, (radius, alpha)) in beads.iter().enumerate() {
                        let a = (t - i as f32 * 0.09) * std::f32::consts::TAU;
                        let d = radius * size / 12.0 * 2.0;
                        el = el.child(
                            div()
                                .absolute()
                                .left(px(size / 2.0 + r * a.cos() - d / 2.0))
                                .top(px(size / 2.0 + r * a.sin() - d / 2.0))
                                .size(px(d))
                                .rounded_full()
                                .bg(color.opacity(*alpha)),
                        );
                    }
                    el
                },
            ))
    }
}
