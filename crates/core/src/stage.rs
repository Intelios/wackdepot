//! Staging and discarding — file-level and hunk-level.
//!
//! Hunk ops rebuild a minimal patch from a parsed [`FileDiff`] + [`Hunk`]
//! (`header_text` + `patch_text`) and pipe it to `git apply`. The three
//! directions map to the same patch with different flags:
//!
//! | action | apply flags | base the patch applies to |
//! |---|---|---|
//! | `stage_hunk`   | `--cached`    | the index |
//! | `discard_hunk` | `-R`          | the worktree |
//! | `unstage_hunk` | `--cached -R` | the index |

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::diff::{FileDiff, Hunk};
use crate::git::{Git, failure_message};

/// Which `git apply` invocation a hunk op needs.
#[derive(Debug, Clone, Copy)]
enum ApplyMode {
    /// `git apply --cached` — stage.
    ToIndex,
    /// `git apply -R` — discard worktree changes.
    WorktreeReverse,
    /// `git apply --cached -R` — unstage.
    IndexReverse,
}

impl ApplyMode {
    fn flags(self) -> &'static [&'static str] {
        match self {
            Self::ToIndex => &["--cached"],
            Self::WorktreeReverse => &["-R"],
            Self::IndexReverse => &["--cached", "-R"],
        }
    }
}

/// The minimal applyable patch for one hunk of a file diff: the file's own
/// `diff --git`/`index`/`---`/`+++` header plus the hunk's raw text.
pub fn build_hunk_patch(file: &FileDiff, hunk: &Hunk) -> String {
    let mut p = file.header_text.clone();
    if !p.ends_with('\n') {
        p.push('\n');
    }
    p.push_str(&hunk.patch_text);
    if !p.ends_with('\n') {
        p.push('\n');
    }
    p
}

/// The minimal applyable patch for several hunks of the same file.
pub fn build_patch(file: &FileDiff, hunks: &[&Hunk]) -> String {
    let mut p = file.header_text.clone();
    if !p.ends_with('\n') {
        p.push('\n');
    }
    for h in hunks {
        p.push_str(&h.patch_text);
        if !p.ends_with('\n') {
            p.push('\n');
        }
    }
    p
}

fn apply(repo: &Path, patch: &str, mode: ApplyMode, verb: &str) -> Result<(), String> {
    let mut args = vec!["apply", "--whitespace=nowarn"];
    args.extend_from_slice(mode.flags());
    args.push("-");
    let out = Git::new(repo).try_run_stdin(args, patch)?;
    if out.ok() {
        return Ok(());
    }
    let msg = failure_message(&out);
    Err(if msg.is_empty() {
        format!(
            "Couldn't {verb} — git apply exited with status {}.",
            out.status
        )
    } else {
        format!("Couldn't {verb} — {}", lowercase_first(&msg))
    })
}

fn lowercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Stage just one hunk of a file's unstaged diff — the `git add -p` flow,
/// non-interactively. Works on synthesized untracked-file diffs too.
pub fn stage_hunk(repo: &Path, file: &FileDiff, hunk: &Hunk) -> Result<(), String> {
    apply(
        repo,
        &build_hunk_patch(file, hunk),
        ApplyMode::ToIndex,
        "stage that hunk",
    )
}

/// Discard one hunk's worktree changes (`git apply -R` — index untouched).
/// For a new-file diff the hunk IS the file — discarding it routes to
/// [`trash_file`] instead of deleting outright.
pub fn discard_hunk(repo: &Path, file: &FileDiff, hunk: &Hunk) -> Result<(), String> {
    if file.status == crate::diff::DiffFileStatus::Added {
        return trash_file(repo, file.path()).map(|_| ());
    }
    apply(
        repo,
        &build_hunk_patch(file, hunk),
        ApplyMode::WorktreeReverse,
        "discard that hunk",
    )
}

/// Unstage one hunk — reverse-apply it to the index; the worktree keeps it.
pub fn unstage_hunk(repo: &Path, file: &FileDiff, hunk: &Hunk) -> Result<(), String> {
    apply(
        repo,
        &build_hunk_patch(file, hunk),
        ApplyMode::IndexReverse,
        "unstage that hunk",
    )
}

