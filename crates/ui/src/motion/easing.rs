//! CSS `cubic-bezier()` curves, solved exactly (WebKit's UnitBezier), with
//! overshoot allowed — WackCode's POP and GLIDE curves go past 1 on purpose.

/// A cubic bezier timing curve through (0,0) and (1,1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    ax: f64,
    bx: f64,
    cx: f64,
    ay: f64,
    by: f64,
    cy: f64,
}

impl CubicBezier {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        let cx = 3.0 * x1;
        let bx = 3.0 * (x2 - x1) - cx;
        let ax = 1.0 - cx - bx;
        let cy = 3.0 * y1;
        let by = 3.0 * (y2 - y1) - cy;
        let ay = 1.0 - cy - by;
        Self {
            ax,
            bx,
            cx,
            ay,
            by,
            cy,
        }
    }

    fn x(&self, s: f64) -> f64 {
        ((self.ax * s + self.bx) * s + self.cx) * s
    }

    fn y(&self, s: f64) -> f64 {
        ((self.ay * s + self.by) * s + self.cy) * s
    }

    fn dx(&self, s: f64) -> f64 {
        (3.0 * self.ax * s + 2.0 * self.bx) * s + self.cx
    }

    /// Find the curve parameter whose x is `x`.
    fn solve_x(&self, x: f64) -> f64 {
        let mut s = x;
        for _ in 0..8 {
            let err = self.x(s) - x;
            if err.abs() < 1e-7 {
                return s;
            }
            let d = self.dx(s);
            if d.abs() < 1e-6 {
                break;
            }
            s -= err / d;
        }
        // Bisection fallback for flat stretches.
        let (mut lo, mut hi) = (0.0, 1.0);
        s = x;
        while lo < hi {
            let v = self.x(s);
            if (v - x).abs() < 1e-7 {
                break;
            }
            if x > v {
                lo = s;
            } else {
                hi = s;
            }
            s = (hi - lo) * 0.5 + lo;
            if hi - lo < 1e-9 {
                break;
            }
        }
        s
    }

    /// Eased progress for linear progress `t` (clamped to 0–1). The result
    /// may leave 0–1 for overshooting curves.
    pub fn eval(&self, t: f32) -> f32 {
        let t = f64::from(t.clamp(0.0, 1.0));
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        self.y(self.solve_x(t)) as f32
    }
}

/// WackCode's default: easeOutCubic-ish, for everything that just moves.
pub const EASE: CubicBezier = CubicBezier::new(0.33, 1.0, 0.68, 1.0);
/// Pops and entrances that land with a little overshoot.
pub const POP: CubicBezier = CubicBezier::new(0.3, 1.4, 0.4, 1.0);
/// A stronger pop (confirm arm, seal stamp).
pub const POP_STRONG: CubicBezier = CubicBezier::new(0.34, 1.56, 0.64, 1.0);
/// Long glides (comet, composer moves).
pub const GLIDE: CubicBezier = CubicBezier::new(0.34, 1.3, 0.64, 1.0);
/// Leaving: ease-in.
pub const EXIT: CubicBezier = CubicBezier::new(0.32, 0.0, 0.67, 0.0);
/// Constant speed.
pub const LINEAR: CubicBezier = CubicBezier::new(0.0, 0.0, 1.0, 1.0);
