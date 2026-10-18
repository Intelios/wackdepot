//! The window shell — the single owning entity. Layout: titlebar drag
//! strips, glass rail (repo picker + tabs + file/commit list + commit box),
//! content pane (diff/commit detail). State, ops and refresh live here;
//! per-area render code sits in `changes`/`diffview`/`history`/`dialogs`/
//! `pickers` as `impl Shell` blocks.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{
    AppContext as _, Context, Entity, MouseButton, Stateful, Window, WindowControlArea, prelude::*,
};
use wackdepot_core::{branch, commit, diff, stash, status};

use crate::input::TextInput;
use crate::settings::{self, Settings};
use crate::state::{Bucket, BusyOp, Dialog, Notice, PopoverKind, Selected, Tab};
use crate::theme::Theme;
use crate::watcher::{self, RepoWatcher};

/// How often the shell polls the watcher flag and soft-refreshes.
const WATCH_POLL: Duration = Duration::from_millis(200);
/// How often a repo's full git data reloads even without file events
/// (covers remote-side changes and anything the watcher can't see).
const REFRESH_TICK: Duration = Duration::from_secs(30);

pub struct Shell {
    pub settings: Settings,
    /// Open repo (into `repos`); `None` shows the welcome pane.
    pub active: Option<PathBuf>,
    /// The repo data snapshot; `None` until first refresh lands.
    pub repo: RepoData,
    pub tab: Tab,
    pub selected: Option<Selected>,
    /// Diff of the selected file/commit; refreshed on select + repo refresh.
    pub selected_diff: Option<SelectedDiff>,
    /// File-diff list for the selected commit (history detail pane).
    pub commit_files: Vec<diff::FileDiff>,
    pub popover: Option<PopoverKind>,
    pub dialog: Option<Dialog>,
    pub busy: Option<BusyOp>,
    /// Progress line from a running sync op (last stderr line).
    pub progress: Option<String>,
    pub notice: Option<Notice>,
    notice_id: u64,
    /// Refresh generation: bumped when an op mutates the repo so the next
    /// poll reloads status. A refresh marks `refreshed_gen = refresh_gen`.
    pub refresh_gen: u64,
    refreshed_gen: u64,
    last_refresh: Instant,
    watcher: Option<RepoWatcher>,
    pub summary: Entity<TextInput>,
    pub description: Entity<TextInput>,
    pub search: Entity<TextInput>,
    pub clone_url: Entity<TextInput>,
    pub clone_dir: Entity<TextInput>,
    pub new_repo_name: Entity<TextInput>,
    pub new_repo_dir: Entity<TextInput>,
    pub branch_name: Entity<TextInput>,
    /// Base ref for "New branch" — defaults to HEAD.
    pub branch_base: Entity<TextInput>,
    /// Stash list for the sync menu.
    pub stashes: Vec<stash::StashEntry>,
    /// Collapsed groups in the changes list.
    pub collapsed: HashSet<&'static str>,
    titlebar_should_move: bool,
    /// The diff pane's whitespace mode ("ignore whitespace" toggle).
    pub ignore_ws: bool,
    /// The repo's git user identity for the commit box.
    pub author_name: String,
    pub author_email: String,
    /// Settings ▸ editor app name ("WackCode", "Zed", …).
    pub editor_app: Entity<TextInput>,
    /// Whether the tip commit is unpushed (undo is safe to offer).
    pub tip_is_unpushed: bool,
    /// Last geometry written to settings — only save on change.
    last_saved_geo: Option<settings::WindowGeo>,
}

/// Everything we snapshot from git for the open repo.
#[derive(Default)]
pub struct RepoData {
    pub status: Option<status::RepoStatus>,
    pub branches: Vec<branch::BranchInfo>,
    pub log: Vec<commit::LogEntry>,
}

/// The loaded diff for the right pane — file diffs always carry their
/// bucket so hunk actions know which side to apply against.
pub struct SelectedDiff {
    pub file: diff::FileDiff,
    pub bucket: Bucket,
    /// For Commit selections: all diffs of that commit.
    pub all: Vec<diff::FileDiff>,
    pub commit: Option<commit::LogEntry>,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = settings::load();
        let saved_editor = settings.editor_app.clone().unwrap_or_default();
        let mk = |placeholder: &str, cx: &mut Context<Shell>| {
            cx.new(|cx| TextInput::new(placeholder.to_string(), cx))
        };
        let active = settings
            .last_repo
            .clone()
            .filter(|p| p.exists() && wackdepot_core::repo::is_repo(p));

