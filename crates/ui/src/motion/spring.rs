//! Damped springs solved in closed form — deterministic at any frame rate,
//! retargetable mid-flight without a velocity jump.

/// Stiffness / damping / mass, as in motion/react.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringCfg {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
}

impl SpringCfg {
    pub const fn new(stiffness: f64, damping: f64) -> Self {
        Self {
            stiffness,
            damping,
            mass: 1.0,
        }
    }

    pub const fn with_mass(self, mass: f64) -> Self {
        Self { mass, ..self }
    }
}

/// The gliding active pill under tabs (ζ≈0.82).
pub const GLIDE_PILL: SpringCfg = SpringCfg::new(480.0, 36.0);
/// List rows moving to a new slot.
pub const LIST_ROW: SpringCfg = SpringCfg::new(520.0, 34.0);
/// Chips and marks popping in (bouncier, ζ≈0.61).
pub const CHIP_POP: SpringCfg = SpringCfg::new(520.0, 28.0);
/// Rolling-digit drums.
pub const DIGITS: SpringCfg = SpringCfg::new(520.0, 40.0);
/// Text swaps.
pub const TEXT_SWAP: SpringCfg = SpringCfg::new(420.0, 32.0);
/// Things flying a distance (stage fly).
pub const FLY: SpringCfg = SpringCfg::new(420.0, 30.0).with_mass(0.8);

/// One spring in flight: displacement `x0` from its target and velocity
/// `v0` at time `t0` (seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub cfg: SpringCfg,
    pub x0: f64,
    pub v0: f64,
    pub t0: f64,
}

impl Spring {
    /// Displacement from the target at time `t`.
    pub fn offset(&self, t: f64) -> f64 {
        let t = (t - self.t0).max(0.0);
        let SpringCfg {
            stiffness: k,
            damping: c,
            mass: m,
        } = self.cfg;
        let w0 = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());
        let (x0, v0) = (self.x0, self.v0);
        if (zeta - 1.0).abs() < 1e-6 {
            (-w0 * t).exp() * (x0 + (v0 + w0 * x0) * t)
        } else if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            (-zeta * w0 * t).exp()
                * (x0 * (wd * t).cos() + (v0 + zeta * w0 * x0) / wd * (wd * t).sin())
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let r1 = -w0 * (zeta - root);
            let r2 = -w0 * (zeta + root);
            let c1 = (v0 - r2 * x0) / (r1 - r2);
            let c2 = x0 - c1;
            c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
        }
    }

    /// Velocity at time `t` (numerical derivative; plenty for retargeting).
    pub fn velocity(&self, t: f64) -> f64 {
        let h = 1e-4;
        (self.offset(t + h) - self.offset((t - h).max(self.t0))) / (t + h - (t - h).max(self.t0))
    }

    /// Whether the spring has come to rest at `t` (relative to how far it
    /// started, with an absolute floor for tiny moves).
    pub fn settled(&self, t: f64) -> bool {
        let eps = (self.x0.abs().max(self.v0.abs() * 0.05) * 0.002).max(0.002);
        self.offset(t).abs() < eps && self.velocity(t).abs() < eps * 20.0
    }
}
