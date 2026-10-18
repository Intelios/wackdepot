//! The theme: WackCode's token engine (every colour derived in OKLCH from an
//! accent + background pair), the fixed semantic colours, the glass tint,
//! and the type/metric constants. Nothing outside this module writes a
//! colour literal (see `lint.rs`).

pub mod color;
pub mod fonts;
pub mod metrics;
pub mod presets;
pub mod recipe;
pub mod semantic;
#[cfg(test)]
mod tests;

use std::ops::Deref;
use std::sync::Arc;

use gpui::{App, BoxShadow, Global, Hsla, SharedString, point, px};

pub use recipe::Palette;
pub use semantic::{MarkColors, Semantic};

/// What a theme is resolved from (Settings › Appearance).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeInput {
    pub accent: Option<String>,
    pub background: Option<String>,
    /// Glass tint, 0–90 (percent); the shell's alpha over the glass.
    pub glass_tint: u8,
}

impl Default for ThemeInput {
    fn default() -> Self {
        Self {
            accent: None,
            background: None,
            glass_tint: 40,
        }
    }
}

/// The resolved theme. Derefs to its [`Palette`] so `theme.accent`,
/// `theme.bg`, … read naturally; fixed colours live under `theme.sem`.
#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: Palette,
    pub sem: Semantic,
    /// 0–0.9 — how much of the shell is tint vs. glass.
    pub tint: f32,
    /// Mono family for code, paths, branches and SHAs.
    pub mono: SharedString,
    /// The picked colour was nudged for readability (Settings says so).
    pub accent_adjusted: bool,
    pub background_adjusted: bool,
}

impl Deref for Theme {
    type Target = Palette;
    fn deref(&self) -> &Palette {
        &self.palette
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

impl Theme {
    pub fn resolve(input: &ThemeInput, mono: SharedString) -> Self {
        let r = recipe::resolve(input.accent.as_deref(), input.background.as_deref());
        Self {
            palette: r.palette,
            sem: Semantic::new(),
            tint: f32::from(input.glass_tint.min(90)) / 100.0,
            mono,
            accent_adjusted: r.accent_adjusted,
            background_adjusted: r.background_adjusted,
        }
    }

    // ── Shell over glass. `mix` is 1 with glass showing, 0 when opaque. ──

    /// The whole-window tint.
    pub fn shell(&self, mix: f32) -> Hsla {
        self.bg.opacity(lerp(1.0, self.tint, mix))
    }

    /// The left column, a step denser than the shell.
    pub fn column(&self, mix: f32) -> Hsla {
        self.sidebar
            .opacity(lerp(1.0, (self.tint + 0.15).min(1.0), mix))
    }

    /// Frosted floating surfaces (popovers, menus, toasts) over the in-app blur.
    pub fn frost(&self) -> Hsla {
        self.menu.opacity(0.9)
    }

    // ── Shell fill recipes (WackCode's color-mix tints). ──

    pub fn hover(&self) -> Hsla {
        self.text.opacity(0.06)
    }

    /// Resting neutral pill fill (and its inset rim).
    pub fn pill(&self) -> Hsla {
        self.text.opacity(0.08)
    }

    pub fn pill_hover(&self) -> Hsla {
        self.text.opacity(0.12)
    }

    /// Selected/active: accent 13% with an `accent_line` rim.
    pub fn selected_fill(&self) -> Hsla {
        self.accent.opacity(0.13)
    }

    /// A recessed pill track.
    pub fn recessed(&self) -> Hsla {
        self.sem.black.opacity(0.24)
    }

    pub fn focus_halo(&self) -> Hsla {
        self.accent.opacity(0.10)
    }

    /// Dialog backdrop.
    pub fn scrim(&self) -> Hsla {
        self.sem.black.opacity(0.42)
    }

    /// Floating cards: a soft drop under popovers and dialogs.
    pub fn shadow_pop(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.sem.black.opacity(0.45),
                offset: point(px(0.), px(12.)),
                blur_radius: px(40.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: self.sem.black.opacity(0.3),
                offset: point(px(0.), px(2.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
                inset: false,
            },
        ]
    }

    /// A crisp 1px inner rim in `color` — WackCode's `inset 0 0 0 1px`.
    pub fn rim(&self, color: Hsla) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: true,
        }
    }

    /// The accent glow under a primary pill.
    pub fn accent_glow(&self) -> Vec<BoxShadow> {
        vec![BoxShadow {
            color: self.accent.opacity(0.28),
            offset: point(px(0.), px(3.)),
            blur_radius: px(12.),
            spread_radius: px(0.),
            inset: false,
        }]
    }
}

/// The active theme, as a gpui global.
pub struct ThemeGlobal(pub Arc<Theme>);

impl Global for ThemeGlobal {}

/// `cx.theme()` anywhere an `App` is reachable.
pub trait ActiveTheme {
    fn theme(&self) -> &Arc<Theme>;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Arc<Theme> {
        &self.global::<ThemeGlobal>().0
    }
}

/// Register fonts and install the first theme. Call once at startup.
pub fn init(cx: &mut App, input: &ThemeInput) {
    fonts::register(cx);
    let mono = fonts::mono_family(cx);
    cx.set_global(ThemeGlobal(Arc::new(Theme::resolve(input, mono))));
    crate::native::appearance::set_dark(true);
}

/// Swap in a new theme and repaint every window (cached views included).
pub fn apply(cx: &mut App, input: &ThemeInput) {
    let mono = cx.theme().mono.clone();
    cx.set_global(ThemeGlobal(Arc::new(Theme::resolve(input, mono))));
    cx.refresh_windows();
}
