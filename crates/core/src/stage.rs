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

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::diff::{DiffFileStatus, DiffLineKind, FileDiff, Hunk, git_quote};
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
    if file.status == DiffFileStatus::Added {
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

/// Lines picked in a file diff, as `(hunk index, line index in hunk.lines)`.
/// Only added and removed lines are meaningful; context is ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineSel(pub BTreeSet<(usize, usize)>);

impl LineSel {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn has(&self, hunk: usize, line: usize) -> bool {
        self.0.contains(&(hunk, line))
    }
}

/// Which way a partial patch is built. `Forward` patches apply to the
/// diff's old side (staging into the index). `Reverse` patches are applied
/// with `-R` against the new side (unstaging from the index, discarding from
/// the worktree).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchDir {
    Forward,
    Reverse,
}

/// A patch carrying only the selected lines of `file` — GitHub Desktop's
/// partial-staging rules. Unselected lines are dropped from, or kept as
/// context in, whichever side the patch applies to:
///
/// | line | selected | Forward | Reverse |
/// |---|---|---|---|
/// | `+` | yes | `+` | `+` |
/// | `+` | no  | dropped | context |
/// | `-` | yes | `-` | `-` |
/// | `-` | no  | context | dropped |
///
/// Hunks with nothing selected are left out and headers are recounted.
/// `None` when nothing selected is an added or removed line.
pub fn build_partial_patch(file: &FileDiff, sel: &LineSel, dir: PatchDir) -> Option<String> {
    let mut body = String::new();
    // Running (new − old) line delta of the hunks emitted so far: hunks we
    // skip or trim shift every later hunk's position on the other side.
    let mut delta: i64 = 0;
    let mut any = false;
    let mut total_changes = 0usize;
    let mut picked_changes = 0usize;

    for (hi, hunk) in file.hunks.iter().enumerate() {
        let mut lines = String::new();
        let (mut old_n, mut new_n) = (0u32, 0u32);
        let mut picked = false;
        // Whether the previous source line made it into the patch — a
        // "\ No newline" marker belongs to the line just before it.
        let mut prev_kept = false;
        for (li, line) in hunk.lines.iter().enumerate() {
            let selected = sel.has(hi, li);
            let emit = match (line.kind, selected, dir) {
                (DiffLineKind::Context, _, _) => Some(' '),
                (DiffLineKind::Added, true, _) => Some('+'),
                (DiffLineKind::Removed, true, _) => Some('-'),
                (DiffLineKind::Added, false, PatchDir::Forward) => None,
                (DiffLineKind::Added, false, PatchDir::Reverse) => Some(' '),
                (DiffLineKind::Removed, false, PatchDir::Forward) => Some(' '),
                (DiffLineKind::Removed, false, PatchDir::Reverse) => None,
                (DiffLineKind::NoNewline, _, _) => {
                    if prev_kept {
                        lines.push_str("\\ No newline at end of file\n");
                    }
                    continue;
                }
                // Conflict lines never appear in a stageable diff (a
                // conflicted file isn't hunk-staged) — keep the match
                // exhaustive anyway.
                _ => None,
            };
            if matches!(line.kind, DiffLineKind::Added | DiffLineKind::Removed) {
                total_changes += 1;
                if selected {
                    picked_changes += 1;
                    picked = true;
                }
            }
            prev_kept = emit.is_some();
            let Some(sigil) = emit else { continue };
            match sigil {
                ' ' => {
                    old_n += 1;
                    new_n += 1;
                }
                '-' => old_n += 1,
                _ => new_n += 1,
            }
            lines.push(sigil);
            lines.push_str(&line.text);
            lines.push('\n');
        }
        if !picked {
            continue;
        }
        any = true;
        let (old_start, new_start) = match dir {
            PatchDir::Forward => {
                let new_start = (hunk.old_start as i64 + delta).max(0) as u32;
                (hunk.old_start, start_for(new_start, new_n))
            }
            PatchDir::Reverse => {
                let old_start = (hunk.new_start as i64 - delta).max(0) as u32;
                (start_for(old_start, old_n), hunk.new_start)
            }
        };
        delta += new_n as i64 - old_n as i64;
        body.push_str(&format!(
            "@@ -{old_start},{old_n} +{new_start},{new_n} @@\n"
        ));
        body.push_str(&lines);
    }
    if !any {
        return None;
    }

    // A partial pick out of a whole-file creation or deletion is really a
    // modification of a file that keeps existing — git apply would refuse a
    // "deleted file" patch that leaves lines behind, or a reversed "new
    // file" patch that doesn't remove them all.
    let whole = picked_changes == total_changes;
    let rewrite = !whole
        && matches!(
            (file.status, dir),
            (DiffFileStatus::Deleted, PatchDir::Forward)
                | (DiffFileStatus::Added, PatchDir::Reverse)
        );
    let mut patch = if rewrite {
        let p = git_quote(file.path());
        format!("diff --git a/{p} b/{p}\n--- a/{p}\n+++ b/{p}\n")
    } else {
        let mut h = file.header_text.clone();
        if !h.ends_with('\n') {
            h.push('\n');
        }
        h
    };
    patch.push_str(&body);
    Some(patch)
}

