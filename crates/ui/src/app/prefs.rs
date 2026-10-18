//! The user's settings as a gpui global; every change is saved at once.

use gpui::{App, Global};

use crate::settings::{self, Settings};
use crate::theme::ThemeInput;

pub struct Prefs(pub Settings);

impl Global for Prefs {}

pub fn init(cx: &mut App) {
    cx.set_global(Prefs(settings::load()));
}

pub fn get(cx: &App) -> &Settings {
    &cx.global::<Prefs>().0
}

/// Change settings and persist them.
pub fn update(cx: &mut App, f: impl FnOnce(&mut Settings)) {
    let prefs = cx.global_mut::<Prefs>();
    f(&mut prefs.0);
    settings::save(&prefs.0);
}

/// The theme input the current settings describe.
pub fn theme_input(s: &Settings) -> ThemeInput {
    ThemeInput {
        accent: s.accent_hex.clone(),
        background: s.background_hex.clone(),
        glass_tint: s.glass_tint,
    }
}
