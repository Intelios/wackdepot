//! Dialog bodies: clone, create, and the small name-it dialogs (rename a
//! branch, tag a commit, branch from a commit). Each owns its fields;
//! Enter submits, Esc (the app's Dismiss) cancels.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{
    App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, WeakEntity, Window, div, prelude::*, px,
};
use wackdepot_core::merge::{MergeMethod, MergePreview};

use crate::app::overlays::Overlays;
use crate::components::button::button;
use crate::components::mark::accent_mark;
use crate::components::menu::{menu_heading, menu_row};
use crate::components::pill_track::pill_track;
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::model::RepoModel;
use crate::theme::{ActiveTheme, metrics};

/// A labelled input on a solid well with a focus halo.
pub fn field(
    label: &str,
    input: &Entity<TextInput>,
    trailing: Option<gpui::AnyElement>,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let t = cx.theme().clone();
    let focused = input.read(cx).is_focused(window);
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(metrics::TEXT_SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.text_soft)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(36.))
                .px(px(12.))
                .rounded(px(metrics::RADIUS_INPUT))
                .bg(t.input)
                .border_1()
                .border_color(if focused { t.accent_line_mid } else { t.border })
                .when(focused, |el| {
                    el.shadow(vec![gpui::BoxShadow {
                        color: t.focus_halo(),
                        offset: gpui::point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(3.),
                        inset: false,
                    }])
                })
                .text_size(px(metrics::TEXT_ROW))
                .child(div().flex_1().min_w_0().child(input.clone()))
                .children(trailing),
        )
}

fn head(icon_name: &'static str, title: &str, sub: &str, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(accent_mark(icon_name).size(34.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(metrics::TEXT_TITLE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(metrics::TEXT_META))
                        .text_color(t.text_dim)
                        .child(sub.to_string()),
                ),
        )
}

fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

/// `~/Documents/GitHub` when it exists (GitHub Desktop's default), else home.
fn default_parent() -> PathBuf {
    let gh = home().join("Documents").join("GitHub");
    if gh.is_dir() { gh } else { home() }
}

fn tilde(p: &std::path::Path) -> String {
    let h = home();
    match p.strip_prefix(&h) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

fn untilde(s: &str) -> PathBuf {
    match s.trim().strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(s.trim()),
    }
}

// ── Clone ─────────────────────────────────────────────────────────────

pub struct CloneDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    url: Entity<TextInput>,
    dest: Entity<TextInput>,
    /// The user typed their own destination; stop following the URL.
    dest_edited: bool,
    error: Option<SharedString>,
}

impl CloneDialog {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let url = cx.new(|cx| TextInput::new("https://github.com/owner/repo.git", cx));
        let dest = cx.new(|cx| TextInput::new("~/Documents/GitHub/repo", cx));
        cx.subscribe(&url, |this: &mut Self, _, e: &TextInputEvent, cx| match e {
            TextInputEvent::Changed => {
                this.error = None;
                if !this.dest_edited {
                    let url = this.url.read(cx).content().to_string();
                    let name = wackdepot_core::sync::name_from_url(url.trim()).unwrap_or_default();
                    let path = if name.is_empty() {
                        String::new()
                    } else {
                        tilde(&default_parent().join(name))
                    };
                    this.dest.update(cx, |d, cx| d.set_content(path, cx));
                    this.dest_edited = false;
                }
            }
            TextInputEvent::Submit => this.submit(cx),
        })
        .detach();
        cx.subscribe(
            &dest,
            |this: &mut Self, _, e: &TextInputEvent, cx| match e {
                TextInputEvent::Changed => this.error = None,
                TextInputEvent::Submit => this.submit(cx),
            },
        )
        .detach();
        url.update(cx, |i, cx| i.focus(window, cx));
        Self {
            model,
            overlays,
            url,
            dest,
            dest_edited: false,
            error: None,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let url = self.url.read(cx).content().trim().to_string();
        let dest = self.dest.read(cx).content().trim().to_string();
        if url.is_empty() {
            self.error = Some("Paste the repository's URL first.".into());
            cx.notify();
            return;
        }
        if dest.is_empty() {
            self.error = Some("Pick where the repository should go.".into());
            cx.notify();
            return;
        }
        let dest = untilde(&dest);
        if dest.exists()
            && std::fs::read_dir(&dest)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            self.error = Some(format!("{} already exists and isn't empty.", tilde(&dest)).into());
            cx.notify();
            return;
        }
        self.model.update(cx, |m, cx| m.clone_repo(url, dest, cx));
        if let Some(o) = self.overlays.upgrade() {
            o.update(cx, |o, cx| o.close(cx));
        }
    }

    fn choose(&mut self, cx: &mut Context<Self>) {
        let task = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Clone here".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = task.await
                && let Some(parent) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    let url = this.url.read(cx).content().to_string();
                    let name = wackdepot_core::sync::name_from_url(url.trim())
                        .unwrap_or_else(|| "repo".into());
                    let p = tilde(&parent.join(name));
                    this.dest.update(cx, |d, cx| d.set_content(p, cx));
                    this.dest_edited = true;
                });
            }
        })
        .detach();
    }
}

