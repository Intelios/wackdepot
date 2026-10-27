//! Destructive actions that ask first, with their dialog copy.

use wackdepot_core::stage::ConflictSide;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    DiscardFile {
        path: String,
    },
    /// A multi-file discard: `tracked` paths get `git restore`,
    /// `untracked` paths go to the trash folder.
    DiscardFiles {
        tracked: Vec<String>,
        untracked: Vec<String>,
    },
    TrashFile {
        path: String,
    },
    DiscardAll,
    DiscardHunk {
        path: String,
        hunk: usize,
    },
    DiscardLines {
        path: String,
        count: usize,
    },
    DeleteBranch {
        name: String,
    },
    ForceDeleteBranch {
        name: String,
    },
    AbortOperation {
        what: &'static str,
    },
    DropStash {
        sha: String,
        label: String,
    },
    /// `git add` a conflicted path while its markers are still inside.
    MarkResolved {
        path: String,
    },
    /// `git checkout --ours/--theirs` (or `git rm` when `delete`) a
    /// conflicted path — resolves it with one side's version.
    ResolveConflict {
        path: String,
        side: ConflictSide,
        delete: bool,
    },
    UndoCommit,
    /// `git reset --mixed` back to a commit — the dirty-worktree warning
    /// that asks before running it.
    ResetToCommit {
        sha: String,
        short: String,
    },
}

impl ConfirmAction {
    pub fn title(&self) -> String {
        match self {
            Self::DiscardFile { path } => format!("Discard changes to {}?", file_name(path)),
            Self::DiscardFiles { tracked, untracked } => {
                let n = tracked.len() + untracked.len();
                if tracked.is_empty() {
                    format!("Move {n} files to the trash?")
                } else {
                    format!("Discard changes to {n} files?")
                }
            }
            Self::TrashFile { path } => format!("Move {} to the trash?", file_name(path)),
            Self::DiscardAll => "Discard all changes?".into(),
            Self::DiscardHunk { .. } => "Discard this hunk?".into(),
            Self::DiscardLines { count, .. } if *count == 1 => "Discard this line?".into(),
            Self::DiscardLines { count, .. } => format!("Discard {count} lines?"),
            Self::DeleteBranch { name } => format!("Delete {name}?"),
            Self::ForceDeleteBranch { name } => format!("{name} isn't merged. Delete it anyway?"),
            Self::AbortOperation { what } => format!("Abort the {what}?"),
            Self::DropStash { .. } => "Drop this stash?".into(),
            Self::MarkResolved { path } => format!("Mark {} resolved?", file_name(path)),
            Self::ResolveConflict { path, side, delete } => {
                if *delete {
                    format!("Delete {}?", file_name(path))
                } else {
                    format!("Use {} version of {}?", side_label(*side), file_name(path))
                }
            }
            Self::UndoCommit => "Undo the last commit?".into(),
            Self::ResetToCommit { short, .. } => format!("Reset branch to {short}?"),
        }
    }

