//! Dialog bodies: clone, create, and the small name-it dialogs (rename a
//! branch, tag a commit, branch from a commit). Each owns its fields;
//! Enter submits, Esc (the app's Dismiss) cancels.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, WeakEntity, Window, div, prelude::*, px,
};
use wackdepot_core::gitignore;
use wackdepot_core::license;
use wackdepot_core::merge::{MergeMethod, MergePreview};
use wackdepot_core::stage::ConflictSide;
use wackdepot_core::status::FileChange;
use wackdepot_core::sync::CreateOptions;

use crate::app::overlays::Overlays;
use crate::components::button::{button, icon_button};
use crate::components::mark::{Check, accent_mark, checkbox, monogram, status_mark};
use crate::components::menu::{menu_heading, menu_row, menu_separator, popover};
use crate::components::pill_track::pill_track;
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::model::{Bucket, RepoEvent, RepoModel};
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
    let s = s.trim();
    match s.strip_prefix('~') {
        Some("") => home(),
        // `~/…` expands against home; the leading slash can't be joined
        // directly (an absolute component replaces the base).
        Some(rest) if rest.starts_with('/') => home().join(&rest[1..]),
        // `~other` and plain words stay literal — the caller rejects
        // non-absolute results.
        _ => PathBuf::from(s),
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

/// Which option list the dialog's picker popover is serving.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Gitignore,
    License,
}

