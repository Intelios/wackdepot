//! The app-owned titlebar drag surface (the window is created with
//! `app_owns_titlebar_drag`). A press arms it; the first move with the
//! button held hands off to AppKit's native window drag, so plain hovers and
//! clicks never start one. Double-click zooms, like a native titlebar.
//! Controls placed on it must `.occlude()`.

use gpui::{
    App, Div, ElementId, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent,
    ParentElement, RenderOnce, StatefulInteractiveElement, Styled, Window, WindowControlArea, div,
};

#[derive(IntoElement)]
pub struct DragStrip {
    id: ElementId,
    inner: Div,
}

/// A drag strip wrapping `inner`'s layout and children.
pub fn drag_strip(id: impl Into<ElementId>, inner: Div) -> DragStrip {
    DragStrip {
        id: id.into(),
        inner,
    }
}

impl RenderOnce for DragStrip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let armed = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let (a1, a2, a3, a4) = (armed.clone(), armed.clone(), armed.clone(), armed);
        div()
            .id(self.id)
            .size_full()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                a1.update(cx, |a, _| *a = true);
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                a2.update(cx, |a, _| *a = false);
            })
            .on_mouse_down_out(move |_, _, cx| {
                a3.update(cx, |a, _| *a = false);
            })
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if *a4.read(cx) && event.pressed_button == Some(MouseButton::Left) {
                    a4.update(cx, |a, _| *a = false);
                    window.start_window_move();
                }
            })
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(self.inner.size_full())
    }
}
