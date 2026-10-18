//! WackDepot theme tokens. Every colour in the app comes from `Theme`; never
//! write a raw `rgb()`/`hsla()` outside this file (AGENTS.md). The accent
//! recolours interaction, never meaning — danger, warning, diff and success
//! stay fixed in every theme.
//!
//! Palette carries WackCode's DNA: lime accent on near-black greens for dark;
//! the light appearance is the same hue family lifted, not a generic white.

use gpui::{Hsla, WindowAppearance, hsla};

/// Vertical space occupied by the traffic-light-inset titlebar strip.
pub const TITLEBAR_HEIGHT: f32 = 38.0;
/// Traffic lights sit at (14, 14) and are ~68px wide including their own
/// padding — keep titlebar content clear of them.
pub const TRAFFIC_LIGHT_INSET: f32 = 78.0;
/// Left rail holding repo picker, tabs and file lists.
pub const SIDEBAR_WIDTH: f32 = 320.0;

/// `const`-friendly hex literal: `rgba(0xRRGGBBAA)` as `Hsla`.
pub fn rgba(hex: u32) -> Hsla {
    gpui::rgba(hex).into()
}

#[derive(Clone, Debug)]
#[allow(clippy::wrong_self_convention)]
pub struct Theme {
    /// Interaction accent — WackCode lime.
    pub accent: Hsla,
    /// Text/icons drawn on an accent fill.
    pub on_accent: Hsla,
    /// Accent at a hairline strength for rims and seams.
    pub accent_dim: Hsla,

    // Shell surfaces (translucent over the window blur).
    /// The titlebar strip — fully clear; the blur does the work.
    pub titlebar: Hsla,
    /// The left rail, translucent so it reads as glass.
    pub sidebar: Hsla,
    /// Popovers/menus tint over their own backdrop blur.
    pub menu: Hsla,

    // Content surfaces (opaque; text stays readable).
    pub well: Hsla,
    pub input: Hsla,
    pub panel: Hsla,
    pub bg: Hsla,
    pub surface: Hsla,
    pub elevated: Hsla,

    // Text ladder.
    pub text: Hsla,
    pub text_soft: Hsla,
    pub text_dim: Hsla,
    pub text_faint: Hsla,

    // Lines.
    pub border: Hsla,
    pub border_bright: Hsla,

    // Fixed meaning — same in every theme.
    pub danger: Hsla,
    pub warning: Hsla,
    pub success: Hsla,
    pub diff_add_bg: Hsla,
    pub diff_add_text: Hsla,
    pub diff_del_bg: Hsla,
    pub diff_del_text: Hsla,
    pub diff_hunk_text: Hsla,
    pub diff_hunk_bg: Hsla,

    /// Whether the window blur is active (always true on macOS; kept as a
    /// token so a future "opaque surfaces" preference has a switch point).
    pub frost: bool,
}

impl Theme {
    pub fn dark() -> Self {
        let accent = rgba(0xc2ee4aff);
        Theme {
            accent,
            on_accent: rgba(0x14200aff),
            accent_dim: accent.opacity(0.4),

            titlebar: hsla(0.0, 0.0, 0.0, 0.0),
            sidebar: rgba(0x14160f99),
            menu: rgba(0x1e221acc),

            well: rgba(0x0c0e0aff),
            input: rgba(0x161a11ff),
            panel: rgba(0x171a12ff),
            bg: rgba(0x111310ff),
            surface: rgba(0x191c13ff),
            elevated: rgba(0x20241aff),

            text: rgba(0xeef0e6ff),
            text_soft: rgba(0xb9bdafff),
            text_dim: rgba(0x878c7dff),
            text_faint: rgba(0x5c6155ff),

            border: rgba(0xf5f7ef17),
            border_bright: rgba(0x3a4229ff),

            danger: rgba(0xf47067ff),
            warning: rgba(0xe8a13cff),
            success: rgba(0x6adb6eff),
            diff_add_bg: rgba(0x50dc6424),
            diff_add_text: rgba(0x7ee787ff),
            diff_del_bg: rgba(0xf05a5021),
            diff_del_text: rgba(0xffa198ff),
            diff_hunk_text: rgba(0x9aa4b0ff),
            diff_hunk_bg: rgba(0x7896ff17),

            frost: true,
        }
    }

    pub fn light() -> Self {
        let accent = rgba(0x7fb32eff);
        Theme {
            accent,
            on_accent: rgba(0xfbffe8ff),
            accent_dim: accent.opacity(0.45),

            titlebar: hsla(0.0, 0.0, 0.0, 0.0),
            sidebar: rgba(0xeef0e6b3),
            menu: rgba(0xf7f8f0d9),

            well: rgba(0xe8eae0ff),
            input: rgba(0xf0f2e8ff),
            panel: rgba(0xecefE3ff),
            bg: rgba(0xf4f5efff),
            surface: rgba(0xfbfbf6ff),
            elevated: rgba(0xffffffff),

            text: rgba(0x1d2015ff),
            text_soft: rgba(0x454a38ff),
            text_dim: rgba(0x6d725cff),
            text_faint: rgba(0x9ba088ff),

            border: rgba(0x1d20151a),
            border_bright: rgba(0x1d20152e),

            danger: rgba(0xcf222eff),
            warning: rgba(0x9a6700ff),
            success: rgba(0x1a7f37ff),
            diff_add_bg: rgba(0x1ea03c24),
            diff_add_text: rgba(0x1a7f37ff),
            diff_del_bg: rgba(0xdc3c3221),
            diff_del_text: rgba(0xcf222eff),
            diff_hunk_text: rgba(0x5b6572ff),
            diff_hunk_bg: rgba(0x4a6bff1f),

            frost: true,
        }
    }

    /// Resolve a theme for a window appearance (vibrant variants use the
    /// matching base appearance).
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Theme::light(),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.text.l > 0.5
    }
}