/// `git add -- <path>` — stages new, modified and deleted paths alike.
pub fn stage_file(repo: &Path, path: &str) -> Result<(), String> {
    Git::new(repo).run(["add", "--", path])?;
    Ok(())
}

/// `git add -A` — everything, including deletions and untracked files.
pub fn stage_all(repo: &Path) -> Result<(), String> {
    Git::new(repo).run(["add", "-A"])?;
    Ok(())
}

/// `git restore --staged -- <path>`; falls back to `git reset -q HEAD --` on
/// old git and to `git rm --cached` on an unborn HEAD (there's no HEAD to
/// restore from, but the entry still leaves the index).
pub fn unstage_file(repo: &Path, path: &str) -> Result<(), String> {
    let g = Git::new(repo);
    if g.try_run(["restore", "--staged", "--", path])?.ok() {
        return Ok(());
    }
    if g.try_run(["reset", "-q", "HEAD", "--", path])?.ok() {
        return Ok(());
    }
    g.run(["rm", "-q", "--cached", "--", path])?;
    Ok(())
}

/// Unstage everything (`git restore --staged :/`, same fallbacks).
pub fn unstage_all(repo: &Path) -> Result<(), String> {
    let g = Git::new(repo);
    if g.try_run(["restore", "--staged", ":/"])?.ok() {
        return Ok(());
    }
    if g.try_run(["reset", "-q", "HEAD"])
        .ok()
        .map(|o| o.ok())
        .unwrap_or(false)
    {
        return Ok(());
    }
    g.run(["rm", "-r", "-q", "--cached", ":/"])?;
    Ok(())
}

/// Throw away `path`'s worktree changes. Untracked files are moved to a
/// backup dir ([`trash_file`]) instead of being deleted outright.
pub fn discard_file(repo: &Path, path: &str) -> Result<(), String> {
    let g = Git::new(repo);
    let tracked = g
        .try_run(["ls-files", "--error-unmatch", "--", path])
        .map(|o| o.ok())
        .unwrap_or(false);
    if !tracked {
        trash_file(repo, path)?;
        return Ok(());
    }
    if g.try_run(["restore", "--", path])?.ok() {
        return Ok(());
    }
    // git < 2.23: `checkout --` is the same worktree restore.
    g.run(["checkout", "--", path])?;
    Ok(())
}

