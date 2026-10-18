//! wackdepot-ui — the gpui application. The rebuilt UI lives in `app/` and
//! its siblings; `legacy/` is the placeholder UI it replaces, still the
//! default until cutover. `WACKDEPOT_UI=next` runs the new one.

pub mod app;
pub mod components;
pub mod icons;
mod legacy;
pub mod model;
pub mod motion;
mod native;
pub mod settings;
pub mod theme;
pub mod views;
mod watcher;

#[cfg(test)]
mod lint;

/// Open the main window and run the app. Called once from the binary.
pub fn run_app() {
    if std::env::var("WACKDEPOT_UI").as_deref() == Ok("next") {
        app::run();
    } else {
        legacy::run();
    }
}
