//! Token recipes — WackCode's `TOKENS` table and `derive()`, ported. Every
//! themeable colour is anchored on its hand-picked default (`ref`) and
//! re-derived from the user's accent and background, so the default theme
//! reproduces the original palette exactly and every preset stays coherent.

use gpui::Hsla;

use super::color::{self, Lch, Rgb, contrast, from_lch, from_u32, luminance, round, to_lch};

pub const DEFAULT_ACCENT: u32 = 0xc2ee4a;
pub const DEFAULT_BACKGROUND: u32 = 0x111310;
/// Backgrounds darken until dim text reads at this ratio.
const MIN_DIM_TEXT_CONTRAST: f64 = 3.8;
/// The accent doubles as a text colour, so it must read on the background.
const MIN_ACCENT_CONTRAST: f64 = 4.5;
const NEAR_WHITE: u32 = 0xf7f8f5;
const SYNTAX_HUE_LEAN: f64 = 0.18;
const SYNTAX_CHROMA_FLOOR: f64 = 0.35;

/// How a token follows the user's colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    /// Keeps `ref`'s lightness offset from the background; hue and tint follow it.
    Surface,
    /// Keeps `ref`'s absolute lightness.
    Text,
    /// Sits between background and accent in lightness, in the accent's hue.
    Accent,
    /// Dark ink drawn on the accent.
    OnAccent,
    /// Code ink: own hue slot, leaning towards the accent.
    Syntax,
}

macro_rules! tokens {
    ($($field:ident = $recipe:ident $hex:literal $css:literal;)*) => {
        /// Every derived colour (plus the accent itself), resolved for one
        /// `(accent, background)` pair.
        #[derive(Debug, Clone, PartialEq)]
        pub struct Palette {
            pub accent: Hsla,
            $(pub $field: Hsla,)*
        }

        /// `(css name, recipe, default hex)` — the CSS names tie each token
        /// back to WackCode's stylesheet (and the golden test).
        pub const TOKENS: &[(&str, Recipe, u32)] = &[$(($css, Recipe::$recipe, $hex),)*];

        impl Palette {
            fn build(accent: Hsla, mut f: impl FnMut(&str, Recipe, u32) -> Hsla) -> Self {
                Self { accent, $($field: f($css, Recipe::$recipe, $hex),)* }
            }

            /// The colour behind a WackCode CSS variable name.
            pub fn by_css(&self, css: &str) -> Option<Hsla> {
                match css {
                    "--wc-accent" => Some(self.accent),
                    $($css => Some(self.$field),)*
                    _ => None,
                }
            }
        }
    };
}

tokens! {
    // Surfaces, darkest to lightest.
    bg = Surface 0x111310 "--bg";
    sidebar = Surface 0x0d0f0c "--wc-sidebar";
    well = Surface 0x0c0e0b "--wc-well";
    panel = Surface 0x10120f "--wc-panel";
    input = Surface 0x0e100d "--wc-input";
    surface = Surface 0x161914 "--surface";
    sidebar_hover = Surface 0x171a15 "--wc-sidebar-hover";
    composer = Surface 0x181b16 "--wc-composer";
    elevated = Surface 0x1a1e17 "--wc-elevated";
    surface_raised = Surface 0x1b1f19 "--surface-raised";
    menu = Surface 0x20241c "--wc-menu";
    code_bg = Surface 0x20251d "--wc-code-bg";
    selected = Surface 0x21261d "--wc-selected";
    surface_hover = Surface 0x22271f "--surface-hover";
    track = Surface 0x242a20 "--wc-track";
    toggle = Surface 0x292d26 "--wc-toggle";
    border = Surface 0x2b3028 "--border";
    code_border = Surface 0x30382a "--wc-code-border";
    border_bright = Surface 0x3a4135 "--border-bright";
    // Text.
    text = Text 0xe8ece4 "--text";
    text_body = Text 0xdbe0d7 "--wc-text-body";
    code_heading = Text 0xd3dacd "--wc-code-heading";
    code_text = Text 0xcad1c5 "--wc-code-text";
    text_soft = Text 0xadb5a7 "--text-soft";
    knob = Text 0x7b8275 "--wc-knob";
    text_dim = Text 0x757e70 "--text-dim";
    placeholder = Text 0x6d7467 "--wc-placeholder";
    text_faint = Text 0x5d6458 "--text-faint";
    // Accent family.
    accent_hover = Accent 0xd0f664 "--wc-accent-hover";
    accent_text = Accent 0xcce98a "--wc-accent-text";
    accent_code = Accent 0xd2e4a3 "--wc-accent-code";
    accent_glint = Accent 0xf7ffcf "--wc-accent-glint";
    accent_wash = Accent 0x151a10 "--wc-accent-wash";
    accent_tint = Accent 0x1b2114 "--wc-accent-tint";
    accent_tint_strong = Accent 0x222b18 "--wc-accent-tint-strong";
    toggle_on = Accent 0x34451e "--wc-toggle-on";
    accent_ring = Accent 0x3c4b2d "--wc-accent-ring";
    accent_line_soft = Accent 0x43511f "--wc-accent-line-soft";
    accent_line_mid = Accent 0x506134 "--wc-accent-line-mid";
    accent_line = Accent 0x58752d "--wc-accent-line";
    focus_line = Accent 0x596649 "--wc-focus-line";
    input_focus = Accent 0x5f6d4e "--wc-input-focus";
    // Syntax (keywords carry the accent itself).
    code_keyword = Accent 0xd2e4a3 "--wc-code-keyword";
    code_string = Syntax 0xd8b08c "--wc-code-string";
    code_number = Syntax 0xc9a3d2 "--wc-code-number";
    code_title = Syntax 0x9fc7d8 "--wc-code-title";
    code_meta = Syntax 0xa3c9c0 "--wc-code-meta";
    code_comment = Syntax 0x7d8674 "--wc-code-comment";
    on_accent = OnAccent 0x172000 "--wc-on-accent";
}