/// Move an untracked file (or directory) out of the working tree into
/// `$TMPDIR/wackdepot-trash/<ts>/<name>` — never a hard delete, so a
/// misclick is recoverable. Returns the backup location.
pub fn trash_file(repo: &Path, path: &str) -> Result<PathBuf, String> {
    let src = repo.join(path);
    if !src.exists() && !src.is_symlink() {
        return Err(format!("Couldn't discard {path} — the file isn't there."));
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::env::temp_dir()
        .join("wackdepot-trash")
        .join(format!("{ts}"));
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Couldn't create a trash folder for {path} — {e}."))?;
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let dst = dir.join(name);
    if fs::rename(&src, &dst).is_err() {
        // Cross-volume fallback: copy then remove (files only — a directory
        // copy isn't worth the risk; report it instead).
        if src.is_dir() {
            return Err(format!(
                "Couldn't move {path} to the trash folder across volumes — delete it manually."
            ));
        }
        fs::copy(&src, &dst)
            .and_then(|_| fs::remove_file(&src))
            .map_err(|e| format!("Couldn't move {path} to the trash folder — {e}."))?;
    }
    Ok(dst)
}

/// `git add` a conflicted path — marks it resolved once the user has edited
/// the conflict markers away.
pub fn mark_resolved(repo: &Path, path: &str) -> Result<(), String> {
    stage_file(repo, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::diff::{DiffLineKind, DiffSide, diff};
    use crate::status::status;
    use crate::testutil::{read_file, temp_repo, write_file};

    fn staged_names(repo: &Path) -> Vec<String> {
        Git::new(repo)
            .run(["diff", "--cached", "--name-only"])
            .unwrap()
            .stdout
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn stage_and_unstage_file() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_file(dir.path(), "a.txt").unwrap();
        assert_eq!(staged_names(dir.path()), vec!["a.txt"]);

        unstage_file(dir.path(), "a.txt").unwrap();
        assert!(staged_names(dir.path()).is_empty());
        // Unborn-HEAD unstage leaves the file untracked, not deleted.
        assert!(dir.path().join("a.txt").exists());
    }

    #[test]
    fn stage_all_and_unstage_all() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        write_file(dir.path(), "b.txt", "2\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        write_file(dir.path(), "a.txt", "1!\n");
        write_file(dir.path(), "c.txt", "new\n");
        stage_all(dir.path()).unwrap();
        assert_eq!(staged_names(dir.path()).len(), 2);
        unstage_all(dir.path()).unwrap();
        assert!(staged_names(dir.path()).is_empty());
    }

    #[test]
    fn discard_hunk_reverts_worktree() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "f.txt", "a\nb\nc\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();

        write_file(dir.path(), "f.txt", "a\nB\nc\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        discard_hunk(dir.path(), &d, &d.hunks[0]).unwrap();
        assert_eq!(read_file(dir.path(), "f.txt"), "a\nb\nc\n");
        assert!(status(dir.path()).unwrap().unstaged.is_empty());
    }

    #[test]
    fn unstage_hunk_keeps_worktree() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "f.txt", "a\nb\nc\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();

        write_file(dir.path(), "f.txt", "a\nB\nc\n");
        stage_file(dir.path(), "f.txt").unwrap();

        let d = diff(dir.path(), "f.txt", DiffSide::Staged).unwrap();
        assert_eq!(d.hunks.len(), 1);
        unstage_hunk(dir.path(), &d, &d.hunks[0]).unwrap();

        assert!(staged_names(dir.path()).is_empty());
        assert_eq!(read_file(dir.path(), "f.txt"), "a\nB\nc\n");
        let s = status(dir.path()).unwrap();
        assert_eq!(s.unstaged.len(), 1);
    }

    #[test]
    fn discard_file_restores_and_trashes() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "t.txt", "orig\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();

        write_file(dir.path(), "t.txt", "dirty\n");
        discard_file(dir.path(), "t.txt").unwrap();
        assert_eq!(read_file(dir.path(), "t.txt"), "orig\n");

        // Untracked: moved to trash, not deleted.
        write_file(dir.path(), "u.txt", "temp\n");
        let backup = trash_file(dir.path(), "u.txt").unwrap();
        assert!(!dir.path().join("u.txt").exists());
        assert!(backup.exists());
        discard_file(dir.path(), "missing.txt").unwrap_err();

        // discard_hunk on a new-file diff trashes too (the hunk IS the file).
        write_file(dir.path(), "v.txt", "temp\n");
        let d = diff(dir.path(), "v.txt", DiffSide::Unstaged).unwrap();
        discard_hunk(dir.path(), &d, &d.hunks[0]).unwrap();
        assert!(!dir.path().join("v.txt").exists());
    }

    #[test]
    fn stage_hunk_on_partial_diff() {
        let (dir, _g) = temp_repo();
        let mut lines: Vec<String> = (1..=20).map(|i| format!("line{i}")).collect();
        write_file(dir.path(), "f.txt", &(lines.join("\n") + "\n"));
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        lines[0] = "TOP".into();
        lines[19] = "BOTTOM".into();
        write_file(dir.path(), "f.txt", &(lines.join("\n") + "\n"));

        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.hunks.len(), 2);
        // Stage only the second hunk.
        stage_hunk(dir.path(), &d, &d.hunks[1]).unwrap();

        let staged = Git::new(dir.path())
            .run(["diff", "--cached"])
            .unwrap()
            .stdout;
        assert!(staged.contains("+BOTTOM"));
        assert!(!staged.contains("+TOP"));

        // Worktree still has both changes; the index has only BOTTOM.
        let remaining = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(remaining.hunks.len(), 1);
        assert!(
            remaining.hunks[0]
                .lines
                .iter()
                .any(|l| { l.kind == DiffLineKind::Added && l.text == "TOP" })
        );
    }
}
