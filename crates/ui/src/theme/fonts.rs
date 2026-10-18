//! Bundled Inter (SIL OFL 1.1, see assets/fonts/Inter-OFL.txt) for UI text,
//! and the system mono for code, paths and branch names.

use std::borrow::Cow;

use gpui::{App, SharedString};

pub const SANS: &str = "Inter";

const FONTS: [&[u8]; 4] = [
    include_bytes!("../../../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../../assets/fonts/Inter-Bold.ttf"),
];

/// Register the bundled faces with gpui's text system.
pub fn register(cx: &mut App) {
    let faces = FONTS.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(e) = cx.text_system().add_fonts(faces) {
        eprintln!("WackDepot couldn't load its bundled fonts: {e}");
    }
}

/// "SF Mono" when the system has it (it ships with macOS but is sometimes
/// only registered under Xcode/Terminal), else Menlo.
pub fn mono_family(cx: &App) -> SharedString {
    let names = cx.text_system().all_font_names();
    if names.iter().any(|n| n == "SF Mono") {
        "SF Mono".into()
    } else {
        "Menlo".into()
    }
}
