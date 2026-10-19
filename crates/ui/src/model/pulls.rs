//! Hosted state has its own lifetimes and generations, independent of local
//! status snapshots. Browsing a PR never queues a Git operation.

use super::{Beat, BusyOp, Done, RepoEvent, RepoModel, Tab};
use crate::app::prefs;
use crate::github::GitHubSession;
use gh::drafts::{Draft, DraftStore, PendingWrite, WriteKind};
use gpui::{Context, Entity, EventEmitter};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wackdepot_github::{
    self as gh, CheckState, Client, Comparison, CreateOptions, Filter, PullRequest,
    PullRequestDetail, Repository, RepositoryId,
};

#[derive(Clone)]
pub struct SourceRepository {
    pub remote: wackdepot_core::remote::Remote,
    pub repository: Repository,
}

pub enum PullEvent {
    Draft,
    Changed,
}

pub struct PullRequestModel {
    pub git: Entity<RepoModel>,
    pub session: Entity<GitHubSession>,
    pub target: Option<Repository>,
    pub sources: Vec<SourceRepository>,
    pub list: Vec<PullRequest>,
    pub filter: Filter,
    pub query: String,
    pub next_cursor: Option<String>,
    pub incomplete: bool,
    pub selected: Option<u64>,
    pub detail: Option<Arc<PullRequestDetail>>,
    pub loading: bool,
    pub detail_loading: bool,
    pub busy: bool,
    pub reconciling: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub creating: bool,
    pub draft: Draft,
    pub options: CreateOptions,
    pub comparison: Option<Comparison>,
    pub current: Option<PullRequest>,
    pub current_checks: CheckState,
    pub foreground: bool,
    pub inline: Option<gh::ReviewAnchor>,
    pub last_checkout: Option<String>,
    pub active_thread: Option<String>,
    pub generation: u64,
    list_generation: u64,
    detail_generation: u64,
    preview_generation: u64,
    path: Option<PathBuf>,
    account: Option<u64>,
    account_generation: u64,
    store: DraftStore,
    last_refresh: Instant,
    last_branch: String,
}
impl EventEmitter<PullEvent> for PullRequestModel {}

