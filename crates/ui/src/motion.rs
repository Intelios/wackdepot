//! The motion kit: small helpers over gpui `Animation` in the WackCode
//! spirit — menu-in scale/fades, dialog rises, a working pulse. Every
//! animation is opacity/offset only (cheap) and honors `App::reduce_motion`
//! automatically (gpui renders the still end state when set).

use std::time::Duration;

use gpui::{Animation, AnimationExt, ElementId, IntoElement, Styled, pulsating_between, px};

/// Zeron's cubic-bezier(0.16,1,0.3,1) ≈ ease-out-quint — our entrance curve.
fn entrance() -> Animation {
    Animation::new(Duration::from_millis(160)).with_easing(gpui::ease_out_quint())
}

/// Popovers and menus: fade + 4px lift.
pub fn menu_in<E>(id: impl Into<ElementId>, el: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    el.with_animation(id, entrance(), |el, t| {
        el.opacity(t).mt(px((1.0 - t) * 4.0))
    })
}

/// Dialogs and toasts: fade + 10px rise, slightly slower.
pub fn dialog_in<E>(id: impl Into<ElementId>, el: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(220)).with_easing(gpui::ease_out_quint()),
        |el, t| el.opacity(t).mt(px((1.0 - t) * 10.0)),
    )
}

/// The busy pulse: the accent bar breathes while an op runs.
pub fn pulse<E>(id: impl Into<ElementId>, el: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(1100))
            .repeat()
            .with_easing(pulsating_between(0.25, 1.0)),
        |el, t| el.opacity(t),
    )
}

/// Panel/view entrances: a quick directional slide + fade.
pub fn slide_in<E>(id: impl Into<ElementId>, el: E, dx: f32) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    el.with_animation(id, entrance(), move |el, t| {
        el.opacity(t).ml(px((1.0 - t) * dx))
    })
}
