//! Whole-file text for either side of a diff — what syntax highlighting
//! runs over, so a hunk's lines are coloured with the full file's context
//! rather than parsed from a fragment.

use std::fs;
use std::path::Path;

use crate::diff::DiffSide;
use crate::git::Git;

/// Files larger than this aren't loaded for highlighting.
const MAX_BYTES: usize = 512 * 1024;

/// The text on each side of a change. `None` where the side doesn't exist
/// (a new or deleted file) or isn't text (binary, oversized).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Versions {
    pub old: Option<String>,
    pub new: Option<String>,
}

/// The two sides of a working-tree change:
/// - `Unstaged`: index (`:path`, falling back to `HEAD`) → worktree file.
/// - `Staged`: `HEAD:path` → index (`:path`).
///
/// `old_path` is the pre-rename path when it differs.
pub fn file_versions(repo: &Path, path: &str, old_path: Option<&str>, side: DiffSide) -> Versions {
    let g = Git::new(repo);
    let old_path = old_path.unwrap_or(path);
    match side {
        DiffSide::Unstaged => Versions {
            old: show(&g, &format!(":{old_path}"))
                .or_else(|| show(&g, &format!("HEAD:{old_path}"))),
            new: read_worktree(repo, path),
        },
        DiffSide::Staged => Versions {
            old: show(&g, &format!("HEAD:{old_path}")),
            new: show(&g, &format!(":{path}")),
        },
    }
}

/// The two index stages of a conflicted file — ours (`:2:`) and theirs
/// (`:3:`) — for highlighting the combined-diff view. A side deleted in
/// the merge has no blob and comes back `None`.
pub fn conflict_versions(repo: &Path, path: &str) -> Versions {
    let g = Git::new(repo);
    Versions {
        old: show(&g, &format!(":2:{path}")),
        new: show(&g, &format!(":3:{path}")),
    }
}

/// The two sides of a file in commit `sha`: first parent → commit. Root
/// commits have no old side.
pub fn commit_file_versions(
    repo: &Path,
    sha: &str,
    path: &str,
    old_path: Option<&str>,
) -> Versions {
    let g = Git::new(repo);
    let old_path = old_path.unwrap_or(path);
    Versions {
        old: show(&g, &format!("{sha}^:{old_path}")),
        new: show(&g, &format!("{sha}:{path}")),
    }
}

/// Text at an exact pair of revisions, used by unpublished PR previews.
pub fn versions_between(
    repo: &Path,
    base: &str,
    head: &str,
    old_path: Option<&str>,
    new_path: Option<&str>,
) -> Versions {
    let git = Git::new(repo);
    Versions {
        old: old_path.and_then(|path| show(&git, &format!("{base}:{path}"))),
        new: new_path.and_then(|path| show(&git, &format!("{head}:{path}"))),
    }
}

fn show(g: &Git, spec: &str) -> Option<String> {
    let out = g.try_run(["cat-file", "-p", spec]).ok()?;
    if !out.ok() {
        return None;
    }
    text_or_none(out.stdout.into_bytes())
}

fn read_worktree(repo: &Path, path: &str) -> Option<String> {
    let full = repo.join(path);
    if fs::metadata(&full).ok()?.len() as usize > MAX_BYTES {
        return None;
    }
    text_or_none(fs::read(full).ok()?)
}

/// UTF-8 text under the size cap with no NUL in the first 8KB.
fn text_or_none(bytes: Vec<u8>) -> Option<String> {
    if bytes.len() > MAX_BYTES || bytes.iter().take(8192).any(|&b| b == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::stage::{stage_all, stage_file};
    use crate::testutil::{temp_repo, write_file};

    #[test]
    fn sides_follow_the_bucket() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.rs", "one\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();

        write_file(dir.path(), "f.rs", "two\n");
        stage_file(dir.path(), "f.rs").unwrap();
        write_file(dir.path(), "f.rs", "three\n");

        let staged = file_versions(dir.path(), "f.rs", None, DiffSide::Staged);
        assert_eq!(staged.old.as_deref(), Some("one\n"));
        assert_eq!(staged.new.as_deref(), Some("two\n"));
        let unstaged = file_versions(dir.path(), "f.rs", None, DiffSide::Unstaged);
        assert_eq!(unstaged.old.as_deref(), Some("two\n"));
        assert_eq!(unstaged.new.as_deref(), Some("three\n"));

        let sha = g
            .run(["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        let root = commit_file_versions(dir.path(), &sha, "f.rs", None);
        assert_eq!(root.old, None);
        assert_eq!(root.new.as_deref(), Some("one\n"));
    }

    #[test]
    fn untracked_and_binary() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "new.txt", "hi\n");
        let v = file_versions(dir.path(), "new.txt", None, DiffSide::Unstaged);
        assert_eq!(v.old, None);
        assert_eq!(v.new.as_deref(), Some("hi\n"));

        std::fs::write(dir.path().join("b.bin"), [1u8, 0, 2]).unwrap();
        let v = file_versions(dir.path(), "b.bin", None, DiffSide::Unstaged);
        assert_eq!(v.new, None);
    }
}
