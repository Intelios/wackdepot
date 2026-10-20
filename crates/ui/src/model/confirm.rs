//! Destructive actions that ask first, with their dialog copy.

use wackdepot_core::stage::ConflictSide;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    DiscardFile {
        path: String,
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
            Self::DiscardFile { .. } | Self::DiscardHunk { .. } | Self::DiscardLines { .. } => {
                "The working copy goes back to the last staged or committed version. This can't be undone.".into()
            }
            Self::TrashFile { .. } => "It's untracked, so it moves to WackDepot's trash folder instead of being deleted.".into(),
            Self::DiscardAll => "Every unstaged change is thrown away and untracked files move to the trash folder.".into(),
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
