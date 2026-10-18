//! Numbers that roll like a drum when they change: the old value slides
//! out and the new one in (up when it grew, down when it shrank).

use gpui::{
    App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
};

use crate::motion::{Motion, spring};

#[derive(IntoElement)]
pub struct Rolling {
    id: ElementId,
    value: i64,
    prefix: SharedString,
    height: f32,
}

/// `value` with an optional prefix (e.g. "↑"), `height` the line box.
pub fn rolling(id: impl Into<ElementId>, value: i64, height: f32) -> Rolling {
    Rolling {
        id: id.into(),
        value,
        prefix: SharedString::default(),
        height,
    }
}

impl Rolling {
    pub fn prefix(mut self, p: impl Into<SharedString>) -> Self {
        self.prefix = p.into();
        self
    }
}

struct State {
    shown: i64,
    previous: Option<i64>,
    motion: Motion,
}

impl RenderOnce for Rolling {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| State {
            shown: self.value,
            previous: None,
            motion: Motion::default(),
        });
        let h = self.height;
        let value = self.value;
        let mut frame = false;
        let (offset, prev, up) = state.update(cx, |s, cx| {
            s.motion.begin(cx);
            if s.shown != value {
                let up = value > s.shown;
                s.previous = Some(s.shown);
                s.shown = value;
                // Start a full line away and spring home.
                s.motion
                    .kick(("roll", 0), if up { h } else { -h }, 0.0, spring::DIGITS);
            }
            let off = s.motion.spring(("roll", 0), 0.0, spring::DIGITS);
            if off.abs() < 0.5 {
                s.previous = None;
            }
            frame = s.motion.end_frame();
            (off, s.previous, value >= s.previous.unwrap_or(value))
        });
        if frame {
            window.request_animation_frame();
        }
        let text = |v: i64| format!("{}{v}", self.prefix);
        let mut el = div().relative().h(px(h)).overflow_hidden().child(
            div()
                .relative()
                .top(px(offset))
                .h(px(h))
                .flex()
                .items_center()
                .child(text(value)),
        );
        if let Some(p) = prev {
            let out = if up { offset - h } else { offset + h };
            el = el.child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(out))
                    .h(px(h))
                    .flex()
                    .items_center()
                    .child(text(p)),
            );
        }
        el
    }
}
