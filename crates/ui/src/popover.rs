//! Frosted anchored popovers: a `deferred(anchored())` card that blurs the
//! content behind it (frost.rs), snaps to the window, and dismisses on an
//! outside mouse-down or Escape. One anatomy for every menu in the app:
//! optional search row, then rows with a round mark, mono name and key hints.

use gpui::{
    Anchor, App, ClickEvent, Div, IntoElement, Stateful, Window, anchored, deferred, div,
    prelude::*, px,
};

use crate::frost::{self, MENU_BLUR};
use crate::motion;
use crate::theme::Theme;

/// A frosted popover card anchored to its parent element. The caller wraps
/// the trigger element with `.child(menu(...))` inside `.when(open, ...)`.
/// `on_dismiss` fires on outside mouse-down.
pub fn menu<E, F>(
    theme: &Theme,
    width: f32,
    on_dismiss: F,
    children: impl IntoIterator<Item = E>,
) -> impl IntoElement
where
    E: IntoElement,
    F: Fn(&mut Window, &mut App) + 'static,
{
    let card = div()
        .w(px(width))
        .flex()
        .flex_col()
        .rounded_lg()
        .bg(theme.menu)
        .border_1()
        .border_color(theme.border)
        .shadow_lg()
        .p(px(6.))
        .children(children.into_iter().map(IntoElement::into_element))
        .on_mouse_down_out(move |_, window, cx| on_dismiss(window, cx));

    deferred(
        anchored()
            .anchor(Anchor::BottomLeft)
            .snap_to_window_with_margin(px(8.))
            .child(frost::frosted(
                10.0,
                MENU_BLUR,
                theme.frost,
                motion::menu_in("popover", card),
            )),
    )
    .priority(1)
}

/// One popover row: round mark slot, name, optional dim trailing text.
/// `.occlude()` so it stays out of any titlebar drag strip above.
pub fn row(
    theme: &Theme,
    id: impl Into<gpui::ElementId>,
    mark: Option<impl IntoElement>,
    label: impl IntoElement,
    trailing: Option<impl IntoElement>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let mut r = div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .h_7()
        .rounded_md()
        .cursor_pointer()
        .occlude()
        .text_color(theme.text)
        .text_sm()
        .hover(|s| s.bg(theme.accent.opacity(0.12)))
        .on_click(on_click);
    if selected {
        r = r.bg(theme.accent.opacity(0.18));
    }
    if let Some(m) = mark {
        r = r.child(
            div()
                .w_5()
                .h_5()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(m),
        );
    } else {
        r = r.child(div().w_5().h_5().flex_none());
    }
    r = r.child(div().flex_1().overflow_hidden().child(label));
    if let Some(t) = trailing {
        r = r.child(div().text_xs().text_color(theme.text_dim).child(t));
    }
    r
}
