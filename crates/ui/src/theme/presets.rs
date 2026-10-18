//! One-click accent + background pairs (Settings › Appearance) — the same
//! six WackCode ships, so the two apps can be themed to match.

/// A named theme preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub accent: &'static str,
    pub background: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "wackcode",
        name: "WackDepot",
        accent: "#c2ee4a",
        background: "#111310",
    },
    Preset {
        id: "midnight",
        name: "Midnight",
        accent: "#6cc4ff",
        background: "#0f1218",
    },
    Preset {
        id: "grape",
        name: "Grape",
        accent: "#b69cff",
        background: "#14111b",
    },
    Preset {
        id: "rose",
        name: "Rosé",
        accent: "#ff8fb8",
        background: "#181114",
    },
    Preset {
        id: "ember",
        name: "Ember",
        accent: "#ff9a5c",
        background: "#17120f",
    },
    Preset {
        id: "mono",
        name: "Mono",
        accent: "#e6e6e6",
        background: "#111111",
    },
];

impl Preset {
    /// `(background, accent)` for drawing the preset's swatch.
    pub fn swatch(&self) -> (gpui::Hsla, gpui::Hsla) {
        let c = |hex: &str| {
            super::color::to_hsla(super::color::parse_hex(hex).unwrap_or_default(), 1.0)
        };
        (c(self.background), c(self.accent))
    }
}