impl Render for CloneDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let entity = cx.entity();
        let choose = {
            let e = entity.clone();
            button("clone-choose", "Choose…")
                .small()
                .on_click(move |_, _, cx| e.update(cx, |d, cx| d.choose(cx)))
        };
        let cancel = {
            let o = self.overlays.clone();
            button("clone-cancel", "Cancel").on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            })
        };
        let go = {
            let e = entity.clone();
            button("clone-go", "Clone")
                .primary()
                .icon("cloud")
                .on_click(move |_, _, cx| e.update(cx, |d, cx| d.submit(cx)))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(head(
                "cloud",
                "Clone a repository",
                "Copy a remote repository to this Mac.",
                cx,
            ))
            .child(field("Repository URL", &self.url, None, window, cx))
            .child(field(
                "Destination",
                &self.dest,
                Some(choose.into_any_element()),
                window,
                cx,
            ))
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_size(px(12.)).text_color(t.sem.danger).child(e)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(cancel)
                    .child(go),
            )
    }
}

// ── New repository ────────────────────────────────────────────────────

pub struct NewRepoDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    name: Entity<TextInput>,
    parent: Entity<TextInput>,
    error: Option<SharedString>,
}

impl NewRepoDialog {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| TextInput::new("my-project", cx));
        let parent = cx.new(|cx| {
            let mut i = TextInput::new("~/Documents/GitHub", cx);
            i.set_content(tilde(&default_parent()), cx);
            i
        });
        for input in [&name, &parent] {
            cx.subscribe(
                input,
                |this: &mut Self, _, e: &TextInputEvent, cx| match e {
                    TextInputEvent::Changed => this.error = None,
                    TextInputEvent::Submit => this.submit(cx),
                },
            )
            .detach();
        }
        name.update(cx, |i, cx| i.focus(window, cx));
        Self {
            model,
            overlays,
            name,
            parent,
            error: None,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).content().trim().to_string();
        let parent = self.parent.read(cx).content().trim().to_string();
        if name.is_empty() || name.contains('/') {
            self.error = Some("Give the repository a folder name (no slashes).".into());
            cx.notify();
            return;
        }
        let path = untilde(&parent).join(&name);
        if path.exists()
            && std::fs::read_dir(&path)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            self.error = Some(format!("{} already exists and isn't empty.", tilde(&path)).into());
            cx.notify();
            return;
        }
        self.model.update(cx, |m, cx| m.create_repo(path, cx));
        if let Some(o) = self.overlays.upgrade() {
            o.update(cx, |o, cx| o.close(cx));
        }
    }

    fn choose(&mut self, cx: &mut Context<Self>) {
        let task = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Create here".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = task.await
                && let Some(p) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.parent.update(cx, |i, cx| i.set_content(tilde(&p), cx))
                });
            }
        })
        .detach();
    }
}

