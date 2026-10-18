//! The window shell: titlebar drag strips, glass rail, content surface.
//! M0 layout — real views land in later milestones.

use gpui::{
    Context, MouseButton, Render, Stateful, Styled, Window, WindowControlArea, div, prelude::*,
    px,
};

use crate::theme::{self, Theme};

pub struct Shell {
    /// Armed by mouse-down on a drag strip; the next mouse-move hands the
    /// drag to AppKit's native session (zeron pattern — keeps double-click
    /// zoom working natively).
    titlebar_should_move: bool,
}

impl Shell {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Shell {
            titlebar_should_move: false,
        }
    }

    /// A titlebar drag surface. Controls placed inside it must `.occlude()`
    /// themselves so they stay out of the drag hit-test and event surface.
    fn drag_strip(
        &self,
        id: &'static str,
        el: gpui::Div,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        el.id(id)
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.titlebar_should_move = false))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.titlebar_should_move = false),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.titlebar_should_move = true),
            )
            // Hand the drag to AppKit's native session only while the button
            // is actually held — a mere hover must never start a native drag
            // (which would resolve a second click as system zoom).
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, _| {
                    if this.titlebar_should_move && event.pressed_button == Some(MouseButton::Left)
                    {
                        this.titlebar_should_move = false;
                        window.start_window_move();
                    }
                }),
            )
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());

        let rail_titlebar = self.drag_strip(
            "rail-titlebar",
            div()
                .h(px(theme::TITLEBAR_HEIGHT))
                .w_full()
                .flex_none()
                .pl(px(theme::TRAFFIC_LIGHT_INSET)),
            cx,
        );

        let content_titlebar = self.drag_strip(
            "content-titlebar",
            div()
                .h(px(theme::TITLEBAR_HEIGHT))
                .w_full()
                .flex_none()
                .flex()
                .items_center()
                .px_3()
                .border_b_1()
                .border_color(theme.border)
                .text_color(theme.text_soft)
                .text_sm()
                .child("WackDepot"),
            cx,
        );

        let rail = div()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(rail_titlebar)
            .child(
                div()
                    .flex_1()
                    .p_3()
                    .text_color(theme.text_dim)
                    .child("WackDepot — glass rail"),
            );

        let content = div()
            .flex_1()
            .h_full()
            .bg(theme.bg)
            .flex()
            .flex_col()
            .child(content_titlebar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_dim)
                    .child("frosted hello"),
            );

        div()
            .size_full()
            .font_family(".SystemUIFont")
            .text_color(theme.text)
            .flex()
            .child(rail)
            .child(content)
    }
}
