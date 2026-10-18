//! Frosted popovers and menus: anchored, blurred, a quick pop-in, rows with
//! 9px corners and an accent wash on what's in use.

use gpui::{
    Anchor, AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, ParentElement,
    RenderOnce, SharedString, Styled, Window, anchored, deferred, div, prelude::*, px,
};

use crate::components::frost::frosted;
use crate::icons::icon;
use crate::motion::{EASE, Motion};
use crate::theme::{ActiveTheme, metrics};

type Handler = Box<dyn Fn(&mut Window, &mut App) + 'static>;
type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A floating card anchored at its parent's corner. Put it as a child of
/// the trigger (the trigger needs `.relative()`).
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    width: f32,
    corner: Anchor,
    content: Vec<AnyElement>,
    on_dismiss: Option<Handler>,
    pad: f32,
}

pub fn popover(id: impl Into<ElementId>) -> Popover {
    Popover {
        id: id.into(),
        width: 280.0,
        corner: Anchor::TopLeft,
        content: Vec::new(),
        on_dismiss: None,
        pad: 5.0,
    }
}

impl Popover {
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Which corner of the card attaches to the anchor point (default:
    /// top-left, opening down and right).
    pub fn corner(mut self, c: Anchor) -> Self {
        self.corner = c;
        self
    }

    pub fn padding(mut self, p: f32) -> Self {
        self.pad = p;
        self
    }

    pub fn child(mut self, el: impl IntoElement) -> Self {
        self.content.push(el.into_any_element());
        self
    }

    pub fn children(mut self, els: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.content
            .extend(els.into_iter().map(|e| e.into_any_element()));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Popover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let motion = window.use_keyed_state(self.id.clone(), cx, |_, _| Motion::default());
        let mut frame = false;
        let shown = motion.update(cx, |m, cx| {
            m.begin(cx);
            let v = m.tween_from(("in", 0), 0.0, 1.0, 140.0, 0.0, EASE);
            frame = m.end_frame();
            v
        });
        if frame {
            window.request_animation_frame();
        }
        let rise = (1.0 - shown) * 4.0;
        let dismiss = self.on_dismiss;
        let opens_up = matches!(self.corner, Anchor::BottomLeft | Anchor::BottomRight);
        let card = div()
            .id(self.id)
            .w(px(self.width))
            .p(px(self.pad))
            .rounded(px(metrics::RADIUS_POPOVER))
            .bg(t.frost())
            .border_1()
            .border_color(t.border_bright.opacity(0.7))
            .shadow(t.shadow_pop())
            .flex()
            .flex_col()
            .gap(px(1.))
            .text_size(px(metrics::TEXT_ROW))
            .occlude()
            .on_mouse_down_out(move |_, w, cx| {
                if let Some(f) = &dismiss {
                    f(w, cx);
                }
            })
            .children(self.content);
        deferred(
            anchored()
                .anchor(self.corner)
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .relative()
                        .top(px(if opens_up { -6.0 + rise } else { 6.0 - rise }))
                        .opacity(shown.clamp(0.0, 1.0))
                        .child(frosted(
                            metrics::RADIUS_POPOVER,
                            metrics::FROST_BLUR * shown.clamp(0.0, 1.0),
                            card,
                        )),
                ),
        )
        .priority(2)
    }
}

/// A menu row: icon, label, optional hint, accent wash when selected.
#[derive(IntoElement)]
pub struct MenuRow {
    id: ElementId,
    icon: Option<&'static str>,
    lead: Option<AnyElement>,
    label: SharedString,
    mono: bool,
    hint: Option<SharedString>,
    trailing: Option<AnyElement>,
    selected: bool,
    danger: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

pub fn menu_row(id: impl Into<ElementId>, label: impl Into<SharedString>) -> MenuRow {
    MenuRow {
        id: id.into(),
        icon: None,
        lead: None,
        label: label.into(),
        mono: false,
        hint: None,
        trailing: None,
        selected: false,
        danger: false,
        disabled: false,
        on_click: None,
    }
}

impl MenuRow {
    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn lead(mut self, el: impl IntoElement) -> Self {
        self.lead = Some(el.into_any_element());
        self
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn hint(mut self, h: impl Into<SharedString>) -> Self {
        self.hint = Some(h.into());
        self
    }

    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
    }

    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for MenuRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let ink = if self.danger {
            t.sem.danger
        } else if self.selected {
            t.text
        } else {
            t.text_body
        };
        let mut row = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(32.))
            .px(px(9.))
            .rounded(px(metrics::RADIUS_MENU_ROW))
            .text_color(ink);
        if self.selected {
            row = row
                .bg(t.selected_fill())
                .shadow(vec![t.rim(t.accent_line_soft)]);
        }
        if let Some(lead) = self.lead {
            row = row.child(lead);
        } else if let Some(name) = self.icon {
            row = row.child(icon(name).size(px(15.)).text_color(if self.danger {
                t.sem.danger
            } else if self.selected {
                t.accent
            } else {
                t.text_dim
            }));
        }
        let mut label = div().flex_1().min_w_0().truncate().child(self.label);
        if self.mono {
            label = label.font_family(t.mono.clone()).text_size(px(12.5));
        }
        row = row.child(label);
        if let Some(h) = self.hint {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(t.text_faint)
                    .child(h),
            );
        }
        if let Some(tr) = self.trailing {
            row = row.child(tr);
        }
        if self.disabled {
            return row.opacity(0.48);
        }
        let hover = if self.danger {
            t.sem.danger_bg
        } else {
            t.hover()
        };
        row = row.cursor_pointer().hover(move |s| s.bg(hover));
        if let Some(f) = self.on_click {
            row = row.on_click(move |e, w, cx| {
                cx.stop_propagation();
                f(e, w, cx)
            });
        }
        row
    }
}

pub fn menu_separator(cx: &App) -> impl IntoElement {
    div().h(px(1.)).mx(px(4.)).my(px(5.)).bg(cx.theme().border)
}

pub fn menu_heading(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    div()
        .px(px(9.))
        .pt(px(6.))
        .pb(px(4.))
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .text_color(t.text_faint)
        .child(text.into().to_uppercase())
}