impl Render for NewRepoDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let entity = cx.entity();
        let choose = {
            let e = entity.clone();
            button("new-choose", "Choose…")
                .small()
                .on_click(move |_, _, cx| e.update(cx, |d, cx| d.choose(cx)))
        };
        let cancel = {
            let o = self.overlays.clone();
            button("new-cancel", "Cancel").on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            })
        };
        let go = {
            let e = entity.clone();
            button("new-go", "Create")
                .primary()
                .icon("plus")
                .on_click(move |_, _, cx| e.update(cx, |d, cx| d.submit(cx)))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(head(
                "folder-plus",
                "New repository",
                "A fresh repository on main, ready for its first commit.",
                cx,
            ))
            .child(field("Name", &self.name, None, window, cx))
            .child(field(
                "Inside",
                &self.parent,
                Some(choose.into_any_element()),
                window,
                cx,
            ))
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_size(px(12.)).text_color(t.sem.danger).child(e)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(cancel)
                    .child(go),
            )
    }
}

// ── Name dialogs ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum NameKind {
    RenameBranch { old: String },
    Tag { sha: String, short: String },
    BranchAt { sha: String, short: String },
}

pub struct NameDialog {
    kind: NameKind,
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    name: Entity<TextInput>,
    message: Option<Entity<TextInput>>,
}

impl NameDialog {
    pub fn new(
        kind: NameKind,
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| {
            let mut i = TextInput::new(
                match &kind {
                    NameKind::Tag { .. } => "v1.0.0",
                    _ => "feature/new-idea",
                },
                cx,
            );
            if let NameKind::RenameBranch { old } = &kind {
                i.set_content(old.clone(), cx);
            }
            i
        });
        let message = matches!(kind, NameKind::Tag { .. })
            .then(|| cx.new(|cx| TextInput::new("Optional — makes it an annotated tag", cx)));
        for input in std::iter::once(&name).chain(message.iter()) {
            cx.subscribe(input, |this: &mut Self, _, e: &TextInputEvent, cx| {
                if *e == TextInputEvent::Submit {
                    this.submit(cx);
                }
            })
            .detach();
        }
        name.update(cx, |i, cx| i.focus(window, cx));
        Self {
            kind,
            model,
            overlays,
            name,
            message,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).content().trim().to_string();
        if name.is_empty() {
            return;
        }
        let kind = self.kind.clone();
        let message = self
            .message
            .as_ref()
            .map(|m| m.read(cx).content().to_string())
            .unwrap_or_default();
        self.model.update(cx, |m, cx| match kind {
            NameKind::RenameBranch { old } => m.rename_branch(old, name, cx),
            NameKind::Tag { sha, .. } => m.create_tag(name, sha, message, cx),
            NameKind::BranchAt { sha, .. } => m.create_branch(name, sha, cx),
        });
        if let Some(o) = self.overlays.upgrade() {
            o.update(cx, |o, cx| o.close(cx));
        }
    }
}

impl Render for NameDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let (glyph, title, sub, label, go_label) = match &self.kind {
            NameKind::RenameBranch { old } => (
                "pencil",
                "Rename branch".to_string(),
                format!("Renaming {old}."),
                "New name",
                "Rename",
            ),
            NameKind::Tag { short, .. } => (
                "tag",
                "Create tag".to_string(),
                format!("Tag commit {short}."),
                "Tag name",
                "Create tag",
            ),
            NameKind::BranchAt { short, .. } => (
                "branch",
                "Branch from here".to_string(),
                format!("A new branch starting at {short}, checked out."),
                "Branch name",
                "Create branch",
            ),
        };
        let cancel = {
            let o = self.overlays.clone();
            button("name-cancel", "Cancel").on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            })
        };
        let go = button("name-go", go_label)
            .primary()
            .on_click(move |_, _, cx| entity.update(cx, |d, cx| d.submit(cx)));
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(head(glyph, &title, &sub, cx))
            .child(field(label, &self.name, None, window, cx))
            .children(
                self.message
                    .as_ref()
                    .map(|m| field("Message", m, None, window, cx)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(cancel)
                    .child(go),
            )
    }
}

// ── Merge ─────────────────────────────────────────────────────────────

