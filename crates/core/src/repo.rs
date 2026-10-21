//! Repository discovery — `is_repo`, `repo_root`, `find_repos`, `git_version`.

use std::path::{Path, PathBuf};

use crate::git::{Git, run_global};

/// Whether `path` sits inside a git working tree (bare repos count too —
/// `rev-parse --git-dir` succeeds for both).
pub fn is_repo(path: &Path) -> bool {
    Git::new(path)
        .try_run(["rev-parse", "--git-dir"])
        .map(|o| o.ok())
        .unwrap_or(false)
}

/// The working-tree root containing `path` — resolves through subdirectories
/// and linked worktrees (`rev-parse --show-toplevel`).
pub fn repo_root(path: &Path) -> Result<PathBuf, String> {
    let out = Git::new(path).run(["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.stdout.trim()))
}

/// Directory names the scan never descends into: build outputs, package
/// caches and virtualenvs — they can hold vendored clones, never projects.
const SCAN_SKIP: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    "Pods",
    "DerivedData",
    "__pycache__",
    "venv",
];

/// Every repository under `root`, found by a pure filesystem walk (no git
/// subprocess — fast on thousands of directories, and testable without git).
///
/// A folder is a repo when it holds a `.git` entry, directory *or* file —
/// linked worktrees and submodule checkouts use a file. The walk stops at a
/// repo, so nested clones inside one are not separate projects; if `root`
/// itself is a repo the result is just `root`. Hidden folders and [`SCAN_SKIP`]
/// names are never descended, and directory symlinks are not followed (loop
/// safety). Unreadable subdirectories are skipped silently.
///
/// `Err` only when `root` isn't a directory — a user-facing sentence.
/// Results are sorted for stable display.
pub fn find_repos(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Err("That folder can't be read.".into());
    }
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir.join(".git").exists() {
            found.push(dir);
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || SCAN_SKIP.contains(&name.as_ref()) {
                continue;
            }
            // `file_type` never traverses symlinks, so this skips files,
            // symlinks-to-anything and other non-directories alike.
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            stack.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

/// The installed git's version string, e.g. `git version 2.51.2` → `2.51.2`.
pub fn git_version() -> Result<String, String> {
    let out = run_global(["--version"])?;
    Ok(out
        .stdout
        .trim()
        .strip_prefix("git version ")
        .unwrap_or(out.stdout.trim())
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_repo;

    #[test]
    fn detects_repos_and_roots() {
        let (dir, _g) = temp_repo();
        assert!(is_repo(dir.path()));
        assert_eq!(
            repo_root(dir.path()).unwrap(),
            dir.path().canonicalize().unwrap()
        );

        // Subdirectories resolve back to the root.
        let sub = dir.path().join("deep/nested");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(is_repo(&sub));
        assert_eq!(repo_root(&sub).unwrap(), dir.path().canonicalize().unwrap());

        // A plain directory is not a repo.
        let plain = tempfile::tempdir().unwrap();
        assert!(!is_repo(plain.path()));
        assert!(repo_root(plain.path()).is_err());
    }

    #[test]
    fn version_parses() {
        let v = git_version().unwrap();
        assert!(v.chars().next().is_some_and(|c| c.is_ascii_digit()));
    }

    /// A folder with a `.git` entry stands in for a repo — the scan never
    /// runs git, so tests need no real one.
    fn fake_repo(parent: &Path, rel: &str) -> PathBuf {
        let p = parent.join(rel);
        std::fs::create_dir_all(p.join(".git")).unwrap();
        p
    }

    fn scan(root: &Path) -> Vec<String> {
        find_repos(root)
            .unwrap()
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn finds_direct_and_nested_repos() {
        let dir = tempfile::tempdir().unwrap();
        fake_repo(dir.path(), "alpha");
        fake_repo(dir.path(), "org/inner/nested");
        std::fs::create_dir_all(dir.path().join("plain/other")).unwrap();
        assert_eq!(scan(dir.path()), vec!["alpha", "org/inner/nested"]);
    }

    #[test]
    fn stops_at_repos() {
        let dir = tempfile::tempdir().unwrap();
        fake_repo(dir.path(), "outer");
        fake_repo(dir.path(), "outer/vendored");
        // Only the outer repo — the clone inside it is not a project.
        assert_eq!(scan(dir.path()), vec!["outer"]);

        // A repo root scans to itself alone.
        assert_eq!(scan(&dir.path().join("outer")), vec![""]);
    }

    #[test]
    fn git_file_counts_as_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let wt = dir.path().join("worktree");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), "gitdir: ../real/.git/worktrees/wt").unwrap();
        assert_eq!(scan(dir.path()), vec!["worktree"]);
    }

    #[test]
    fn skips_build_and_cache_dirs() {
        let dir = tempfile::tempdir().unwrap();
        for skip in ["node_modules", "target", "dist", "vendor", ".hidden"] {
            fake_repo(dir.path(), &format!("{skip}/decoy"));
        }
        fake_repo(dir.path(), "real");
        assert_eq!(scan(dir.path()), vec!["real"]);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        fake_repo(dir.path(), "real");
        // A self-referential link must not hang the walk, and a link to a
        // real repo must not surface it a second time.
        std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
        assert_eq!(scan(dir.path()), vec!["real"]);
    }

    #[test]
    fn sorts_and_rejects_non_directories() {
        let dir = tempfile::tempdir().unwrap();
        fake_repo(dir.path(), "zeta");
        fake_repo(dir.path(), "alpha");
        assert_eq!(scan(dir.path()), vec!["alpha", "zeta"]);

        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(find_repos(&file).is_err());
        assert!(find_repos(&dir.path().join("missing")).is_err());
    }
}