pub struct NewRepoDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    name: Entity<TextInput>,
    description: Entity<TextInput>,
    parent: Entity<TextInput>,
    readme: bool,
    /// The picked .gitignore template's name, `None` for a bare repo.
    gitignore: Option<String>,
    /// The picked license's display name.
    license: Option<String>,
    /// Which picker's popover is open (both share one filter input).
    pick_open: Option<Pick>,
    pick_filter: Entity<TextInput>,
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
        let description = cx.new(|cx| TextInput::new("What this repository does", cx));
        let parent = cx.new(|cx| {
            let mut i = TextInput::new("~/Documents/GitHub", cx);
            i.set_content(tilde(&default_parent()), cx);
            i
        });
        for input in [&name, &description, &parent] {
            cx.subscribe(
                input,
                |this: &mut Self, _, e: &TextInputEvent, cx| match e {
                    TextInputEvent::Changed => this.error = None,
                    TextInputEvent::Submit => this.submit(cx),
                },
            )
            .detach();
        }
        let pick_filter = cx.new(|cx| TextInput::new("Filter", cx));
        cx.subscribe(
            &pick_filter,
            |this: &mut Self, _, e: &TextInputEvent, cx| match e {
                TextInputEvent::Changed => cx.notify(),
                TextInputEvent::Submit => this.pick_first(cx),
            },
        )
        .detach();
        name.update(cx, |i, cx| i.focus(window, cx));
        Self {
            model,
            overlays,
            name,
            description,
            parent,
            readme: false,
            gitignore: None,
            license: None,
            pick_open: None,
            pick_filter,
            error: None,
        }
    }

    /// The options a picker lists — licenses put their featured three first
    /// like GitHub Desktop's dropdown.
    fn pick_items(pick: Pick) -> Vec<String> {
        match pick {
            Pick::Gitignore => gitignore::templates().map(str::to_string).collect(),
            Pick::License => {
                let (featured, rest): (Vec<_>, Vec<_>) =
                    license::licenses().partition(|l| l.featured);
                featured
                    .into_iter()
                    .chain(rest)
                    .map(|l| l.name.to_string())
                    .collect()
            }
        }
    }

    fn picked(&self, pick: Pick) -> &Option<String> {
        match pick {
            Pick::Gitignore => &self.gitignore,
            Pick::License => &self.license,
        }
    }

    fn set_pick(&mut self, pick: Pick, name: Option<String>) {
        match pick {
            Pick::Gitignore => self.gitignore = name,
            Pick::License => self.license = name,
        }
        self.pick_open = None;
    }

    /// Enter in a picker filter picks its top match.
    fn pick_first(&mut self, cx: &mut Context<Self>) {
        let Some(pick) = self.pick_open else { return };
        let filter = self.pick_filter.read(cx).content().to_lowercase();
        if filter.is_empty() {
            return;
        }
        if let Some(name) = Self::pick_items(pick)
            .into_iter()
            .find(|n| n.to_lowercase().contains(&filter))
        {
            self.set_pick(pick, Some(name));
            cx.notify();
        }
    }

    fn toggle_pick(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_open = if self.pick_open == Some(pick) {
            None
        } else {
            Some(pick)
        };
        if self.pick_open.is_some() {
            self.pick_filter.update(cx, |i, cx| {
                i.clear(cx);
                i.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).content().trim().to_string();
        let parent = self.parent.read(cx).content().trim().to_string();
        if name.is_empty() || name.contains('/') {
            self.error = Some("Give the repository a folder name (no slashes).".into());
            cx.notify();
            return;
        }
        let parent = untilde(&parent);
        if !parent.is_absolute() {
            // A relative base silently lands in the app's working directory
            // — nowhere the user can find. Ask for an absolute path instead.
            self.error =
                Some("The folder to create in must be an absolute path (or start with ~/).".into());
            cx.notify();
            return;
        }
        let path = parent.join(&name);
        if path.exists() {
            match std::fs::read_dir(&path).map(|mut d| d.next().is_some()) {
                Ok(true) => {
                    self.error =
                        Some(format!("{} already exists and isn't empty.", tilde(&path)).into());
                    cx.notify();
                    return;
                }
                Err(e) => {
                    self.error =
                        Some(format!("Couldn't look inside {} — {e}.", tilde(&path)).into());
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        let opts = CreateOptions {
            description: self.description.read(cx).content().trim().to_string(),
            readme: self.readme,
            gitignore: self.gitignore.clone(),
            license: self.license.clone(),
        };
        self.model.update(cx, |m, cx| m.create_repo(path, opts, cx));
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
        let readme = {
            let e = entity.clone();
            let checked = self.readme;
            check_row(
                "new-readme",
                "Initialize this repository with a README",
                checked,
                cx,
            )
            .on_click(move |_, _, cx| {
                e.update(cx, |d, cx| {
                    d.readme = !checked;
                    cx.notify();
                })
            })
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(head(
                "folder-plus",
                "New repository",
                "A fresh repository on main, with its first commit made.",
                cx,
            ))
            .child(field("Name", &self.name, None, window, cx))
            .child(field("Description", &self.description, None, window, cx))
            .child(field(
                "Inside",
                &self.parent,
                Some(choose.into_any_element()),
                window,
                cx,
            ))
            .child(readme)
            .child(self.option_row(Pick::Gitignore, "Git ignore", cx))
            .child(self.option_row(Pick::License, "License", cx))
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

impl NewRepoDialog {
    /// A labelled field-lookalike row that opens `pick`'s filterable
    /// popover — GitHub Desktop's dropdowns, as frosted menus.
    fn option_row(
        &self,
        pick: Pick,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        let (value, color) = match self.picked(pick) {
            Some(name) => (name.clone(), t.text),
            None => ("None".to_string(), t.text_faint),
        };
        let open = self.pick_open == Some(pick);
        let mut row = div()
            .id(label)
            .relative()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .px(px(12.))
            .rounded(px(metrics::RADIUS_INPUT))
            .bg(t.input)
            .border_1()
            .border_color(t.border)
            .text_size(px(metrics::TEXT_ROW))
            .cursor_pointer()
            .occlude()
            .hover(|s| s.border_color(t.border_bright))
            .on_click(cx.listener(move |this, _, window, cx| this.toggle_pick(pick, window, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(color)
                    .child(value),
            )
            .child(
                crate::icons::icon("chevron-down")
                    .size(px(12.))
                    .text_color(t.text_dim),
            );
        if open {
            row = row
                .border_color(t.accent_line_mid)
                .child(self.option_menu(pick, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(metrics::TEXT_SMALL))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.text_soft)
                    .child(label),
            )
            .child(row)
    }

    /// The frosted option menu: a filter field, "None", then every option
    /// matching the filter — licenses keep Desktop's featured divider.
    fn option_menu(&self, pick: Pick, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let filter = self.pick_filter.read(cx).content().to_lowercase();
        let picked = self.picked(pick).clone();
        let this = cx.entity().downgrade();
        let pick_name = move |d: &mut Self, name: Option<String>, cx: &mut Context<Self>| {
            d.set_pick(pick, name);
            cx.notify();
        };
        let dismiss = {
            let this = cx.entity().downgrade();
            move |_: &mut Window, cx: &mut gpui::App| {
                if let Some(this) = this.upgrade() {
                    this.update(cx, |d, cx| {
                        d.pick_open = None;
                        cx.notify();
                    });
                }
            }
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        {
            let this = this.clone();
            rows.push(
                menu_row("pick-none", "None")
                    .icon(if picked.is_none() { "check" } else { "file" })
                    .selected(picked.is_none())
                    .on_click(move |_, _, cx| {
                        if let Some(this) = this.upgrade() {
                            this.update(cx, |d, cx| pick_name(d, None, cx));
                        }
                    })
                    .into_any_element(),
            );
        }
        let matches = |name: &str| filter.is_empty() || name.to_lowercase().contains(&filter);
        let add_rows = |names: &[String], rows: &mut Vec<gpui::AnyElement>| {
            for name in names {
                let selected = picked.as_deref() == Some(name.as_str());
                let this = this.clone();
                let name = name.clone();
                rows.push(
                    menu_row(("pick", rows.len()), name.clone())
                        .icon(if selected { "check" } else { "file" })
                        .selected(selected)
                        .on_click(move |_, _, cx| {
                            if let Some(this) = this.upgrade() {
                                this.update(cx, |d, cx| pick_name(d, Some(name.clone()), cx));
                            }
                        })
                        .into_any_element(),
                );
            }
        };
        if pick == Pick::License {
            // Desktop shows the featured licenses, a divider, then the rest.
            let items = Self::pick_items(pick);
            let n_featured = license::licenses().filter(|l| l.featured).count();
            let (featured, rest) = items.split_at(n_featured.min(items.len()));
            let featured: Vec<String> = featured.iter().filter(|n| matches(n)).cloned().collect();
            let rest: Vec<String> = rest.iter().filter(|n| matches(n)).cloned().collect();
            add_rows(&featured, &mut rows);
            if !featured.is_empty() && !rest.is_empty() {
                rows.push(menu_separator(cx).into_any_element());
            }
            add_rows(&rest, &mut rows);
        } else {
            let items: Vec<String> = Self::pick_items(pick)
                .into_iter()
                .filter(|n| matches(n))
                .collect();
            add_rows(&items, &mut rows);
        }
        let empty = rows.len() == 1 && !filter.is_empty();
        let (id, note) = match pick {
            Pick::Gitignore => (
                "gitignore-popover",
                "From github/gitignore — lands as .gitignore.",
            ),
            Pick::License => (
                "license-popover",
                "From choosealicense.com — lands as LICENSE.",
            ),
        };
        popover(id)
            .width(300.)
            .on_dismiss(dismiss)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(34.))
                    .px(px(10.))
                    .mb(px(4.))
                    .rounded(px(10.))
                    .bg(t.recessed())
                    .text_size(px(13.))
                    .child(
                        crate::icons::icon("search")
                            .size(px(14.))
                            .text_color(t.text_dim),
                    )
                    .child(div().flex_1().child(self.pick_filter.clone())),
            )
            .child(
                div()
                    .id("pick-list")
                    .max_h(px(280.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows)
                    .when(empty, |el| {
                        el.child(
                            div()
                                .px(px(9.))
                                .py(px(8.))
                                .text_size(px(12.5))
                                .text_color(t.text_dim)
                                .child("Nothing matches."),
                        )
                    }),
            )
            .child(
                div()
                    .px(px(10.))
                    .pt(px(5.))
                    .pb(px(2.))
                    .text_size(px(11.))
                    .text_color(t.text_faint)
                    .child(note),
            )
    }
}

/// A small checkbox row — an accent check in a rounded square, its label
/// beside. Used by New repository (README) and Publish (private).
fn check_row(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    checked: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let t = cx.theme().clone();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(9.))
        .h(px(22.))
        .cursor_pointer()
        .child(
            div()
                .size(px(16.))
                .flex_none()
                .rounded(px(5.))
                .border_1()
                .border_color(if checked {
                    t.accent_line
                } else {
                    t.border_bright
                })
                .when(checked, |el| el.bg(t.accent))
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |el| {
                    el.child(
                        crate::icons::icon("check")
                            .size(px(11.))
                            .text_color(t.on_accent),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(metrics::TEXT_ROW))
                .text_color(t.text_soft)
                .child(label),
        )
}

// ── Publish ───────────────────────────────────────────────────────────

/// Put a remote-less repo on GitHub: create it under the connected
/// account, wire up `origin`, and push the current branch — GitHub
/// Desktop's "Publish repository". Without a connected account it offers
/// the sign-in link instead of the form.
pub struct PublishDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    session: Entity<crate::github::GitHubSession>,
    name: Entity<TextInput>,
    description: Entity<TextInput>,
    private: bool,
    /// Focus the name field on first render — construction can arrive via
    /// the model's publish event, without a window to focus through.
    needs_focus: bool,
    error: Option<SharedString>,
}

impl PublishDialog {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        cx: &mut Context<Self>,
    ) -> Self {
        let session = crate::github::session(cx);
        cx.observe(&session, |_, _, cx| cx.notify()).detach();
        let name = cx.new(|cx| TextInput::new("repository-name", cx));
        if let Some(repo) = model.read(cx).name() {
            name.update(cx, |i, cx| {
                i.set_content(license::sanitized_repository_name(&repo), cx)
            });
        }
        let description = cx.new(|cx| TextInput::new("What this repository does", cx));
        for input in [&name, &description] {
            cx.subscribe(
                input,
                |this: &mut Self, _, e: &TextInputEvent, cx| match e {
                    TextInputEvent::Changed => this.error = None,
                    TextInputEvent::Submit => this.submit(cx),
                },
            )
            .detach();
        }
        let needs_focus = session.read(cx).user.is_some();
        Self {
            model,
            overlays,
            session,
            name,
            description,
            private: true,
            needs_focus,
            error: None,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let name = license::sanitized_repository_name(self.name.read(cx).content().trim());
        if name.is_empty() {
            self.error = Some("Give the repository a name first.".into());
            cx.notify();
            return;
        }
        let description = self.description.read(cx).content().trim().to_string();
        let client = self.session.read(cx).client.clone();
        let private = self.private;
        self.model.update(cx, |m, cx| {
            m.publish_repo(client, name, description, private, cx)
        });
        if let Some(o) = self.overlays.upgrade() {
            o.update(cx, |o, cx| o.close(cx));
        }
    }
}

impl Render for PublishDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_focus && self.session.read(cx).user.is_some() {
            self.needs_focus = false;
            self.name.update(cx, |i, cx| i.focus(window, cx));
        }
        let t = cx.theme().clone();
        let entity = cx.entity();
        let session = self.session.read(cx);
        let user = session.user.clone();
        let connecting = session.connecting;
        let session_error = session.error.clone();
        let cancel = {
            let o = self.overlays.clone();
            button("publish-cancel", "Cancel").on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            })
        };
        let mut card = div().flex().flex_col().gap(px(16.)).child(head(
            "cloud",
            "Publish repository",
            "Share this repository on github.com.",
            cx,
        ));
        match user {
            None => {
                let session = self.session.clone();
                let connect = button(
                    "publish-connect",
                    if connecting {
                        "Connecting…"
                    } else {
                        "Connect GitHub…"
                    },
                )
                .primary()
                .icon("external")
                .disabled(connecting)
                .on_click(move |_, _, cx| session.update(cx, |s, cx| s.connect(cx)));
                card = card
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_soft)
                            .child("Publishing creates a repository under your GitHub account, adds it as origin, and pushes the current branch."),
                    )
                    .children(
                        session_error
                            .map(|e| div().text_size(px(12.)).text_color(t.sem.danger).child(e)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(cancel)
                            .child(connect),
                    );
            }
            Some(user) => {
                let go = {
                    let e = entity.clone();
                    button("publish-go", "Publish repository")
                        .primary()
                        .icon("push")
                        .on_click(move |_, _, cx| e.update(cx, |d, cx| d.submit(cx)))
                };
                let private = {
                    let e = entity.clone();
                    let checked = self.private;
                    check_row("publish-private", "Keep this code private", checked, cx).on_click(
                        move |_, _, cx| {
                            e.update(cx, |d, cx| {
                                d.private = !checked;
                                cx.notify();
                            })
                        },
                    )
                };
                card = card
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_faint)
                            .child(format!("Publishing as @{}.", user.login)),
                    )
                    .child(field("Name", &self.name, None, window, cx))
                    .child(field("Description", &self.description, None, window, cx))
                    .child(private)
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
                    );
            }
        }
        card
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

// ── Conflicts ─────────────────────────────────────────────────────────

/// The merge-conflicts overview: every unmerged path with its wholesale
/// outs — take ours, take theirs, or open the file and work the markers.
/// Resolving is immediate; rows leave the list as the snapshot catches up.
pub struct ConflictsDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
}

impl ConflictsDialog {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&model, |_: &mut Self, _, e: &RepoEvent, cx| {
            if matches!(e, RepoEvent::Snapshot) {
                cx.notify();
            }
        })
        .detach();
        Self { model, overlays }
    }

    /// One conflicted path: status mark, name, and the side/deletion
    /// resolution buttons its unmerged codes allow.
    fn conflict_row(&self, i: usize, f: &FileChange, cx: &mut Context<Self>) -> impl IntoElement {
        let (dir, name) = match f.path.rfind('/') {
            Some(ix) => (f.path[..=ix].to_string(), f.path[ix + 1..].to_string()),
            None => (String::new(), f.path.clone()),
        };
        let m = self.model.clone();
        let path = f.path.clone();
        let open = icon_button(("conflict-open", i), "external")
            .small()
            .on_click(move |_, _, cx| m.update(cx, |m, cx| m.open_file_in_editor(&path, cx)));
        let side = |id: &'static str, label: &'static str, s: ConflictSide, delete: bool| {
            let (m, path) = (self.model.clone(), f.path.clone());
            button((id, i), label).small().on_click(move |_, _, cx| {
                m.update(cx, |m, cx| m.resolve_conflict(path.clone(), s, delete, cx))
            })
        };
        // Porcelain's unmerged XY: 'D' means that side deleted the file —
        // resolving to it is `git rm`, not `checkout --theirs`.
        let (ours, theirs) = (f.x != 'D', f.y != 'D');
        let mut actions = div().flex_none().flex().items_center().gap(px(4.));
        actions = match (ours, theirs) {
            (true, true) => actions
                .child(side("row-ours", "Use ours", ConflictSide::Ours, false))
                .child(side(
                    "row-theirs",
                    "Use theirs",
                    ConflictSide::Theirs,
                    false,
                )),
            (true, false) => actions
                .child(side("row-ours", "Use ours", ConflictSide::Ours, false))
                .child(side(
                    "row-delete",
                    "Delete file",
                    ConflictSide::Theirs,
                    true,
                )),
            (false, true) => actions
                .child(side("row-delete", "Keep deleted", ConflictSide::Ours, true))
                .child(side(
                    "row-theirs",
                    "Use theirs",
                    ConflictSide::Theirs,
                    false,
                )),
            (false, false) => {
                actions.child(side("row-delete", "Keep deleted", ConflictSide::Ours, true))
            }
        };
        actions = actions.child(open);
        let t = cx.theme().clone();
        div()
            .id(("conflict-row", i))
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(34.))
            .px(px(9.))
            .rounded(px(metrics::RADIUS_MENU_ROW))
            .child(status_mark('U'))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .font_family(t.mono.clone())
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .justify_end()
                            .child(div().flex_none().text_color(t.text_faint).child(dir)),
                    )
                    .child(div().flex_none().text_color(t.text).child(name)),
            )
            .child(actions)
    }
}

