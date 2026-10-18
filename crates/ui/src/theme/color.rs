//! Colour maths for the theme engine — a straight port of WackCode's
//! `src/theme.ts` (OKLab/OKLCH with Ottosson's toe-corrected lightness), so
//! the same `(accent, background)` pair yields the same palette in both apps.

use gpui::{Hsla, Rgba};

/// sRGB channels, 0–255 (fractional while computing).
pub type Rgb = [f64; 3];

/// OKLCH with toe-corrected lightness `l` (0–1), chroma `c`, hue `h` in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// `#rrggbb` / `#rgb` (the `#` optional) to channels.
pub fn parse_hex(hex: &str) -> Option<Rgb> {
    let digits = hex.trim().trim_start_matches('#');
    let full: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_string(),
        _ => return None,
    };
    let channel = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok().map(f64::from);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// `0xrrggbb` to channels.
pub const fn from_u32(hex: u32) -> Rgb {
    [
        ((hex >> 16) & 0xff) as f64,
        ((hex >> 8) & 0xff) as f64,
        (hex & 0xff) as f64,
    ]
}

/// Round and clamp each channel to an integer 0–255 — what `toHex` does.
pub fn round(rgb: Rgb) -> Rgb {
    rgb.map(|c| c.clamp(0.0, 255.0).round())
}

pub fn to_hex(rgb: Rgb) -> String {
    let [r, g, b] = round(rgb);
    format!("#{:02x}{:02x}{:02x}", r as u8, g as u8, b as u8)
}

pub fn to_hsla(rgb: Rgb, alpha: f32) -> Hsla {
    let [r, g, b] = round(rgb);
    Hsla::from(Rgba {
        r: (r / 255.0) as f32,
        g: (g / 255.0) as f32,
        b: (b / 255.0) as f32,
        a: alpha,
    })
}

fn to_linear(channel: f64) -> f64 {
    let v = channel / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f64) -> f64 {
    let encoded = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    encoded * 255.0
}

fn rgb_to_oklab([r, g, b]: Rgb) -> [f64; 3] {
    let (r, g, b) = (to_linear(r), to_linear(g), to_linear(b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    ]
}

/// Unclamped linear sRGB; channels outside 0–1 mean out of gamut.
fn oklab_to_linear([lightness, a, b]: [f64; 3]) -> [f64; 3] {
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
}

// Ottosson's toe: OKLab lightness is too compressed near black for the small
// offsets dark surfaces need, so every lightness here is toe-corrected.
const K1: f64 = 0.206;
const K2: f64 = 0.03;
const K3: f64 = (1.0 + K1) / (1.0 + K2);

fn toe(l: f64) -> f64 {
    0.5 * (K3 * l - K1 + ((K3 * l - K1).powi(2) + 4.0 * K2 * K3 * l).sqrt())
}

fn untoe(l: f64) -> f64 {
    (l * l + K1 * l) / (K3 * (l + K2))
}

pub fn to_lch(rgb: Rgb) -> Lch {
    let [l, a, b] = rgb_to_oklab(rgb);
    Lch {
        l: toe(l),
        c: a.hypot(b),
        h: b.atan2(a).to_degrees(),
    }
}

/// OKLCH to sRGB, lowering chroma until the colour fits the gamut.
pub fn from_lch(Lch { l, c, h }: Lch) -> Rgb {
    let lightness = untoe(l.clamp(0.0, 1.0));
    let radians = h.to_radians();
    let mut chroma = c.max(0.0);
    let encode = |lin: [f64; 3]| lin.map(|v| from_linear(v.clamp(0.0, 1.0)));
    for _ in 0..24 {
        let lin = oklab_to_linear([lightness, chroma * radians.cos(), chroma * radians.sin()]);
        if lin.iter().all(|v| (-0.0005..=1.0005).contains(v)) {
            return encode(lin);
        }
        chroma *= 0.85;
    }
    encode(oklab_to_linear([lightness, 0.0, 0.0]))
}

pub fn luminance([r, g, b]: Rgb) -> f64 {
    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// WCAG 2 contrast ratio between two opaque colours.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (luminance(round(a)), luminance(round(b)));
    let (light, dark) = if x > y { (x, y) } else { (y, x) };
    (light + 0.05) / (dark + 0.05)
}
