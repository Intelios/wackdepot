//! One hand-drawn icon set on a 24px grid, 1.8 stroke, round caps,
//! `currentColor`. Same language as WackCode's Icons.tsx. Add a path and a
//! name to `IconName`; don't pull in an icon library.
//!
//! SVGs are embedded at compile time and served through a static
//! `AssetSource`, so `svg().path("icons/x.svg")` always resolves.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString, Svg, prelude::*, svg};

const STROKE_SVG_HEADER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">"##;

/// name → svg body (paths only; the shared header adds the stroke style).
/// WackCode's paths where it has the glyph; new ones drawn in the same hand.
static ICONS: &[(&str, &str)] = &[
    ("folder", r#"<path d="M3 6.5h6l2 2h10v10.5H3z"/>"#),
    (
        "folder-plus",
        r#"<path d="M3 6.5h6l2 2h10v10.5H3z"/><path d="M12 11.5v5M9.5 14h5"/>"#,
    ),
    (
        "branch",
        r#"<circle cx="7" cy="5" r="2"/><circle cx="7" cy="19" r="2"/><circle cx="17" cy="8" r="2"/><path d="M7 7v10M9 15c4 0 6-2 6-5"/>"#,
    ),
    (
        "commit",
        r#"<circle cx="12" cy="12" r="3.5"/><path d="M3.5 12h5M15.5 12h5"/>"#,
    ),
    (
        "push",
        r#"<path d="M12 19V5"/><path d="m6 11 6-6 6 6"/><path d="M4.5 21.5h15"/>"#,
    ),
    (
        "pull",
        r#"<path d="M12 3v14"/><path d="m6 11 6 6 6-6"/><path d="M4.5 21.5h15"/>"#,
    ),
    (
        "fetch",
        r#"<path d="M20 12a8 8 0 1 1-2.35-5.65"/><path d="M20 4v4.5h-4.5"/><path d="M12 8.5v6"/><path d="m9.5 12 2.5 2.5 2.5-2.5"/>"#,
    ),
    (
        "refresh",
        r#"<path d="M20 7v5h-5"/><path d="M19 12a7 7 0 1 0-2 5"/>"#,
    ),
    (
        "pull-request",
        r#"<circle cx="6" cy="5.5" r="2"/><circle cx="6" cy="18.5" r="2"/><path d="M6 7.5v9"/><circle cx="18" cy="8.5" r="2"/><path d="M18 10.5c0 4-4 3.5-7 5.5"/>"#,
    ),
    (
        "split",
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/><path d="M6 9h3M6 12.5h3M15 9h3M15 15.5h3"/>"#,
    ),
    (
        "unified",
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9h10M7 12.5h7M7 16h9"/>"#,
    ),
    (
        "undo",
        r#"<path d="M9 14 4.5 9.5 9 5"/><path d="M4.5 9.5H14a5.5 5.5 0 0 1 0 11h-3"/>"#,
    ),
    (
        "more",
        r#"<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>"#,
    ),
    ("check", r#"<path d="m4.5 12.5 5 5 10-11"/>"#),
    ("close", r#"<path d="M6 6l12 12M18 6 6 18"/>"#),
    ("plus", r#"<path d="M12 5v14M5 12h14"/>"#),
    ("minus", r#"<path d="M5 12h14"/>"#),
    (
        "settings",
        r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21h-4v-.1A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H3v-4h.1A1.7 1.7 0 0 0 4.6 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.83-2.83.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V3h4v.1A1.7 1.7 0 0 0 15.4 4a1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.4 9c.13.37.34.7.6 1 .3.27.67.4 1.1.4h.1v4h-.1A1.7 1.7 0 0 0 19.4 15Z"/>"#,
    ),
    (
        "search",
        r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>"#,
    ),
    ("chevron", r#"<path d="m9 18 6-6-6-6"/>"#),
    ("chevron-down", r#"<path d="m6 9 6 6 6-6"/>"#),
    ("back", r#"<path d="m15 18-6-6 6-6"/>"#),
    ("trash", r#"<path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13"/>"#),
    (
        "copy",
        r#"<rect x="8" y="8" width="11" height="11" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/>"#,
    ),
    (
        "external",
        r#"<path d="M14 4h6v6"/><path d="M20 4 10.5 13.5"/><path d="M19 14v5.5h-14.5V9.5H10"/>"#,
    ),
    (
        "terminal",
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M12.5 15H17"/>"#,
    ),
    (
        "file",
        r#"<path d="M6 2.5h8l4 4V21.5H6z"/><path d="M14 2.5V7h4"/>"#,
    ),
    (
        "file-code",
        r#"<path d="M6 2.5h8l4 4V21.5H6z"/><path d="M14 2.5V7h4"/><path d="m10 12-2 2 2 2M14 12l2 2-2 2"/>"#,
    ),
    (
        "globe",
        r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.4 2.6 3.6 5.6 3.6 9s-1.2 6.4-3.6 9c-2.4-2.6-3.6-5.6-3.6-9S9.6 5.6 12 3z"/>"#,
    ),
    (
        "palette",
        r#"<path d="M12 3a9 9 0 0 0 0 18c1.1 0 1.8-.8 1.8-1.7 0-.5-.2-.9-.5-1.2-.3-.3-.5-.7-.5-1.2 0-.9.8-1.7 1.8-1.7H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3Z"/><circle cx="7.5" cy="11" r="1.2"/><circle cx="10" cy="7" r="1.2"/><circle cx="15" cy="7" r="1.2"/>"#,
    ),
    (
        "spark",
        r#"<path d="m12 3 1.2 4.8L18 9l-4.8 1.2L12 15l-1.2-4.8L6 9l4.8-1.2z"/><path d="m18 15 .7 2.3L21 18l-2.3.7L18 21l-.7-2.3L15 18l2.3-.7z"/>"#,
    ),
    (
        "lock",
        r#"<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>"#,
    ),
    (
        "clock",
        r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 12V7"/><path d="m12 12 3 1.8"/>"#,
    ),
    (
        "link",
        r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#,
    ),
    ("alert", r#"<path d="M12 4.5v9.5"/><path d="M12 19h.01"/>"#),
    (
        "pencil",
        r#"<path d="M4 20l1-4L16.5 4.5a2.1 2.1 0 0 1 3 3L8 19z"/><path d="m14.5 6.5 3 3"/>"#,
    ),
    (
        "merge",
        r#"<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="12" r="2"/><path d="M6 7v10"/><path d="M6 7c0 3 3.5 5 10 5"/>"#,
    ),
    (
        "rebase",
        r#"<circle cx="6" cy="19" r="2"/><circle cx="18" cy="5" r="2"/><circle cx="18" cy="13" r="2"/><path d="M6 17V9a4 4 0 0 1 4-4h6M18 7v4"/>"#,
    ),
    (
        "cherry",
        r#"<circle cx="7.5" cy="17" r="3"/><circle cx="16.5" cy="17" r="3"/><path d="M7.5 14c1-5 3.5-8 7.5-10M16.5 14c-.5-3-1.5-6-1.5-10"/>"#,
    ),
    (
        "tag",
        r#"<path d="M3.5 12.6V4.5a1 1 0 0 1 1-1h8.1l8 8a1.4 1.4 0 0 1 0 2l-7.1 7.1a1.4 1.4 0 0 1-2 0z"/><circle cx="8.5" cy="8.5" r="1.4"/>"#,
    ),
    (
        "user-plus",
        r#"<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M19 8v6M16 11h6"/>"#,
    ),
    (
        "amend",
        r#"<path d="M4 20l1-4L14.5 6.5a2.1 2.1 0 0 1 3 3L8 19z"/><path d="M15 20h5"/>"#,
    ),
    (
        "conflict",
        r#"<path d="M12 3.5 21 19.5H3z"/><path d="M12 10v4.5"/><path d="M12 17.5h.01"/>"#,
    ),
    (
        "stash",
        r#"<path d="M4 8h16v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z"/><path d="M3 4.5h18V8H3z"/><path d="M10 12h4"/>"#,
    ),
    (
        "cloud",
        r#"<path d="M7 18.5a4.5 4.5 0 0 1-.4-9 6 6 0 0 1 11.4 1.4 3.8 3.8 0 0 1-.6 7.6z"/>"#,
    ),
    (
        "eye-off",
        r#"<path d="M3 3l18 18"/><path d="M10.6 6.2A9.7 9.7 0 0 1 12 6c5 0 8.5 4.5 9.5 6a16 16 0 0 1-2.9 3.4M6.6 7.6A16 16 0 0 0 2.5 12c1 1.5 4.5 6 9.5 6a9 9 0 0 0 4.1-1"/><path d="M9.9 10a3 3 0 0 0 4.1 4.1"/>"#,
    ),
    (
        "dot",
        r#"<circle cx="12" cy="12" r="3.5" fill="currentColor"/>"#,
    ),
    (
        "arrow-up",
        r#"<path d="M12 19V5"/><path d="m6 11 6-6 6 6"/>"#,
    ),
    (
        "arrow-down",
        r#"<path d="M12 5v14"/><path d="m6 13 6 6 6-6"/>"#,
    ),
    (
        "repo",
        r#"<path d="M5 4.5A1.5 1.5 0 0 1 6.5 3H19v15H6.5A1.5 1.5 0 0 0 5 19.5z"/><path d="M5 19.5A1.5 1.5 0 0 0 6.5 21H19v-3"/><path d="M9 7h6"/>"#,
    ),
    (
        "whitespace",
        r#"<path d="M4 15v3h16v-3"/><path d="M9 9h6M12 9v6"/>"#,
    ),
    (
        "expand",
        r#"<path d="M12 3v6M9 6l3-3 3 3M12 21v-6M9 18l3 3 3-3"/>"#,
    ),
    (
        "git-branch",
        r#"<circle cx="7" cy="5" r="2"/><circle cx="7" cy="19" r="2"/><circle cx="17" cy="8" r="2"/><path d="M7 7v10M9 15c4 0 6-2 6-5"/>"#,
    ),
    (
        "history",
        r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 12V7"/><path d="m12 12 3 1.8"/>"#,
    ),
    (
        "sync",
        r#"<path d="M20 7v5h-5"/><path d="M19 12a7 7 0 1 0-2 5"/>"#,
    ),
    ("x", r#"<path d="M6 6l12 12M18 6 6 18"/>"#),
    ("chevron-right", r#"<path d="m9 18 6-6-6-6"/>"#),
    ("chevron-left", r#"<path d="m15 18-6-6 6-6"/>"#),
    (
        "ellipsis",
        r#"<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>"#,
    ),
    (
        "warning",
        r#"<path d="M12 3.5 21 19.5H3z"/><path d="M12 10v4.5"/><path d="M12 17.5h.01"/>"#,
    ),
    (
        "git-merge",
        r#"<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="12" r="2"/><path d="M6 7v10"/><path d="M6 7c0 3 3.5 5 10 5"/>"#,
    ),
    (
        "repo-add",
        r#"<path d="M3 6.5h6l2 2h10v10.5H3z"/><path d="M12 11.5v5M9.5 14h5"/>"#,
    ),
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
    svg().path(format!("icons/{name}.svg")).size_4()
}