impl Render for ConflictsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let files: Vec<FileChange> = self.model.read(cx).snap.bucket(Bucket::Conflicted).to_vec();
        let n = files.len();
        let rows: Vec<AnyElement> = if files.is_empty() {
            vec![
                div()
                    .px(px(9.))
                    .py(px(10.))
                    .text_size(px(12.5))
                    .text_color(t.text_dim)
                    .child("Every conflict is resolved — continue the operation to finish.")
                    .into_any_element(),
            ]
        } else {
            files
                .iter()
                .enumerate()
                .map(|(i, f)| self.conflict_row(i, f, cx).into_any_element())
                .collect()
        };
        let o = self.overlays.clone();
        let done = button("conflicts-done", "Done")
            .primary()
            .on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            });
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(head(
                "conflict",
                &format!("{n} conflicted file{}", if n == 1 { "" } else { "s" }),
                "Take a side outright, or open a file and work the markers.",
                cx,
            ))
            .child(
                div()
                    .id("conflicts-list")
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows),
            )
            .child(div().flex().justify_end().child(done))
    }
}

// ── Scan ─────────────────────────────────────────────────────────────

/// One repository the folder scan found, ready to tick for adding.
struct ScanHit {
    path: PathBuf,
    name: String,
    /// Where it lives (`~/GitHub/org`), shown dim beside the name.
    rel: String,
    already: bool,
    checked: bool,
}

