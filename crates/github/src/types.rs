use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct User {
    #[serde(default)]
    pub id: u64,
    pub login: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct RepositoryId {
    pub owner: String,
    pub name: String,
}

impl RepositoryId {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn from_remote(remote: &str) -> Option<Self> {
        let remote = remote.trim();
        let path = if let Some(path) = remote.strip_prefix("git@github.com:") {
            path.to_string()
        } else {
            let url = url::Url::parse(remote).ok()?;
            if url.host_str()? != "github.com"
                || !matches!(url.scheme(), "https" | "ssh")
                || url.query().is_some()
                || url.fragment().is_some()
                || url.port().is_some_and(|p| p != 443 && p != 22)
            {
                return None;
            }
            url.path().trim_start_matches('/').to_string()
        };
        let path = path.trim_end_matches('/').trim_end_matches(".git");
        let (owner, name) = path.split_once('/')?;
        let valid = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        };
        (valid(owner) && valid(name) && owner != "." && name != "." && name != "..").then(|| Self {
            owner: owner.into(),
            name: name.into(),
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub maintain: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Repository {
    pub id: u64,
    #[serde(default)]
    pub node_id: String,
    pub name: String,
    pub owner: User,
    #[serde(default)]
    pub default_branch: String,
    /// `https://github.com/owner/name.git` — what `origin` gets on publish.
    #[serde(default)]
    pub clone_url: String,
    #[serde(default)]
    pub ssh_url: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub parent: Option<Box<Repository>>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub allow_merge_commit: bool,
    #[serde(default)]
    pub allow_squash_merge: bool,
    #[serde(default)]
    pub allow_rebase_merge: bool,
}

impl Repository {
    pub fn identity(&self) -> RepositoryId {
        RepositoryId {
            owner: self.owner.login.clone(),
            name: self.name.clone(),
        }
    }
    pub fn merge_methods(&self) -> Vec<MergeMethod> {
        let mut methods = Vec::new();
        if self.allow_squash_merge {
            methods.push(MergeMethod::Squash);
        }
        if self.allow_merge_commit {
            methods.push(MergeMethod::Merge);
        }
        if self.allow_rebase_merge {
            methods.push(MergeMethod::Rebase);
        }
        methods
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BranchRef {
    #[serde(rename = "ref", default)]
    pub name: String,
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub label: String,
    pub repo: Option<Repository>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Team {
    pub slug: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub node_id: String,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    pub merged_at: Option<String>,
    pub html_url: String,
    pub user: User,
    pub head: BranchRef,
    pub base: BranchRef,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub requested_reviewers: Vec<User>,
    #[serde(default)]
    pub requested_teams: Vec<Team>,
    pub mergeable: Option<bool>,
    #[serde(default)]
    pub mergeable_state: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
}

impl PullRequest {
    pub fn state_label(&self) -> &'static str {
        if self.merged_at.is_some() {
            "Merged"
        } else if self.state == "closed" {
            "Closed"
        } else if self.draft {
            "Draft"
        } else {
            "Open"
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    Open,
    Mine,
    ReviewRequested,
    Drafts,
    Closed,
    Merged,
}
impl Filter {
    pub const ALL: [Self; 6] = [
        Self::Open,
        Self::Mine,
        Self::ReviewRequested,
        Self::Drafts,
        Self::Closed,
        Self::Merged,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Mine => "Mine",
            Self::ReviewRequested => "Review requested",
            Self::Drafts => "Drafts",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
        }
    }
    pub fn query(self, login: &str) -> String {
        match self {
            Self::Open => "is:open".into(),
            Self::Mine => format!("is:open author:{login}"),
            Self::ReviewRequested => format!("is:open review-requested:{login}"),
            Self::Drafts => "is:open draft:true".into(),
            Self::Closed => "is:closed is:unmerged".into(),
            Self::Merged => "is:merged".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PullRequestPage {
    pub items: Vec<PullRequest>,
    pub has_next: bool,
    pub next_cursor: Option<String>,
    pub incomplete: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChangedFile {
    pub filename: String,
    pub previous_filename: Option<String>,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
    #[serde(default)]
    pub blob_url: String,
}

impl ChangedFile {
    /// An absent patch is not necessarily binary. Never silently label a
    /// truncated patch as the complete file or accept anchors on it.
    pub fn patch_complete(&self) -> bool {
        match &self.patch {
            Some(patch) if patch.len() <= 512 * 1024 => {
                let added = patch.lines().filter(|l| l.starts_with('+')).count() as u64;
                let removed = patch.lines().filter(|l| l.starts_with('-')).count() as u64;
                added == self.additions && removed == self.deletions
            }
            None => {
                self.additions == 0
                    && self.deletions == 0
                    && matches!(self.status.as_str(), "renamed" | "copied" | "changed")
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CommitInfo {
    pub message: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PullCommit {
    pub sha: String,
    pub commit: CommitInfo,
    #[serde(default)]
    pub html_url: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub node_id: String,
    pub body: String,
    pub user: User,
    pub created_at: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffSide {
    Left,
    #[default]
    Right,
}
impl DiffSide {
    pub fn api(self) -> &'static str {
        match self {
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewAnchor {
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    #[serde(default)]
    pub start_line: Option<u32>,
    pub head_sha: String,
}

impl ReviewAnchor {
    pub fn validate(&self, head: &str) -> Result<(), String> {
        if self.head_sha != head {
            return Err("This pull request has new commits. Refresh the diff and review this comment's location before posting.".into());
        }
        if self.path.is_empty()
            || self.line == 0
            || self.start_line.is_some_and(|n| n == 0 || n > self.line)
        {
            return Err("Select a valid line or line range in the diff first.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReviewComment {
    pub id: String,
    pub body: String,
    pub author: String,
    pub created_at: String,
    pub can_update: bool,
    pub can_delete: bool,
    pub pending: bool,
    pub diff_hunk: String,
}
#[derive(Clone, Debug, Default)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<u32>,
    pub start_line: Option<u32>,
    pub side: DiffSide,
    pub resolved: bool,
    pub outdated: bool,
    pub can_resolve: bool,
    pub can_unresolve: bool,
    pub comments: Vec<ReviewComment>,
}
#[derive(Clone, Debug, Default)]
pub struct ReviewInfo {
    pub decision: String,
    pub merge_state: String,
    pub can_update: bool,
    pub pending_id: Option<String>,
    pub pending_head: Option<String>,
    pub pending_body: String,
    pub threads: Vec<ReviewThread>,
    pub viewed: Vec<String>,
    pub submitted: Vec<SubmittedReview>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SubmittedReview {
    pub id: u64,
    pub user: User,
    #[serde(default)]
    pub body: String,
    pub state: String,
    pub submitted_at: Option<String>,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    None,
    Pending,
    Passing,
    Failing,
}
impl CheckState {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No checks",
            Self::Pending => "Checks pending",
            Self::Passing => "Checks passing",
            Self::Failing => "Checks failed",
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub detail: String,
    pub url: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
    pub head_sha: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct WorkflowJob {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
}
#[derive(Clone, Debug, Default)]
pub struct Checks {
    pub checks: Vec<Check>,
    pub workflows: Vec<WorkflowRun>,
    pub jobs: Vec<WorkflowJob>,
}
impl Checks {
    pub fn state(&self) -> CheckState {
        if self.checks.iter().any(|c| c.state == CheckState::Failing) {
            CheckState::Failing
        } else if self.checks.iter().any(|c| c.state == CheckState::Pending) {
            CheckState::Pending
        } else if self.checks.is_empty() {
            CheckState::None
        } else {
            CheckState::Passing
        }
    }
}

#[derive(Clone, Debug)]
pub struct PullRequestDetail {
    pub pull: PullRequest,
    pub files: Vec<ChangedFile>,
    pub files_complete: bool,
    pub commits: Vec<PullCommit>,
    pub comments: Vec<Comment>,
    pub reviews: ReviewInfo,
    pub checks: Checks,
    pub merge_base_sha: String,
}
impl PullRequestDetail {
    pub fn merge_blocker(&self, repo: &Repository) -> Option<String> {
        if self.pull.state != "open" || self.pull.merged_at.is_some() {
            return Some("This pull request is no longer open.".into());
        }
        if self.pull.draft {
            return Some("Mark this draft ready for review before merging.".into());
        }
        if !repo.permissions.push && !repo.permissions.admin && !repo.permissions.maintain {
            return Some("You don't have permission to merge into this repository.".into());
        }
        if repo.merge_methods().is_empty() {
            return Some("This repository has no enabled merge methods.".into());
        }
        if self.pull.mergeable == Some(false) {
            return Some("Resolve the merge conflicts before merging.".into());
        }
        match self.reviews.merge_state.as_str() {
            "CLEAN" | "UNSTABLE" if self.pull.mergeable == Some(true) && self.reviews.decision != "CHANGES_REQUESTED" && self.reviews.decision != "REVIEW_REQUIRED" => None,
            "BEHIND" => Some("Update this branch with the base branch before merging.".into()),
            "DIRTY" => Some("Resolve the merge conflicts before merging.".into()),
            "BLOCKED" => Some("GitHub requires more reviews, successful checks, resolved conversations, or a merge queue before merging.".into()),
            _ => Some("GitHub hasn't confirmed that this pull request can be merged. Refresh its requirements before merging.".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}
impl MergeMethod {
    pub fn api(self) -> &'static str {
        match self {
            Self::Squash => "squash",
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Squash => "Squash and merge",
            Self::Merge => "Create merge commit",
            Self::Rebase => "Rebase and merge",
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CreatePullRequestInput {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
    pub head_repo: String,
    pub draft: bool,
    pub reviewers: Vec<String>,
    pub teams: Vec<String>,
    pub labels: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct CreateOutcome {
    pub pull: PullRequest,
    pub metadata_error: Option<String>,
    pub metadata_remaining: Option<CreatePullRequestInput>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewEvent {
    Comment,
    Approve,
    RequestChanges,
}
impl ReviewEvent {
    pub fn api(self) -> &'static str {
        match self {
            Self::Comment => "COMMENT",
            Self::Approve => "APPROVE",
            Self::RequestChanges => "REQUEST_CHANGES",
        }
    }
}
#[derive(Clone, Debug)]
pub struct MergeInput {
    pub method: MergeMethod,
    pub expected_head_sha: String,
    pub title: String,
    pub body: String,
}
#[derive(Clone, Debug, Default)]
pub struct Template {
    pub name: String,
    pub body: String,
}
#[derive(Clone, Debug, Default)]
pub struct CreateOptions {
    pub branches: Vec<String>,
    pub reviewers: Vec<User>,
    pub teams: Vec<Team>,
    pub labels: Vec<Label>,
    pub templates: Vec<Template>,
}
#[derive(Clone, Debug, Default)]
pub struct Comparison {
    pub files: Vec<ChangedFile>,
    pub commits: Vec<PullCommit>,
    pub complete: bool,
    pub head_sha: String,
    pub base_sha: String,
    pub local: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlobContent {
    Text(String),
    Binary,
    TooLarge,
    Unavailable,
}
