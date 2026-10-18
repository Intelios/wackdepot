//! Persistence: `~/Library/Application Support/WackDepot/wackdepot.json`.
//! Every field is serde-defaulted so older files load forward-compatibly;
//! writes go through `save` which writes atomically (tmp + rename).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Appearance override: `System` follows macOS, the others pin a variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppearanceMode {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Repositories the user added, in sidebar order (paths, not copies).
    #[serde(default)]
    pub repos: Vec<PathBuf>,
    /// The last repo shown, restored on launch.
    #[serde(default)]
    pub last_repo: Option<PathBuf>,
    #[serde(default)]
    pub appearance: AppearanceMode,
    /// Preferred editor for "Open in Editor" (macOS app name, e.g. "WackCode").
    #[serde(default)]
    pub editor_app: Option<String>,
    /// Window geometry (points) restored between launches.
    #[serde(default)]
    pub window: Option<WindowGeo>,
    /// Accent colour (`#rrggbb`); `None` is the WackDepot lime.
    #[serde(default)]
    pub accent_hex: Option<String>,
    /// Background colour (`#rrggbb`); `None` is the default olive-black.
    #[serde(default)]
    pub background_hex: Option<String>,
    /// What sits behind the window: native glass or a solid colour.
    #[serde(default)]
    pub backdrop: Backdrop,
    /// How much of the shell is tint over the glass, 0–90.
    #[serde(default = "default_glass_tint")]
    pub glass_tint: u8,
    /// Reduce motion: follow macOS, or always on.
    #[serde(default)]
    pub reduce_motion: MotionPref,
    /// Left column width (points), when the user has dragged it.
    #[serde(default)]
    pub sidebar_width: Option<f32>,
    /// Side-by-side diffs instead of unified.
    #[serde(default)]
    pub diff_split: bool,
    /// Hide whitespace-only changes in diffs.
    #[serde(default)]
    pub ignore_whitespace: bool,
}

fn default_glass_tint() -> u8 {
    40
}

impl Default for Settings {
    fn default() -> Self {
        // Same values serde fills in for missing keys.
        serde_json::from_str("{}").expect("empty settings parse")
    }
}

/// Window backdrop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backdrop {
    #[default]
    Glass,
    Solid,
}

/// Reduce-motion preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionPref {
    #[default]
    System,
    Always,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct WindowGeo {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn support_dir() -> PathBuf {
    dirs_base().join("Application Support").join("WackDepot")
}

fn dirs_base() -> PathBuf {
    // macOS app support lives under ~/Library — plain HOME is the home dir,
    // not the Library root.
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join("Library"))
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn settings_path() -> PathBuf {
    support_dir().join("wackdepot.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Atomic write: tmp file + rename so a crash mid-save can't corrupt it.
pub fn save(settings: &Settings) {
    let dir = support_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = settings_path();
    let tmp = dir.join("wackdepot.json.tmp");
    let Ok(text) = serde_json::to_string_pretty(settings) else {
        return;
    };
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_and_missing_keys_load() {
        // Forward-compat: a newer file with extra keys still parses.
        let s: Settings = serde_json::from_str(r#"{"repos":[],"future":42}"#).unwrap();
        assert!(s.repos.is_empty());
        // Missing keys default.
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.appearance, AppearanceMode::System);
        assert_eq!(s.glass_tint, 40);
        assert_eq!(s.backdrop, Backdrop::Glass);
    }
}