enum ScanState {
    Scanning,
    Found(Vec<ScanHit>),
    Empty,
    Failed(String),
}

/// GitHub Desktop's "Add local repositories" scan: the picked folders are
/// walked in the background, then every repository found is listed for
/// choosing. Already-added ones show as `Already added` and stay ticked.
pub struct ScanDialog {
    model: Entity<RepoModel>,
    overlays: WeakEntity<Overlays>,
    roots: Vec<PathBuf>,
    state: ScanState,
}

impl ScanDialog {
    pub fn new(
        model: Entity<RepoModel>,
        overlays: WeakEntity<Overlays>,
        roots: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let scan_roots = roots.clone();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let mut out: Vec<PathBuf> = Vec::new();
                    let mut seen = std::collections::HashSet::new();
                    for root in &scan_roots {
                        for p in wackdepot_core::repo::find_repos(root)? {
                            let key = std::fs::canonicalize(&p).unwrap_or(p);
                            if seen.insert(key.clone()) {
                                out.push(key);
                            }
                        }
                    }
                    out.sort();
                    Ok(out)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state = match found {
                    Ok(paths) if paths.is_empty() => ScanState::Empty,
                    Ok(paths) => {
                        let known = crate::app::prefs::get(cx).repos.clone();
                        ScanState::Found(
                            paths
                                .into_iter()
                                .map(|p| {
                                    let already = known.contains(&p);
                                    ScanHit {
                                        rel: tilde(p.parent().unwrap_or(&p)),
                                        name: p
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default(),
                                        path: p,
                                        already,
                                        checked: !already,
                                    }
                                })
                                .collect(),
                        )
                    }
                    Err(e) => ScanState::Failed(e),
                };
                cx.notify();
            });
        })
        .detach();
        Self {
            model,
            overlays,
            roots,
            state: ScanState::Scanning,
        }
    }

    fn toggle(&mut self, i: usize, cx: &mut Context<Self>) {
        if let ScanState::Found(hits) = &mut self.state
            && let Some(h) = hits.get_mut(i)
            && !h.already
        {
            h.checked = !h.checked;
        }
        cx.notify();
    }

    /// The master checkbox: all ticked → clear the new rows, else tick them all.
    fn toggle_all(&mut self, cx: &mut Context<Self>) {
        if let ScanState::Found(hits) = &mut self.state {
            let turn_on = !hits.iter().all(|h| h.already || h.checked);
            for h in hits.iter_mut().filter(|h| !h.already) {
                h.checked = turn_on;
            }
        }
        cx.notify();
    }

    fn add_selected(&mut self, cx: &mut Context<Self>) {
        let ScanState::Found(hits) = &self.state else {
            return;
        };
        let paths: Vec<PathBuf> = hits
            .iter()
            .filter(|h| h.checked && !h.already)
            .map(|h| h.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        self.model.update(cx, |m, cx| m.add_repos(paths, cx));
        if let Some(o) = self.overlays.upgrade() {
            o.update(cx, |o, cx| o.close(cx));
        }
    }

    /// The footer's re-scan button (every state but Found).
    fn choose_another(&self, _: &App) -> impl IntoElement {
        let o = self.overlays.clone();
        button("scan-choose", "Choose Another Folder…").on_click(move |_, _, cx| {
            if let Some(o) = o.upgrade() {
                o.update(cx, |o, cx| o.pick_scan_folders(cx));
            }
        })
    }

    fn hit_row(&self, i: usize, h: &ScanHit, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let entity = cx.entity();
        let state = if h.checked { Check::On } else { Check::Off };
        let mut row = div()
            .id(("scan-hit", i))
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(34.))
            .px(px(9.))
            .rounded(px(metrics::RADIUS_MENU_ROW))
            .child(
                checkbox(("scan-check", i), state)
                    .disabled(h.already)
                    .on_click(move |_, _, cx| entity.update(cx, |d, cx| d.toggle(i, cx))),
            )
            .child(monogram(h.name.clone()).size(20.))
            .child(
                div()
                    .flex_none()
                    .text_size(px(metrics::TEXT_ROW))
                    .child(h.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.5))
                    .text_color(t.text_faint)
                    .child(h.rel.clone()),
            );
        if h.already {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(t.text_faint)
                    .child("Already added"),
            );
        } else {
            let entity = cx.entity();
            row = row
                .cursor_pointer()
                .hover(|s| s.bg(t.hover()))
                .on_click(move |_, _, cx| entity.update(cx, |d, cx| d.toggle(i, cx)));
        }
        row
    }
}

