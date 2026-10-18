//! Liquid Glass (`NSGlassEffectView`, macOS 26+) behind the whole window —
//! ported from WackCode's `src-tauri/src/glass.rs`.
//!
//! gpui draws into its own view inside the window's content view, so the
//! glass goes into that content view as a sibling *below* gpui's view. The
//! window itself must be created `Transparent` so nothing opaque covers it.

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
    NSWindowOrderingMode,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The private `variant` value WackCode calls Frosted (the sidebar material).
const FROSTED_VARIANT: isize = 16;

/// An installed glass view; removed from the window when dropped.
pub struct NativeGlass {
    view: Retained<NSGlassEffectView>,
}

impl NativeGlass {
    /// Whether this macOS has `NSGlassEffectView` at all.
    pub fn supported() -> bool {
        AnyClass::get(c"NSGlassEffectView").is_some()
    }

    /// Put a frosted glass view under gpui's content in `window`. `None`
    /// when glass isn't available or the native view can't be reached.
    pub fn install(window: &gpui::Window) -> Option<Self> {
        if !Self::supported() {
            return None;
        }
        let mtm = MainThreadMarker::new()?;
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        // SAFETY: the handle comes from gpui's live window and names its own
        // content NSView; we're on the main thread (checked above).
        let gpui_view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
        let content = unsafe { gpui_view.superview() }?;

        let glass =
            NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), content.bounds());
        glass.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // SAFETY: probing optional selectors on a live view we own.
        unsafe { set_frosted(&glass) };
        content.addSubview_positioned_relativeTo(
            &glass,
            NSWindowOrderingMode::Below,
            Some(gpui_view),
        );
        Some(Self { view: glass })
    }

    /// Show or hide the glass (hidden while the window is opaque, or its
    /// grey inactive material would show through the edges).
    pub fn set_visible(&self, visible: bool) {
        self.view.setHidden(!visible);
    }
}

impl Drop for NativeGlass {
    fn drop(&mut self) {
        self.view.removeFromSuperview();
    }
}

/// Frosted variant via the undocumented `variant` selector, in WackCode's
/// probe order; degrades to the public Regular style if Apple removes it.
unsafe fn set_frosted(view: &NSGlassEffectView) {
    let responds: bool = unsafe { msg_send![view, respondsToSelector: sel!(set_variant:)] };
    if responds {
        let _: () = unsafe { msg_send![view, set_variant: FROSTED_VARIANT] };
        return;
    }
    let responds: bool = unsafe { msg_send![view, respondsToSelector: sel!(setVariant:)] };
    if responds {
        let _: () = unsafe { msg_send![view, setVariant: FROSTED_VARIANT] };
        return;
    }
    view.setStyle(NSGlassEffectViewStyle::Regular);
}
