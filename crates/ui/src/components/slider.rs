//! A thin accent slider: 4px track, round thumb; click or drag to set.

use std::rc::Rc;

use gpui::{
    App, Bounds, ElementId, IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels,
    RenderOnce, Styled, Window, canvas, div, prelude::*, px, relative,
};

use crate::theme::ActiveTheme;

type OnChange = Rc<dyn Fn(f32, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    /// 0–1.
    value: f32,
    on_change: Option<OnChange>,
}

pub fn slider(id: impl Into<ElementId>, value: f32) -> Slider {
    Slider {
        id: id.into(),
        value: value.clamp(0.0, 1.0),
        on_change: None,
    }
}

impl Slider {
    /// Called with the new 0–1 value while dragging.
    pub fn on_change(mut self, f: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        // The track's bounds and whether a drag is live, kept between frames.
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            (Bounds::<Pixels>::default(), false)
        });
        let at =
            move |x: Pixels, b: Bounds<Pixels>| ((x - b.left()) / b.size.width).clamp(0.0, 1.0);
        let (s1, s2, s3, s4) = (state.clone(), state.clone(), state.clone(), state.clone());
        let (f1, f2) = (self.on_change.clone(), self.on_change.clone());
        let v = self.value;
        div()
            .id(self.id)
            .relative()
            .h(px(22.))
            .w_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |e, window, cx| {
                let b = s1.read(cx).0;
                s1.update(cx, |s, _| s.1 = true);
                if let Some(f) = &f1 {
                    f(at(e.position.x, b), window, cx);
                }
            })
            .on_mouse_move(move |e: &MouseMoveEvent, window, cx| {
                let (b, dragging) = *s2.read(cx);
                if dragging && e.pressed_button == Some(MouseButton::Left) {
                    if let Some(f) = &f2 {
                        f(at(e.position.x, b), window, cx);
                    }
                } else if dragging {
                    s2.update(cx, |s, _| s.1 = false);
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                s3.update(cx, |s, _| s.1 = false)
            })
            .child(
                canvas(
                    move |bounds, _, cx| s4.update(cx, |s, _| s.0 = bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(t.track)
                    .child(div().h_full().w(relative(v)).rounded_full().bg(t.accent)),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(v))
                    .ml(px(-7.))
                    .size(px(14.))
                    .rounded_full()
                    .bg(t.text)
                    .shadow(t.shadow_pop()),
            )
    }
}