impl PullRequestModel {
    pub fn new(
        git: Entity<RepoModel>,
        session: Entity<GitHubSession>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&session, |this: &mut Self, _, cx| {
            let session = this.session.read(cx);
            let account = session.user.as_ref().map(|u| u.id);
            if this.account != account || this.account_generation != session.generation {
                this.account_generation = session.generation;
                this.rebind(cx);
            }
        })
        .detach();
        cx.subscribe(
            &git,
            |this: &mut Self, _, event: &RepoEvent, cx| match event {
                RepoEvent::Opened => this.rebind(cx),
                RepoEvent::Snapshot => {
                    let branch = this.git.read(cx).branch_label();
                    if branch != this.last_branch {
                        this.last_branch = branch;
                        this.refresh_current(cx);
                    }
                }
                RepoEvent::Beat(Beat::Pushed | Beat::Pulled) => this.refresh(cx),
                _ => {}
            },
        )
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.foreground
                            && this.last_refresh.elapsed() >= Duration::from_secs(60)
                            && !this.busy
                        {
                            this.refresh(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut model = Self {
            git,
            session,
            target: None,
            sources: Vec::new(),
            list: Vec::new(),
            filter: Filter::Open,
            query: String::new(),
            next_cursor: None,
            incomplete: false,
            selected: None,
            detail: None,
            loading: false,
            detail_loading: false,
            busy: false,
            reconciling: false,
            error: None,
            message: None,
            creating: false,
            draft: Draft::default(),
            options: CreateOptions::default(),
            comparison: None,
            current: None,
            current_checks: CheckState::None,
            foreground: true,
            inline: None,
            last_checkout: None,
            active_thread: None,
            generation: 0,
            list_generation: 0,
            detail_generation: 0,
            preview_generation: 0,
            path: None,
            account: None,
            account_generation: 0,
            store: DraftStore::default(),
            last_refresh: Instant::now(),
            last_branch: String::new(),
        };
        model.rebind(cx);
        model
    }

    fn changed(&self, cx: &mut Context<Self>) {
        cx.emit(PullEvent::Changed);
        cx.notify();
    }
    pub fn draft_changed(&self, cx: &mut Context<Self>) {
        cx.emit(PullEvent::Draft);
        cx.notify();
    }
    pub fn client(&self, cx: &Context<Self>) -> Client {
        self.session.read(cx).client.clone()
    }
    pub fn login(&self, cx: &Context<Self>) -> String {
        self.session
            .read(cx)
            .user
            .as_ref()
            .map(|u| u.login.clone())
            .unwrap_or_default()
    }

    pub fn rebind(&mut self, cx: &mut Context<Self>) {
        self.persist();
        self.generation += 1;
        self.list_generation += 1;
        self.detail_generation += 1;
        self.preview_generation += 1;
        self.path = self.git.read(cx).repo.clone();
        self.account = self.session.read(cx).user.as_ref().map(|u| u.id);
        self.target = None;
        self.sources.clear();
        self.list.clear();
        self.detail = None;
        self.selected = None;
        self.current = None;
        self.current_checks = CheckState::None;
        self.creating = false;
        self.draft = Draft::default();
        self.options = CreateOptions::default();
        self.comparison = None;
        self.error = None;
        self.message = None;
        self.busy = false;
        self.reconciling = false;
        self.loading = false;
        self.detail_loading = false;
        self.inline = None;
        self.next_cursor = None;
        let (Some(path), Some(_)) = (self.path.clone(), self.account) else {
            self.draft_changed(cx);
            return;
        };
        let saved = prefs::get(cx).github_repositories.get(&path).cloned();
        let generation = self.generation;
        let client = self.client(cx);
        self.loading = true;
        cx.spawn(async move |this,cx| {
            let result=cx.background_executor().spawn(async move {
                let mut remotes=wackdepot_core::remote::list(&path)?;
                remotes.sort_by_key(|r|if r.name=="upstream" {0}else if r.name=="origin" {1}else{2});
                let mut sources=Vec::new();
                for remote in remotes {
                    if let Some(id)=RepositoryId::from_remote(&remote.fetch_url) {
                        let repository=client.repository(&id)?;sources.push(SourceRepository {remote,repository});
                    }
                }
                let target=if let Some(id)=saved {client.repository(&id)?} else {
                    let source=sources.first().ok_or("This repository has no GitHub.com remote. Add an existing GitHub remote to use pull requests.")?;
                    source.repository.parent.as_deref().unwrap_or(&source.repository).clone()
                };
                Ok::<_,String>((target,sources))
            }).await;
            let _=this.update(cx,|this,cx| {
                if this.generation!=generation{return;}
                this.loading=false;
                match result {Ok((target,sources))=>{this.target=Some(target);this.sources=sources;this.refresh(cx);},Err(error)=>this.error=Some(error)}
                this.draft_changed(cx);
            });
        }).detach();
        self.changed(cx);
    }

    pub fn choose_repository(&mut self, id: RepositoryId, cx: &mut Context<Self>) {
        if let Some(path) = self.path.clone() {
            prefs::update(cx, |p| {
                p.github_repositories.insert(path, id);
            });
            self.rebind(cx);
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.last_refresh = Instant::now();
        if self.target.is_none() {
            return;
        }
        self.refresh_list(false, cx);
        self.refresh_current(cx);
        if self.selected.is_some() && !self.creating {
            self.refresh_detail(cx);
        }
    }
    pub fn search(&mut self, text: String, filter: Filter, cx: &mut Context<Self>) {
        self.query = text;
        self.filter = filter;
        self.list_generation += 1;
        self.loading = false;
        let generation = self.list_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.list_generation == generation {
                    this.refresh_list(false, cx);
                }
            });
        })
        .detach();
        self.changed(cx);
    }
    pub fn refresh_list(&mut self, append: bool, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let id = target.identity();
        let client = self.client(cx);
        let login = self.login(cx);
        let filter = self.filter;
        let query = self.query.clone();
        let cursor = if append {
            self.next_cursor.clone()
        } else {
            None
        };
        self.list_generation += 1;
        let generation = self.list_generation;
        let repo_generation = self.generation;
        self.loading = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.list(&id, filter, &query, &login, cursor.as_deref()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != repo_generation || this.list_generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(page) => {
                        if append {
                            for item in page.items {
                                if !this.list.iter().any(|p| p.number == item.number) {
                                    this.list.push(item);
                                }
                            }
                        } else {
                            this.list = page.items;
                        }
                        this.next_cursor = if page.has_next {
                            page.next_cursor
                        } else {
                            None
                        };
                        this.incomplete = page.incomplete;
                        if this.creating {
                            this.reconciling = false;
                        }
                    }
                    Err(error) => this.error = Some(error),
                }
                this.changed(cx);
            });
        })
        .detach();
        self.changed(cx);
    }
    pub fn select(&mut self, number: u64, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.persist();
        self.creating = false;
        self.selected = Some(number);
        self.detail = None;
        self.inline = None;
        self.error = None;
        self.message = None;
        self.load_draft(number);
        self.refresh_detail(cx);
        self.draft_changed(cx);
        self.git
            .update(cx, |m, cx| m.set_tab(Tab::PullRequests, cx));
    }
    pub fn refresh_detail(&mut self, cx: &mut Context<Self>) {
        let (Some(target), Some(number)) = (self.target.as_ref(), self.selected) else {
            return;
        };
        self.detail_generation += 1;
        let generation = self.detail_generation;
        let repo_generation = self.generation;
        let client = self.client(cx);
        let id = target.identity();
        let login = self.login(cx);
        self.detail_loading = true;
        cx.spawn(async move |this,cx| {
            let result=cx.background_executor().spawn(async move {client.detail(&id,number,&login)}).await;
            let _=this.update(cx,|this,cx| {
                if this.generation!=repo_generation || this.detail_generation!=generation || this.selected!=Some(number){return;}
                this.detail_loading=false;
                match result {
                    Ok(detail)=>{
                        if this.draft.review_head.is_empty(){this.draft.review_head=detail.pull.head.sha.clone();}
                        if !this.draft.review_body_initialized{if this.draft.review_body.is_empty(){this.draft.review_body=detail.reviews.pending_body.clone();}this.draft.review_body_initialized=true;}
                        if this.inline.as_ref().is_some_and(|a|a.head_sha!=detail.pull.head.sha){this.message=Some("New commits arrived. Your draft comment is preserved; select its location again before posting.".into());}
                        if this.draft.reconcile(&detail, &this.login(cx)) {
                            this.message=Some("GitHub completed the previous action. Your saved draft has been reconciled.".into());
                            this.inline=None; this.active_thread=None; this.persist();
                        }
                        this.detail=Some(Arc::new(detail));this.reconciling=false;
                    },Err(error)=>this.error=Some(error)
                }this.draft_changed(cx);
            });
        }).detach();
        self.changed(cx);
    }
    fn refresh_current(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let Some(source) = self
            .sources
            .iter()
            .find(|s| s.remote.name == "origin")
            .or_else(|| self.sources.first())
        else {
            return;
        };
        let branch = self.git.read(cx).branch_label();
        if branch.is_empty() {
            return;
        }
        let owner = source.repository.owner.login.clone();
        let id = target.identity();
        let client = self.client(cx);
        let generation = self.generation;
        self.last_branch = branch.clone();
        cx.spawn(async move |this, cx| {
            let lookup = branch.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let pull = client.branch_pull(&id, &owner, &lookup)?;
                    let checks = match &pull {
                        Some(p) => client.checks(&id, &p.head.sha)?.state(),
                        None => CheckState::None,
                    };
                    Ok::<_, String>((pull, checks))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || this.last_branch != branch {
                    return;
                }
                if let Ok((pull, checks)) = result {
                    this.current = pull;
                    this.current_checks = checks;
                    this.changed(cx);
                }
            });
        })
        .detach();
    }

    fn draft_number(&self) -> u64 {
        if self.creating {
            0
        } else {
            self.selected.unwrap_or(0)
        }
    }
    fn load_draft(&mut self, number: u64) {
        if let (Some(account), Some(target)) = (self.account, self.target.as_ref()) {
            match self.store.load(account, target.id, number) {
                Ok(draft) => self.draft = draft,
                Err(error) => {
                    self.error = Some(error);
                    self.draft = Draft::default();
                }
            }
        }
    }
    fn persist(&mut self) {
        if let (Some(account), Some(target)) = (self.account, self.target.as_ref())
            && let Err(error) =
                self.store
                    .save(account, target.id, self.draft_number(), &self.draft)
        {
            self.error = Some(error);
        }
    }
    pub fn edit_draft(&mut self, edit: impl FnOnce(&mut Draft), cx: &mut Context<Self>) {
        edit(&mut self.draft);
        self.persist();
        self.changed(cx);
    }

    pub fn start_create(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.target.is_none() {
            return;
        }
        self.persist();
        self.creating = true;
        self.inline = None;
        self.error = None;
        self.message = None;
        self.load_draft(0);
        if self.draft.creation.base.is_empty() {
            self.draft.creation.base = self.target.as_ref().unwrap().default_branch.clone();
        }
        if self.draft.creation.head.is_empty() {
            let branch = self
                .git
                .read(cx)
                .snap
                .status
                .as_ref()
                .and_then(|s| s.branch.clone())
                .unwrap_or_default();
            if let Some(source) = self
                .sources
                .iter()
                .find(|s| s.repository.permissions.push)
                .or_else(|| self.sources.first())
            {
                self.draft.creation.head = format!("{}:{branch}", source.repository.owner.login);
                self.draft.creation.head_repo = source.repository.name.clone();
            }
            if self.draft.creation.title.is_empty() {
                self.draft.creation.title = branch.replace(['-', '_'], " ");
            }
        }
        let id = self.target.as_ref().unwrap().identity();
        let client = self.client(cx);
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.options(&id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || !this.creating {
                    return;
                }
                match result {
                    Ok(options) => {
                        if this.draft.creation.body.is_empty()
                            && let Some(template) = options.templates.first()
                        {
                            this.draft.creation.body = template.body.clone();
                        }
                        this.options = options;
                        this.persist();
                        this.draft_changed(cx);
                    }
                    Err(error) => {
                        this.error = Some(error);
                        this.changed(cx);
                    }
                }
            });
        })
        .detach();
        self.persist();
        self.git
            .update(cx, |m, cx| m.set_tab(Tab::PullRequests, cx));
        self.draft_changed(cx);
    }
    pub fn preview(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let id = target.identity();
        let input = self.draft.creation.clone();
        let local =
            input.head.split_once(':').and_then(|(owner, branch)| {
                if !self.sources.iter().any(|s| {
                    s.repository.owner.login == owner && s.repository.name == input.head_repo
                }) || !self
                    .git
                    .read(cx)
                    .snap
                    .branches
                    .iter()
                    .any(|b| b.name == branch)
                {
                    return None;
                }
                self.path.clone().map(|path| {
                    let base = self
                        .sources
                        .iter()
                        .find(|s| s.repository.identity() == id)
                        .map(|s| format!("refs/remotes/{}/{}", s.remote.name, input.base))
                        .unwrap_or_else(|| format!("refs/heads/{}", input.base));
                    (path, base, format!("refs/heads/{branch}"))
                })
            });
        let client = self.client(cx);
        self.preview_generation += 1;
        let generation = self.preview_generation;
        let repo_generation = self.generation;
        self.comparison = None;
        self.message = Some("Loading the committed changes…".into());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some((path, base, head)) = local
                        && let Ok(comparison) = wackdepot_core::remote::compare(&path, &base, &head)
                    {
                        let files = comparison
                            .files
                            .into_iter()
                            .map(|file| {
                                let additions = file
                                    .hunks
                                    .iter()
                                    .flat_map(|h| &h.lines)
                                    .filter(|l| l.kind == wackdepot_core::diff::DiffLineKind::Added)
                                    .count() as u64;
                                let deletions = file
                                    .hunks
                                    .iter()
                                    .flat_map(|h| &h.lines)
                                    .filter(|l| {
                                        l.kind == wackdepot_core::diff::DiffLineKind::Removed
                                    })
                                    .count() as u64;
                                gh::ChangedFile {
                                    filename: file.path().to_string(),
                                    previous_filename: file.old_path.clone(),
                                    status: match file.status {
                                        wackdepot_core::diff::DiffFileStatus::Added => "added",
                                        wackdepot_core::diff::DiffFileStatus::Deleted => "removed",
                                        wackdepot_core::diff::DiffFileStatus::Renamed => "renamed",
                                        _ => "modified",
                                    }
                                    .into(),
                                    additions,
                                    deletions,
                                    patch: (!file.is_binary).then(|| {
                                        file.hunks
                                            .iter()
                                            .map(|h| h.patch_text.as_str())
                                            .collect::<String>()
                                    }),
                                    blob_url: String::new(),
                                }
                            })
                            .collect();
                        return Ok(Comparison {
                            files,
                            commits: comparison
                                .commits
                                .into_iter()
                                .map(|(sha, message)| gh::PullCommit {
                                    sha,
                                    commit: gh::CommitInfo { message },
                                    html_url: String::new(),
                                })
                                .collect(),
                            head_sha: comparison.head_sha,
                            base_sha: comparison.base_sha,
                            complete: true,
                            local: true,
                        });
                    }
                    client.compare(&id, &input.base, &input.head)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != repo_generation
                    || this.preview_generation != generation
                    || !this.creating
                {
                    return;
                }
                match result {
                    Ok(comparison) => {
                        this.comparison = Some(comparison);
                        this.message = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                this.changed(cx);
            });
        })
        .detach();
        self.changed(cx);
    }

    pub fn invalidate_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_generation += 1;
        self.comparison = None;
        self.message = None;
        self.changed(cx);
    }

    pub fn perform<T: Send + 'static>(
        &mut self,
        label: &'static str,
        operation: impl FnOnce(Client, RepositoryId, String) -> Result<T, String> + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.reconciling || self.session.read(cx).user.is_none() {
            return;
        }
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let id = target.identity();
        let client = self.client(cx);
        let login = self.login(cx);
        let generation = self.generation;
        self.busy = true;
        self.message = Some(label.into());
        self.error = None;
        self.detail_generation += 1;
        self.detail_loading = false;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { operation(client, id, login) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                this.message = None;
                match result {
                    Ok(value) => done(this, value, cx),
                    Err(error) => {
                        this.error = Some(error);
                        this.reconciling = true;
                        this.refresh(cx);
                    }
                }
                this.persist();
                this.changed(cx);
            });
        })
        .detach();
        self.changed(cx);
    }

    pub fn create(&mut self, publish: bool, cx: &mut Context<Self>) {
        if self.busy || self.reconciling {
            return;
        }
        if publish {
            self.publish_then_create(cx);
            return;
        }
        let input = self.draft.creation.clone();
        let submitted = input.clone();
        if let Some(number) = self
            .draft
            .created_pr
            .filter(|_| self.draft.metadata_pending)
        {
            let mut input = input;
            if let Some(remaining) = &self.draft.metadata_remaining {
                if remaining.labels.is_empty() {
                    input.labels.clear();
                }
                if remaining.reviewers.is_empty() && remaining.teams.is_empty() {
                    input.reviewers.clear();
                    input.teams.clear();
                }
            }
            self.perform(
                "Assigning reviewers and labels…",
                move |client, id, _| client.retry_creation_metadata(&id, number, &input),
                move |this, outcome, cx| {
                    this.draft.metadata_remaining = outcome.metadata_remaining;
                    if let Some(error) = outcome.metadata_error {
                        this.error = Some(error);
                        this.persist();
                        this.draft_changed(cx);
                    } else {
                        this.draft = Draft::default();
                        this.persist();
                        this.select(number, cx);
                    }
                },
                cx,
            );
            return;
        }
        self.perform("Creating pull request…",move |client,id,_|client.create(&id,&input),move |this,outcome,cx| {
            this.draft.created_pr=Some(outcome.pull.number);this.draft.metadata_pending=outcome.metadata_error.is_some();this.draft.metadata_remaining=outcome.metadata_remaining;
            this.persist();
            if let Some(error)=outcome.metadata_error {this.error=Some(format!("Pull request #{} was created. {error} Retry assigning its reviewers and labels here.",outcome.pull.number));this.draft_changed(cx);}
            else {if this.draft.creation==submitted{this.draft=Draft::default();}else{this.draft.created_pr=None;this.draft.metadata_pending=false;}this.persist();this.select(outcome.pull.number,cx);this.refresh_list(false,cx);}
        },cx);
    }
    fn publish_then_create(&mut self, cx: &mut Context<Self>) {
        let Some((owner, branch)) = self
            .draft
            .creation
            .head
            .split_once(':')
            .map(|(a, b)| (a.to_string(), b.to_string()))
        else {
            self.error = Some("Choose the head repository and branch before publishing.".into());
            self.changed(cx);
            return;
        };
        let Some(source) = self
            .sources
            .iter()
            .find(|s| {
                s.repository.owner.login == owner
                    && s.repository.name == self.draft.creation.head_repo
                    && s.repository.permissions.push
                    && gh::RepositoryId::from_remote(&s.remote.push_url).as_ref()
                        == Some(&s.repository.identity())
            })
            .cloned()
        else {
            self.error = Some(
                "Choose an existing writable fork remote before publishing this branch.".into(),
            );
            self.changed(cx);
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let executable = std::env::current_exe().ok();
        self.busy = true;
        self.message = Some("Publishing branch…".into());
        self.git.update(cx, |git, cx| {
            git.run(
                BusyOp::Push,
                move |path, sink| {
                    let runner = executable.as_deref().map_or_else(
                        || wackdepot_core::Git::new(path),
                        |exe| wackdepot_core::Git::new(path).with_github_credentials(exe),
                    );
                    let result = wackdepot_core::remote::publish(
                        &runner,
                        &source.remote.name,
                        &branch,
                        &source.remote.push_url,
                        &mut |progress| {
                            if let Ok(mut p) = sink.lock() {
                                *p = Some(progress);
                            }
                        },
                    );
                    let _ = sender.send(result.clone());
                    result.map(|_| Done::quiet().beat(Beat::Pushed))
                },
                cx,
            )
        });
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    receiver.recv().unwrap_or_else(|_| {
                        Err("Publishing stopped before Git reported its result.".into())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(()) => this.create(false, cx),
                    Err(error) => {
                        this.error = Some(error);
                        this.message = None;
                        this.changed(cx);
                    }
                }
            });
        })
        .detach();
        self.changed(cx);
    }

    pub fn checkout(&mut self, stash: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(detail), Some(target)) = (self.detail.as_ref(), self.target.as_ref()) else {
            return;
        };
        let target_id = target.identity();
        let source = self
            .sources
            .iter()
            .find(|s| s.repository.identity() == target_id)
            .map(|s| s.remote.name.clone())
            .unwrap_or_else(|| format!("https://github.com/{}.git", target_id.full_name()));
        let tracking = detail.pull.head.repo.as_ref().and_then(|head| {
            self.sources
                .iter()
                .find(|s| {
                    s.repository.id == head.id
                        && s.repository.permissions.push
                        && gh::RepositoryId::from_remote(&s.remote.push_url).as_ref()
                            == Some(&s.repository.identity())
                })
                .map(|s| (s.remote.name.clone(), detail.pull.head.name.clone()))
        });
        let input = wackdepot_core::remote::CheckoutPullRequest {
            source,
            number: detail.pull.number,
            expected_sha: detail.pull.head.sha.clone(),
            local_name: if tracking.is_some() {
                detail.pull.head.name.clone()
            } else {
                format!("pr/{}", detail.pull.number)
            },
            tracking,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let executable = std::env::current_exe().ok();
        self.busy = true;
        self.message = Some("Checking out pull request…".into());
        self.git.update(cx, |git, cx| {
            git.run(
                BusyOp::Checkout,
                move |path, sink| {
                    let result = (|| {
                        if stash {
                            wackdepot_core::stash::stash_push(
                                path,
                                Some("WackDepot: before checking out a pull request"),
                                true,
                            )?;
                        }
                        let runner = executable.as_deref().map_or_else(
                            || wackdepot_core::Git::new(path),
                            |exe| wackdepot_core::Git::new(path).with_github_credentials(exe),
                        );
                        wackdepot_core::remote::checkout_pull_request(
                            &runner,
                            &input,
                            &mut |progress| {
                                if let Ok(mut p) = sink.lock() {
                                    *p = Some(progress);
                                }
                            },
                        )
                    })();
                    let _ = sender.send(result.clone());
                    result.map(|branch| {
                        Done::toast(format!("Checked out {branch}."))
                            .beat(Beat::CheckedOut { branch })
                    })
                },
                cx,
            )
        });
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    receiver.recv().unwrap_or_else(|_| {
                        Err("Checkout stopped before Git reported its result.".into())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                this.message = None;
                match result {
                    Ok(branch) => {
                        this.last_checkout = Some(branch);
                        this.refresh_current(cx);
                    }
                    Err(error) => this.error = Some(error),
                }
                this.changed(cx);
            });
        })
        .detach();
        self.changed(cx);
    }

    pub fn add_inline(&mut self, body: String, immediate: bool, cx: &mut Context<Self>) {
        if self.busy || self.reconciling {
            return;
        }
        let (Some(detail), Some(anchor)) = (self.detail.as_ref(), self.inline.clone()) else {
            return;
        };
        if let Err(error) = anchor.validate(&detail.pull.head.sha) {
            self.error = Some(error);
            self.changed(cx);
            return;
        }
        let number = detail.pull.number;
        let node = detail.pull.node_id.clone();
        let head = detail.pull.head.sha.clone();
        let pending = detail.reviews.pending_id.clone();
        if body.trim().is_empty() {
            return;
        }
        let posted_body = body.clone();
        let posted_anchor = anchor.clone();
        let retry = self.prepare_write(
            WriteKind::Inline {
                anchor: anchor.clone(),
                immediate,
            },
            body.clone(),
        );
        if !self
            .draft
            .comments
            .iter()
            .any(|c| c.anchor == anchor && c.body == body)
        {
            self.draft.comments.push(gh::drafts::LocalComment {
                body: body.clone(),
                anchor: anchor.clone(),
            });
            self.persist();
        }
        self.perform(
            "Posting review comment…",
            move |client, id, login| {
                if let Some(retry) = retry
                    && retry.confirmed(&client.detail(&id, number, &login)?, &login)
                {
                    return Ok(());
                }
                if immediate {
                    client.comment_line(&id, number, &anchor, &body)
                } else {
                    let review = match pending {
                        Some(id) => id,
                        None => client.begin_review(&node, &head)?,
                    };
                    client.add_thread(&node, Some(&review), &anchor, &head, &body)
                }
            },
            move |this, _, cx| {
                this.draft.pending_write = None;
                this.draft
                    .comments
                    .retain(|c| c.anchor != posted_anchor || c.body != posted_body);
                if !this
                    .draft
                    .comments
                    .iter()
                    .any(|c| c.anchor == posted_anchor)
                    && this.inline.as_ref() == Some(&posted_anchor)
                {
                    this.inline = None;
                }
                this.persist();
                this.draft_changed(cx);
                this.refresh_detail(cx);
            },
            cx,
        );
    }
    pub fn submit_review(&mut self, event: gh::ReviewEvent, cx: &mut Context<Self>) {
        if self.busy || self.reconciling {
            return;
        }
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let pull = detail.pull.clone();
        let pending = detail.reviews.pending_id.clone();
        let body = self.draft.review_body.clone();
        let posted_body = body.clone();
        if !self.draft.review_head.is_empty() && self.draft.review_head != pull.head.sha {
            self.error=Some("New commits arrived since you started this review. Inspect the latest files and acknowledge the new revision before submitting.".into());
            self.changed(cx);
            return;
        }
        if detail
            .reviews
            .pending_head
            .as_ref()
            .is_some_and(|sha| sha != &pull.head.sha)
        {
            self.error=Some("The pending review refers to an older revision. Discard it or review its comments on GitHub before starting a new review.".into());
            self.changed(cx);
            return;
        }
        if event != gh::ReviewEvent::Comment
            && pull.user.login.eq_ignore_ascii_case(&self.login(cx))
        {
            self.error=Some("You can comment on your own pull request, but GitHub requires another reviewer to approve it or request changes.".into());
            self.changed(cx);
            return;
        }
        let retry = self.prepare_write(
            WriteKind::Review {
                event: event.api().into(),
            },
            body.clone(),
        );
        self.perform(
            "Submitting review…",
            move |client, id, login| {
                if let Some(retry) = retry
                    && retry.confirmed(&client.detail(&id, pull.number, &login)?, &login)
                {
                    return Ok(());
                }
                let review = match pending {
                    Some(id) => id,
                    None => client.begin_review(&pull.node_id, &pull.head.sha)?,
                };
                client.submit_review(&id, pull.number, &review, &pull.head.sha, &body, event)
            },
            move |this, _, cx| {
                this.draft.pending_write = None;
                if this.draft.review_body == posted_body {
                    this.draft.review_body.clear();
                }
                this.persist();
                this.refresh_detail(cx);
                this.draft_changed(cx);
            },
            cx,
        );
    }

    fn prepare_write(&mut self, kind: WriteKind, body: String) -> Option<PendingWrite> {
        if let Some(write) = &self.draft.pending_write
            && write.kind == kind
            && write.body == body
        {
            return Some(write.clone());
        }
        if let Some(detail) = &self.detail {
            self.draft.pending_write = Some(PendingWrite::new(kind, body, detail));
            self.persist();
        }
        None
    }

    pub fn post_conversation(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.reconciling || self.draft.conversation_body.trim().is_empty() {
            return;
        }
        let Some(number) = self.selected else {
            return;
        };
        let body = self.draft.conversation_body.clone();
        let posted = body.clone();
        let retry = self.prepare_write(WriteKind::Conversation, body.clone());
        self.perform(
            "Posting comment…",
            move |client, id, login| {
                if let Some(retry) = retry
                    && retry.confirmed(&client.detail(&id, number, &login)?, &login)
                {
                    return Ok(());
                }
                client.comment(&id, number, &body)
            },
            move |m, _, cx| {
                m.draft.pending_write = None;
                if m.draft.conversation_body == posted {
                    m.draft.conversation_body.clear();
                }
                m.refresh_detail(cx);
                m.draft_changed(cx);
            },
            cx,
        );
    }

    pub fn post_reply(&mut self, thread: String, cx: &mut Context<Self>) {
        if self.busy || self.reconciling {
            return;
        }
        let Some(number) = self.selected else {
            return;
        };
        let body = self.draft.replies.get(&thread).cloned().unwrap_or_default();
        if body.trim().is_empty() {
            return;
        }
        let retry = if thread.starts_with("edit:") {
            None
        } else {
            self.prepare_write(
                WriteKind::Reply {
                    thread: thread.clone(),
                },
                body.clone(),
            )
        };
        let done_thread = thread.clone();
        let posted = body.clone();
        self.perform(
            "Posting discussion reply…",
            move |client, id, login| {
                if let Some(retry) = retry
                    && retry.confirmed(&client.detail(&id, number, &login)?, &login)
                {
                    return Ok(());
                }
                if let Some(id) = thread.strip_prefix("edit:") {
                    client.edit_review_comment(id, &body)
                } else {
                    client.reply(&thread, &body)
                }
            },
            move |m, _, cx| {
                m.draft.pending_write = None;
                if m.draft.replies.get(&done_thread) == Some(&posted) {
                    m.draft.replies.remove(&done_thread);
                }
                m.active_thread = None;
                m.refresh_detail(cx);
                m.draft_changed(cx);
            },
            cx,
        );
    }
}
