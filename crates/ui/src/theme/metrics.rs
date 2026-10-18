//! Layout and type metrics — one place for the numbers the design spec
//! fixes (comfortable density, WackCode radii, zeron titlebar geometry).

/// Titlebar row; traffic lights sit at (14, 14), centred in it.
pub const TITLEBAR_H: f32 = 38.0;
/// Where titlebar content may start, clear of the traffic lights.
pub const TRAFFIC_INSET: f32 = 80.0;
/// Glass gap around and between the column and the content card.
pub const GAP: f32 = 8.0;

/// Left column width: default and drag limits.
pub const SIDEBAR_W: f32 = 320.0;
pub const SIDEBAR_MIN: f32 = 260.0;
pub const SIDEBAR_MAX: f32 = 480.0;

/// Comfortable list rows.
pub const ROW_H: f32 = 34.0;
pub const GROUP_H: f32 = 30.0;
/// Code rows in the diff.
pub const CODE_ROW_H: f32 = 20.0;

pub const RADIUS_CARD: f32 = 14.0;
pub const RADIUS_COMPOSER: f32 = 18.0;
pub const RADIUS_POPOVER: f32 = 14.0;
pub const RADIUS_MENU_ROW: f32 = 9.0;
pub const RADIUS_INPUT: f32 = 10.0;

/// Type scale (px).
pub const TEXT_HERO: f32 = 23.0;
pub const TEXT_TITLE: f32 = 15.5;
pub const TEXT_BASE: f32 = 14.0;
pub const TEXT_ROW: f32 = 13.5;
pub const TEXT_META: f32 = 12.0;
pub const TEXT_SMALL: f32 = 11.5;
pub const TEXT_KICKER: f32 = 10.5;
pub const TEXT_CODE: f32 = 12.0;

/// Frosted surfaces: in-app gaussian sigma.
pub const FROST_BLUR: f32 = 16.0;
