//! One hand-drawn icon set on a 24px grid, 1.8 stroke, round caps,
//! `currentColor`. Same language as WackCode's Icons.tsx. Add a path and a
//! name to `IconName`; don't pull in an icon library.
//!
//! SVGs are embedded at compile time and served through a static
//! `AssetSource`, so `svg().path("icons/x.svg")` always resolves.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString, Svg, px, svg};

const STROKE_SVG_HEADER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">"##;

macro_rules! icon {
    ($name:literal, $($body:tt)*) => {
        ($name, concat!($($body)*))
    };
}

/// name → svg body (paths only; shared header adds stroke style).
static ICONS: &[(&str, &str)] = &[
    icon!("folder", r#"<path d="M3.5 6.5a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z"/>"#),
    icon!("git-branch", r#"<circle cx="7" cy="5.5" r="2.2"/><circle cx="7" cy="18.5" r="2.2"/><circle cx="17" cy="8" r="2.2"/><path d="M7 7.7v8.6"/><path d="M17 10.2c0 3.8-3 4.8-6.2 5.6"/>"#),
    icon!("history", r#"<path d="M4.5 12a7.5 7.5 0 1 1 2.2 5.3"/><path d="M4.5 12H2.5M4.5 12l1.8-1.8"/><path d="M12 8v4.4l3 1.8"/>"#),
    icon!("fetch", r#"<path d="M12 4v9"/><path d="M8 9.5 12 13.5 16 9.5"/><path d="M5 17.5h14"/><path d="M19.5 5.5 18 7"/>"#),
    icon!("push", r#"<path d="M12 20v-9"/><path d="M8 14.5 12 10.5l4 4"/><path d="M5 6.5h14"/>"#),
    icon!("pull", r#"<path d="M12 4v9"/><path d="M8 9.5 12 13.5 16 9.5"/><path d="M5 17.5h14"/>"#),
    icon!("sync", r#"<path d="M18.5 8.5A7.5 7.5 0 0 0 5.8 9.4"/><path d="M5.5 15.5a7.5 7.5 0 0 0 12.7-.9"/><path d="M18.5 4.5v4h-4"/><path d="M5.5 19.5v-4h4"/>"#),
    icon!("plus", r#"<path d="M12 5.5v13"/><path d="M5.5 12h13"/>"#),
    icon!("check", r#"<path d="m5 12.5 4.5 4.5L19 7.5"/>"#),
    icon!("x", r#"<path d="m6.5 6.5 11 11"/><path d="m17.5 6.5-11 11"/>"#),
    icon!("chevron-down", r#"<path d="m6.5 9.5 5.5 5 5.5-5"/>"#),
    icon!("chevron-right", r#"<path d="m9.5 6.5 5 5.5-5 5.5"/>"#),
    icon!("chevron-left", r#"<path d="m14.5 6.5-5 5.5 5 5.5"/>"#),
    icon!("settings", r#"<circle cx="12" cy="12" r="3.2"/><path d="M12 3.8v2.2M12 18v2.2M20.2 12H18M6 12H3.8M17.8 6.2l-1.6 1.6M7.8 16.2l-1.6 1.6M17.8 17.8l-1.6-1.6M7.8 7.8 6.2 6.2"/>"#),
    icon!("undo", r#"<path d="M8.5 5.5 4.5 9.5l4 4"/><path d="M5 9.5h9a5 5 0 0 1 0 10h-3"/>"#),
    icon!("trash", r#"<path d="M5 7h14"/><path d="M9.5 7V5a1.5 1.5 0 0 1 1.5-1.5h2A1.5 1.5 0 0 1 14.5 5v2"/><path d="M7 7l.8 11a2 2 0 0 0 2 1.8h4.4a2 2 0 0 0 2-1.8L17 7"/>"#),
    icon!("external", r#"<path d="M10 5.5H6.5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V14"/><path d="M14 4.5h5.5V10"/><path d="M19 5 11.5 12.5"/>"#),
    icon!("ellipsis", r#"<circle cx="5.5" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="18.5" cy="12" r="1.3" fill="currentColor" stroke="none"/>"#),
    icon!("warning", r#"<path d="M12 4.5 21 19.5H3Z"/><path d="M12 10v4"/><circle cx="12" cy="16.8" r=".9" fill="currentColor" stroke="none"/>"#),
    icon!("git-merge", r#"<circle cx="7" cy="6" r="2.2"/><circle cx="7" cy="18" r="2.2"/><circle cx="17" cy="18" r="2.2"/><path d="M17 15.8V11a3 3 0 0 0-3-3H9.2"/>"#),
    icon!("search", r#"<circle cx="10.5" cy="10.5" r="6"/><path d="m15 15 5 5"/>"#),
    icon!("file", r#"<path d="M6 3.5h7.5L18.5 8v12a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 20V5A1.5 1.5 0 0 1 7 3.5Z"/><path d="M13.5 3.5V8H18"/>"#),
    icon!("dot", r#"<circle cx="12" cy="12" r="4" fill="currentColor" stroke="none"/>"#),
    icon!("commit", r#"<circle cx="12" cy="12" r="3.5"/><path d="M3.5 12h5"/><path d="M15.5 12h5"/>"#),
    icon!("stash", r#"<path d="M4.5 8.5 12 4l7.5 4.5v7L12 20l-7.5-4.5Z"/><path d="M4.5 8.5 12 13l7.5-4.5"/><path d="M12 13v7"/>"#),
    icon!("cloud", r#"<path d="M7 18a4.5 4.5 0 0 1-.6-8.96A6 6 0 0 1 18 8.5a4 4 0 0 1 .6 9.46"/>"#),
    icon!("globe", r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17"/><path d="M12 3.5c-4.8 4.9-4.8 12.1 0 17"/><path d="M12 3.5c4.8 4.9 4.8 12.1 0 17"/>"#),
    icon!("eye-off", r#"<path d="M4 4.5 20 19.5"/><path d="M9.9 5.6A9.5 9.5 0 0 1 12 5.5c5 0 8.5 4 9.5 6.5a17.7 17.7 0 0 1-3.2 3.9M6.1 7.8A17 17 0 0 0 2.5 12c1 2.5 4.5 6.5 9.5 6.5a9.3 9.3 0 0 0 3.5-.7"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/>"#),
    icon!("repo-add", r#"<path d="M3.5 6.5a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z"/><path d="M12 10.5v5"/><path d="M9.5 13h5"/>"#),
];

/// Static asset source for the embedded icons (path "icons/<name>.svg").
pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let name = path.strip_prefix("icons/").unwrap_or(path);
        let name = name.strip_suffix(".svg").unwrap_or(name);
        for (n, body) in ICONS {
            if *n == name {
                let svg_text = format!("{}{}{}", STROKE_SVG_HEADER, body, "</svg>");
                return Ok(Some(Cow::Owned(svg_text.into_bytes())));
            }
        }
        Ok(None)
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(n, _)| SharedString::from(format!("icons/{n}.svg")))
            .collect())
    }
}

/// A 16px icon in `currentColor`; size/color come from the caller's styles.
pub fn icon(name: &'static str) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(16.))
}