/// Pick a branch to merge (or squash) into the current one. Rows get a
/// `N commits` / `up to date` / conflict note as `merge_preview` answers
/// arrive — computed in one background pass at open.
pub struct MergeDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    filter: Entity<TextInput>,
    /// The picked source, if any.
    selected: Option<String>,
    method: MergeMethod,
    /// Per-source preview, filled in the background after open.
    previews: HashMap<String, MergePreview>,
}

impl MergeDialog {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        preselect: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| TextInput::new("Filter branches", cx));
        cx.subscribe(&filter, |this: &mut Self, _, e: &TextInputEvent, cx| {
            if *e == TextInputEvent::Submit {
                this.go(cx);
            }
            cx.notify();
        })
        .detach();
        filter.update(cx, |i, cx| i.focus(window, cx));
        if let Some(repo) = model.read(cx).repo.clone() {
            let names = {
                let m = model.read(cx);
                m.snap
                    .branches
                    .iter()
                    .map(|b| b.name.clone())
                    .chain(m.snap.remote_branches.iter().cloned())
                    .collect::<Vec<_>>()
            };
            cx.spawn(async move |this, cx| {
                let previews = cx
                    .background_executor()
                    .spawn(async move {
                        names
                            .iter()
                            .map(|n| (n.clone(), wackdepot_core::merge::merge_preview(&repo, n)))
                            .collect::<HashMap<_, _>>()
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.previews = previews;
                    cx.notify();
                });
            })
            .detach();
        }
        Self {
            model,
            overlays,
            filter,
            selected: preselect,
            method: MergeMethod::Commit,
            previews: HashMap::new(),
        }
    }

    /// Every merge source after the filter: local branches, then remotes.
    fn visible(&self, cx: &App) -> (Vec<String>, Vec<String>) {
        let model = self.model.read(cx);
        let filter = self.filter.read(cx).content().to_lowercase();
        let current = model.snap.status.as_ref().and_then(|s| s.branch.clone());
        let keep = |name: &String| {
            Some(name) != current.as_ref()
                && (filter.is_empty() || name.to_lowercase().contains(&filter))
        };
        let local = model
            .snap
            .branches
            .iter()
            .map(|b| b.name.clone())
            .filter(keep)
            .collect();
        let remote = model
            .snap
            .remote_branches
            .iter()
            .filter(|n| keep(n))
            .cloned()
            .collect();
        (local, remote)
    }

    /// A source can be merged once its preview says it brings commits;
    /// before previews land everything counts as mergeable.
    fn enabled(&self, name: &str) -> bool {
        self.previews.get(name).is_none_or(|p| p.incoming > 0)
    }

    /// The explicit pick, else the first mergeable row in view.
    fn chosen(&self, cx: &App) -> Option<String> {
        if let Some(name) = &self.selected
            && self.enabled(name)
        {
            return Some(name.clone());
        }
        let (local, remote) = self.visible(cx);
        local.into_iter().chain(remote).find(|n| self.enabled(n))
    }

    fn go(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.chosen(cx) else {
            return;
        };
        self.model
            .update(cx, |m, cx| m.merge_branch(name, self.method, cx));
        if let Some(o) = self.overlays.upgrade() {
            o.update(cx, |o, cx| o.close(cx));
        }
    }

    fn source_row(
        &self,
        i: usize,
        name: &str,
        remote: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        let preview = self.previews.get(name);
        let stale = preview.is_some_and(|p| p.incoming == 0);
        let conflicts = preview.and_then(|p| p.conflicts.as_ref()).cloned();
        let incoming = preview.map(|p| p.incoming).unwrap_or(0);
        let mut row = menu_row(("merge-src", i), name.to_string())
            .mono()
            .icon(if remote { "cloud" } else { "branch" })
            .selected(self.selected.as_deref() == Some(name));
        if stale {
            row = row.hint("up to date").disabled(true);
        } else {
            if conflicts.as_ref().is_some_and(|c| !c.is_empty()) {
                let n = conflicts.as_ref().map(|c| c.len()).unwrap_or(0);
                row = row.trailing(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(t.sem.warning)
                        .child(format!("{n} conflict{}", if n == 1 { "" } else { "s" })),
                );
            } else if preview.is_some() {
                row = row.hint(format!(
                    "{incoming} commit{}",
                    if incoming == 1 { "" } else { "s" }
                ));
            }
            let e = cx.entity();
            let name = name.to_string();
            row = row.on_click(move |_, _, cx| {
                e.update(cx, |d: &mut Self, cx| {
                    d.selected = Some(name.clone());
                    cx.notify();
                })
            });
        }
        row
    }
}

