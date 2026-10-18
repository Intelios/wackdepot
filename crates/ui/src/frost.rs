//! `frosted` — the frosted-glass float: wraps a popover/dialog card so its
//! entire subtree paints inside one scene layer with a backdrop blur painted
//! first. Ported from zeron's crates/ui frost.rs (MIT).
//!
//! The single layer order is the point: with per-primitive ordering a hover
//! repaint elsewhere could reassign the card's quads below the blur. Inside
//! one layer the relationship is structural: blur first, then everything.

use gpui::{
    AnyElement, App, Bounds, Corners, Element, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window, px,
};

/// Shared backdrop-blur sigma for floating menus, popovers and palettes.
pub const MENU_BLUR: f32 = 16.0;

/// Frost `child` (a popover card): backdrop-blurred on glass, pass-through
/// where frost is off. `corner_radius` must match the card's rounding.
pub fn frosted(corner_radius: f32, blur_radius: f32, frost: bool, child: impl IntoElement) -> Frosted {
    Frosted {
        corner_radius,
        blur_radius,
        frost,
        child: child.into_any_element(),
    }
}

pub struct Frosted {
    corner_radius: f32,
    blur_radius: f32,
    frost: bool,
    child: AnyElement,
}

impl Element for Frosted {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.frost {
            window.paint_layer(bounds, |window| {
                window.paint_backdrop_blur(
                    bounds,
                    Corners::all(px(self.corner_radius)),
                    px(self.blur_radius),
                );
                self.child.paint(window, cx);
            });
        } else {
            self.child.paint(window, cx);
        }
    }
}

impl IntoElement for Frosted {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
