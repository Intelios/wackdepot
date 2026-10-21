//! `LICENSE` — the license texts [choosealicense.com](https://github.com/github/choosealicense.com)
//! (CC0) publishes for API consumers, vendored in `licenses/` and indexed by
//! `build.rs` — the same list GitHub Desktop's create-repository dialog shows.
//!
//! Each entry's name is the upstream `nickname`/`title` and its body keeps the
//! `[token]`/`{token}` placeholders filled from [`LicenseFields`] when the
//! file is written.

use std::fs;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/license_index.rs"));

/// A licensable text the create-repository dialog offers.
#[derive(Debug, Clone, Copy)]
pub struct License {
    /// Display name (`nickname` or `title` upstream).
    pub name: &'static str,
    /// Shown ahead of the rest in the picker (upstream `featured: true`).
    pub featured: bool,
    text: &'static str,
}

impl License {
    /// The license text, front matter stripped, ending in a newline.
    pub fn body(&self) -> String {
        let body = self.text.split("---").nth(2).unwrap_or(self.text).trim();
        format!("{body}\n")
    }
}

/// Every shippable license, name-sorted — the picker's order.
pub fn licenses() -> impl Iterator<Item = License> {
    LICENSES.iter().map(|(name, featured, text)| License {
        name,
        featured: *featured,
        text,
    })
}

/// A license by display name, `None` for an unknown one.
pub fn license(name: &str) -> Option<License> {
    licenses().find(|l| l.name.eq_ignore_ascii_case(name))
}

/// The values a license's placeholders are filled with — mirroring the
/// fields GitHub Desktop substitutes (`[token]` and `{token}` both count).
#[derive(Debug, Clone, Default)]
pub struct LicenseFields {
    pub fullname: String,
    pub email: String,
    pub project: String,
    pub description: String,
    pub year: String,
}

/// `fields` applied to `body`: every `[token]`/`{token}` for each field is
/// replaced with its value.
pub fn fill(body: &str, fields: &LicenseFields) -> String {
    let tokens = [
        ("fullname", &fields.fullname),
        ("email", &fields.email),
        ("project", &fields.project),
        ("description", &fields.description),
        ("year", &fields.year),
    ];
    let mut out = body.to_string();
    for (token, value) in tokens {
        out = out
            .replace(&format!("[{token}]"), &format!("{{{token}}}"))
            .replace(&format!("{{{token}}}"), value);
    }
    out
}

/// Write `license`'s text (placeholders filled from `fields`) as
/// `<repo>/LICENSE`.
pub fn write_license(repo: &Path, license: &License, fields: &LicenseFields) -> Result<(), String> {
    fs::write(repo.join("LICENSE"), fill(&license.body(), fields))
        .map_err(|e| format!("Couldn't write LICENSE — {e}."))
}

/// A name GitHub accepts for a repository: everything outside `[\w.-]`
/// (ASCII word characters — so emoji and non-ASCII letters too) becomes `-`,
/// like GitHub Desktop's `sanitizedRepositoryName`. May come back empty.
pub fn sanitized_repository_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_list_matches_desktops() {
        let all: Vec<_> = licenses().collect();
        assert_eq!(all.len(), 13);
        // Featured first in the dialog; these are upstream's featured three.
        let featured: Vec<_> = all.iter().filter(|l| l.featured).map(|l| l.name).collect();
        assert_eq!(featured, ["Apache License 2.0", "GNU GPLv3", "MIT License"]);
        assert!(license("MIT License").is_some());
        assert!(license("mit license").is_some());
        assert!(license("WTFPL").is_none()); // hidden upstream → not shipped
    }

    #[test]
    fn body_strips_front_matter_and_ends_in_newline() {
        let body = license("MIT License").unwrap().body();
        assert!(body.starts_with("MIT License"));
        assert!(body.ends_with('\n'));
        assert!(body.contains("[year] [fullname]"));
    }

    #[test]
    fn fields_fill_both_placeholder_styles() {
        let fields = LicenseFields {
            fullname: "Ada".into(),
            year: "2026".into(),
            project: "thing".into(),
            ..Default::default()
        };
        let out = fill("[year] {year} [fullname] [project] [email]", &fields);
        assert_eq!(out, "2026 2026 Ada thing ");
        let mit = fill(&license("MIT License").unwrap().body(), &fields);
        assert!(mit.contains("Copyright (c) 2026 Ada"));
        assert!(!mit.contains("[year]") && !mit.contains("[fullname]"));
    }

    #[test]
    fn writes_the_filled_license() {
        let dir = tempfile::tempdir().unwrap();
        let fields = LicenseFields {
            fullname: "Ada".into(),
            year: "2026".into(),
            ..Default::default()
        };
        write_license(dir.path(), &license("MIT License").unwrap(), &fields).unwrap();
        let text = fs::read_to_string(dir.path().join("LICENSE")).unwrap();
        assert!(text.contains("Copyright (c) 2026 Ada"));
    }

    #[test]
    fn sanitizes_repository_names() {
        assert_eq!(sanitized_repository_name("my repo!"), "my-repo-");
        assert_eq!(
            sanitized_repository_name("already.ok_name-1"),
            "already.ok_name-1"
        );
        assert_eq!(sanitized_repository_name("café"), "caf-");
        assert_eq!(sanitized_repository_name(""), "");
        assert_eq!(sanitized_repository_name("!!!"), "---");
    }
}