fn default_bg() -> Lch {
    to_lch(from_u32(DEFAULT_BACKGROUND))
}

fn default_accent() -> Lch {
    to_lch(from_u32(DEFAULT_ACCENT))
}

/// Shortest signed rotation from `from` to `to`, in degrees (−180…180).
fn angle_delta(from: f64, to: f64) -> f64 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// Apply `recipe` to a token whose default is `r`.
pub fn derive(recipe: Recipe, r: Lch, background: Lch, accent: Lch) -> Lch {
    let (dbg, dacc) = (default_bg(), default_accent());
    match recipe {
        Recipe::Syntax => {
            let scale = SYNTAX_CHROMA_FLOOR
                + (1.0 - SYNTAX_CHROMA_FLOOR) * (accent.c / dacc.c).clamp(0.0, 1.0);
            Lch {
                l: r.l,
                c: r.c * scale,
                h: r.h + SYNTAX_HUE_LEAN * angle_delta(accent.h, dacc.h),
            }
        }
        Recipe::Surface | Recipe::Text => {
            // Tint beyond the background's own chroma fades out towards grey.
            let tint = (background.c / dbg.c).min(1.0);
            Lch {
                l: if recipe == Recipe::Text {
                    r.l
                } else {
                    background.l + (r.l - dbg.l)
                },
                c: background.c + (r.c - dbg.c) * tint,
                h: background.h + (r.h - dbg.h),
            }
        }
        Recipe::Accent | Recipe::OnAccent => {
            let chroma = if dacc.c > 0.0 { r.c / dacc.c } else { 0.0 };
            let hue = accent.h + (r.h - dacc.h);
            if recipe == Recipe::OnAccent {
                return Lch {
                    l: r.l,
                    c: accent.c * chroma,
                    h: hue,
                };
            }
            let position = (r.l - dbg.l) / (dacc.l - dbg.l);
            Lch {
                l: background.l + (accent.l - background.l) * position,
                c: accent.c * chroma,
                h: hue,
            }
        }
    }
}

/// The user's background, darkened if needed so dim text stays readable on it.
pub fn clamp_background(rgb: Rgb) -> Rgb {
    let rgb = round(rgb);
    let lch = to_lch(rgb);
    let dim = to_lch(from_u32(0x757e70));
    let dacc = default_accent();
    let mut candidate = rgb;
    let mut l = lch.l;
    while l > 0.0 {
        candidate = round(from_lch(Lch { l, ..lch }));
        let derived_dim = round(from_lch(derive(Recipe::Text, dim, to_lch(candidate), dacc)));
        // Dark theme only: the background must also be the darker of the two.
        if luminance(candidate) < luminance(derived_dim)
            && contrast(derived_dim, candidate) >= MIN_DIM_TEXT_CONTRAST
        {
            break;
        }
        l -= 0.005;
    }
    // The first step re-encodes the colour; a one-unit drift counts as unchanged.
    let unchanged = candidate.iter().zip(rgb).all(|(a, b)| (a - b).abs() <= 1.0);
    if unchanged { rgb } else { candidate }
}

/// The user's accent, lightened until it reads as text on `background`.
pub fn readable_accent(rgb: Rgb, background: Rgb) -> Rgb {
    let mut candidate = round(rgb);
    let lch = to_lch(candidate);
    let mut l = lch.l;
    while contrast(candidate, background) < MIN_ACCENT_CONTRAST && l < 1.0 {
        candidate = round(from_lch(Lch { l, ..lch }));
        l += 0.01;
    }
    candidate
}

/// A resolved palette plus what it was resolved from.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub palette: Palette,
    /// Accent and background actually used (after the readability clamps).
    pub accent: Rgb,
    pub background: Rgb,
    pub accent_adjusted: bool,
    pub background_adjusted: bool,
}

/// Resolve every token for `accent` on `background` (both `#rrggbb`; invalid
/// or missing values fall back to the WackCode defaults).
pub fn resolve(accent: Option<&str>, background: Option<&str>) -> Resolved {
    let picked_bg = background
        .and_then(color::parse_hex)
        .unwrap_or(from_u32(DEFAULT_BACKGROUND));
    let bg = clamp_background(picked_bg);
    let picked_accent = accent
        .and_then(color::parse_hex)
        .unwrap_or(from_u32(DEFAULT_ACCENT));
    let acc = readable_accent(picked_accent, bg);
    let (bg_lch, acc_lch) = (to_lch(bg), to_lch(acc));

    let mut on_accent = None;
    let mut palette = Palette::build(color::to_hsla(acc, 1.0), |css, recipe, hex| {
        let rgb = round(from_lch(derive(
            recipe,
            to_lch(from_u32(hex)),
            bg_lch,
            acc_lch,
        )));
        if recipe == Recipe::OnAccent {
            on_accent = Some(rgb);
        }
        // The background itself is exact, not a round trip through OKLCH.
        let rgb = if css == "--bg" { bg } else { rgb };
        color::to_hsla(rgb, 1.0)
    });
    // Mid-tone accents may not reach 4.5:1 with either ink; take the better.
    let near_white = from_u32(NEAR_WHITE);
    if let Some(ink) = on_accent
        && contrast(near_white, acc) > contrast(ink, acc)
    {
        palette.on_accent = color::to_hsla(near_white, 1.0);
    }
    Resolved {
        palette,
        accent: acc,
        background: bg,
        accent_adjusted: acc != round(picked_accent),
        background_adjusted: bg != round(picked_bg),
    }
}
