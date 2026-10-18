//! wackdepot-core — all git logic for WackDepot. Pure and testable: every
//! operation shells out to the system `git` binary, so it inherits the user's
//! credential helpers and avoids libgit2 edge cases.
//!
//! Conventions:
//! - Fallible functions return `Result<T, String>`; the `Err` is a user-facing
//!   sentence, shown verbatim in the UI.
//! - `GIT_TERMINAL_PROMPT=0` is always set so a GUI-launched git never blocks
//!   waiting for a tty credential prompt.
