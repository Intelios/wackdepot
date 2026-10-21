//! Pills and circles — everything pressable is one or the other.

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::*, px,
};

use crate::icons::icon;
use crate::theme::{ActiveTheme, metrics};

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Accent fill — one per view, for the main action.
    Primary,
    /// Neutral pill (the default).
    Secondary,
    /// Text only until hovered.
    Ghost,
    /// Destructive.
    Danger,
    /// Accent-tinted: the selected/lit state of a pill.
    Lit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    fn height(self) -> f32 {
        match self {
            Size::Small => 24.0,
            Size::Medium => 28.0,
            Size::Large => 34.0,
        }
    }
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<&'static str>,
    trailing: Option<AnyElement>,
    variant: Variant,
    size: Size,
    circle: bool,
    disabled: bool,
    full: bool,
    mono: bool,
    on_click: Option<OnClick>,
}

/// A pill with a label.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button {
        id: id.into(),
        label: Some(label.into()),
        icon: None,
        trailing: None,
        variant: Variant::Secondary,
        size: Size::Medium,
        circle: false,
        disabled: false,
        full: false,
        mono: false,
        on_click: None,
    }
}

/// A round icon button.
pub fn icon_button(id: impl Into<ElementId>, name: &'static str) -> Button {
    Button {
        id: id.into(),
        label: None,
        icon: Some(name),
        trailing: None,
        variant: Variant::Ghost,
        size: Size::Medium,
        circle: true,
        disabled: false,
        full: false,
        mono: false,
        on_click: None,
    }
}

impl Button {
    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
    }

    pub fn variant(mut self, v: Variant) -> Self {
        self.variant = v;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(Variant::Primary)
    }

    pub fn ghost(self) -> Self {
        self.variant(Variant::Ghost)
    }

    pub fn danger(self) -> Self {
        self.variant(Variant::Danger)
    }

    pub fn lit(self, lit: bool) -> Self {
        if lit {
            self.variant(Variant::Lit)
        } else {
            self
        }
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    pub fn small(self) -> Self {
        self.size(Size::Small)
    }

    pub fn large(self) -> Self {
        self.size(Size::Large)
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full = true;
        self
    }

    /// Label in the mono face (branch names, SHAs).
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let h = self.size.height();
        let text_size = match self.size {
            Size::Small => metrics::TEXT_SMALL,
            Size::Medium => 12.5,
            Size::Large => metrics::TEXT_ROW,
        };
        let icon_size = match self.size {
            Size::Small => 13.0,
            Size::Medium => 15.0,
            Size::Large => 16.0,
        };
        let (bg, fg, rim, hover_bg, hover_fg) = match self.variant {
            Variant::Primary => (t.accent, t.on_accent, None, t.accent_hover, t.on_accent),
            Variant::Secondary => (
                t.pill(),
                t.text_soft,
                Some(t.pill()),
                t.pill_hover(),
                t.text,
            ),
            Variant::Ghost => (t.pill().opacity(0.), t.text_soft, None, t.hover(), t.text),
            Variant::Danger => (
                t.pill().opacity(0.),
                t.sem.danger,
                Some(t.sem.danger_line),
                t.sem.danger_bg,
                t.sem.danger,
            ),
            Variant::Lit => (
                t.selected_fill(),
                t.accent_text,
                Some(t.accent_line),
                t.accent.opacity(0.18),
                t.text,
            ),
        };
        let mut el = div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .h(px(h))
            .rounded_full()
            .text_size(px(text_size))
            .font_weight(FontWeight::MEDIUM)
            .bg(bg)
            .text_color(fg);
        // No `.occlude()` here: a button that blocks hitboxes painted
        // behind it also un-hovers a `group_hover` parent — hover pills
        // would vanish under the cursor. Floating surfaces that need to
        // block (popovers, dialog scrims, pills) occlude themselves.
        if self.circle {
            el = el.w(px(h));
        } else {
            el = el.px(px(if self.size == Size::Large { 16. } else { 11. }));
        }
        if self.full {
            el = el.w_full();
        }
        if self.mono {
            el = el.font_family(t.mono.clone());
        }
        let mut shadows = Vec::new();
        if let Some(rim) = rim {
            shadows.push(t.rim(rim));
        }
        if self.variant == Variant::Primary && !self.disabled {
            shadows.extend(t.accent_glow());
        }
        el = el.shadow(shadows);
        if let Some(name) = self.icon {
            el = el.child(icon(name).size(px(icon_size)).text_color(fg));
        }
        if let Some(label) = self.label {
            el = el.child(div().whitespace_nowrap().child(label));
        }
        if let Some(trailing) = self.trailing {
            el = el.child(trailing);
        }
        if self.disabled {
            return el.opacity(0.48).cursor_default();
        }
        el = el
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
            .active(|s| s.opacity(0.82));
        if let Some(f) = self.on_click {
            el = el.on_click(move |e, w, cx| {
                cx.stop_propagation();
                f(e, w, cx)
            });
        }
        el
    }
}
