//! Every git action the UI can take, as a queued op on the model's lane.
//! Destructive ones are only reached after a confirm (see
//! [`RepoModel::perform`]).

use std::path::{Path, PathBuf};

use gpui::Context;
use wackdepot_core::commit::{CommitOptions, PickOutcome};
use wackdepot_core::diff::{FileDiff, Hunk};
use wackdepot_core::merge::{MergeMethod, MergeOutcome};
use wackdepot_core::stage::{ConflictSide, LineSel};
use wackdepot_core::stash::StashAction;
use wackdepot_core::status::Operation;
use wackdepot_core::{branch, commit, gitignore, merge, stage, stash, status, sync};

use super::web::GitHubTarget;
use super::{Beat, Bucket, BusyOp, ConfirmAction, Done, NoticeKind, RepoModel, Tab};

fn sink_progress(
    sink: &super::ProgressSink,
) -> impl FnMut(wackdepot_core::progress::Progress) + '_ {
    move |p| {
        if let Ok(mut slot) = sink.lock() {
            *slot = Some(p);
        }
    }
}

impl RepoModel {
    // ── Sync ───────────────────────────────────────────────────────────

    /// The sync pill's primary action: publish without an upstream, pull
    /// when behind, push when ahead, otherwise fetch.
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Some(s) = self.snap.status.as_ref() else {
            return;
        };
        if s.upstream.is_none() && s.branch.is_some() {
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
        self.run(
            BusyOp::Fetch,
            |r, sink| {
                sync::fetch_with_progress(r, &mut sink_progress(sink))?;
                Ok(Done::quiet())
            },
            cx,
        );
    }

