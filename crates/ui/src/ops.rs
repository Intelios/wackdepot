//! User actions: every click/key entry point lands here, wraps a
//! `wackdepot_core` call in `run_op` (busy + refresh + toasts), or asks via
//! `Dialog::Confirm` first when the action is destructive.

use std::path::PathBuf;

use gpui::Context;
use wackdepot_core::{branch, commit, diff, stage, stash, status, sync};

use crate::shell::Shell;
use crate::state::{Bucket, BusyOp, ConfirmAction, Dialog};

impl Shell {
    // ------------------------------------------------------------ sync ops

    /// The header button: pull when behind, push when ahead, publish when
    /// the branch has no upstream, otherwise a plain fetch.
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Some(s) = &self.repo.status else {
            return;
        };
        if s.upstream.is_none() {
            self.push(cx);
        } else if s.behind > 0 {
            self.pull(cx);
        } else if s.ahead > 0 {
            self.push(cx);
        } else {
            self.fetch(cx);
        }
    }

    pub fn fetch(&mut self, cx: &mut Context<Self>) {
        self.run_op(BusyOp::Fetch, sync::fetch, cx);
    }

    pub fn pull(&mut self, cx: &mut Context<Self>) {
        self.run_op(BusyOp::Pull, sync::pull, cx);
    }

    pub fn push(&mut self, cx: &mut Context<Self>) {
        self.run_op(BusyOp::Push, sync::push, cx);
    }

    // ----------------------------------------------------------- branches

    pub fn checkout(&mut self, name: String, cx: &mut Context<Self>) {
        self.popover = None;
        self.selected = None;
        self.selected_diff = None;
        self.run_op(
            BusyOp::Checkout,
            move |r| branch::checkout(r, &name).map(|()| format!("Switched to {name}.")),
            cx,
        );
    }

    pub fn create_branch(&mut self, cx: &mut Context<Self>) {
        let name = self.branch_name.read(cx).content().trim().to_string();
        if name.is_empty() {
            self.notify_error("Give the branch a name first.".into(), cx);
            return;
        }
        let base = self.branch_base.read(cx).content().trim().to_string();
        let base = if base.is_empty() { None } else { Some(base) };
        let label = name.clone();
        self.run_op(
            BusyOp::Other("Creating branch"),
            move |r| {
                branch::create_branch(r, &name, base.as_deref().unwrap_or("HEAD"))?;
                branch::checkout(r, &name)?;
                Ok(format!("Created and switched to {label}."))
            },
            cx,
        );
        self.branch_name.update(cx, |i, cx| i.reset_in(cx));
        self.branch_base.update(cx, |i, cx| i.reset_in(cx));
    }

    pub fn delete_branch(&mut self, name: String, force: bool, cx: &mut Context<Self>) {
        self.popover = None;
        let label = name.clone();
        self.run_op(
            BusyOp::Other("Deleting branch"),
            move |r| branch::delete_branch(r, &name, force).map(|()| format!("Deleted {label}.")),
            cx,
        );
    }

    // ----------------------------------------------------------- file ops

    /// Toggle a file's staged state (untracked stages; conflicted marks
    /// resolved).
    pub fn toggle_stage(&mut self, bucket: Bucket, index: usize, cx: &mut Context<Self>) {
        let Some(change) = self.bucket_list(bucket).get(index).cloned() else {
            return;
        };
        let path = change.path.clone();
        match bucket {
            Bucket::Unstaged | Bucket::Untracked | Bucket::Conflicted => {
                self.run_op(
                    BusyOp::Other("Staging"),
                    move |r| {
                        if bucket == Bucket::Conflicted {
                            stage::mark_resolved(r, &path)
                        } else {
                            stage::stage_file(r, &path)
                        }
                    },
                    cx,
                );
            }
            Bucket::Staged => {
                self.run_op(
                    BusyOp::Other("Unstaging"),
                    move |r| stage::unstage_file(r, &path),
                    cx,
                );
            }
        }
    }

    /// Stage (or unstage) everything in a group header.
    pub fn stage_bucket(&mut self, bucket: Bucket, cx: &mut Context<Self>) {
        match bucket {
            Bucket::Staged => self.run_op(BusyOp::Other("Unstaging"), stage::unstage_all, cx),
            _ => self.run_op(BusyOp::Other("Staging"), stage::stage_all, cx),
        }
    }

    /// Ask before discarding worktree changes (or trashing an untracked file).
    pub fn ask_discard_file(&mut self, path: String, untracked: bool, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Confirm(if untracked {
            ConfirmAction::TrashFile { path }
        } else {
            ConfirmAction::DiscardFile { path }
        }));
        cx.notify();
    }

    pub fn ask_discard_all(&mut self, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Confirm(ConfirmAction::DiscardAll));
        cx.notify();
    }

    pub fn ask_delete_branch(&mut self, name: String, force: bool, cx: &mut Context<Self>) {
        self.popover = None;
        self.dialog = Some(Dialog::Confirm(ConfirmAction::DeleteBranch { name, force }));
        cx.notify();
    }

    pub fn ask_abort_merge(&mut self, cx: &mut Context<Self>) {
        self.popover = None;
        self.dialog = Some(Dialog::Confirm(ConfirmAction::AbortMerge));
        cx.notify();
    }

    pub fn ask_drop_stash(&mut self, name: String, cx: &mut Context<Self>) {
        self.popover = None;
        self.dialog = Some(Dialog::Confirm(ConfirmAction::DropStash { name }));
        cx.notify();
    }

    /// The confirmed destructive action, fired by the dialog's danger button.
    pub fn confirm(&mut self, action: ConfirmAction, cx: &mut Context<Self>) {
        self.dialog = None;
        match action {
            ConfirmAction::DiscardFile { path } => {
                self.run_op(
                    BusyOp::Other("Discarding"),
                    move |r| stage::discard_file(r, &path),
                    cx,
                );
            }
            ConfirmAction::TrashFile { path } => {
                self.run_op(
                    BusyOp::Other("Trashing"),
                    move |r| {
                        stage::trash_file(r, &path)
                            .map(|p| format!("Moved {} to trash.", p.display()))
                    },
                    cx,
                );
            }
            ConfirmAction::DiscardAll => {
                self.run_op(
                    BusyOp::Other("Discarding"),
                    |r| {
                        let status = status::status(r)?;
                        for f in status.unstaged.iter().chain(status.untracked.iter()) {
                            stage::discard_file(r, &f.path)?;
                        }
                        Ok("All changes discarded.".to_string())
                    },
                    cx,
                );
            }
            ConfirmAction::DeleteBranch { name, force } => {
                self.delete_branch(name, force, cx);
            }
            ConfirmAction::AbortMerge => {
                self.run_op(BusyOp::Other("Aborting merge"), status::abort_merge, cx);
            }
            ConfirmAction::DropStash { name } => {
                self.run_op(
                    BusyOp::Other("Dropping stash"),
                    move |r| stash::stash_drop(r, Some(&name)),
                    cx,
                );
            }
        }
    }

    // ----------------------------------------------------------- hunk ops

    /// `file`/`hunk` are cloned into the op — diffs can be large, but hunks
    /// only carry their own lines.
    pub fn stage_hunk(&mut self, file: diff::FileDiff, hunk: diff::Hunk, cx: &mut Context<Self>) {
        self.run_op(
            BusyOp::Other("Staging hunk"),
            move |r| stage::stage_hunk(r, &file, &hunk),
            cx,
        );
    }

    pub fn unstage_hunk(&mut self, file: diff::FileDiff, hunk: diff::Hunk, cx: &mut Context<Self>) {
        self.run_op(
            BusyOp::Other("Unstaging hunk"),
            move |r| stage::unstage_hunk(r, &file, &hunk),
            cx,
        );
    }

    pub fn discard_hunk(&mut self, file: diff::FileDiff, hunk: diff::Hunk, cx: &mut Context<Self>) {
        self.run_op(
            BusyOp::Other("Discarding hunk"),
            move |r| stage::discard_hunk(r, &file, &hunk),
            cx,
        );
    }

    // ------------------------------------------------------------- commit

    pub fn can_commit(&self, cx: &Context<Self>) -> bool {
        let has_staged = self
            .repo
            .status
            .as_ref()
            .is_some_and(|s| !s.staged.is_empty());
        let has_summary = !self.summary.read(cx).content().trim().is_empty();
        has_staged && has_summary && self.busy.is_none()
    }

    pub fn do_commit(&mut self, cx: &mut Context<Self>) {
        if !self.can_commit(cx) {
            return;
        }
        let summary = self.summary.read(cx).content().trim().to_string();
        let description = self.description.read(cx).content().trim().to_string();
        self.summary.update(cx, |i, cx| i.reset_in(cx));
        self.description.update(cx, |i, cx| i.reset_in(cx));
        self.run_op(
            BusyOp::Commit,
            move |r| commit::commit(r, &summary, &description).map(|()| "Committed.".to_string()),
            cx,
        );
    }

    /// Soft-undo of the tip commit — only offered when unpushed.
    pub fn undo_commit(&mut self, cx: &mut Context<Self>) {
        self.run_op(
            BusyOp::Other("Undoing commit"),
            |r| {
                commit::undo_last_commit(r)?;
                Ok("Last commit undone — changes are staged again.".to_string())
            },
            cx,
        );
    }

    /// Whether the tip commit is unpushed (undo is safe to offer).
    pub fn tip_unpushed(&self, cx: &mut Context<Self>) {
        let Some(repo) = self.active.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { commit::tip_is_unpushed(&repo).unwrap_or(false) })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.tip_is_unpushed = out;
                cx.notify();
            });
        })
        .detach();
    }

    // -------------------------------------------------------------- stash

    pub fn stash_push(&mut self, cx: &mut Context<Self>) {
        self.popover = None;
        self.run_op(
            BusyOp::Other("Stashing"),
            |r| stash::stash_push(r, None, false).map(|()| "Stashed.".to_string()),
            cx,
        );
    }

    pub fn stash_apply(&mut self, cx: &mut Context<Self>) {
        self.popover = None;
        self.run_op(
            BusyOp::Other("Applying stash"),
            |r| stash::stash_apply(r, None).map(|()| "Stash applied.".to_string()),
            cx,
        );
    }

    pub fn stash_pop(&mut self, cx: &mut Context<Self>) {
        self.popover = None;
        self.run_op(
            BusyOp::Other("Popping stash"),
            |r| stash::stash_pop(r, None).map(|()| "Stash popped.".to_string()),
            cx,
        );
    }

    // ------------------------------------------------------------- merge

    pub fn mark_resolved(&mut self, path: String, cx: &mut Context<Self>) {
        self.run_op(
            BusyOp::Other("Resolving"),
            move |r| stage::mark_resolved(r, &path),
            cx,
        );
    }

    // ------------------------------------------------------- repo picking

    /// Native folder picker → open_repo.
    pub fn add_existing(&mut self, cx: &mut Context<Self>) {
        self.popover = None;
        self.dialog = None;
        let task = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a repository folder".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = task.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, move |this, cx| this.open_repo(path, cx));
            }
        })
        .detach();
    }

    /// Native folder picker for "clone into" / "create in" parents.
    pub fn pick_dir(&mut self, into: PickTarget, cx: &mut Context<Self>) {
        let task = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a folder".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = task.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, move |this, cx| {
                    let text = path.to_string_lossy().to_string();
                    match into {
                        PickTarget::CloneDir => {
                            this.clone_dir.update(cx, |i, cx| i.set_content(&text, cx))
                        }
                        PickTarget::NewRepoDir => this
                            .new_repo_dir
                            .update(cx, |i, cx| i.set_content(&text, cx)),
                    }
                });
            }
        })
        .detach();
    }

    pub fn do_clone(&mut self, cx: &mut Context<Self>) {
        let url = self.clone_url.read(cx).content().trim().to_string();
        let dir = self.clone_dir.read(cx).content().trim().to_string();
        if url.is_empty() {
            self.notify_error("Paste the repository URL first.".into(), cx);
            return;
        }
        if dir.is_empty() {
            self.notify_error("Pick the folder to clone into.".into(), cx);
            return;
        }
        let parent = PathBuf::from(&dir);
        self.dialog = None;
        self.clone_url.update(cx, |i, cx| i.reset_in(cx));
        self.clone_dir.update(cx, |i, cx| i.reset_in(cx));
        self.busy = Some(BusyOp::Clone);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { sync::clone(&url, &parent, None) })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.busy = None;
                match out {
                    Ok(path) => {
                        this.open_repo(path, cx);
                        this.notify("Cloned — you're looking at it.", false, cx);
                    }
                    Err(e) => this.notify_error(e, cx),
                }
            });
        })
        .detach();
    }

    pub fn do_new_repo(&mut self, cx: &mut Context<Self>) {
        let name = self.new_repo_name.read(cx).content().trim().to_string();
        let dir = self.new_repo_dir.read(cx).content().trim().to_string();
        if name.is_empty() || dir.is_empty() {
            self.notify_error("Name the repo and pick where it lives.".into(), cx);
            return;
        }
        let path = PathBuf::from(&dir).join(&name);
        self.dialog = None;
        self.new_repo_name.update(cx, |i, cx| i.reset_in(cx));
        self.new_repo_dir.update(cx, |i, cx| i.reset_in(cx));
        match sync::create_repo(&path) {
            Ok(()) => {
                self.open_repo(path, cx);
                self.notify("Repository created.", false, cx);
            }
            Err(e) => self.notify_error(e, cx),
        }
    }

    // ------------------------------------------------------------ open-in

    pub fn open_in_finder(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.active.clone() {
            let _ = std::process::Command::new("open").arg(&p).spawn();
            self.popover = None;
            cx.notify();
        }
    }

    pub fn open_in_terminal(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.active.clone() {
            let _ = std::process::Command::new("open")
                .arg("-a")
                .arg("Terminal")
                .arg(&p)
                .spawn();
            self.popover = None;
            cx.notify();
        }
    }

    pub fn open_in_editor(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.active.clone() else {
            return;
        };
        let app = self
            .settings
            .editor_app
            .clone()
            .unwrap_or_else(|| "WackCode".into());
        let ok = std::process::Command::new("open")
            .arg("-a")
            .arg(&app)
            .arg(&p)
            .spawn()
            .is_ok();
        self.popover = None;
        if !ok {
            self.notify_error(format!("Couldn't open {app} — is it installed?"), cx);
        } else {
            cx.notify();
        }
    }

    /// Refresh the stash list before the sync menu renders.
    pub fn refresh_stashes(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.active.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { stash::stash_list(&repo).unwrap_or_default() })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.stashes = out;
                cx.notify();
            });
        })
        .detach();
    }
}

/// Which dialog field a folder pick feeds.
#[derive(Clone, Copy)]
pub enum PickTarget {
    CloneDir,
    NewRepoDir,
}