    pub fn body(&self) -> String {
        match self {
            Self::DiscardFile { .. } => {
                "The file goes back to its last committed version. This can't be undone.".into()
            }
            Self::DiscardFiles { tracked, untracked } => {
                match (tracked.is_empty(), untracked.is_empty()) {
                    (true, false) => {
                        "They're untracked, so they move to WackDepot's trash folder instead of being deleted.".into()
                    }
                    (false, true) => {
                        "Each file goes back to its last committed version. This can't be undone."
                            .into()
                    }
                    (false, false) => format!(
                        "{} {} back to their last committed version{} — that can't be undone. {} {} to WackDepot's trash folder.",
                        plural(tracked.len(), "tracked file"),
                        if tracked.len() == 1 { "goes" } else { "go" },
                        if tracked.len() == 1 { "" } else { "s" },
                        plural(untracked.len(), "untracked file"),
                        if untracked.len() == 1 { "moves" } else { "move" },
                    ),
                    _ => String::new(),
                }
            }
            Self::DiscardHunk { .. } | Self::DiscardLines { .. } => {
                "The working copy goes back to the last staged or committed version. This can't be undone.".into()
            }
            Self::TrashFile { .. } => "It's untracked, so it moves to WackDepot's trash folder instead of being deleted.".into(),
            Self::DiscardAll => "Every changed file goes back to its committed version and untracked files move to the trash folder.".into(),
            Self::DeleteBranch { .. } => "The branch is removed locally. Its commits stay reachable from wherever they were merged.".into(),
            Self::ForceDeleteBranch { .. } => "Its commits aren't on any other branch, so they'll only be recoverable from the reflog.".into(),
            Self::AbortOperation { .. } => "The repository goes back to how it was before it started. Resolved conflicts are lost.".into(),
            Self::DropStash { label, .. } => format!("“{label}” is deleted for good."),
            Self::MarkResolved { .. } => "Its conflict markers are still inside — they'd be staged as-is and could end up committed.".into(),
            Self::ResolveConflict { delete: true, .. } => {
                "The file is removed to resolve the conflict — the deletion is staged.".into()
            }
            Self::ResolveConflict { side, .. } => format!(
                "The file becomes exactly {} version, losing any edits made since the merge started — and it's marked resolved.",
                match side {
                    ConflictSide::Ours => "this branch's",
                    ConflictSide::Theirs => "the incoming",
                }
            ),
            Self::UndoCommit => "The commit is removed and its changes go back to Staged, ready to commit again.".into(),
            Self::ResetToCommit { .. } => {
                "You have changes in progress. The reset keeps them, but they'll be mixed in with the rewound commits' changes in the Changes list.".into()
            }
        }
    }

    pub fn confirm_label(&self) -> &'static str {
        match self {
            Self::DiscardFile { .. }
            | Self::DiscardAll
            | Self::DiscardHunk { .. }
            | Self::DiscardLines { .. } => "Discard",
            Self::DiscardFiles { tracked, .. } => {
                if tracked.is_empty() {
                    "Move to trash"
                } else {
                    "Discard"
                }
            }
            Self::TrashFile { .. } => "Move to trash",
            Self::DeleteBranch { .. } | Self::ForceDeleteBranch { .. } => "Delete",
            Self::AbortOperation { .. } => "Abort",
            Self::DropStash { .. } => "Drop",
            Self::MarkResolved { .. } => "Mark resolved",
            Self::ResolveConflict { delete: true, .. } => "Delete file",
            Self::ResolveConflict {
                side: ConflictSide::Ours,
                ..
            } => "Use ours",
            Self::ResolveConflict { .. } => "Use theirs",
            Self::UndoCommit => "Undo commit",
            Self::ResetToCommit { .. } => "Reset",
        }
    }
}

fn side_label(side: ConflictSide) -> &'static str {
    match side {
        ConflictSide::Ours => "our",
        ConflictSide::Theirs => "their",
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `n file`/`n files` for batch copy.
fn plural(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(tracked: &[&str], untracked: &[&str]) -> ConfirmAction {
        ConfirmAction::DiscardFiles {
            tracked: tracked.iter().map(|s| s.to_string()).collect(),
            untracked: untracked.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn batch_discard_copy() {
        let tracked_only = files(&["a.rs", "b.rs", "c.rs"], &[]);
        assert_eq!(tracked_only.title(), "Discard changes to 3 files?");
        assert!(tracked_only.body().contains("can't be undone"));
        assert_eq!(tracked_only.confirm_label(), "Discard");

        let untracked_only = files(&[], &["new.md", "other.md"]);
        assert_eq!(untracked_only.title(), "Move 2 files to the trash?");
        assert!(untracked_only.body().contains("trash folder"));
        assert_eq!(untracked_only.confirm_label(), "Move to trash");

        let mixed = files(&["a.rs"], &["n1", "n2"]);
        assert_eq!(mixed.title(), "Discard changes to 3 files?");
        assert!(mixed.body().contains("1 tracked file goes back"));
        assert!(mixed.body().contains("2 untracked files move"));
    }
}
