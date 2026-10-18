//! The few places WackDepot talks to AppKit directly: the Liquid Glass view
//! behind gpui's content, the app-wide appearance, and accessibility
//! display settings. Everything else goes through gpui.

pub mod accessibility;
pub mod appearance;
pub mod glass;
