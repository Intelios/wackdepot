//! macOS accessibility display settings that change how WackDepot draws.

use objc2_app_kit::NSWorkspace;

/// System Settings › Accessibility › Display › Reduce motion.
pub fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// System Settings › Accessibility › Display › Reduce transparency — the
/// window stays opaque when it's on.
pub fn reduce_transparency() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}
