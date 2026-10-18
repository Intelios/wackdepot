//! The repository model: one entity per window owning the open repo's
//! snapshot, selection, diff, and a serial lane of git operations. Views
//! subscribe to [`RepoEvent`]s; nothing here renders.

pub mod confirm;
pub mod diff_load;
mod ops;
mod optimistic;
pub mod selection;
pub mod snapshot;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{App, Context, EventEmitter};
use wackdepot_core::diff::DiffOptions;
use wackdepot_core::progress::Progress;

use crate::app::prefs;
use crate::watcher::{self, RepoWatcher};

pub use confirm::ConfirmAction;
pub use diff_load::{DiffDoc, DiffState, DocKind};
pub use selection::{Selection, Tab};
pub use snapshot::{Bucket, Snapshot};

/// How often the watcher flag and the soft tick are checked.
const POLL: Duration = Duration::from_millis(200);
/// Full reload cadence without file events (remote-side changes).
const SOFT_TICK: Duration = Duration::from_secs(30);

/// What an op is doing, for the sync pill and busy states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub fn label(self) -> &'static str {
        match self {
            BusyOp::Fetch => "Fetching",
            BusyOp::Pull => "Pulling",
            BusyOp::Push => "Pushing",
            BusyOp::Clone => "Cloning",
            BusyOp::Commit => "Committing",
            BusyOp::Checkout => "Switching",
            BusyOp::Other(s) => s,
        }
    }

    /// Network ops show live progress on the sync pill.
    pub fn is_sync(self) -> bool {
        matches!(
            self,
            BusyOp::Fetch | BusyOp::Pull | BusyOp::Push | BusyOp::Clone
        )
    }
}

/// A toast.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
    pub action: Option<NoticeAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

/// A one-click follow-up offered on a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeAction {
    Pull,
    Retry(BusyOp),
}