impl Render for ScanDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let desc = if self.roots.len() == 1 {
            tilde(&self.roots[0])
        } else {
            format!("{} folders", self.roots.len())
        };
        let body: AnyElement = match &self.state {
            ScanState::Scanning => div()
                .flex()
                .items_center()
                .gap(px(9.))
                .px(px(9.))
                .py(px(10.))
                .child(crate::components::spinner::orbit("scan-spin"))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(t.text_dim)
                        .child(format!("Looking through {desc}…")),
                )
                .into_any_element(),
            ScanState::Empty => div()
                .px(px(9.))
                .py(px(10.))
                .text_size(px(12.5))
                .text_color(t.text_dim)
                .child(format!("No repositories found in {desc}."))
                .into_any_element(),
            ScanState::Failed(e) => div()
                .px(px(9.))
                .py(px(10.))
                .text_size(px(12.5))
                .text_color(t.sem.danger)
                .child(e.clone())
                .into_any_element(),
            ScanState::Found(hits) => {
                let new_rows = hits.iter().filter(|h| !h.already).count();
                let ticked = hits.iter().filter(|h| !h.already && h.checked).count();
                let master = if ticked == 0 {
                    Check::Off
                } else if ticked == new_rows {
                    Check::On
                } else {
                    Check::Mixed
                };
                let entity = cx.entity();
                let rows: Vec<AnyElement> = hits
                    .iter()
                    .enumerate()
                    .map(|(i, h)| self.hit_row(i, h, cx).into_any_element())
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .h(px(24.))
                            .px(px(9.))
                            .child(checkbox(("scan-all", 0usize), master).on_click(
                                move |_, _, cx| entity.update(cx, |d, cx| d.toggle_all(cx)),
                            ))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(t.text_soft)
                                    .child("Select all"),
                            ),
                    )
                    .child(
                        div()
                            .id("scan-list")
                            .max_h(px(300.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .children(rows),
                    )
                    .into_any_element()
            }
        };
        let cancel = {
            let o = self.overlays.clone();
            button("scan-cancel", "Cancel").on_click(move |_, _, cx| {
                if let Some(o) = o.upgrade() {
                    o.update(cx, |o, cx| o.close(cx));
                }
            })
        };
        let footer = match &self.state {
            ScanState::Found(hits) => {
                let n = hits.iter().filter(|h| h.checked && !h.already).count();
                let label = format!("Add {n} repositor{}", if n == 1 { "y" } else { "ies" });
                let entity = cx.entity();
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(cancel)
                    .child(
                        button("scan-go", label)
                            .primary()
                            .icon("plus")
                            .disabled(n == 0)
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |d, cx| d.add_selected(cx))
                            }),
                    )
                    .into_any_element()
            }
            _ => div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(cancel)
                .child(self.choose_another(cx))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(head(
                "search",
                "Add repositories",
                &format!("Searched {desc}."),
                cx,
            ))
            .child(body)
            .child(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::untilde;
    use std::path::PathBuf;

    #[test]
    fn untilde_expands_home_forms() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        assert_eq!(untilde("~/Documents/GitHub"), home.join("Documents/GitHub"));
        assert_eq!(untilde("~"), home.clone());
        // Whitespace around the path doesn't defeat the expansion.
        assert_eq!(untilde("  ~/code  "), home.join("code"));
    }

    #[test]
    fn untilde_leaves_other_paths_alone() {
        // `~name` isn't this user's home — it stays literal so the caller
        // can reject it instead of creating a `~name` folder somewhere.
        assert_eq!(untilde("~root/code"), PathBuf::from("~root/code"));
        assert_eq!(untilde("/tmp/x"), PathBuf::from("/tmp/x"));
        assert_eq!(untilde("relative"), PathBuf::from("relative"));
    }
}
