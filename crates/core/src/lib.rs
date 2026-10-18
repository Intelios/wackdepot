//! wackdepot-core — all git logic for WackDepot. Pure and testable: every
//! operation shells out to the system `git` binary, so it inherits the user's
//! credential helpers and avoids libgit2 edge cases.
//!
//! Conventions:
//! - Fallible functions return `Result<T, String>`; the `Err` is a user-facing
//!   sentence, shown verbatim in the UI.
//! - `GIT_TERMINAL_PROMPT=0` is always set so a GUI-launched git never blocks
//!   waiting for a tty credential prompt.
//!
//! ## Map
//! - [`git`] — the subprocess runner everything funnels through (`-C <repo>`).
//! - [`status`] — `git status --porcelain=v2` parsing → `RepoStatus`, plus
//!   merge-state helpers (`is_merging`, `abort_merge`).
//! - [`diff`] — unified-diff parsing (`parse_diff`), per-file diffs,
//!   synthesized diffs for untracked files, `diff_commit`.
//! - [`stage`] — hunk-level staging/discard via `git apply`, file-level
//!   stage/unstage/discard/trash, conflict resolution.
//! - [`commit`] — committing, safe undo, author identity, `list_log`.
//! - [`branch`] — list/create/checkout/delete branches.
//! - [`sync`] — fetch/pull/push/clone, repo creation.
//! - [`stash`] — stash list/push/apply/pop/drop.
//! - [`repo`] — repo discovery (`is_repo`, `repo_root`), `git_version`.
//! - [`blob`] — whole-file text for either side of a change (highlighting).
//! - [`words`] — word-level highlights for paired changed lines.
//! - [`graph`] — history lanes laid out from commit parents.
//! - [`progress`] — git progress-meter parsing for live sync feedback.
//! - [`config`] — global identity (`user.name` / `user.email`).

pub mod blob;
pub mod branch;
pub mod commit;
pub mod config;
pub mod diff;
mod git;
pub mod graph;
pub mod progress;
pub mod repo;
pub mod stage;
pub mod stash;
pub mod status;
pub mod sync;
pub mod words;

#[cfg(test)]
mod testutil;

pub use git::{Git, GitOutput, failure_message};
