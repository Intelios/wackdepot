//! Account-scoped local drafts. Nothing is written to the user's repository.

use crate::{CreatePullRequestInput, MergeMethod, ReviewAnchor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    pub review_body: String,
    pub conversation_body: String,
    pub comments: Vec<LocalComment>,
    pub merge_method: MergeMethod,
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
