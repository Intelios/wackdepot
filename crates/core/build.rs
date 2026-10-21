//! Builds the .gitignore template index: one `("Name", include_str!(…))`
//! pair per file vendored in `gitignore/` (the github/gitignore set, CC0).

use std::env;
use std::fs;
use std::path::Path;

fn main() {
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