/// A hunk side that contributes no lines starts one line earlier by
/// convention (`@@ -0,0 +1,2 @@` for a new file).
fn start_for(start: u32, count: u32) -> u32 {
    if count == 0 {
        start.saturating_sub(1)
    } else {
        start.max(1)
    }
}

/// Stage only the selected lines of an unstaged (or untracked) file diff.
pub fn stage_lines(repo: &Path, file: &FileDiff, sel: &LineSel) -> Result<(), String> {
    let patch = build_partial_patch(file, sel, PatchDir::Forward)
        .ok_or_else(|| "Couldn't stage — no changed lines are selected.".to_string())?;
    apply(repo, &patch, ApplyMode::ToIndex, "stage those lines")
}

/// Unstage only the selected lines of a staged file diff; the worktree keeps
/// them.
pub fn unstage_lines(repo: &Path, file: &FileDiff, sel: &LineSel) -> Result<(), String> {
    let patch = build_partial_patch(file, sel, PatchDir::Reverse)
        .ok_or_else(|| "Couldn't unstage — no changed lines are selected.".to_string())?;
    apply(repo, &patch, ApplyMode::IndexReverse, "unstage those lines")
}

/// Throw away only the selected lines of an unstaged file diff. The file as
/// it was is copied to the trash folder first, so a misclick is recoverable.
pub fn discard_lines(repo: &Path, file: &FileDiff, sel: &LineSel) -> Result<(), String> {
    let patch = build_partial_patch(file, sel, PatchDir::Reverse)
        .ok_or_else(|| "Couldn't discard — no changed lines are selected.".to_string())?;
    backup_copy(repo, file.path())?;
    apply(
        repo,
        &patch,
        ApplyMode::WorktreeReverse,
        "discard those lines",
    )
}

/// Copy `path` into a fresh trash folder (the original stays put).
fn backup_copy(repo: &Path, path: &str) -> Result<PathBuf, String> {
    let src = repo.join(path);
    if !src.is_file() {
        return Ok(src);
    }
    let dir = trash_dir(path)?;
    let dst = dir.join(src.file_name().unwrap_or_default());
    fs::copy(&src, &dst)
        .map_err(|e| format!("Couldn't back up {path} before discarding — {e}."))?;
    Ok(dst)
}

/// A fresh `$TMPDIR/wackdepot-trash/<ts>/` folder.
fn trash_dir(path: &str) -> Result<PathBuf, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir()
        .join("wackdepot-trash")
        .join(format!("{ts}"));
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Couldn't create a trash folder for {path} — {e}."))?;
    Ok(dir)
}

/// `git add -- <path>` — stages new, modified and deleted paths alike.
pub fn stage_file(repo: &Path, path: &str) -> Result<(), String> {
    Git::new(repo).run(["add", "--", path])?;
    Ok(())
}

