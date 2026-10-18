//! Source-level guards. Colours come only from `theme/` (AGENTS.md: theme
//! tokens only); the legacy UI is exempt until it is deleted.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "legacy" && name != "theme" {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn colours_come_only_from_the_theme() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let banned = ["rgb(", "rgba(", "hsla(", "hsl(", "Hsla {"];
    let mut hits = Vec::new();
    for file in files {
        if file.ends_with("lint.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).expect("read source");
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if banned.iter().any(|b| code.contains(b)) {
                hits.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "raw colour literals outside theme/:\n{}",
        hits.join("\n")
    );
}
