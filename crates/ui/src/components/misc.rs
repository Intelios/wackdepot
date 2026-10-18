//! Small pieces: count pills, key caps, banners, empty states, kickers.

use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, px,
};

use crate::components::mark::accent_mark;
use crate::components::seam::dotted_ring;
use crate::icons::icon;
use crate::theme::{ActiveTheme, metrics};

/// A small rounded count (tabular figures).
pub fn count_pill(n: usize, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    div()
        .flex_none()
        .min_w(px(18.))
        .h(px(17.))
        .px(px(6.))
        .rounded_full()
        .bg(t.pill())
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.text_soft)
        .child(n.to_string())
}

/// A key cap, macOS glyphs (⌘⇧⌥⌃).
pub fn kbd(keys: &str, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    div()
        .flex_none()
        .h(px(17.))
        .px(px(5.))
        .rounded(px(5.))
        .bg(t.pill())
        .flex()
        .items_center()
        .text_size(px(10.5))
        .text_color(t.text_dim)
        .child(keys.to_string())
}

/// An uppercase kicker label (card heads, menu headings).
pub fn kicker(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    div()
        .text_size(px(metrics::TEXT_KICKER))
        .font_weight(FontWeight::BOLD)
        .text_color(t.accent_text)
        .child(text.into().to_uppercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Warning,
    Danger,
    Info,
}

/// A rounded banner with a title, a line of copy and actions.
#[derive(IntoElement)]
pub struct Banner {
    tone: Tone,
    title: SharedString,
    body: Option<SharedString>,
    actions: Vec<AnyElement>,
}

pub fn banner(tone: Tone, title: impl Into<SharedString>) -> Banner {
    Banner {
        tone,
        title: title.into(),
        body: None,
        actions: Vec::new(),
    }
}

impl Banner {
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn action(mut self, el: impl IntoElement) -> Self {
        self.actions.push(el.into_any_element());
        self
    }
}

impl RenderOnce for Banner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let (bg, line, ink, glyph) = match self.tone {
            Tone::Warning => (
                t.sem.warning_bg,
                t.sem.warning_line,
                t.sem.warning,
                "conflict",
            ),
            Tone::Danger => (t.sem.danger_bg, t.sem.danger_line, t.sem.danger, "alert"),
            Tone::Info => (t.accent_wash, t.accent_line_soft, t.accent, "spark"),
        };
        div()
            .rounded(px(12.))
            .bg(bg)
            .border_1()
            .border_color(line)
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(icon(glyph).size(px(15.)).mt(px(1.)).text_color(ink))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(ink)
                                    .child(self.title),
                            )
                            .children(self.body.map(|b| {
                                div()
                                    .text_size(px(metrics::TEXT_SMALL))
                                    .text_color(t.text_soft)
                                    .child(b)
                            })),
                    ),
            )
            .when_some((!self.actions.is_empty()).then_some(()), |el, _| {
                el.child(div().flex().gap(px(6.)).pl(px(23.)).children(self.actions))
            })
    }
}

use gpui::prelude::FluentBuilder as _;

/// An empty state: a mark inside a dotted orbit, a line, a sub-line, and
/// optional actions.
#[derive(IntoElement)]
pub struct EmptyState {
    icon: &'static str,
    title: SharedString,
    body: Option<SharedString>,
    actions: Vec<AnyElement>,
    art: Option<AnyElement>,
}

pub fn empty_state(icon: &'static str, title: impl Into<SharedString>) -> EmptyState {
    EmptyState {
        icon,
        title: title.into(),
        body: None,
        actions: Vec::new(),
        art: None,
    }
}

impl EmptyState {
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn action(mut self, el: impl IntoElement) -> Self {
        self.actions.push(el.into_any_element());
        self
    }

    /// Replace the orbit mark with custom art (the mascot).
    pub fn art(mut self, el: impl IntoElement) -> Self {
        self.art = Some(el.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let art = self.art.unwrap_or_else(|| {
            div()
                .relative()
                .size(px(60.))
                .flex()
                .items_center()
                .justify_center()
                .child(div().absolute().inset_0().child(dotted_ring(60.0)))
                .child(accent_mark(self.icon).size(44.))
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .max_w(px(260.))
            .child(art)
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(metrics::TEXT_ROW))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text)
                    .text_center()
                    .child(self.title),
            )
            .children(self.body.map(|b| {
                div()
                    .text_size(px(metrics::TEXT_META))
                    .text_color(t.text_dim)
                    .text_center()
                    .child(b)
            }))
            .when(!self.actions.is_empty(), |el| {
                el.child(div().mt(px(6.)).flex().gap(px(8.)).children(self.actions))
            })
    }
}
