//! `git merge` — the start of the operation whose middle and end already
//! lived in [`crate::status`] (`MERGE_HEAD` detection, continue/abort).
//! Squash merges live here too: they leave `SQUASH_MSG` instead of
//! `MERGE_HEAD`, which [`crate::status::Operation::Squash`] tracks.
//!
//! [`merge_preview`] uses `git merge-tree --write-tree`, which merges two
//! commits into a tree object without touching the index or working tree —
//! so the dialog can say "this will conflict" before anything happens.

use std::path::Path;

use crate::git::{Git, failure_message};
use crate::status::{self, Operation};

/// How the other branch's changes join the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    /// `git merge` — fast-forward when possible, else a real merge commit.
    Commit,
    /// `git merge --squash` — all of their changes staged on top of HEAD,
    /// one ordinary commit away, no second parent.
    Squash,
}

/// How `git merge` ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// HEAD just moved to the other branch's tip — no merge commit.
    FastForward,
    /// A real merge commit was created.
    Merged,
    /// The other branch is already an ancestor of HEAD.
    AlreadyUpToDate,
    /// git stopped on conflicts — the repo is now `Operation::Merge`,
    /// waiting for resolve + continue/abort.
    Conflicts,
    /// `git merge --squash` staged everything cleanly; `SQUASH_MSG` is set
    /// and `Operation::Squash` shows until the user commits or aborts.
    SquashedClean,
    /// `git merge --squash` stopped on conflicts (no `MERGE_HEAD` — the
    /// squash state is `Operation::Squash` instead).
    SquashedConflicts,
}

/// `git merge --no-edit [--squash] <name>` into the current HEAD.
///
/// `--no-edit` makes sure a clean merge never tries to open an editor
/// (there's no tty). The failure path re-reads porcelain status rather
/// than guessing from stderr: an in-progress merge/squash is a normal
/// outcome, anything else is a sentence for the UI.
pub fn merge(repo: &Path, name: &str, method: MergeMethod) -> Result<MergeOutcome, String> {
    let g = Git::new(repo);
    let mut args = vec!["merge", "--no-edit"];
    if method == MergeMethod::Squash {
        args.push("--squash");
    }
    args.push(name);
    let out = g.try_run(args)?;
    if out.ok() {
        let text = out.stdout;
        if text.contains("Already up to date") {
            return Ok(MergeOutcome::AlreadyUpToDate);
        }
        if method != MergeMethod::Squash && text.contains("Fast-forward") {
            return Ok(MergeOutcome::FastForward);
        }
        return Ok(match method {
            MergeMethod::Commit => MergeOutcome::Merged,
            MergeMethod::Squash => MergeOutcome::SquashedClean,
        });
    }
    // A merge that exits non-zero can still be "working": conflicts leave
    // the repo mid-operation. Real failures (clobbered local changes, bad
    // refs) leave nothing behind — status decides which this was.
    if let Ok(s) = status::status(repo) {
        match s.operation {
            Some(Operation::Merge) => return Ok(MergeOutcome::Conflicts),
            Some(Operation::Squash) => return Ok(MergeOutcome::SquashedConflicts),
            _ => {}
        }
    }
    Err(failure_message(&out))
}

/// What merging `name` into HEAD would do, without touching anything.
#[derive(Debug, Clone, Default)]
pub struct MergePreview {
    /// Commits `name` has that HEAD doesn't — what the merge brings in.
    pub incoming: u32,
    /// Commits HEAD has that `name` doesn't.
    pub outgoing: u32,
    /// Paths `git merge-tree` predicts would conflict (empty = none
    /// expected). `None` when prediction isn't available — old gits lack
    /// `merge-tree --write-tree`, and the answer is advisory anyway.
    pub conflicts: Option<Vec<String>>,
}

