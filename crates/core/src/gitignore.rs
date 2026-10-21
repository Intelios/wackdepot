//! `.gitignore` — the vendored [github/gitignore](https://github.com/github/gitignore)
//! template set (CC0) plus the edits the UI makes: picking a template when a
//! repo is created, and "ignore this file / all `*.ext` files" from the
//! changes list.
//!
//! Template bodies are indexed by `build.rs` scanning `gitignore/`; the file
//! names stay exactly what the upstream repo calls them so names match what
//! GitHub shows.

use std::fs;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/gitignore_templates.rs"));

/// Every template name, sorted ignoring case — the picker's order.
pub fn templates() -> impl Iterator<Item = &'static str> {
    TEMPLATES.iter().map(|(name, _)| *name)
}

/// A template's body, `None` for an unknown name.
pub fn template(name: &str) -> Option<&'static str> {
    TEMPLATES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, body)| *body)
}

/// `path` as a gitignore pattern: the literal path rooted at the repo root.
/// Characters gitignore would read specially are backslash-escaped — `#` and
/// `!` lead a comment/negation, `[…]` is a character class, `*` and `?` are
/// wildcards — and whitespace at the edges is escaped before it can be
/// trimmed away.
pub fn escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 1);
    out.push('/');
    for c in path.trim_matches('/').chars() {
        match c {
            '#' | '!' | '[' | ']' | '*' | '?' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    if out.starts_with("/ ") {
        out.replace_range(1..2, "\\ ");
    }
    if out.ends_with(' ') {
        out.pop();
        out.push_str("\\ ");
    }
    out
}

/// The ignore-everything-of-this-kind pattern for `path`'s extension:
/// `foo.log` → `*.log`. `None` when the path has no usable extension, so the
/// caller can hide the menu row.
pub fn extension_pattern(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    let (stem, ext) = name.rsplit_once('.')?;
    // `.env` has no extension — the leading dot is the name (like
    // Node's `path.extname`), so there's no "all *.env files" row.
    (!stem.is_empty() && !ext.is_empty()).then(|| format!("*.{ext}"))
}

/// Write `contents` as `<repo>/.gitignore` — what picking a template in
/// the new-repo dialog becomes. The repo is fresh, so there's nothing to
/// merge with.
pub fn write_gitignore(repo: &Path, contents: &str) -> Result<(), String> {
    fs::write(repo.join(".gitignore"), contents)
        .map_err(|e| format!("Couldn't write .gitignore — {e}."))
}

/// Append `patterns` to `<repo>/.gitignore`, creating the file if it's
/// missing. Patterns already present are skipped — right-clicking ignore
/// twice mustn't pile up duplicates.
pub fn update_gitignore(repo: &Path, patterns: &[String]) -> Result<(), String> {
    if patterns.is_empty() {
        return Ok(());
    }
    let file = repo.join(".gitignore");
    let existing = fs::read_to_string(&file).unwrap_or_default();
    let have: Vec<&str> = existing.lines().map(str::trim).collect();
    let mut text = existing.trim_end().to_string();
    for pattern in patterns {
        if have.contains(&pattern.as_str()) {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(pattern);
    }
    fs::write(&file, format!("{text}\n")).map_err(|e| format!("Couldn't update .gitignore — {e}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_indexed() {
        let names: Vec<_> = templates().collect();
        assert!(names.len() > 100);
        // Sorted case-insensitively.
        let mut sorted = names.clone();
        sorted.sort_by_key(|a| a.to_lowercase());
        assert_eq!(names, sorted);
        // The stalwarts shipped.
        for n in ["Node", "Rust", "Python", "macOS", "JetBrains", "Go"] {
            assert!(template(n).is_some(), "{n} missing");
        }
        assert!(template("Node").unwrap().contains("node_modules"));
        assert!(template("not-a-template").is_none());
    }

    #[test]
    fn template_lookup_is_case_insensitive() {
        assert_eq!(template("rust"), template("Rust"));
        assert_eq!(template("macos"), template("macOS"));
    }

    #[test]
    fn escape_literals() {
        assert_eq!(escape("src/main.rs"), "/src/main.rs");
        assert_eq!(escape("#tag.txt"), "/\\#tag.txt");
        assert_eq!(escape("a!b"), "/a\\!b");
        assert_eq!(escape("crazy[0].txt"), "/crazy\\[0\\].txt");
        assert_eq!(escape("what?.txt"), "/what\\?.txt");
        assert_eq!(escape("star*file"), "/star\\*file");
        // Root-anchored no matter what the caller passes.
        assert_eq!(escape("/already/rooted"), "/already/rooted");
        assert_eq!(escape("trailing "), "/trailing\\ ");
        assert_eq!(escape(" lead"), "/\\ lead");
    }

    #[test]
    fn extension_patterns() {
        assert_eq!(extension_pattern("foo.log").as_deref(), Some("*.log"));
        assert_eq!(extension_pattern("a/b.min.js").as_deref(), Some("*.js"));
        assert_eq!(extension_pattern("Makefile"), None);
        assert_eq!(extension_pattern(".env"), None); // no name before the dot
        assert_eq!(extension_pattern("dir/"), None);
    }

    #[test]
    fn update_creates_and_appends() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        update_gitignore(repo, &["/target".into(), "*.log".into()]).unwrap();
        assert_eq!(
            fs::read_to_string(repo.join(".gitignore")).unwrap(),
            "/target\n*.log\n"
        );

        // Appends on a newline and skips what's already there.
        update_gitignore(repo, &["*.log".into(), "build/".into()]).unwrap();
        assert_eq!(
            fs::read_to_string(repo.join(".gitignore")).unwrap(),
            "/target\n*.log\nbuild/\n"
        );

        // A file without a trailing newline still gets a clean append.
        fs::write(repo.join(".gitignore"), "one").unwrap();
        update_gitignore(repo, &["two".into()]).unwrap();
        assert_eq!(
            fs::read_to_string(repo.join(".gitignore")).unwrap(),
            "one\ntwo\n"
        );
    }
}