/// `git add -A -- <paths…>` in batches — one process per 200 paths instead
/// of one per file (stage-all on a big change set). `-A` so a missing
/// worktree file (a conflict resolved by deleting it) stages the deletion
/// instead of failing.
pub fn stage_paths(repo: &Path, paths: &[String]) -> Result<(), String> {
    let g = Git::new(repo);
    for chunk in paths.chunks(200) {
        let mut args: Vec<&str> = vec!["add", "-A", "--"];
        args.extend(chunk.iter().map(String::as_str));
        g.run(args)?;
    }
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

/// Throw away ALL of `path`'s changes — worktree and index alike, back to
/// HEAD (`git restore --staged --worktree`). A file HEAD doesn't know
/// (untracked, or added to the index only) leaves the index then goes to
/// the trash folder.
pub fn discard_fully(repo: &Path, path: &str) -> Result<(), String> {
    let g = Git::new(repo);
    let in_index = g
        .try_run(["ls-files", "--error-unmatch", "--", path])
        .map(|o| o.ok())
        .unwrap_or(false);
    let in_head = g
        .try_run(["cat-file", "-e", &format!("HEAD:./{path}")])
        .map(|o| o.ok())
        .unwrap_or(false);
    if !in_head {
        if in_index {
            unstage_file(repo, path)?;
        }
        trash_file(repo, path)?;
        return Ok(());
    }
    if g.try_run(["restore", "--staged", "--worktree", "--", path])?
        .ok()
    {
        return Ok(());
    }
    g.run(["checkout", "HEAD", "--", path])?;
    Ok(())
}

/// Sync the index with the commit selection right before committing:
/// everything changed is staged unless `excluded` marks it out, and an
/// excluded path that's in the index is fully unstaged. A path staged
/// only in part keeps its staged hunks — "include this file" means what
/// the user picked for it, never a secret `add` of the rest.
pub fn stage_for_commit(repo: &Path, excluded: &HashSet<String>) -> Result<(), String> {
    let s = crate::status::status(repo)?;
    let stage: Vec<String> = s
        .unstaged
        .iter()
        .chain(s.untracked.iter())
        .filter(|f| !excluded.contains(&f.path))
        // Partially staged stays partial — its staged side is the pick.
        .filter(|f| !s.staged.iter().any(|g| g.path == f.path))
        .map(|f| f.path.clone())
        .collect();
    let unstage: Vec<String> = s
        .staged
        .iter()
        .filter(|f| excluded.contains(&f.path))
        .map(|f| f.path.clone())
        .collect();
    stage_paths(repo, &stage)?;
    for p in &unstage {
        unstage_file(repo, p)?;
    }
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
    let dir = trash_dir(path)?;
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

/// `git add -A` a conflicted path — marks it resolved once the user has
/// edited the conflict markers away, or deleted the file.
pub fn mark_resolved(repo: &Path, path: &str) -> Result<(), String> {
    Git::new(repo).run(["add", "-A", "--", path])?;
    Ok(())
}

/// Which parent's version of a conflicted path wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSide {
    /// Stage 2 — the version the merge started on (HEAD).
    Ours,
    /// Stage 3 — the version being merged in.
    Theirs,
}

impl ConflictSide {
    fn flag(self) -> &'static str {
        match self {
            Self::Ours => "--ours",
            Self::Theirs => "--theirs",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ours => "our",
            Self::Theirs => "their",
        }
    }
}