/// Moments the UI celebrates (signature motion beats).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Beat {
    Staged { paths: Vec<String> },
    Unstaged { paths: Vec<String> },
    Committed { sha: String },
    Pushed,
    Pulled,
    CheckedOut { branch: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepoEvent {
    /// A different repo (or none) is open.
    Opened,
    /// A new snapshot landed.
    Snapshot,
    /// Tab or selection changed.
    Selection,
    /// The content card's diff changed.
    Diff,
    /// An op started/finished or progress moved.
    Busy,
    Notice(Notice),
    /// The model needs the user to confirm something (e.g. force-delete).
    Confirm(ConfirmAction),
    Beat(Beat),
}

/// What a finished op wants to happen next.
#[derive(Default)]
pub struct Done {
    pub toast: Option<String>,
    pub beat: Option<Beat>,
    pub confirm: Option<ConfirmAction>,
    /// Open this repo afterwards (clone / create).
    pub open: Option<PathBuf>,
}

impl Done {
    pub fn quiet() -> Self {
        Self::default()
    }

    pub fn toast(text: impl Into<String>) -> Self {
        Self {
            toast: Some(text.into()),
            ..Self::default()
        }
    }

    pub fn beat(mut self, beat: Beat) -> Self {
        self.beat = Some(beat);
        self
    }
}

type ProgressSink = Arc<Mutex<Option<Progress>>>;
type JobFn = Box<dyn FnOnce(&Path, &ProgressSink) -> Result<Done, String> + Send>;

struct Job {
    busy: BusyOp,
    /// The repo the job was queued against (`None` for clone/create).
    repo: Option<PathBuf>,
    run: JobFn,
}

pub struct RepoModel {
    pub repo: Option<PathBuf>,
    pub snap: Arc<Snapshot>,
    /// The first snapshot for this repo has landed.
    pub loaded: bool,
    pub tab: Tab,
    pub selection: Option<Selection>,
    remembered: [Option<Selection>; 2],
    pub diff: DiffState,
    diff_gen: u64,
    pub busy: Option<BusyOp>,
    pub progress: Option<Progress>,
    sink: ProgressSink,
    queue: VecDeque<Job>,
    running: bool,
    /// Bumped whenever an op finishes; a refresh started before is stale.
    epoch: u64,
    refreshing: bool,
    refresh_queued: bool,
    last_refresh: Instant,
    watcher: Option<RepoWatcher>,
}

impl EventEmitter<RepoEvent> for RepoModel {}

impl RepoModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // `WACKDEPOT_OPEN=<path>` opens a repo at launch (dev and scripting).
        let last = std::env::var_os("WACKDEPOT_OPEN")
            .map(PathBuf::from)
            .or_else(|| prefs::get(cx).last_repo.clone())
            .filter(|p| p.exists() && wackdepot_core::repo::is_repo(p));
        let mut model = Self {
            repo: None,
            snap: Arc::default(),
            loaded: false,
            tab: Tab::Changes,
            selection: None,
            remembered: [None, None],
            diff: DiffState::Empty,
            diff_gen: 0,
            busy: None,
            progress: None,
            sink: Arc::default(),
            queue: VecDeque::new(),
            running: false,
            epoch: 0,
            refreshing: false,
            refresh_queued: false,
            last_refresh: Instant::now(),
            watcher: None,
        };
        if let Some(path) = last {
            model.open(path, cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    return;
                }
            }
        })
        .detach();
        model
    }

    pub fn name(&self) -> Option<String> {
        self.repo
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    }

    pub fn branch_label(&self) -> String {
        match self.snap.status.as_ref() {
            Some(s) => match (&s.branch, &s.detached_at) {
                (Some(b), _) => b.clone(),
                (None, Some(sha)) => format!("Detached at {}", &sha[..sha.len().min(7)]),
                _ => "No branch".into(),
            },
            None => String::new(),
        }
    }

    // ── Repo lifecycle ─────────────────────────────────────────────────

    /// Open `path` (any folder inside a repo), remember it, and load it.
    pub fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let root = match wackdepot_core::repo::repo_root(&path) {
            Ok(root) => root,
            Err(_) => {
                self.notice(
                    format!("{} isn't a git repository.", path.display()),
                    NoticeKind::Error,
                    cx,
                );
                return;
            }
        };
        prefs::update(cx, |s| {
            if !s.repos.contains(&root) {
                s.repos.push(root.clone());
            }
            s.last_repo = Some(root.clone());
        });
        if self.repo.as_ref() == Some(&root) {
            return;
        }
        self.repo = Some(root.clone());
        self.snap = Arc::default();
        self.loaded = false;
        self.selection = None;
        self.remembered = [None, None];
        self.diff = DiffState::Empty;
        self.watcher = watcher::watch(&root);
        cx.emit(RepoEvent::Opened);
        self.request_refresh(cx);
        cx.notify();
    }

    /// Forget a repo from the list (never touches its files).
    pub fn forget(&mut self, path: &Path, cx: &mut Context<Self>) {
        prefs::update(cx, |s| {
            s.repos.retain(|p| p != path);
            if s.last_repo.as_deref() == Some(path) {
                s.last_repo = None;
            }
        });
        if self.repo.as_deref() == Some(path) {
            self.repo = None;
            self.snap = Arc::default();
            self.loaded = false;
            self.selection = None;
            self.diff = DiffState::Empty;
            self.watcher = None;
            cx.emit(RepoEvent::Opened);
        }
        cx.notify();
    }

    // ── Refresh ────────────────────────────────────────────────────────

    fn poll(&mut self, cx: &mut Context<Self>) {
        let progress = self.sink.lock().ok().and_then(|p| p.clone());
        if progress != self.progress && self.busy.is_some() {
            self.progress = progress;
            cx.emit(RepoEvent::Busy);
            cx.notify();
        }
        if self.repo.is_none() {
            return;
        }
        let dirty = self.watcher.as_ref().is_some_and(|w| w.take_dirty());
        if dirty || self.last_refresh.elapsed() >= SOFT_TICK || self.refresh_queued {
            self.request_refresh(cx);
        }
    }

    /// Reload the snapshot in the background. Deferred while an op runs
    /// (it would read a half-written state) and discarded if an op
    /// finished while it was loading.
    pub fn request_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if self.running || self.refreshing || !self.queue.is_empty() {
            self.refresh_queued = true;
            return;
        }
        self.refreshing = true;
        self.refresh_queued = false;
        let epoch = self.epoch;
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn({
                    let repo = repo.clone();
                    async move { Snapshot::load(&repo) }
                })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.refreshing = false;
                this.last_refresh = Instant::now();
                if this.repo.as_ref() != Some(&repo) || this.epoch != epoch {
                    this.refresh_queued = true;
                    return;
                }
                match loaded {
                    Ok(snap) => this.apply_snapshot(snap, cx),
                    Err(e) => this.notice(e, NoticeKind::Error, cx),
                }
            });
        })
        .detach();
    }

    fn apply_snapshot(&mut self, snap: Snapshot, cx: &mut Context<Self>) {
        let snap = Arc::new(snap);
        let prev = std::mem::replace(&mut self.snap, snap);
        let first = !self.loaded;
        self.loaded = true;
        let repaired = match (&self.selection, first) {
            (Some(sel), _) => selection::repair(sel, &prev, &self.snap),
            (None, true) if self.tab == Tab::Changes => selection::first_file(&self.snap),
            (None, _) => None,
        };
        for slot in &mut self.remembered {
            if let Some(sel) = slot.as_ref() {
                *slot = selection::repair(sel, &prev, &self.snap);
            }
        }
        cx.emit(RepoEvent::Snapshot);
        let changed = repaired != self.selection;
        self.selection = repaired;
        if changed {
            cx.emit(RepoEvent::Selection);
        }
        self.load_diff(cx);
        cx.notify();
    }

    // ── Tabs & selection ───────────────────────────────────────────────

    pub fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.remembered[self.tab.index()] = self.selection.take();
        self.tab = tab;
        self.selection = self.remembered[tab.index()].take().or_else(|| match tab {
            Tab::Changes => selection::first_file(&self.snap),
            Tab::History => self.snap.log.first().map(|c| Selection::Commit {
                sha: c.sha.clone(),
                file: None,
            }),
        });
        cx.emit(RepoEvent::Selection);
        self.load_diff(cx);
        cx.notify();
    }

    pub fn select(&mut self, sel: Option<Selection>, cx: &mut Context<Self>) {
        if self.selection == sel {
            return;
        }
        self.selection = sel;
        cx.emit(RepoEvent::Selection);
        self.load_diff(cx);
        cx.notify();
    }

    /// Diff options from settings (whitespace).
    pub fn diff_options(cx: &App) -> DiffOptions {
        DiffOptions {
            ignore_whitespace: prefs::get(cx).ignore_whitespace,
            context: None,
        }
    }

    /// (Re)load the content card for the current selection.
    pub fn load_diff(&mut self, cx: &mut Context<Self>) {
        self.diff_gen += 1;
        let gen_id = self.diff_gen;
        let (Some(repo), Some(sel)) = (self.repo.clone(), self.selection.clone()) else {
            self.diff = DiffState::Empty;
            cx.emit(RepoEvent::Diff);
            return;
        };
        // Keep showing the last diff while the next one loads: reloading
        // the same selection refreshes in place, and a different selection
        // keeps the card's chrome (the card dims it). Only when there is no
        // doc to keep does the card drop to its loading state.
        match &self.diff {
            DiffState::Ready(d) if same_target(&d.selection, &sel) => {}
            DiffState::Ready(_) | DiffState::Loading => {}
            _ => {
                self.diff = DiffState::Loading;
                cx.emit(RepoEvent::Diff);
            }
        }
        let opts = Self::diff_options(cx);
        let snap = self.snap.clone();
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { diff_load::load(&repo, &sel, &snap.log, opts) })
                .await;
            let _ = this.update(cx, move |this, cx| {
                if this.diff_gen != gen_id {
                    return;
                }
                this.diff = match out {
                    Ok(doc) => DiffState::Ready(Arc::new(doc)),
                    Err(e) => DiffState::Error(e),
                };
                cx.emit(RepoEvent::Diff);
                cx.notify();
            });
        })
        .detach();
    }

    // ── The op lane ────────────────────────────────────────────────────

    /// Queue a git op against the open repo. Ops run one at a time, in
    /// order, so fast clicking can't race on `index.lock`.
    pub fn run(
        &mut self,
        busy: BusyOp,
        job: impl FnOnce(&Path, &ProgressSink) -> Result<Done, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        // Any refresh already in flight read the repo before this op: drop it.
        self.epoch += 1;
        self.queue.push_back(Job {
            busy,
            repo: Some(repo),
            run: Box::new(job),
        });
        self.pump(cx);
    }

    /// Show `path` moved to its new bucket right away (see `optimistic`).
    pub(crate) fn optimistic_toggle(&mut self, path: &str, from: Bucket, cx: &mut Context<Self>) {
        let Some(status) = self.snap.status.as_ref() else {
            return;
        };
        let next = optimistic::toggle(status, path, from);
        let prev = self.snap.clone();
        let mut stats = prev.stats.clone();
        // Line stats follow the file to its new side.
        if let Some(st) = prev.line_stat(from, path) {
            if from == Bucket::Staged {
                stats.staged.remove(path);
                stats.unstaged.insert(path.to_string(), st);
            } else {
                stats.unstaged.remove(path);
                stats.staged.insert(path.to_string(), st);
            }
        }
        self.snap = Arc::new(Snapshot {
            status: Some(next),
            branches: prev.branches.clone(),
            log: prev.log.clone(),
            graph: prev.graph.clone(),
            stashes: prev.stashes.clone(),
            unpushed: prev.unpushed.clone(),
            stats,
            identity: prev.identity.clone(),
            tip_unpushed: prev.tip_unpushed,
        });
        if let Some(sel) = &self.selection {
            self.selection = selection::repair(sel, &prev, &self.snap);
        }
        cx.emit(RepoEvent::Snapshot);
        cx.emit(RepoEvent::Selection);
        cx.notify();
    }

    /// Queue an op that isn't tied to the open repo (clone, create). The
    /// job receives an empty path.
    pub fn run_detached(
        &mut self,
        busy: BusyOp,
        job: impl FnOnce(&Path, &ProgressSink) -> Result<Done, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.queue.push_back(Job {
            busy,
            repo: None,
            run: Box::new(job),
        });
        self.pump(cx);
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(job) = self.queue.pop_front() else {
            return;
        };
        self.running = true;
        self.busy = Some(job.busy);
        self.progress = None;
        if let Ok(mut p) = self.sink.lock() {
            *p = None;
        }
        cx.emit(RepoEvent::Busy);
        cx.notify();
        let sink = self.sink.clone();
        let busy = job.busy;
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move {
                    let repo = job.repo.unwrap_or_default();
                    (job.run)(&repo, &sink)
                })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.running = false;
                this.busy = None;
                this.progress = None;
                this.epoch += 1;
                match out {
                    Ok(done) => this.finish(done, cx),
                    Err(e) => {
                        let action = retry_action(busy, &e);
                        cx.emit(RepoEvent::Notice(Notice {
                            text: e,
                            kind: NoticeKind::Error,
                            action,
                        }));
                    }
                }
                cx.emit(RepoEvent::Busy);
                this.request_refresh(cx);
                cx.notify();
                this.pump(cx);
            });
        })
        .detach();
    }

    fn finish(&mut self, done: Done, cx: &mut Context<Self>) {
        if let Some(path) = done.open {
            self.open(path, cx);
        }
        if let Some(text) = done.toast {
            self.notice(text, NoticeKind::Info, cx);
        }
        if let Some(beat) = done.beat {
            cx.emit(RepoEvent::Beat(beat));
        }
        if let Some(action) = done.confirm {
            cx.emit(RepoEvent::Confirm(action));
        }
    }

    pub fn notice(&mut self, text: impl Into<String>, kind: NoticeKind, cx: &mut Context<Self>) {
        cx.emit(RepoEvent::Notice(Notice {
            text: text.into(),
            kind,
            action: None,
        }));
    }
}

/// Same file/commit (ignoring which commit file is picked).
pub(crate) fn same_target(a: &Selection, b: &Selection) -> bool {
    match (a, b) {
        (
            Selection::File {
                path: p1,
                bucket: b1,
            },
            Selection::File {
                path: p2,
                bucket: b2,
            },
        ) => p1 == p2 && b1 == b2,
        (Selection::Commit { sha: s1, file: f1 }, Selection::Commit { sha: s2, file: f2 }) => {
            s1 == s2 && (f2.is_none() || f1 == f2)
        }
        _ => false,
    }
}

/// The follow-up worth offering on a failed op's toast.
fn retry_action(busy: BusyOp, err: &str) -> Option<NoticeAction> {
    let lower = err.to_lowercase();
    if busy == BusyOp::Push
        && (lower.contains("fetch first")
            || lower.contains("non-fast-forward")
            || lower.contains("rejected"))
    {
        return Some(NoticeAction::Pull);
    }
    busy.is_sync().then_some(NoticeAction::Retry(busy))
}
