//! Account-scoped local drafts. Nothing is written to the user's repository.

use crate::{CreatePullRequestInput, MergeMethod, ReviewAnchor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteKind {
    Conversation,
    Inline {
        anchor: ReviewAnchor,
        immediate: bool,
    },
    Reply {
        thread: String,
    },
    Review {
        event: String,
    },
}

/// A persisted baseline makes non-idempotent writes recoverable even when the
/// server completed a request whose response was lost, or the app restarted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWrite {
    pub kind: WriteKind,
    pub body: String,
    pub known_ids: Vec<String>,
}
impl PendingWrite {
    pub fn new(kind: WriteKind, body: String, detail: &crate::PullRequestDetail) -> Self {
        let known_ids = match &kind {
            WriteKind::Conversation => detail.comments.iter().map(|c| c.id.to_string()).collect(),
            WriteKind::Review { .. } => detail
                .reviews
                .submitted
                .iter()
                .map(|r| r.id.to_string())
                .collect(),
            _ => detail
                .reviews
                .threads
                .iter()
                .flat_map(|t| &t.comments)
                .map(|c| c.id.clone())
                .collect(),
        };
        Self {
            kind,
            body,
            known_ids,
        }
    }
    pub fn confirmed(&self, detail: &crate::PullRequestDetail, login: &str) -> bool {
        let unseen = |id: &str| !self.known_ids.iter().any(|known| known == id);
        match &self.kind {
            WriteKind::Conversation => detail.comments.iter().any(|c| {
                unseen(&c.id.to_string())
                    && c.body == self.body
                    && c.user.login.eq_ignore_ascii_case(login)
            }),
            WriteKind::Review { event } => {
                let state = match event.as_str() {
                    "APPROVE" => "APPROVED",
                    "REQUEST_CHANGES" => "CHANGES_REQUESTED",
                    _ => "COMMENTED",
                };
                detail.reviews.submitted.iter().any(|r| {
                    unseen(&r.id.to_string())
                        && r.body == self.body
                        && r.state == state
                        && r.user.login.eq_ignore_ascii_case(login)
                })
            }
            WriteKind::Inline { anchor, immediate } => detail.reviews.threads.iter().any(|t| {
                !t.outdated
                    && t.path == anchor.path
                    && t.side == anchor.side
                    && t.line == Some(anchor.line)
                    && t.start_line == anchor.start_line
                    && t.comments.iter().any(|c| {
                        unseen(&c.id)
                            && c.body == self.body
                            && c.author.eq_ignore_ascii_case(login)
                            && (*immediate || c.pending)
                    })
            }),
            WriteKind::Reply { thread } => detail
                .reviews
                .threads
                .iter()
                .filter(|t| &t.id == thread)
                .flat_map(|t| &t.comments)
                .any(|c| {
                    unseen(&c.id) && c.body == self.body && c.author.eq_ignore_ascii_case(login)
                }),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalComment {
    pub body: String,
    pub anchor: ReviewAnchor,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Draft {
    pub creation: CreatePullRequestInput,
    pub created_pr: Option<u64>,
    pub metadata_pending: bool,
    pub metadata_remaining: Option<CreatePullRequestInput>,
    pub review_body: String,
    pub review_body_initialized: bool,
    pub review_head: String,
    pub conversation_body: String,
    pub comments: Vec<LocalComment>,
    pub merge_method: MergeMethod,
    pub merge_title: String,
    pub merge_body: String,
    pub replies: std::collections::HashMap<String, String>,
    pub pending_write: Option<PendingWrite>,
}

impl Draft {
    pub fn reconcile(&mut self, detail: &crate::PullRequestDetail, login: &str) -> bool {
        let Some(write) = self
            .pending_write
            .as_ref()
            .filter(|w| w.confirmed(detail, login))
            .cloned()
        else {
            return false;
        };
        match &write.kind {
            WriteKind::Conversation if self.conversation_body == write.body => {
                self.conversation_body.clear()
            }
            WriteKind::Inline { anchor, .. } => self
                .comments
                .retain(|c| c.anchor != *anchor || c.body != write.body),
            WriteKind::Reply { thread } if self.replies.get(thread) == Some(&write.body) => {
                self.replies.remove(thread);
            }
            WriteKind::Review { .. } if self.review_body == write.body => self.review_body.clear(),
            _ => {}
        }
        self.pending_write = None;
        true
    }
}

#[derive(Clone)]
pub struct DraftStore {
    root: PathBuf,
}
impl Default for DraftStore {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        Self {
            root: home
                .join("Library/Application Support/WackDepot")
                .join(if cfg!(debug_assertions) {
                    "github-dev"
                } else {
                    "github"
                })
                .join("drafts"),
        }
    }
}
impl DraftStore {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }
    fn path(&self, account: u64, repository: u64, number: u64) -> PathBuf {
        self.root
            .join(account.to_string())
            .join(repository.to_string())
            .join(format!("{number}.json"))
    }
    pub fn load(&self, account: u64, repository: u64, number: u64) -> Result<Draft, String> {
        match std::fs::read(self.path(account,repository,number)) {
            Ok(bytes)=>serde_json::from_slice(&bytes).map_err(|_|"Couldn't read this pull request's saved draft. Its original file has been preserved.".into()),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(Draft::default()),
            Err(_)=>Err("Couldn't read this pull request's saved draft.".into())
        }
    }
    pub fn save(
        &self,
        account: u64,
        repository: u64,
        number: u64,
        draft: &Draft,
    ) -> Result<(), String> {
        let path = self.path(account, repository, number);
        let parent = path.parent().expect("draft parent");
        std::fs::create_dir_all(parent)
            .map_err(|_| "Couldn't create the pull request drafts folder.".to_string())?;
        let bytes = serde_json::to_vec(draft)
            .map_err(|_| "Couldn't prepare this pull request's draft.".to_string())?;
        if let Ok(existing) = std::fs::read(&path)
            && serde_json::from_slice::<Draft>(&existing).is_err()
        {
            let backup = path.with_extension(format!("unreadable-{}.json", crate::auth::now()));
            std::fs::copy(&path,backup).map_err(|_|"Couldn't preserve the unreadable draft before saving. Its original file has been kept.".to_string())?;
        }
        atomic_write(&path, &bytes)
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temp = path.with_extension("json.tmp");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "Couldn't save this pull request's draft.".to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Couldn't finish saving this pull request's draft.".to_string())?;
    std::fs::rename(temp, path)
        .map_err(|_| "Couldn't replace this pull request's saved draft.".into())
}
