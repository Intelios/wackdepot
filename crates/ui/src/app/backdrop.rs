//! What's behind the window, and the opaque-when-unfocused rule: a
//! transparent window costs WindowServer GPU even while idle, so the window
//! turns solid while inactive. The root paints a tint crossfade (`mix`), and
//! the native switch happens only once it's fully opaque, so it never flashes.

use gpui::{App, Window, WindowBackgroundAppearance};

use crate::motion::{EASE, EXIT, Motion};
use crate::native::{accessibility, glass::NativeGlass};
use crate::settings::{Backdrop as BackdropPref, Settings};

enum Mode {
    Glass(NativeGlass),
    /// zui's NSVisualEffectView blur (no Liquid Glass on this macOS).
    Blur,
    Solid,
}

pub struct Backdrop {
    mode: Mode,
    active: bool,
    /// The native window is currently opaque.
    opaque: bool,
}

/// The appearance to open the window with.
pub fn initial_appearance(s: &Settings) -> WindowBackgroundAppearance {
    if s.backdrop == BackdropPref::Solid || accessibility::reduce_transparency() {
        WindowBackgroundAppearance::Opaque
    } else if NativeGlass::supported() {
        WindowBackgroundAppearance::Transparent
    } else {
        WindowBackgroundAppearance::Blurred
    }
}

impl Backdrop {
    pub fn new(window: &mut Window, s: &Settings) -> Self {
        let mode = match initial_appearance(s) {
            WindowBackgroundAppearance::Transparent => NativeGlass::install(window)
                .map(Mode::Glass)
                .unwrap_or(Mode::Blur),
            WindowBackgroundAppearance::Blurred => Mode::Blur,
            _ => Mode::Solid,
        };
        let opaque = matches!(mode, Mode::Solid);
        Self {
            mode,
            active: true,
            opaque,
        }
    }

    fn see_through(&self) -> WindowBackgroundAppearance {
        match self.mode {
            Mode::Glass(_) => WindowBackgroundAppearance::Transparent,
            Mode::Blur => WindowBackgroundAppearance::Blurred,
            Mode::Solid => WindowBackgroundAppearance::Opaque,
        }
    }

    /// Focus changed. Going active turns the glass back on straight away
    /// (the root is still painting opaque, so nothing shows yet).
    pub fn set_active(&mut self, active: bool, window: &mut Window) {
        self.active = active;
        if active && self.opaque && !matches!(self.mode, Mode::Solid) {
            if let Mode::Glass(g) = &self.mode {
                g.set_visible(true);
            }
            window.set_background_appearance(self.see_through());
            self.opaque = false;
        }
    }

    /// Settings › Appearance changed the backdrop (or Reduce Transparency
    /// changed): switch between glass and solid in place.
    pub fn reconfigure(&mut self, window: &mut Window, s: &Settings) {
        let want_solid = s.backdrop == BackdropPref::Solid || accessibility::reduce_transparency();
        match (&self.mode, want_solid) {
            (Mode::Solid, false) => {
                self.mode = NativeGlass::install(window)
                    .map(Mode::Glass)
                    .unwrap_or(Mode::Blur);
                if let Mode::Glass(g) = &self.mode {
                    g.set_visible(self.active);
                }
                if self.active {
                    window.set_background_appearance(self.see_through());
                    self.opaque = false;
                } else {
                    window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                    self.opaque = true;
                }
            }
            (Mode::Glass(_) | Mode::Blur, true) => {
                self.mode = Mode::Solid;
                window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                self.opaque = true;
            }
            _ => {}
        }
    }

    /// 1 with the glass fully showing, 0 when the shell is opaque.
    pub fn mix(&self, motion: &mut Motion) -> f32 {
        let see_through = self.active && !matches!(self.mode, Mode::Solid);
        if see_through {
            motion.tween(("glass_mix", 0), 1.0, 220.0, EASE)
        } else {
            motion.tween(("glass_mix", 0), 0.0, 160.0, EXIT)
        }
    }

    /// After painting at `mix`: once fully opaque while inactive, make the
    /// native window opaque too (deferred, outside this frame's paint).
    pub fn needs_opaque(&self, mix: f32) -> bool {
        !self.active && !self.opaque && mix <= 0.0
    }

    pub fn go_opaque(&mut self, window: &mut Window) {
        if self.active || self.opaque {
            return;
        }
        window.set_background_appearance(WindowBackgroundAppearance::Opaque);
        if let Mode::Glass(g) = &self.mode {
            g.set_visible(false);
        }
        self.opaque = true;
    }
}

/// Reduce motion: the user's override, else macOS's setting.
pub fn sync_reduce_motion(cx: &mut App) {
    let forced = super::prefs::get(cx).reduce_motion == crate::settings::MotionPref::Always;
    cx.set_reduce_motion(forced || accessibility::reduce_motion());
}