impl Render for MergeDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let current = self.model.read(cx).branch_label();
        let (local, remote) = self.visible(cx);
        let mut rows = Vec::new();
        let mut i = 0usize;
        if !local.is_empty() {
            rows.push(menu_heading("Local branches", cx).into_any_element());
            for name in &local {
                rows.push(self.source_row(i, name, false, cx).into_any_element());
                i += 1;
            }
        }
        if !remote.is_empty() {
            rows.push(menu_heading("Remote branches", cx).into_any_element());
            for name in &remote {
                rows.push(self.source_row(i, name, true, cx).into_any_element());
                i += 1;
            }
        }
        if rows.is_empty() {
            rows.push(
                div()
                    .px(px(9.))
                    .py(px(10.))
                    .text_size(px(12.5))
                    .text_color(t.text_dim)
                    .child("No branches match.")
                    .into_any_element(),
            );
        }

        let verb = match self.method {
            MergeMethod::Commit => "Merge",
            MergeMethod::Squash => "Squash",
        };
        let (status_text, status_color) = match self.selected.clone() {
            None => (
                "Pick a branch — the row tells you what merging it would do.".to_string(),
                t.text_faint,
            ),
            Some(name) => match self.previews.get(&name) {
                None => (format!("Checking {name}…"), t.text_faint),
                Some(p) if p.conflicts.as_ref().is_some_and(|c| !c.is_empty()) => {
                    let files = p.conflicts.as_deref().unwrap_or(&[]);
                    let shown = files.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                    let more = files.len().saturating_sub(3);
                    (
                        format!(
                            "{verb}ing {name} would conflict in {shown}{}.",
                            if more > 0 {
                                format!(" + {more} more")
                            } else {
                                String::new()
                            }
                        ),
                        t.sem.warning,
                    )
                }
                Some(p) => (
                    format!(
                        "{verb}ing {} commit{} from {name} into {current} — no conflicts expected.",
                        p.incoming,
                        if p.incoming == 1 { "" } else { "s" },
                    ),
                    t.text_dim,
                ),
            },
        };

        let entity = cx.entity();
        let cancel = {
            let o = self.overlays.clone();
            button("merge-cancel", "Cancel").on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            })
        };
        let can_go = self.chosen(cx).is_some();
        let go = button("merge-go", format!("{verb} into {current}"))
            .primary()
            .icon("merge")
            .disabled(!can_go)
            .on_click(move |_, _, cx| entity.update(cx, |d, cx| d.go(cx)));
        let e = cx.entity();
        let method_track = pill_track("merge-method")
            .item("Merge")
            .item("Squash")
            .selected(match self.method {
                MergeMethod::Commit => 0,
                MergeMethod::Squash => 1,
            })
            .on_select(move |i, _, cx| {
                e.update(cx, |d, cx| {
                    d.method = if i == 1 {
                        MergeMethod::Squash
                    } else {
                        MergeMethod::Commit
                    };
                    cx.notify();
                })
            });
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(head(
                "merge",
                &format!("Merge into {current}"),
                "Choose a branch to bring into this one.",
                cx,
            ))
            .child(field("From", &self.filter, None, window, cx))
            .child(
                div()
                    .id("merge-list")
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(method_track)
                    .child(div().text_size(px(11.)).text_color(t.text_faint).child(
                        match self.method {
                            MergeMethod::Commit => "A merge commit joins the histories.",
                            MergeMethod::Squash => "Their changes land staged as one commit.",
                        },
                    )),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(status_color)
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(cancel)
                    .child(go),
            )
    }
}