/// Resolve a conflicted path wholesale: `git checkout --ours/--theirs`
/// puts that side's version in the worktree, then `git add` marks it
/// resolved. With `delete` it's `git rm` instead — how a modify/delete
/// conflict keeps the deletion. `Err` when the chosen side doesn't exist
/// (a side that deleted the file has no version to take).
pub fn resolve_conflict(
    repo: &Path,
    path: &str,
    side: ConflictSide,
    delete: bool,
) -> Result<(), String> {
    let g = Git::new(repo);
    if delete {
        g.run(["rm", "-q", "--", path])?;
        return Ok(());
    }
    let out = g.try_run(["checkout", side.flag(), "--", path])?;
    if !out.ok() {
        let msg = failure_message(&out);
        return Err(if msg.is_empty() {
            format!(
                "Couldn't take {} version of {path} — that side has none.",
                side.label()
            )
        } else {
            format!(
                "Couldn't take {} version of {path} — {}",
                side.label(),
                lowercase_first(&msg)
            )
        });
    }
    mark_resolved(repo, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::diff::{DiffLineKind, DiffSide, diff};
    use crate::status::status;
    use crate::testutil::{git_out, read_file, temp_repo, write_file};

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

    /// Select every changed line of `file` whose text satisfies `pick`.
    fn select(file: &FileDiff, pick: impl Fn(&str) -> bool) -> LineSel {
        let mut sel = LineSel::default();
        for (hi, h) in file.hunks.iter().enumerate() {
            for (li, l) in h.lines.iter().enumerate() {
                if matches!(l.kind, DiffLineKind::Added | DiffLineKind::Removed) && pick(&l.text) {
                    sel.0.insert((hi, li));
                }
            }
        }
        sel
    }

    fn base_repo(contents: &str) -> (tempfile::TempDir, Git) {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", contents);
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        (dir, g)
    }

    fn cached(g: &Git) -> String {
        g.run(["show", ":f.txt"]).unwrap().stdout
    }

    #[test]
    fn stage_some_lines_of_a_mixed_hunk() {
        let (dir, g) = base_repo("a\nb\nc\nd\n");
        write_file(dir.path(), "f.txt", "a\nB\nc\nnew\nd\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        // Take the b→B replacement, leave the inserted "new" line.
        stage_lines(dir.path(), &d, &select(&d, |t| t == "b" || t == "B")).unwrap();
        assert_eq!(cached(&g), "a\nB\nc\nd\n");
        assert_eq!(read_file(dir.path(), "f.txt"), "a\nB\nc\nnew\nd\n");

        // Only removing "b" (without adding "B") is a valid pick too.
        let (dir, g) = base_repo("a\nb\nc\n");
        write_file(dir.path(), "f.txt", "a\nB\nc\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        stage_lines(dir.path(), &d, &select(&d, |t| t == "b")).unwrap();
        assert_eq!(cached(&g), "a\nc\n");
    }

    #[test]
    fn later_hunks_shift_after_a_trimmed_one() {
        let lines: Vec<String> = (1..=30).map(|i| format!("l{i}")).collect();
        let (dir, g) = base_repo(&(lines.join("\n") + "\n"));
        let mut edited = lines.clone();
        edited.insert(2, "ins1".into());
        edited.insert(3, "ins2".into());
        edited[26] = "CHANGED".into();
        write_file(dir.path(), "f.txt", &(edited.join("\n") + "\n"));
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.hunks.len(), 2);
        // Skip the insertion entirely; stage only the far change.
        stage_lines(
            dir.path(),
            &d,
            &select(&d, |t| t == "CHANGED" || t == "l25"),
        )
        .unwrap();
        let mut expect = lines.clone();
        expect[24] = "CHANGED".into();
        assert_eq!(cached(&g), expect.join("\n") + "\n");
    }

    #[test]
    fn partial_new_file_and_partial_unstage() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "keep\nskip\nalso\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        stage_lines(dir.path(), &d, &select(&d, |t| t != "skip")).unwrap();
        assert_eq!(cached(&g), "keep\nalso\n");

        // Unstage "also" again from the staged (new-file) side.
        let staged = diff(dir.path(), "f.txt", DiffSide::Staged).unwrap();
        assert_eq!(staged.status, DiffFileStatus::Added);
        unstage_lines(dir.path(), &staged, &select(&staged, |t| t == "also")).unwrap();
        assert_eq!(cached(&g), "keep\n");
        assert_eq!(read_file(dir.path(), "f.txt"), "keep\nskip\nalso\n");
    }

    #[test]
    fn discard_selected_lines_only() {
        let (dir, g) = base_repo("a\nb\nc\n");
        write_file(dir.path(), "f.txt", "a\nx\nb\ny\nc\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        discard_lines(dir.path(), &d, &select(&d, |t| t == "x")).unwrap();
        assert_eq!(read_file(dir.path(), "f.txt"), "a\nb\ny\nc\n");
        assert_eq!(cached(&g), "a\nb\nc\n");
    }

    #[test]
    fn partial_deletion_keeps_the_file() {
        let (dir, g) = base_repo("one\ntwo\n");
        std::fs::remove_file(dir.path().join("f.txt")).unwrap();
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Deleted);
        stage_lines(dir.path(), &d, &select(&d, |t| t == "one")).unwrap();
        assert_eq!(cached(&g), "two\n");
    }

    #[test]
    fn no_newline_at_end_survives() {
        let (dir, g) = base_repo("a\nb");
        write_file(dir.path(), "f.txt", "A\nb");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        stage_lines(dir.path(), &d, &select(&d, |t| t == "a" || t == "A")).unwrap();
        assert_eq!(cached(&g), "A\nb");
    }

    /// Stage a pseudo-random subset of lines over many edits and check the
    /// index holds exactly the base with those lines applied.
    #[test]
    fn randomized_partial_stage_matches_model() {
        let mut seed: u64 = 0x5eed_1234;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..12 {
            let base: Vec<String> = (0..40).map(|i| format!("r{round}l{i}")).collect();
            let (dir, g) = base_repo(&(base.join("\n") + "\n"));
            // Edit: some lines replaced, some removed, some inserted.
            let mut edited: Vec<String> = Vec::new();
            for (i, l) in base.iter().enumerate() {
                match next() % 7 {
                    0 => {}
                    1 => edited.push(format!("{l}-changed")),
                    2 => {
                        edited.push(l.clone());
                        edited.push(format!("new{round}-{i}"));
                    }
                    _ => edited.push(l.clone()),
                }
            }
            write_file(dir.path(), "f.txt", &(edited.join("\n") + "\n"));
            let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
            let mut sel = LineSel::default();
            for (hi, h) in d.hunks.iter().enumerate() {
                for (li, l) in h.lines.iter().enumerate() {
                    if matches!(l.kind, DiffLineKind::Added | DiffLineKind::Removed)
                        && next() % 2 == 0
                    {
                        sel.0.insert((hi, li));
                    }
                }
            }
            if build_partial_patch(&d, &sel, PatchDir::Forward).is_none() {
                continue;
            }
            stage_lines(dir.path(), &d, &sel).unwrap();

            // Model: walk the diff, keeping old lines unless a selected
            // removal drops them, and adding selected insertions.
            let mut model: Vec<String> = Vec::new();
            let mut old_i = 0usize;
            for (hi, h) in d.hunks.iter().enumerate() {
                let start = h.old_start.saturating_sub(1) as usize;
                while old_i < start {
                    model.push(base[old_i].clone());
                    old_i += 1;
                }
                for (li, l) in h.lines.iter().enumerate() {
                    let picked = sel.0.contains(&(hi, li));
                    match l.kind {
                        DiffLineKind::Context => {
                            model.push(base[old_i].clone());
                            old_i += 1;
                        }
                        DiffLineKind::Removed => {
                            if !picked {
                                model.push(base[old_i].clone());
                            }
                            old_i += 1;
                        }
                        DiffLineKind::Added => {
                            if picked {
                                model.push(l.text.clone());
                            }
                        }
                        DiffLineKind::NoNewline => {}
                        _ => {}
                    }
                }
            }
            model.extend(base[old_i..].iter().cloned());
            assert_eq!(cached(&g), model.join("\n") + "\n", "round {round}");
        }
    }

    /// A repo left mid-merge with `f.txt` conflicted (diverging edits on
    /// main and a `side` branch).
    fn conflicted_repo() -> (tempfile::TempDir, Git) {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "a\nb\nc\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "side"]).unwrap();
        write_file(dir.path(), "f.txt", "a\nSIDE\nc\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "f.txt", "a\nMAIN\nc\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main", "").unwrap();
        assert!(!git_out(&g, ["merge", "side"]).ok());
        (dir, g)
    }

    /// A modify/delete conflict: `dside` deleted `f.txt`, main changed it.
    fn modify_delete_repo() -> (tempfile::TempDir, Git) {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "keep\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "dside"]).unwrap();
        g.run(["rm", "-q", "f.txt"]).unwrap();
        commit(dir.path(), "del", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "f.txt", "changed\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "mod", "").unwrap();
        assert!(!git_out(&g, ["merge", "dside"]).ok());
        (dir, g)
    }

    fn unmerged(repo: &Path) -> usize {
        Git::new(repo)
            .run(["ls-files", "-u"])
            .unwrap()
            .stdout
            .lines()
            .count()
    }

    #[test]
    fn resolve_conflict_takes_a_side() {
        let (dir, _g) = conflicted_repo();
        resolve_conflict(dir.path(), "f.txt", ConflictSide::Theirs, false).unwrap();
        assert_eq!(unmerged(dir.path()), 0);
        assert_eq!(read_file(dir.path(), "f.txt"), "a\nSIDE\nc\n");

        let (dir, _g) = conflicted_repo();
        resolve_conflict(dir.path(), "f.txt", ConflictSide::Ours, false).unwrap();
        assert_eq!(unmerged(dir.path()), 0);
        assert_eq!(read_file(dir.path(), "f.txt"), "a\nMAIN\nc\n");
    }

    #[test]
    fn resolve_conflict_delete_keeps_the_deletion() {
        // modify/delete: they deleted it, we changed it.
        let (dir, _g) = modify_delete_repo();
        resolve_conflict(dir.path(), "f.txt", ConflictSide::Theirs, true).unwrap();
        assert_eq!(unmerged(dir.path()), 0);
        assert!(!dir.path().join("f.txt").exists());
        // The deletion is staged.
        let s = status(dir.path()).unwrap();
        assert!(s.staged.iter().any(|f| f.path == "f.txt" && f.x == 'D'));
    }

    #[test]
    fn resolve_conflict_errors_when_the_side_deleted() {
        // modify/delete: taking "their" version doesn't exist — they deleted it.
        let (dir, _g) = modify_delete_repo();
        let e = resolve_conflict(dir.path(), "f.txt", ConflictSide::Theirs, false).unwrap_err();
        assert!(e.contains("f.txt"), "{e}");
        assert!(unmerged(dir.path()) > 0);
        // Ours is fine.
        resolve_conflict(dir.path(), "f.txt", ConflictSide::Ours, false).unwrap();
        assert_eq!(read_file(dir.path(), "f.txt"), "changed\n");
    }

    #[test]
    fn mark_resolved_stages_a_deletion() {
        let (dir, _g) = modify_delete_repo();
        // The user resolves by deleting the file outright.
        std::fs::remove_file(dir.path().join("f.txt")).unwrap();
        mark_resolved(dir.path(), "f.txt").unwrap();
        assert_eq!(unmerged(dir.path()), 0);
        let s = status(dir.path()).unwrap();
        assert!(s.staged.iter().any(|f| f.path == "f.txt" && f.x == 'D'));
    }

    #[test]
    fn stage_for_commit_syncs_the_index_to_the_selection() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "keep.txt", "1\n");
        write_file(dir.path(), "drop.txt", "2\n");
        let base: Vec<String> = (1..=20).map(|i| format!("l{i}")).collect();
        write_file(dir.path(), "partial.txt", &(base.join("\n") + "\n"));
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();

        write_file(dir.path(), "keep.txt", "1 changed\n");
        write_file(dir.path(), "drop.txt", "2 changed\n");
        write_file(dir.path(), "new.txt", "n\n");
        // drop.txt arrives staged; partial.txt gets only its first of two
        // hunks staged.
        stage_file(dir.path(), "drop.txt").unwrap();
        let mut partial: Vec<String> = (1..=20).map(|i| format!("l{i}")).collect();
        partial[0] = "TOP".into();
        partial[19] = "BOTTOM".into();
        write_file(dir.path(), "partial.txt", &(partial.join("\n") + "\n"));
        let d = diff(dir.path(), "partial.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.hunks.len(), 2);
        stage_hunk(dir.path(), &d, &d.hunks[0]).unwrap();

        let excluded: HashSet<String> = ["drop.txt".to_string()].into_iter().collect();
        stage_for_commit(dir.path(), &excluded).unwrap();

        let staged = staged_names(dir.path());
        assert!(staged.contains(&"keep.txt".to_string()));
        assert!(staged.contains(&"new.txt".to_string()));
        // The excluded file left the index entirely…
        assert!(!staged.contains(&"drop.txt".to_string()));
        // …but its worktree edit is still there.
        assert_eq!(read_file(dir.path(), "drop.txt"), "2 changed\n");
        // The partially staged file keeps exactly its pick.
        let cached_partial = g.run(["show", ":partial.txt"]).unwrap().stdout;
        assert!(cached_partial.contains("TOP"));
        assert!(!cached_partial.contains("BOTTOM"));
    }

    #[test]
    fn discard_fully_covers_index_and_worktree() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "t.txt", "orig\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        write_file(dir.path(), "t.txt", "dirty\n");
        stage_file(dir.path(), "t.txt").unwrap();
        discard_fully(dir.path(), "t.txt").unwrap();
        let s = status(dir.path()).unwrap();
        assert!(s.staged.is_empty() && s.unstaged.is_empty());
        assert_eq!(read_file(dir.path(), "t.txt"), "orig\n");

        // Staged for addition: HEAD has no version, so it goes to trash.
        write_file(dir.path(), "fresh.txt", "x\n");
        stage_file(dir.path(), "fresh.txt").unwrap();
        discard_fully(dir.path(), "fresh.txt").unwrap();
        assert!(!dir.path().join("fresh.txt").exists());
        assert!(status(dir.path()).unwrap().staged.is_empty());
    }

    #[test]
    fn stage_paths_batches() {
        let (dir, _g) = temp_repo();
        let paths: Vec<String> = (0..450).map(|i| format!("f{i}.txt")).collect();
        for p in &paths {
            write_file(dir.path(), p, "x\n");
        }
        stage_paths(dir.path(), &paths).unwrap();
        assert_eq!(staged_names(dir.path()).len(), 450);
    }
}
