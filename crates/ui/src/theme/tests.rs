//! The port must reproduce WackCode's palette. `golden.json` was generated
//! by running WackCode's own `src/theme.ts` (`resolveTheme`) for every
//! preset plus one input that trips both readability clamps.

use std::collections::BTreeMap;

use super::color::{from_u32, parse_hex, to_hex};
use super::presets::PRESETS;
use super::recipe::{TOKENS, resolve};

fn rgb_of(h: gpui::Hsla) -> [f64; 3] {
    let c = h.to_rgb();
    [c.r as f64 * 255.0, c.g as f64 * 255.0, c.b as f64 * 255.0]
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1.5)
}

#[test]
fn default_theme_reproduces_the_hand_picked_palette() {
    let r = resolve(None, None);
    for (css, _, hex) in TOKENS {
        let got = rgb_of(r.palette.by_css(css).unwrap());
        assert!(
            close(got, from_u32(*hex)),
            "{css}: got {}, want #{hex:06x}",
            to_hex(got)
        );
    }
}

#[test]
fn matches_wackcode_for_every_preset_and_the_clamps() {
    let golden: BTreeMap<String, BTreeMap<String, String>> =
        serde_json::from_str(include_str!("golden.json")).unwrap();
    let mut inputs: Vec<(&str, &str, &str)> = PRESETS
        .iter()
        .map(|p| (p.id, p.accent, p.background))
        .collect();
    inputs.push(("clamped", "#203040", "#6a6a70"));

    for (id, accent, background) in inputs {
        let r = resolve(Some(accent), Some(background));
        let want = &golden[id];
        let mut checked = 0;
        for (css, hex) in want {
            let Some(got) = r.palette.by_css(css) else {
                continue;
            };
            let got = rgb_of(got);
            let want = parse_hex(hex).unwrap();
            assert!(
                close(got, want),
                "{id} {css}: got {}, want {hex}",
                to_hex(got)
            );
            checked += 1;
        }
        assert_eq!(checked, TOKENS.len() + 1, "{id}: every token compared");
    }
    let clamped = resolve(Some("#203040"), Some("#6a6a70"));
    assert!(clamped.background_adjusted && clamped.accent_adjusted);
}
