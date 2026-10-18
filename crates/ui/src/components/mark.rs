//! Round marks: git status letters, accent marks that lead cards and
//! groups, and repo monograms.

use gpui::{
    App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
};

use crate::icons::icon;
use crate::theme::{ActiveTheme, MarkColors, Theme};

/// The status letter and colours for a change kind.
pub fn status_mark_colors(t: &Theme, letter: char) -> MarkColors {
    match letter {
        'A' | '?' => t.sem.added,
        'D' => t.sem.deleted,
        'R' | 'C' => t.sem.renamed,
        'U' => t.sem.conflicted,
        _ => MarkColors {
            ink: t.sem.warning,
            fill: t.surface_hover,
        },
    }
}

/// A 20px round status letter (A, M, D, R, U).
#[derive(IntoElement)]
pub struct StatusMark {
    letter: char,
    size: f32,
}

pub fn status_mark(letter: char) -> StatusMark {
    StatusMark { letter, size: 20.0 }
}

impl StatusMark {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
}

impl RenderOnce for StatusMark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let c = status_mark_colors(t, self.letter);
        let shown = if self.letter == '?' { 'A' } else { self.letter };
        div()
            .size(px(self.size))
            .flex_none()
            .rounded_full()
            .bg(c.fill)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.size * 0.5))
            .font_weight(FontWeight::BOLD)
            .text_color(c.ink)
            .child(shown.to_string())
    }
}

/// The accent mark that leads a card head, group, or empty state.
#[derive(IntoElement)]
pub struct AccentMark {
    icon: &'static str,
    size: f32,
    live: bool,
}

pub fn accent_mark(name: &'static str) -> AccentMark {
    AccentMark {
        icon: name,
        size: 22.0,
        live: false,
    }
}

impl AccentMark {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }

    /// Brighter while something is happening in its card.
    pub fn live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }
}

impl RenderOnce for AccentMark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        div()
            .size(px(self.size))
            .flex_none()
            .rounded_full()
            .bg(if self.live {
                t.accent_tint_strong
            } else {
                t.accent_tint
            })
            .shadow(vec![t.rim(t.accent.opacity(0.26))])
            .flex()
            .items_center()
            .justify_center()
            .child(
                icon(self.icon)
                    .size(px(self.size * 0.6))
                    .text_color(t.accent),
            )
    }
}

/// A round repo monogram: grey at rest, accent-lit when it has changes,
/// filled for the open repo.
#[derive(IntoElement)]
pub struct Monogram {
    name: SharedString,
    size: f32,
    current: bool,
    lit: bool,
}

pub fn monogram(name: impl Into<SharedString>) -> Monogram {
    Monogram {
        name: name.into(),
        size: 20.0,
        current: false,
        lit: false,
    }
}

impl Monogram {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }

    pub fn current(mut self, c: bool) -> Self {
        self.current = c;
        self
    }

    pub fn lit(mut self, l: bool) -> Self {
        self.lit = l;
        self
    }
}

impl RenderOnce for Monogram {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let letter = self
            .name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "·".into());
        let (bg, fg) = if self.current {
            (t.accent, t.on_accent)
        } else if self.lit {
            (t.accent_tint_strong, t.accent_text)
        } else {
            (t.pill(), t.text_soft)
        };
        let mut el = div()
            .size(px(self.size))
            .flex_none()
            .rounded_full()
            .bg(bg)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.size * 0.5))
            .font_weight(FontWeight::BOLD)
            .text_color(fg)
            .child(letter);
        if self.current {
            el = el.shadow(vec![gpui::BoxShadow {
                color: t.accent.opacity(0.32),
                offset: gpui::point(px(0.), px(0.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
                inset: false,
            }]);
        }
        el
    }
}
