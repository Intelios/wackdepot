//! Builds the .gitignore template index: one `("Name", include_str!(…))`
//! pair per file vendored in `gitignore/` (the github/gitignore set, CC0) —
//! and the license index for `licenses/` (the choosealicense.com texts
//! GitHub Desktop shows in its create-repository dialog).
//!
//! License front matter (`title`/`nickname`/`featured`/`hidden`) is parsed
//! here, at vendoring time, applying Desktop's own rules: a license is only
//! shipped when it explicitly says `hidden: false`, and its display name is
//! `nickname` when present, else `title`.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    gitignore_index();
    license_index();
}

fn gitignore_index() {
    let dir = Path::new("gitignore");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("gitignore/ template dir")
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".gitignore").map(str::to_string)
        })
        .collect();
    names.sort_by_key(|a| a.to_lowercase());

    let mut out = String::from("static TEMPLATES: &[(&str, &str)] = &[\n");
    for name in &names {
        out.push_str(&format!(
            "    ({name:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/gitignore/{name}.gitignore\"))),\n"
        ));
    }
    out.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("gitignore_templates.rs"),
        out,
    )
    .unwrap();
}

/// The `key: value` front matter line, unquoted.
fn front_matter<'a>(fm: &'a str, key: &str) -> Option<&'a str> {
    fm.lines()
        .find_map(|l| l.trim().strip_prefix(&format!("{key}:")))
        .map(|v| v.trim().trim_matches('"'))
}

fn license_index() {
    let dir = Path::new("licenses");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<(
        String, /*stem*/
        String, /*name*/
        bool,   /*featured*/
    )> = Vec::new();
    for e in fs::read_dir(dir).expect("licenses/ dir") {
        let stem = e.unwrap().file_name().into_string().unwrap();
        let Some(stem) = stem.strip_suffix(".txt").map(str::to_string) else {
            continue;
        };
        let text = fs::read_to_string(dir.join(format!("{stem}.txt"))).unwrap();
        let Some(fm) = text.split("---").nth(1) else {
            panic!("licenses/{stem}.txt has no front matter");
        };
        // Only licenses explicitly published to API consumers ship.
        if front_matter(fm, "hidden") != Some("false") {
            continue;
        }
        let name = front_matter(fm, "nickname")
            .or_else(|| front_matter(fm, "title"))
            .unwrap_or_else(|| panic!("licenses/{stem}.txt has no title"))
            .to_string();
        let featured = front_matter(fm, "featured") == Some("true");
        entries.push((stem, name, featured));
    }
    entries.sort_by_key(|(_, name, _)| name.to_lowercase());

    let mut out = String::from("static LICENSES: &[(&str, bool, &str)] = &[\n");
    for (stem, name, featured) in &entries {
        out.push_str(&format!(
            "    ({name:?}, {featured:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/licenses/{stem}.txt\"))),\n"
        ));
    }
    out.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("license_index.rs"),
        out,
    )
    .unwrap();
}