        let mut shell = Shell {
            settings,
            active,
            repo: RepoData::default(),
            tab: Tab::Changes,
            selected: None,
            selected_diff: None,
            commit_files: Vec::new(),
            popover: None,
            dialog: None,
            busy: None,
            progress: None,
            notice: None,
            notice_id: 0,
            refresh_gen: 1,
            refreshed_gen: 0,
            last_refresh: Instant::now() - REFRESH_TICK,
            watcher: None,
            summary: mk("Summary (required)", cx),
            description: mk("Description", cx),
            search: mk("Filter", cx),
            clone_url: mk("Repository URL", cx),
            clone_dir: mk("Local folder", cx),
            new_repo_name: mk("Repository name", cx),
            new_repo_dir: mk("Local folder", cx),
            branch_name: mk("Branch name", cx),
            branch_base: mk("HEAD", cx),
            editor_app: {
                let e = mk("WackCode", cx);
                if !saved_editor.is_empty() {
                    e.update(cx, |i, cx| i.set_content(&saved_editor, cx));
                }
                e
            },
            stashes: Vec::new(),
            collapsed: HashSet::new(),
            titlebar_should_move: false,
            ignore_ws: false,
            author_name: String::new(),
            author_email: String::new(),
            tip_is_unpushed: false,
            last_saved_geo: None,
        };
        if shell.active.is_some() {
            shell.start_watcher();
            shell.request_refresh(cx);
        }
        // Poll the watcher flag and the soft tick on one timer.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WATCH_POLL).await;
                if this.update(cx, |this, cx| this.poll_refresh(cx)).is_err() {
                    return;
                }
            }
        })
        .detach();
        shell
    }

    // ------------------------------------------------------------ core flow

    /// Refresh when the watcher fired, an op bumped `refresh_gen`, or the
    /// soft tick expired.
    fn poll_refresh(&mut self, cx: &mut Context<Self>) {
        let dirty = self.watcher.as_ref().is_some_and(|w| w.take_dirty());
        let stale_gen = self.refresh_gen != self.refreshed_gen;
        let timed_out = self.last_refresh.elapsed() >= REFRESH_TICK;
        if self.active.is_some() && (dirty || stale_gen || timed_out) {
            self.request_refresh(cx);
        }
    }

    /// Rebuild `RepoData` for the active repo on the background executor.
    pub fn request_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.active.clone() else {
            return;
        };
        let gen_id = self.refresh_gen;
        cx.spawn(async move |this, cx| {
            let data = cx
                .background_executor()
                .spawn({
                    let repo = repo.clone();
                    async move { load_repo_data(&repo) }
                })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.refreshed_gen = gen_id;
                this.last_refresh = Instant::now();
                match data {
                    Ok(data) => this.apply_repo_data(data, cx),
                    Err(e) => this.notify_error(e, cx),
                }
                this.refresh_selected_diff(cx);
                this.tip_unpushed(cx);
            });
        })
        .detach();
    }

    /// Replace the snapshot; repair a stale selection (file vanished).
    fn apply_repo_data(&mut self, data: RepoData, cx: &mut Context<Self>) {
        self.repo = data;
        self.repair_selection();
        cx.notify();
    }

    /// If the selected file/commit disappeared, drop or remap the selection.
    fn repair_selection(&mut self) {
        match self.selected {
            Some(Selected::File { bucket, index }) => {
                let len = self.bucket_list(bucket).len();
                if index >= len {
                    self.selected = (len > 0).then_some(Selected::File { bucket, index: 0 });
                    if self.selected.is_none() {
                        self.selected_diff = None;
                    }
                }
            }
            Some(Selected::Commit { index }) if index >= self.repo.log.len() => {
                self.selected =
                    (!self.repo.log.is_empty()).then_some(Selected::Commit { index: 0 });
                if self.selected.is_none() {
                    self.selected_diff = None;
                }
            }
            _ => {}
        }
    }

    /// Files in one bucket of the current status snapshot.
    pub fn bucket_list(&self, bucket: Bucket) -> &[status::FileChange] {
        let Some(s) = &self.repo.status else {
            return &[];
        };
        match bucket {
            Bucket::Staged => &s.staged,
            Bucket::Unstaged => &s.unstaged,
            Bucket::Untracked => &s.untracked,
            Bucket::Conflicted => &s.conflicted,
        }
    }

    /// Load the diff for the current selection (file or commit).
    pub fn refresh_selected_diff(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.active.clone() else {
            return;
        };
        let sel = self.selected.clone();
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { load_selected_diff(&repo, sel.as_ref()) })
                .await;
            let _ = this.update(cx, move |this, cx| {
                match out {
                    Ok(d) => this.selected_diff = d,
                    Err(e) => this.notify_error(e, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------ ops

    /// Run a fallible git op in the background, then refresh. `busy` shows
    /// the titlebar spinner; `Ok("msg")` becomes a toast, `Ok(())` stays
    /// quiet, `Err` becomes an error toast.
    pub fn run_op<F, M>(&mut self, busy: BusyOp, op: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&Path) -> Result<M, String> + Send + 'static,
        M: OpOut + Send + 'static,
    {
        let Some(repo) = self.active.clone() else {
            return;
        };
        self.busy = Some(busy);
        self.progress = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { op(&repo) })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.busy = None;
                this.progress = None;
                match out {
                    Ok(msg) => {
                        this.refresh_gen += 1;
                        if let Some(msg) = msg.op_message() {
                            this.notify(msg, false, cx);
                        }
                    }
                    Err(e) => this.notify_error(e, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Toast: `text` verbatim (error strings are already user sentences).
    pub fn notify(&mut self, text: impl Into<String>, is_error: bool, cx: &mut Context<Self>) {
        self.notice_id += 1;
        let id = self.notice_id;
        self.notice = Some(Notice {
            text: text.into(),
            is_error,
            id,
        });
        cx.notify();
        // Auto-dismiss after 6s unless replaced.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(6)).await;
            let _ = this.update(cx, move |this, cx| {
                if this.notice.as_ref().is_some_and(|n| n.id == id) {
                    this.notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn notify_error(&mut self, text: String, cx: &mut Context<Self>) {
        self.notify(text, true, cx);
    }

    // ------------------------------------------------------------ repo mgmt

    /// Open `path` (must be a repo); add to settings, switch, watch, refresh.
    pub fn open_repo(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let root = match wackdepot_core::repo::repo_root(&path) {
            Ok(root) => root,
            Err(_) => {
                self.notify_error("That folder isn't a git repository.".into(), cx);
                return;
            }
        };
        if !self.settings.repos.contains(&root) {
            self.settings.repos.push(root.clone());
        }
        self.settings.last_repo = Some(root.clone());
        settings::save(&self.settings);
        if self.active.as_ref() == Some(&root) {
            self.popover = None;
            cx.notify();
            return;
        }
        self.active = Some(root);
        self.repo = RepoData::default();
        self.selected = None;
        self.selected_diff = None;
        self.popover = None;
        self.author_name.clear();
        self.author_email.clear();
        self.start_watcher();
        self.request_refresh(cx);
        self.load_identity(cx);
        self.tip_unpushed(cx);
        cx.notify();
    }

    /// Remove a repo from the sidebar list (never touches files).
    pub fn remove_repo(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.settings.repos.retain(|p| p != path);
        if self.settings.last_repo.as_deref() == Some(path) {
            self.settings.last_repo = None;
        }
        settings::save(&self.settings);
        if self.active.as_deref() == Some(path) {
            self.active = None;
            self.repo = RepoData::default();
            self.selected = None;
            self.selected_diff = None;
            self.watcher = None;
        }
        cx.notify();
    }

    fn start_watcher(&mut self) {
        self.watcher = self.active.as_ref().and_then(|p| watcher::watch(p));
    }

    fn load_identity(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.active.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { commit::author(&repo) })
                .await;
            let _ = this.update(cx, move |this, cx| {
                if let Ok((n, e)) = out {
                    this.author_name = n;
                    this.author_email = e;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ----------------------------------------------------- titlebar strips

    /// A titlebar drag surface (zeron pattern): `.occlude()`d controls stay
    /// interactive, empty space drags, double-click zooms natively.
    pub fn drag_strip(
        &self,
        id: &'static str,
        el: gpui::Div,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        el.id(id)
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.titlebar_should_move = false))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.titlebar_should_move = false),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.titlebar_should_move = true),
            )
            // Hand the drag to AppKit's native session only while the button
            // is actually held — a mere hover must never start a native drag.
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, _| {
                    if this.titlebar_should_move && event.pressed_button == Some(MouseButton::Left)
                    {
                        this.titlebar_should_move = false;
                        window.start_window_move();
                    }
                }),
            )
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }

    /// The resolved theme for this frame (settings override macOS).
    pub fn theme(&self, window: &Window) -> Theme {
        use crate::settings::AppearanceMode;
        match self.settings.appearance {
            AppearanceMode::System => Theme::for_appearance(window.appearance()),
            AppearanceMode::Dark => Theme::dark(),
            AppearanceMode::Light => Theme::light(),
        }
    }

    /// The active repo's display name (folder basename).
    pub fn active_name(&self) -> String {
        self.active
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "WackDepot".into())
    }

    /// Current branch label for the titlebar.
    pub fn branch_label(&self) -> String {
        match &self.repo.status {
            Some(s) => s.branch.clone().unwrap_or_else(|| {
                s.detached_at
                    .as_deref()
                    .map(|d| d[..7.min(d.len())].to_string())
                    .unwrap_or_else(|| "detached".into())
            }),
            None => "—".into(),
        }
    }

    /// Persist window geometry when it changes (throttled to real moves).
    pub fn observe_geometry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.observe_window_bounds(window, |this, window, cx| {
            let b = window.bounds();
            let geo = settings::WindowGeo {
                x: b.origin.x.into(),
                y: b.origin.y.into(),
                w: b.size.width.into(),
                h: b.size.height.into(),
            };
            let changed = this.last_saved_geo.is_none_or(|g| {
                (g.x - geo.x).abs() > 1.
                    || (g.y - geo.y).abs() > 1.
                    || (g.w - geo.w).abs() > 1.
                    || (g.h - geo.h).abs() > 1.
            });
            if changed {
                this.last_saved_geo = Some(geo);
                this.settings.window = Some(geo);
                settings::save(&this.settings);
            }
            cx.notify();
        })
        .detach();
    }

    /// Dismiss the topmost layer: dialog → popover → notice.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            self.dialog = None;
        } else if self.popover.is_some() {
            self.popover = None;
        } else {
            self.notice = None;
        }
        cx.notify();
    }
}

/// What a background op returns: `()` for silent success, `String` for a
/// toast message.
pub trait OpOut {
    fn op_message(self) -> Option<String>;
}

impl OpOut for () {
    fn op_message(self) -> Option<String> {
        None
    }
}

impl OpOut for String {
    fn op_message(self) -> Option<String> {
        Some(self)
    }
}

fn load_repo_data(repo: &Path) -> Result<RepoData, String> {
    let status =
        status::status(repo).map_err(|e| format!("Couldn't read {} — {e}", repo.display()))?;
    let branches = branch::list_branches(repo).unwrap_or_default();
    let log = commit::list_log(repo, 400).unwrap_or_default();
    Ok(RepoData {
        status: Some(status),
        branches,
        log,
    })
}

fn load_selected_diff(repo: &Path, sel: Option<&Selected>) -> Result<Option<SelectedDiff>, String> {
    let Some(sel) = sel else {
        return Ok(None);
    };
    match sel {
        Selected::File { bucket, index } => {
            let status = status::status(repo)?;
            let list = match bucket {
                Bucket::Staged => &status.staged,
                Bucket::Unstaged => &status.unstaged,
                Bucket::Untracked => &status.untracked,
                Bucket::Conflicted => &status.conflicted,
            };
            let Some(change) = list.get(*index) else {
                return Ok(None);
            };
            let side = match bucket {
                Bucket::Staged => diff::DiffSide::Staged,
                _ => diff::DiffSide::Unstaged,
            };
            let file = diff::diff(repo, &change.path, side)?;
            Ok(Some(SelectedDiff {
                file,
                bucket: *bucket,
                all: Vec::new(),
                commit: None,
            }))
        }
        Selected::Commit { index } => {
            let log = commit::list_log(repo, 400)?;
            let Some(entry) = log.get(*index) else {
                return Ok(None);
            };
            let all = diff::diff_commit(repo, &entry.sha)?;
            Ok(Some(SelectedDiff {
                file: diff::FileDiff {
                    old_path: None,
                    new_path: Some(entry.subject.clone()),
                    status: diff::DiffFileStatus::Modified,
                    header_text: String::new(),
                    hunks: Vec::new(),
                    is_binary: false,
                },
                bucket: Bucket::Unstaged,
                all,
                commit: Some(entry.clone()),
            }))
        }
    }
}
