//! Repository discovery — `is_repo`, `repo_root`, `git_version`.

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
}
