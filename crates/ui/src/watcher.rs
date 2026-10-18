//! Filesystem watching for the open repo. Watches the worktree and the repo's
//! private git dir (so commits/branches made elsewhere still refresh the UI),
//! debounces the event storm (~150ms), and reports one "dirty" signal.
//!
//! Watcher runs its own thread; `take_dirty` drains a flag on the UI side.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

const DEBOUNCE: Duration = Duration::from_millis(150);
/// Skip these inside the worktree — churn we never react to.
const IGNORED_DIRS: &[&str] = &["node_modules", "target", ".idea", ".vscode"];

/// A live watch on one repository. Dropping it stops the thread's updates.
pub struct RepoWatcher {
    /// Set by the watcher thread once enough quiet time passed with events.
    dirty: Arc<AtomicBool>,
    _watcher: RecommendedWatcher,
    _thread: std::thread::JoinHandle<()>,
}

/// Resolve `repo`'s real git dir (works for worktrees' `.git` file).
fn git_dir(repo: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--git-dir"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let p = PathBuf::from(&text);
    Some(if p.is_absolute() {
        p
    } else {
        repo.join(p)
    })
}

/// Start watching `repo`'s worktree + `.git`. Returns `None` if the platform
/// watcher can't be created (UI then falls back to refresh-on-op only).
pub fn watch(repo: &Path) -> Option<RepoWatcher> {
    let dirty = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && interesting(&event)
        {
            let _ = tx.send(());
        }
    })
    .ok()?;

    watcher.watch(repo, RecursiveMode::Recursive).ok()?;
    if let Some(git) = git_dir(repo) {
        // The git dir sits inside the worktree watch already for a normal
        // repo; a linked worktree's real dir lives elsewhere — add it too.
        let _ = watcher.watch(&git, RecursiveMode::Recursive);
    }

    let flag = dirty.clone();
    let thread = std::thread::Builder::new()
        .name("wackdepot-watch".into())
        .spawn(move || debounce(rx, flag))
        .ok()?;

    Some(RepoWatcher {
        dirty,
        _watcher: watcher,
        _thread: thread,
    })
}

fn debounce(rx: Receiver<()>, dirty: Arc<AtomicBool>) {
    loop {
        // Wait for the first event of a burst.
        if rx.recv().is_err() {
            return; // watcher dropped
        }
        let deadline = Instant::now() + DEBOUNCE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match remaining.is_zero() {
                true => break,
                false => match rx.recv_timeout(remaining) {
                    Ok(()) => {} // more churn — keep waiting out the burst
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                },
            }
        }
        dirty.store(true, Ordering::SeqCst);
    }
}

/// Filter to events that change content/structure we show.
fn interesting(event: &notify::Event) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|p| {
        !p.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            IGNORED_DIRS.contains(&s.as_ref())
        })
    })
}

impl RepoWatcher {
    /// `true` once per burst — drains the flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }
}