/// Preview `git merge <name>`: how many commits it brings in, and whether
/// `git merge-tree` sees conflicts coming. `incoming == 0` means HEAD
/// already contains the branch — the merge would be a no-op.
pub fn merge_preview(repo: &Path, name: &str) -> MergePreview {
    let g = Git::new(repo);
    let mut preview = MergePreview::default();
    if let Ok(out) = g.try_run([
        "rev-list",
        "--left-right",
        "--count",
        &format!("HEAD...{name}"),
    ]) && out.ok()
    {
        let mut counts = out.stdout.split_whitespace();
        preview.outgoing = counts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        preview.incoming = counts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    }
    if preview.incoming == 0 {
        return preview;
    }
    // merge-tree writes the result tree to the object store and reports
    // conflicts without touching the index. With -z, stdout is the tree
    // OID, then NUL-separated conflicted paths, then an empty field and
    // the chatty "Auto-merging…/CONFLICT…" messages — names stop at the
    // first empty field, and the message section is ignored.
    let Ok(out) = g.try_run([
        "merge-tree",
        "--write-tree",
        "--name-only",
        "-z",
        "HEAD",
        name,
    ]) else {
        return preview;
    };
    let mut fields = out.stdout.split('\0');
    let tree = fields.next().unwrap_or("");
    let is_tree = tree.len() == 40 && tree.chars().all(|c| c.is_ascii_hexdigit());
    if out.ok() {
        preview.conflicts = Some(Vec::new());
    } else if is_tree {
        preview.conflicts = Some(
            fields
                .take_while(|f| !f.is_empty())
                .map(str::to_string)
                .collect(),
        );
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{commit, list_log};
    use crate::git::Git;
    use crate::stage::stage_all;
    use crate::status::{abort_operation, continue_operation, operation, status};
    use crate::testutil::{read_file, temp_repo, write_file};

    /// `main` and `side` share a base commit; `side` is one ahead.
    fn two_branches() -> (tempfile::TempDir, Git) {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "base.txt", "base\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "side"]).unwrap();
        write_file(dir.path(), "side.txt", "side\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side work", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        (dir, g)
    }

    /// Both branches edit `base.txt` differently — a guaranteed conflict.
    fn diverged() -> (tempfile::TempDir, Git) {
        let (dir, g) = two_branches();
        write_file(dir.path(), "base.txt", "main\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main edit", "").unwrap();
        g.run(["checkout", "side"]).unwrap();
        write_file(dir.path(), "base.txt", "side\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side edit", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        (dir, g)
    }

    #[test]
    fn fast_forward() {
        let (dir, _g) = two_branches();
        assert_eq!(
            merge(dir.path(), "side", MergeMethod::Commit).unwrap(),
            MergeOutcome::FastForward
        );
        assert!(dir.path().join("side.txt").exists());
        let log = list_log(dir.path(), 5).unwrap();
        assert_eq!(log[0].subject, "side work");
        assert_eq!(log[0].parents.len(), 1);
        assert_eq!(operation(dir.path()), None);
    }

    #[test]
    fn true_merge_commit() {
        let (dir, _g) = two_branches();
        write_file(dir.path(), "main.txt", "main\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main work", "").unwrap();
        assert_eq!(
            merge(dir.path(), "side", MergeMethod::Commit).unwrap(),
            MergeOutcome::Merged
        );
        let log = list_log(dir.path(), 5).unwrap();
        assert_eq!(log[0].parents.len(), 2);
        assert!(log[0].subject.starts_with("Merge branch 'side'"));
        assert!(dir.path().join("side.txt").exists() && dir.path().join("main.txt").exists());
    }

    #[test]
    fn already_up_to_date() {
        let (dir, g) = two_branches();
        g.run(["merge", "side"]).unwrap();
        assert_eq!(
            merge(dir.path(), "side", MergeMethod::Commit).unwrap(),
            MergeOutcome::AlreadyUpToDate
        );
    }

    #[test]
    fn conflicting_merge_waits_for_the_user() {
        let (dir, _g) = diverged();
        assert_eq!(
            merge(dir.path(), "side", MergeMethod::Commit).unwrap(),
            MergeOutcome::Conflicts
        );
        assert_eq!(operation(dir.path()), Some(Operation::Merge));
        let s = status(dir.path()).unwrap();
        assert_eq!(s.conflicted.len(), 1);
        assert_eq!(s.conflicted[0].path, "base.txt");
        abort_operation(dir.path(), Operation::Merge).unwrap();
        assert_eq!(operation(dir.path()), None);
        assert_eq!(read_file(dir.path(), "base.txt"), "main\n");
    }

    #[test]
    fn clean_squash_stages_then_commits() {
        let (dir, _g) = two_branches();
        write_file(dir.path(), "main.txt", "main\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main work", "").unwrap();
        assert_eq!(
            merge(dir.path(), "side", MergeMethod::Squash).unwrap(),
            MergeOutcome::SquashedClean
        );
        assert_eq!(operation(dir.path()), Some(Operation::Squash));
        let s = status(dir.path()).unwrap();
        assert_eq!(
            s.staged.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["side.txt"]
        );
        continue_operation(dir.path(), Operation::Squash).unwrap();
        let log = list_log(dir.path(), 5).unwrap();
        assert_eq!(log[0].subject, "Squashed commit of the following:");
        assert_eq!(log[0].parents.len(), 1); // one parent — not a merge
        assert_eq!(operation(dir.path()), None);
    }

    #[test]
    fn conflicted_squash_aborts_cleanly() {
        let (dir, _g) = diverged();
        assert_eq!(
            merge(dir.path(), "side", MergeMethod::Squash).unwrap(),
            MergeOutcome::SquashedConflicts
        );
        assert_eq!(operation(dir.path()), Some(Operation::Squash));
        assert_eq!(status(dir.path()).unwrap().conflicted.len(), 1);
        // `git merge --abort` can't see a squash; reset --merge is the way out.
        abort_operation(dir.path(), Operation::Squash).unwrap();
        let s = status(dir.path()).unwrap();
        assert!(s.operation.is_none());
        assert!(s.conflicted.is_empty() && s.staged.is_empty());
        assert_eq!(read_file(dir.path(), "base.txt"), "main\n");
    }

    #[test]
    fn preview_counts_and_conflicts() {
        let (dir, g) = two_branches();
        let p = merge_preview(dir.path(), "side");
        assert_eq!((p.incoming, p.outgoing), (1, 0));
        assert_eq!(p.conflicts.as_deref(), Some(&[][..]));

        write_file(dir.path(), "base.txt", "main\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main edit", "").unwrap();
        g.run(["checkout", "side"]).unwrap();
        write_file(dir.path(), "base.txt", "side\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side edit", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        let p = merge_preview(dir.path(), "side");
        assert_eq!((p.incoming, p.outgoing), (2, 1));
        assert_eq!(p.conflicts.as_deref(), Some(&["base.txt".to_string()][..]));
        // The dry run changed nothing.
        assert!(status(dir.path()).unwrap().operation.is_none());
    }

    #[test]
    fn preview_up_to_date_skips_merge_tree() {
        let (dir, g) = two_branches();
        g.run(["merge", "side"]).unwrap();
        let p = merge_preview(dir.path(), "side");
        assert_eq!(p.incoming, 0);
        assert!(p.conflicts.is_none());
    }

    #[test]
    fn merge_over_dirty_clobbered_file_errors() {
        let (dir, _g) = two_branches();
        // side.txt exists on `side` but is untracked-dirty here.
        write_file(dir.path(), "side.txt", "dirty\n");
        let err = merge(dir.path(), "side", MergeMethod::Commit).unwrap_err();
        assert!(err.contains("would be overwritten"), "{err}");
    }

    #[test]
    fn stale_squash_msg_is_not_a_squash() {
        let (dir, g) = two_branches();
        g.run(["merge", "--squash", "side"]).unwrap();
        // Unstaging (git restore --staged) leaves SQUASH_MSG behind; with
        // nothing staged and no conflicts there is no squash to continue,
        // so status must not report one.
        g.run(["restore", "--staged", "."]).unwrap();
        assert!(dir.path().join(".git/SQUASH_MSG").exists());
        assert_eq!(status(dir.path()).unwrap().operation, None);
    }
}
