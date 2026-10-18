//! Motion: WackCode's vocabulary (easings, springs, entrances, glides) on
//! gpui, which has no transforms or springs of its own. Values are driven by
//! [`Motion`] stores that request frames only while something moves; gpui's
//! `with_animation` is reserved for stateless 0–1 loops (spinners, shimmer).
//!
//! Everything honours reduce motion: springs and tweens land on their target
//! immediately and beats are skipped.

pub mod easing;
pub mod presence;
pub mod spring;
pub mod store;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;
use std::time::Instant;

pub use easing::{CubicBezier, EASE, EXIT, GLIDE, LINEAR, POP, POP_STRONG};
pub use presence::{Phase, Presence};
pub use spring::SpringCfg;
pub use store::{Motion, MotionKey, id};

/// Seconds on the motion clock. `WACKDEPOT_SLOWMO=<n>` slows every animation
/// n-fold, for tuning beats by eye.
pub fn now() -> f64 {
    static START: OnceLock<(Instant, f64)> = OnceLock::new();
    let (start, slow) = START.get_or_init(|| {
        let slow = std::env::var("WACKDEPOT_SLOWMO")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| *v >= 1.0)
            .unwrap_or(1.0);
        (Instant::now(), slow)
    });
    start.elapsed().as_secs_f64() / slow
}

/// Seconds-on-the-clock helper re-exported for presence syncing.
pub mod clock {
    pub use super::now;
}
