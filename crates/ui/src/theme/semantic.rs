//! Colours with fixed meaning. An accent recolours interaction, never
//! meaning: danger, warning, success, diff and git-status colours are the
//! same in every theme (WackCode's `SEMANTIC_COLOURS`).

use gpui::{Hsla, Rgba};

const fn rgba(hex: u32, a: f32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a,
    }
}

fn c(hex: u32) -> Hsla {
    rgba(hex, 1.0).into()
}

fn ca(hex: u32, a: f32) -> Hsla {
    rgba(hex, a).into()
}

/// A status letter's ink and the round mark it sits on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkColors {
    pub ink: Hsla,
    pub fill: Hsla,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Semantic {
    pub danger: Hsla,
    pub danger_bg: Hsla,
    pub danger_line: Hsla,
    pub danger_text: Hsla,
    pub warning: Hsla,
    pub warning_bg: Hsla,
    pub warning_line: Hsla,
    pub warning_text: Hsla,
    pub success: Hsla,
    /// Diff ink and row tints.
    pub diff_add: Hsla,
    pub diff_del: Hsla,
    pub diff_hunk: Hsla,
    pub diff_add_bg: Hsla,
    pub diff_del_bg: Hsla,
    pub diff_hunk_bg: Hsla,
    /// Word-level emphasis on top of a changed row.
    pub diff_add_word: Hsla,
    pub diff_del_word: Hsla,
    /// Git status marks.
    pub added: MarkColors,
    pub deleted: MarkColors,
    pub renamed: MarkColors,
    pub conflicted: MarkColors,
    /// History graph lanes, cycled by lane colour index.
    pub lanes: [Hsla; 6],
    /// Pure black at an alpha — scrims, recessed tracks, drop shadows.
    pub black: Hsla,
    /// Traffic-light stand-ins are never drawn; this is for the mascot's eyes
    /// and other true-white details.
    pub white: Hsla,
}

impl Semantic {
    pub fn new() -> Self {
        Self {
            danger: c(0xf08080),
            danger_bg: c(0x351d1d),
            danger_line: c(0x5b3030),
            danger_text: c(0xebb1b1),
            warning: c(0xe6ba69),
            warning_bg: c(0x2c2515),
            warning_line: c(0x5c4a22),
            warning_text: c(0xe6d2a6),
            success: c(0xa8d883),
            diff_add: c(0x56d364),
            diff_del: c(0xf47067),
            diff_hunk: c(0x9aaed2),
            diff_add_bg: ca(0x3fb950, 0.28),
            diff_del_bg: ca(0xf85149, 0.26),
            diff_hunk_bg: ca(0x37496f, 0.13),
            diff_add_word: ca(0x56d364, 0.45),
            diff_del_word: ca(0xf85149, 0.45),
            added: MarkColors {
                ink: c(0xa8d883),
                fill: c(0x1d301b),
            },
            deleted: MarkColors {
                ink: c(0xe99191),
                fill: c(0x381f1f),
            },
            renamed: MarkColors {
                ink: c(0x9aaed2),
                fill: c(0x232c3f),
            },
            conflicted: MarkColors {
                ink: c(0xe6ba69),
                fill: c(0x332a17),
            },
            lanes: [
                c(0x9aaed2),
                c(0xd8b08c),
                c(0xc9a3d2),
                c(0xa3c9c0),
                c(0xe3a2a2),
                c(0xe6d2a6),
            ],
            black: c(0x000000),
            white: c(0xffffff),
        }
    }
}

impl Default for Semantic {
    fn default() -> Self {
        Self::new()
    }
}