    pub fn pull(&mut self, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Pull,
            |r, sink| {
                sync::pull_with_progress(r, &mut sink_progress(sink))?;
                Ok(Done::quiet().beat(Beat::Pulled))
            },
            cx,
        );
    }

    pub fn push(&mut self, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Push,
            |r, sink| {
                sync::push_with_progress(r, &mut sink_progress(sink))?;
                Ok(Done::quiet().beat(Beat::Pushed))
            },
            cx,
        );
    }

    pub fn retry(&mut self, op: BusyOp, cx: &mut Context<Self>) {
        match op {
            BusyOp::Fetch => self.fetch(cx),
            BusyOp::Pull => self.pull(cx),
            BusyOp::Push => self.push(cx),
            _ => {}
        }
    }

    // ── Branches ───────────────────────────────────────────────────────

    pub fn checkout(&mut self, name: String, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Checkout,
            move |r, _| {
                branch::checkout(r, &name)?;
                Ok(Done::quiet().beat(Beat::CheckedOut { branch: name }))
            },
            cx,
        );
    }

    /// Create `name` from `base` (a branch, tag or SHA) and switch to it.
    pub fn create_branch(&mut self, name: String, base: String, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.notice("Give the branch a name first.", NoticeKind::Error, cx);
            return;
        }
        let base = if base.trim().is_empty() {
            "HEAD".to_string()
        } else {
            base.trim().to_string()
        };
        self.run(
            BusyOp::Other("Creating branch"),
            move |r, _| {
                branch::create_branch(r, &name, &base)?;
                branch::checkout(r, &name)?;
                Ok(Done::quiet().beat(Beat::CheckedOut { branch: name }))
            },
            cx,
        );
    }

    pub fn rename_branch(&mut self, old: String, new: String, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Renaming branch"),
            move |r, _| {
                branch::rename_branch(r, &old, &new)?;
                Ok(Done::toast(format!("Renamed {old} to {}.", new.trim())))
            },
            cx,
        );
    }

    /// Delete a branch; when git says it isn't merged, ask again before
    /// forcing it.
    fn delete_branch(&mut self, name: String, force: bool, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Deleting branch"),
            move |r, _| match branch::delete_branch(r, &name, force) {
                Ok(()) => Ok(Done::toast(format!("Deleted {name}."))),
                Err(e) if !force && e.contains("not fully merged") => Ok(Done {
                    confirm: Some(ConfirmAction::ForceDeleteBranch { name }),
                    ..Done::default()
                }),
                Err(e) => Err(e),
            },
            cx,
        );
    }

    pub fn create_tag(
        &mut self,
        name: String,
        target: String,
        message: String,
        cx: &mut Context<Self>,
    ) {
        self.run(
            BusyOp::Other("Tagging"),
            move |r, _| {
                let msg = (!message.trim().is_empty()).then_some(message.as_str());
                branch::create_tag(r, &name, &target, msg)?;
                Ok(Done::toast(format!("Tagged {name}.")))
            },
            cx,
        );
    }

    // ── History actions ────────────────────────────────────────────────

    pub fn revert(&mut self, sha: String, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Reverting"),
            move |r, _| match commit::revert(r, &sha)? {
                PickOutcome::Clean => Ok(Done::toast("Reverted — a new commit undoes it.")),
                PickOutcome::Conflicts => Ok(Done::toast(
                    "The revert hit conflicts. Resolve them, then continue.",
                )),
            },
            cx,
        );
    }

    pub fn cherry_pick(&mut self, sha: String, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Cherry-picking"),
            move |r, _| match commit::cherry_pick(r, &sha)? {
                PickOutcome::Clean => Ok(Done::toast("Cherry-picked onto this branch.")),
                PickOutcome::Conflicts => Ok(Done::toast(
                    "The cherry-pick hit conflicts. Resolve them, then continue.",
                )),
            },
            cx,
        );
    }

    /// `git reset --mixed <sha>` — rewind the branch to a commit, leaving
    /// everything since as plain changes. Always mixed: unpushed work is
    /// never deleted, it just moves to Changes — the same rule GitHub
    /// Desktop's "Reset to Commit…" follows. History's menu only offers
    /// this where every commit being undone is still unpushed.
    pub fn reset_to(&mut self, sha: String, short: String, cx: &mut Context<Self>) {
        // The Changes tab is where the rewound work lands — bring the user
        // there so the reset isn't invisible (Desktop does the same).
        self.set_tab(Tab::Changes, cx);
        self.run(
            BusyOp::Other("Resetting"),
            move |r, _| {
                commit::reset(r, commit::ResetMode::Mixed, &sha)?;
                Ok(Done::toast(format!(
                    "Reset to {short} — the rewound commits are unstaged in Changes."
                )))
            },
            cx,
        );
    }

    /// `git merge [--squash] <name>` into the current branch. Conflicts
    /// don't fail: they leave the repo mid-operation, which the sidebar
    /// banner picks up on the next snapshot.
    pub fn merge_branch(&mut self, name: String, method: MergeMethod, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other(match method {
                MergeMethod::Commit => "Merging",
                MergeMethod::Squash => "Squashing",
            }),
            move |r, _| {
                let current = branch::current_branch(r)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "HEAD".to_string());
                let toast = match merge::merge(r, &name, method)? {
                    MergeOutcome::FastForward => format!("Fast-forwarded {current} to {name}."),
                    MergeOutcome::Merged => format!("Merged {name} into {current}."),
                    MergeOutcome::AlreadyUpToDate => {
                        format!("{current} already has all of {name}.")
                    }
                    MergeOutcome::Conflicts => {
                        format!("Merging {name} hit conflicts — resolve them in the list.")
                    }
                    MergeOutcome::SquashedClean => {
                        format!("Squashed {name} — the changes are staged, ready to commit.")
                    }
                    MergeOutcome::SquashedConflicts => {
                        format!("Squashing {name} hit conflicts — resolve them in the list.")
                    }
                };
                Ok(Done::toast(toast))
            },
            cx,
        );
    }

    pub fn continue_operation(&mut self, op: Operation, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Continuing"),
            move |r, _| {
                status::continue_operation(r, op)?;
                Ok(Done::quiet())
            },
            cx,
        );
    }

    // ── Staging ────────────────────────────────────────────────────────

    /// Stage or unstage one file (a conflicted file is marked resolved).
    pub fn toggle_stage(&mut self, path: String, bucket: Bucket, cx: &mut Context<Self>) {
        self.optimistic_toggle(&path, bucket, cx);
        match bucket {
            Bucket::Staged => self.run(
                BusyOp::Other("Unstaging"),
                move |r, _| {
                    stage::unstage_file(r, &path)?;
                    Ok(Done::quiet().beat(Beat::Unstaged { paths: vec![path] }))
                },
                cx,
            ),
            _ => self.run(
                BusyOp::Other("Staging"),
                move |r, _| {
                    if bucket == Bucket::Conflicted {
                        stage::mark_resolved(r, &path)?;
                    } else {
                        stage::stage_file(r, &path)?;
                    }
                    Ok(Done::quiet().beat(Beat::Staged { paths: vec![path] }))
                },
                cx,
            ),
        }
    }

    /// Resolve one conflict wholesale: take a side's version
    /// (`checkout --ours/--theirs`) or stage the deletion.
    pub fn resolve_conflict(
        &mut self,
        path: String,
        side: ConflictSide,
        delete: bool,
        cx: &mut Context<Self>,
    ) {
        self.run(
            BusyOp::Other("Resolving"),
            move |r, _| {
                stage::resolve_conflict(r, &path, side, delete)?;
                Ok(Done::quiet().beat(Beat::Staged { paths: vec![path] }))
            },
            cx,
        );
    }

    /// The group heading's action: unstage all, resolve all conflicts, or
    /// stage everything in the group.
    pub fn stage_group(&mut self, bucket: Bucket, cx: &mut Context<Self>) {
        let paths: Vec<String> = self
            .snap
            .bucket(bucket)
            .iter()
            .map(|f| f.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        match bucket {
            Bucket::Staged => self.run(
                BusyOp::Other("Unstaging"),
                move |r, _| {
                    stage::unstage_all(r)?;
                    Ok(Done::quiet().beat(Beat::Unstaged { paths }))
                },
                cx,
            ),
            _ => self.run(
                BusyOp::Other("Staging"),
                move |r, _| {
                    // Marking conflicts resolved is also `git add`.
                    stage::stage_paths(r, &paths)?;
                    Ok(Done::quiet().beat(Beat::Staged { paths }))
                },
                cx,
            ),
        }
    }

    pub fn stage_hunk(&mut self, file: FileDiff, hunk: Hunk, cx: &mut Context<Self>) {
        let path = file.path().to_string();
        self.run(
            BusyOp::Other("Staging"),
            move |r, _| {
                stage::stage_hunk(r, &file, &hunk)?;
                Ok(Done::quiet().beat(Beat::Staged { paths: vec![path] }))
            },
            cx,
        );
    }

    pub fn unstage_hunk(&mut self, file: FileDiff, hunk: Hunk, cx: &mut Context<Self>) {
        let path = file.path().to_string();
        self.run(
            BusyOp::Other("Unstaging"),
            move |r, _| {
                stage::unstage_hunk(r, &file, &hunk)?;
                Ok(Done::quiet().beat(Beat::Unstaged { paths: vec![path] }))
            },
            cx,
        );
    }

    pub fn stage_lines(
        &mut self,
        file: FileDiff,
        sel: LineSel,
        unstage: bool,
        cx: &mut Context<Self>,
    ) {
        let path = file.path().to_string();
        self.run(
            BusyOp::Other(if unstage { "Unstaging" } else { "Staging" }),
            move |r, _| {
                if unstage {
                    stage::unstage_lines(r, &file, &sel)?;
                    Ok(Done::quiet().beat(Beat::Unstaged { paths: vec![path] }))
                } else {
                    stage::stage_lines(r, &file, &sel)?;
                    Ok(Done::quiet().beat(Beat::Staged { paths: vec![path] }))
                }
            },
            cx,
        );
    }

    /// Hunk and line discards wait for a confirm; the pending diff is held
    /// here until the user answers.
    pub fn discard_hunk_now(&mut self, file: FileDiff, hunk: Hunk, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Discarding"),
            move |r, _| {
                stage::discard_hunk(r, &file, &hunk)?;
                Ok(Done::quiet())
            },
            cx,
        );
    }

    pub fn discard_lines_now(&mut self, file: FileDiff, sel: LineSel, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Discarding"),
            move |r, _| {
                stage::discard_lines(r, &file, &sel)?;
                Ok(Done::quiet())
            },
            cx,
        );
    }

    // ── Commit ─────────────────────────────────────────────────────────

    pub fn commit(
        &mut self,
        summary: String,
        description: String,
        opts: CommitOptions,
        cx: &mut Context<Self>,
    ) {
        self.run(
            BusyOp::Commit,
            move |r, _| {
                let sha = commit::commit_with(r, &summary, &description, &opts)?;
                Ok(Done::quiet().beat(Beat::Committed { sha }))
            },
            cx,
        );
    }

    // ── Stash ──────────────────────────────────────────────────────────

    pub fn stash_all(&mut self, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Stashing"),
            |r, _| {
                stash::stash_push(r, None, true)?;
                Ok(Done::toast("Stashed your changes."))
            },
            cx,
        );
    }

    pub fn stash_action(&mut self, sha: String, action: StashAction, cx: &mut Context<Self>) {
        self.run(
            BusyOp::Other("Stash"),
            move |r, _| {
                stash::stash_entry_action(r, &sha, action)?;
                Ok(match action {
                    StashAction::Apply => Done::toast("Stash applied."),
                    StashAction::Pop => Done::toast("Stash applied and dropped."),
                    StashAction::Drop => Done::toast("Stash dropped."),
                })
            },
            cx,
        );
    }

    // ── Confirmed actions ──────────────────────────────────────────────

    /// Carry out an action the user confirmed.
    pub fn perform(&mut self, action: ConfirmAction, cx: &mut Context<Self>) {
        match action {
            ConfirmAction::DiscardFile { path } => self.run(
                BusyOp::Other("Discarding"),
                move |r, _| {
                    stage::discard_file(r, &path)?;
                    Ok(Done::quiet())
                },
                cx,
            ),
            ConfirmAction::TrashFile { path } => self.run(
                BusyOp::Other("Discarding"),
                move |r, _| {
                    stage::trash_file(r, &path)?;
                    Ok(Done::toast(format!(
                        "Moved {path} to WackDepot's trash folder."
                    )))
                },
                cx,
            ),
            ConfirmAction::DiscardAll => self.run(
                BusyOp::Other("Discarding"),
                |r, _| {
                    let s = status::status(r)?;
                    for f in s.unstaged.iter().chain(s.untracked.iter()) {
                        stage::discard_file(r, &f.path)?;
                    }
                    Ok(Done::toast("All changes discarded."))
                },
                cx,
            ),
            ConfirmAction::DeleteBranch { name } => self.delete_branch(name, false, cx),
            ConfirmAction::ForceDeleteBranch { name } => self.delete_branch(name, true, cx),
            ConfirmAction::AbortOperation { .. } => {
                let Some(op) = self.snap.status.as_ref().and_then(|s| s.operation) else {
                    return;
                };
                self.run(
                    BusyOp::Other("Aborting"),
                    move |r, _| {
                        status::abort_operation(r, op)?;
                        Ok(Done::quiet())
                    },
                    cx,
                );
            }
            ConfirmAction::DropStash { sha, .. } => self.stash_action(sha, StashAction::Drop, cx),
            ConfirmAction::MarkResolved { path } => {
                self.toggle_stage(path, Bucket::Conflicted, cx);
            }
            ConfirmAction::ResolveConflict { path, side, delete } => {
                self.resolve_conflict(path, side, delete, cx);
            }
            ConfirmAction::ResetToCommit { sha, short } => self.reset_to(sha, short, cx),
            ConfirmAction::UndoCommit => self.run(
                BusyOp::Other("Undoing commit"),
                |r, _| {
                    commit::undo_last_commit(r)?;
                    Ok(Done::toast(
                        "Last commit undone — its changes are staged again.",
                    ))
                },
                cx,
            ),
            // Hunk/line discards carry their diff through the view that
            // asked; see `discard_hunk_now` / `discard_lines_now`.
            ConfirmAction::DiscardHunk { .. } | ConfirmAction::DiscardLines { .. } => {}
        }
    }

    // ── Repositories ───────────────────────────────────────────────────

    /// Native folder picker, then open what was picked.
    pub fn add_existing(&mut self, cx: &mut Context<Self>) {
        let task = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = task.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, move |this, cx| this.open(path, cx));
            }
        })
        .detach();
    }

    /// Clone `url` to `dest` (the folder to create), then open it.
    pub fn clone_repo(&mut self, url: String, dest: PathBuf, cx: &mut Context<Self>) {
        self.run_detached(
            BusyOp::Clone,
            move |_, sink| {
                let parent = dest
                    .parent()
                    .ok_or("That folder path doesn't make sense.")?;
                let name = dest
                    .file_name()
                    .ok_or("That folder path doesn't make sense.")?;
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Couldn't create {} — {e}.", parent.display()))?;
                let path = sync::clone_with_progress(
                    &url,
                    parent,
                    Some(&name.to_string_lossy()),
                    &mut sink_progress(sink),
                )?;
                Ok(Done {
                    toast: Some("Cloned.".into()),
                    open: Some(path),
                    ..Done::default()
                })
            },
            cx,
        );
    }

    /// `git init` a new repo at `path`, then open it. A picked gitignore
    /// template lands as `.gitignore`, ready for the first commit — a
    /// write failure doesn't cancel the repo, it just downgrades the toast.
    pub fn create_repo(&mut self, path: PathBuf, template: Option<String>, cx: &mut Context<Self>) {
        self.run_detached(
            BusyOp::Other("Creating repository"),
            move |_, _| {
                sync::create_repo(&path)?;
                let toast = match template.as_deref().and_then(gitignore::template) {
                    Some(body) => match gitignore::write_gitignore(&path, body) {
                        Ok(()) => "Repository created.".to_string(),
                        Err(e) => e,
                    },
                    None => "Repository created.".to_string(),
                };
                Ok(Done {
                    toast: Some(toast),
                    open: Some(path),
                    ..Done::default()
                })
            },
            cx,
        );
    }

    /// Add `path` to the repo's `.gitignore` — itself (`Ignore file`) or
    /// its extension (`Ignore all .ext files`). Only offered on untracked
    /// files, where the effect is visible; on a tracked file a gitignore
    /// line silently does nothing.
    pub fn ignore_path(&mut self, path: String, by_ext: bool, cx: &mut Context<Self>) {
        let pattern = if by_ext {
            gitignore::extension_pattern(&path)
        } else {
            Some(gitignore::escape(&path))
        };
        let Some(pattern) = pattern else {
            return;
        };
        self.run(
            BusyOp::Other("Updating .gitignore"),
            move |r, _| {
                gitignore::update_gitignore(r, std::slice::from_ref(&pattern))?;
                Ok(Done::toast(format!("Added {pattern} to .gitignore.")))
            },
            cx,
        );
    }

    // ── Open in… ───────────────────────────────────────────────────────

    /// Show one file of the repo in Finder.
    pub fn reveal(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(repo) = &self.repo
            && std::process::Command::new("open")
                .arg("-R")
                .arg(repo.join(path))
                .spawn()
                .is_err()
        {
            self.notice("Couldn't open Finder.", NoticeKind::Error, cx);
        }
    }

    pub fn open_in_finder(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &self.repo
            && std::process::Command::new("open").arg(p).spawn().is_err()
        {
            self.notice("Couldn't open Finder.", NoticeKind::Error, cx);
        }
    }

    /// Open the repository, a branch or a commit on its GitHub page — the
    /// `github.com` URL the snapshot resolved from the remotes.
    pub fn open_on_github(&mut self, target: GitHubTarget, cx: &mut Context<Self>) {
        match &self.snap.github {
            Some(base) => cx.open_url(&target.url(base)),
            None => self.notice(
                "This repository isn't hosted on GitHub.",
                NoticeKind::Info,
                cx,
            ),
        }
    }

    pub fn open_in_terminal(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &self.repo
            && std::process::Command::new("open")
                .args(["-a", "Terminal"])
                .arg(p)
                .spawn()
                .is_err()
        {
            self.notice("Couldn't open Terminal.", NoticeKind::Error, cx);
        }
    }

    /// The configured editor, else the first of WackCode, Zed, VS Code found.
    pub fn open_in_editor(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.repo.clone() else { return };
        self.open_in_editor_app(&p, cx);
    }

    /// Open one file — where conflicts go to be resolved — instead of the
    /// repo root.
    pub fn open_file_in_editor(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        self.open_in_editor_app(&repo.join(path), cx);
    }

    fn open_in_editor_app(&mut self, path: &Path, cx: &mut Context<Self>) {
        let wanted = crate::app::prefs::get(cx).editor_app.clone();
        let app = [
            wanted,
            Some("WackCode".into()),
            Some("Zed".into()),
            Some("Visual Studio Code".into()),
        ]
        .into_iter()
        .flatten()
        .find(|a| {
            std::path::Path::new(&format!("/Applications/{a}.app")).exists()
                || std::path::Path::new(&format!(
                    "{}/Applications/{a}.app",
                    std::env::var("HOME").unwrap_or_default()
                ))
                .exists()
        });
        let Some(app) = app else {
            self.notice(
                "No editor found — pick one in Settings › Git.",
                NoticeKind::Error,
                cx,
            );
            return;
        };
        if std::process::Command::new("open")
            .arg("-a")
            .arg(&app)
            .arg(path)
            .spawn()
            .is_err()
        {
            self.notice(format!("Couldn't open {app}."), NoticeKind::Error, cx);
        }
    }
}
