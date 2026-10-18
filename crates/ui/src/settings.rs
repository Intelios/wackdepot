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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    /// User-set accent, reserved for a future appearance picker. Hex RRGGBB.
    #[serde(default)]
    pub accent_hex: Option<String>,
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
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn settings_path() -> PathBuf {
    support_dir().join("wackdepot.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
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
    }
}
