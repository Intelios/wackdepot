//! Shared state types for the shell — kept free of render code so the view
//! modules stay layout-only.

use std::path::PathBuf;

/// Which left-rail tab is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Changes,
    History,
}

/// Which bucket the selected changed file sits in — the diff call and the
/// stage/unstage affordances key off this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Staged,
    Unstaged,
    Untracked,
    Conflicted,
}

/// What's shown in the right pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selected {
    /// A working-tree file: `bucket` picks the diff side.
    File { bucket: Bucket, index: usize },
    /// A history commit: `index` into the log list.
    Commit { index: usize },
}

/// Floating popovers anchored to a titlebar/rail control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopoverKind {
    RepoPicker,
    BranchPicker,
    SyncMenu,
}

/// Centered dialogs (frosted cards over a dim wash).
#[derive(Clone, Debug)]
pub enum Dialog {
    /// Clone a URL into a chosen parent folder.
    CloneRepo,
    /// Create a new repo: name + parent folder.
    NewRepo,
    /// Destructive-action confirmation.
    Confirm(ConfirmAction),
    /// Appearance + editor preferences.
    Settings,
}

/// What a `Confirm` dialog will do when accepted. Display copy derives from
/// the variant; `run` performs the op.
#[derive(Clone, Debug)]
pub enum ConfirmAction {
    DiscardFile {
        path: String,
    },
    /// Untracked file — moved to the app's trash dir, never `rm`.
    TrashFile {
        path: String,
    },
    DiscardAll,
    DeleteBranch {
        name: String,
        force: bool,
    },
    /// git refused -d (not fully merged): escalate to a force confirm.
    ForceDeleteBranch {
        name: String,
    },
    AbortMerge,
    DropStash {
        name: String,
    },
}

impl ConfirmAction {
    pub fn title(&self) -> String {
        match self {
            ConfirmAction::DiscardFile { path } => format!("Discard changes in {path}?"),
            ConfirmAction::TrashFile { path } => format!("Delete {path}?"),
            ConfirmAction::DiscardAll => "Discard all changes?".into(),
            ConfirmAction::DeleteBranch { name, .. } => format!("Delete {name}?"),
            ConfirmAction::ForceDeleteBranch { name } => format!("Force-delete {name}?"),
            ConfirmAction::AbortMerge => "Abort this merge?".into(),
            ConfirmAction::DropStash { name } => format!("Drop {name}?"),
        }
    }

    pub fn body(&self) -> String {
        match self {
            ConfirmAction::DiscardFile { .. } => {
                "The file goes back to its last committed state. This can't be undone.".into()
            }
            ConfirmAction::TrashFile { .. } => {
                "It's not in git yet, so it can't be restored by git. It moves to WackDepot's trash."
                    .into()
            }
            ConfirmAction::DiscardAll => {
                "Every unstaged change goes back to the last commit. This can't be undone.".into()
            }
            ConfirmAction::DeleteBranch { force, .. } => {
                if *force {
                    "This branch isn't fully merged — deleting it may strand commits.".into()
                } else {
                    "Only possible if it's fully merged.".into()
                }
            }
            ConfirmAction::ForceDeleteBranch { .. } => {
                "It isn't fully merged — its commits may become unreachable.".into()
            }
            ConfirmAction::AbortMerge => {
                "The working tree goes back to before the merge. Conflict resolutions are lost."
                    .into()
            }
            ConfirmAction::DropStash { .. } => "The stashed changes are removed.".into(),
        }
    }
}

/// A long-running git op shown in the titlebar and dimming interactions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusyOp {
    Fetch,
    Pull,
    Push,
    Clone,
    Commit,
    Checkout,
    Other(&'static str),
}

impl BusyOp {
    pub fn label(&self) -> &'static str {
        match self {
            BusyOp::Fetch => "Fetching",
            BusyOp::Pull => "Pulling",
            BusyOp::Push => "Pushing",
            BusyOp::Clone => "Cloning",
            BusyOp::Commit => "Committing",
            BusyOp::Checkout => "Switching branch",
            BusyOp::Other(s) => s,
        }
    }
}

/// A transient toast anchored to the content pane.
#[derive(Clone, Debug)]
pub struct Notice {
    pub text: String,
    /// True for failures — danger-tinted; false for confirmations — accent.
    pub is_error: bool,
    /// Monotonic id so a same-text notice re-triggers animations.
    pub id: u64,
}

/// The repo picker row model — built each render from settings.
#[derive(Clone, Debug)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub name: String,
}
