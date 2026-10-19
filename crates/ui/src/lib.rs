//! wackdepot-ui — the gpui application: the glass shell, the repo model,
//! the views (titlebar, column, diff card, settings), and the theme and
//! motion systems they're drawn with.

pub mod app;
pub mod components;
pub mod github;
pub mod icons;
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
    app::run();
}
