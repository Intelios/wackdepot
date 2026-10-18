//! The per-view motion store. Views call it declaratively every render —
//! "this value should be heading to X" — and it answers with the value for
//! this frame. Only while something is still moving does it ask gpui for
//! another frame, so a settled UI costs nothing.
//!
//! First sight of a key never animates (it starts at its target); use the
//! `*_from` calls for entrances.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use gpui::{App, Window};

use super::clock;
use super::easing::CubicBezier;
use super::spring::{Spring, SpringCfg};

/// A motion channel plus an id within it (path, sha, index…).
pub type MotionKey = (&'static str, u64);

/// Hash anything into a key id.
pub fn id(value: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

#[derive(Debug, Clone, Copy)]
enum Track {
    Spring {
        spring: Spring,
        target: f64,
    },
    Tween {
        from: f64,
        to: f64,
        t0: f64,
        dur: f64,
        ease: CubicBezier,
    },
}

impl Track {
    fn value(&self, t: f64) -> f64 {
        match *self {
            Track::Spring { spring, target } => target + spring.offset(t),
            Track::Tween {
                from,
                to,
                t0,
                dur,
                ease,
            } => {
                if dur <= 0.0 {
                    return to;
                }
                let p = ((t - t0) / dur) as f32;
                if p <= 0.0 {
                    return from;
                }
                from + (to - from) * f64::from(ease.eval(p))
            }
        }
    }

    fn target(&self) -> f64 {
        match *self {
            Track::Spring { target, .. } => target,
            Track::Tween { to, .. } => to,
        }
    }

    fn settled(&self, t: f64) -> bool {
        match *self {
            Track::Spring { spring, .. } => spring.settled(t),
            Track::Tween { t0, dur, .. } => t >= t0 + dur,
        }
    }
}

/// Animated values for one view (or one component's keyed state).
#[derive(Debug, Default)]
pub struct Motion {
    tracks: HashMap<MotionKey, Track>,
    touched: HashSet<MotionKey>,
    now: f64,
    reduce: bool,
    active: bool,
}

impl Motion {
    /// Start a frame: read the clock and the reduce-motion setting.
    pub fn begin(&mut self, cx: &App) {
        self.begin_at(clock::now(), cx.reduce_motion());
    }

    pub(crate) fn begin_at(&mut self, now: f64, reduce: bool) {
        self.now = now;
        self.reduce = reduce;
        self.active = false;
        self.touched.clear();
        if reduce {
            self.tracks.clear();
        }
    }

    /// Finish a frame: ask for another while anything moves, and forget
    /// tracks nobody asked about this frame.
    pub fn end(&mut self, window: &mut Window) {
        if self.end_frame() {
            window.request_animation_frame();
        }
    }

    /// [`Motion::end`] without a window: `true` when another frame is needed.
    pub(crate) fn end_frame(&mut self) -> bool {
        let touched = std::mem::take(&mut self.touched);
        self.tracks.retain(|k, _| touched.contains(k));
        self.touched = touched;
        self.active && !self.reduce
    }

    /// Whether anything is still moving (as of the last call this frame).
    pub fn is_active(&self) -> bool {
        self.active
    }

    fn sample(&mut self, key: MotionKey) -> f32 {
        self.touched.insert(key);
        let Some(track) = self.tracks.get(&key).copied() else {
            return 0.0;
        };
        if track.settled(self.now) {
            let v = track.target();
            // Collapse to a resting tween so later reads are exact.
            self.tracks.insert(
                key,
                Track::Tween {
                    from: v,
                    to: v,
                    t0: self.now,
                    dur: 0.0,
                    ease: super::easing::LINEAR,
                },
            );
            return v as f32;
        }
        self.active = true;
        track.value(self.now) as f32
    }

    /// A spring heading to `target`. Retargets smoothly when it changes.
    pub fn spring(&mut self, key: MotionKey, target: f32, cfg: SpringCfg) -> f32 {
        let target = f64::from(target);
        if self.reduce {
            return target as f32;
        }
        match self.tracks.get(&key).copied() {
            None => {
                self.tracks.insert(key, rest(target, self.now));
            }
            Some(track) if (track.target() - target).abs() > f64::EPSILON => {
                let (pos, vel) = pos_vel(&track, self.now);
                let spring = Spring {
                    cfg,
                    x0: pos - target,
                    v0: vel,
                    t0: self.now,
                };
                self.tracks.insert(key, Track::Spring { spring, target });
            }
            _ => {}
        }
        self.sample(key)
    }

    /// Like [`Motion::spring`], but a brand-new key starts at `from`.
    pub fn spring_from(&mut self, key: MotionKey, from: f32, target: f32, cfg: SpringCfg) -> f32 {
        if !self.reduce && !self.tracks.contains_key(&key) {
            let spring = Spring {
                cfg,
                x0: f64::from(from - target),
                v0: 0.0,
                t0: self.now,
            };
            self.tracks.insert(
                key,
                Track::Spring {
                    spring,
                    target: f64::from(target),
                },
            );
        }
        self.spring(key, target, cfg)
    }

    /// Push a value `by` away from where it is now and spring it back to its
    /// target — the "invert, then play" half of FLIP.
    pub fn kick(&mut self, key: MotionKey, by: f32, target: f32, cfg: SpringCfg) {
        if self.reduce || by.abs() < 0.5 {
            return;
        }
        let target = f64::from(target);
        let (pos, vel) = match self.tracks.get(&key) {
            Some(t) => pos_vel(t, self.now),
            None => (target, 0.0),
        };
        let spring = Spring {
            cfg,
            x0: pos - target + f64::from(by),
            v0: vel,
            t0: self.now,
        };
        self.tracks.insert(key, Track::Spring { spring, target });
    }

    /// A timed tween heading to `target`; restarts from the current value
    /// when the target changes.
    pub fn tween(&mut self, key: MotionKey, target: f32, dur_ms: f32, ease: CubicBezier) -> f32 {
        let target = f64::from(target);
        if self.reduce {
            return target as f32;
        }
        match self.tracks.get(&key).copied() {
            None => {
                self.tracks.insert(key, rest(target, self.now));
            }
            Some(track) if (track.target() - target).abs() > f64::EPSILON => {
                let from = track.value(self.now);
                self.tracks.insert(
                    key,
                    Track::Tween {
                        from,
                        to: target,
                        t0: self.now,
                        dur: f64::from(dur_ms) / 1000.0,
                        ease,
                    },
                );
            }
            _ => {}
        }
        self.sample(key)
    }

    /// A tween that, for a brand-new key, starts at `from` after `delay_ms`
    /// — entrances and staggers.
    pub fn tween_from(
        &mut self,
        key: MotionKey,
        from: f32,
        target: f32,
        dur_ms: f32,
        delay_ms: f32,
        ease: CubicBezier,
    ) -> f32 {
        if !self.reduce && !self.tracks.contains_key(&key) {
            self.tracks.insert(
                key,
                Track::Tween {
                    from: f64::from(from),
                    to: f64::from(target),
                    t0: self.now + f64::from(delay_ms) / 1000.0,
                    dur: f64::from(dur_ms) / 1000.0,
                    ease,
                },
            );
        }
        self.tween(key, target, dur_ms, ease)
    }

    /// Restart a key's tween from `from` (re-trigger a one-shot beat).
    pub fn restart(
        &mut self,
        key: MotionKey,
        from: f32,
        target: f32,
        dur_ms: f32,
        ease: CubicBezier,
    ) {
        if self.reduce {
            return;
        }
        self.tracks.insert(
            key,
            Track::Tween {
                from: f64::from(from),
                to: f64::from(target),
                t0: self.now,
                dur: f64::from(dur_ms) / 1000.0,
                ease,
            },
        );
    }

    /// Whether `key` is still in flight.
    pub fn moving(&self, key: MotionKey) -> bool {
        self.tracks.get(&key).is_some_and(|t| !t.settled(self.now))
    }
}

fn rest(value: f64, now: f64) -> Track {
    Track::Tween {
        from: value,
        to: value,
        t0: now,
        dur: 0.0,
        ease: super::easing::LINEAR,
    }
}

fn pos_vel(track: &Track, now: f64) -> (f64, f64) {
    match *track {
        Track::Spring { spring, target } => (target + spring.offset(now), spring.velocity(now)),
        Track::Tween { .. } => {
            let h = 1e-3;
            let v = (track.value(now + h) - track.value(now - h)) / (2.0 * h);
            (track.value(now), if track.settled(now) { 0.0 } else { v })
        }
    }
}
