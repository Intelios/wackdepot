//! Destructive actions that ask first, with their dialog copy.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    DiscardFile { path: String },
    TrashFile { path: String },
    DiscardAll,
    DiscardHunk { path: String, hunk: usize },
    DiscardLines { path: String, count: usize },
    DeleteBranch { name: String },
    ForceDeleteBranch { name: String },
    AbortOperation { what: &'static str },
    DropStash { sha: String, label: String },
    UndoCommit,
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
            Self::UndoCommit => "Undo the last commit?".into(),
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
            Self::UndoCommit => "The commit is removed and its changes go back to Staged, ready to commit again.".into(),
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
            Self::UndoCommit => "Undo commit",
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
