//! Everything read from git for the open repository in one refresh.

use std::collections::HashSet;
use std::path::Path;

use wackdepot_core::{branch, commit, graph, stash, status};

/// Commits loaded into History.
pub const LOG_LIMIT: usize = 400;

/// One coherent read of the repo. Shared (`Arc`) between the model and views.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub status: Option<status::RepoStatus>,
    pub branches: Vec<branch::BranchInfo>,
    /// Remote-tracking refs (`origin/main`…) — merge sources.
    pub remote_branches: Vec<String>,
    pub log: Vec<commit::LogEntry>,
    pub graph: Vec<graph::GraphRow>,
    pub stashes: Vec<stash::StashEntry>,
    /// Commits on HEAD no remote has yet.
    pub unpushed: HashSet<String>,
    pub stats: status::LineStats,
    /// `(name, email)` git will commit as.
    pub identity: Option<(String, String)>,
    /// The tip commit is unpushed (Undo is safe to offer).
    pub tip_unpushed: bool,
}

impl Snapshot {
    pub fn load(repo: &Path) -> Result<Self, String> {
        let status =
            status::status(repo).map_err(|e| format!("Couldn't read {} — {e}", repo.display()))?;
        let stats = status::line_stats(repo, &status.untracked).unwrap_or_default();
        let log = commit::list_log(repo, LOG_LIMIT).unwrap_or_default();
        Ok(Self {
            graph: graph::layout(&log),
            log,
            branches: branch::list_branches(repo).unwrap_or_default(),
            remote_branches: branch::remote_branches(repo).unwrap_or_default(),
            stashes: stash::stash_list(repo).unwrap_or_default(),
            unpushed: commit::unpushed_shas(repo).unwrap_or_default(),
            stats,
            identity: commit::author(repo).ok(),
            tip_unpushed: commit::tip_is_unpushed(repo).unwrap_or(false),
            status: Some(status),
        })
    }

    pub fn bucket(&self, bucket: Bucket) -> &[status::FileChange] {
        let Some(s) = &self.status else { return &[] };
        match bucket {
            Bucket::Conflicted => &s.conflicted,
            Bucket::Staged => &s.staged,
            Bucket::Unstaged => &s.unstaged,
            Bucket::Untracked => &s.untracked,
        }
    }

    /// `+added −removed` for a file row.
    pub fn line_stat(&self, bucket: Bucket, path: &str) -> Option<(u32, u32)> {
        let map = match bucket {
            Bucket::Staged => &self.stats.staged,
            _ => &self.stats.unstaged,
        };
        map.get(path).copied()
    }

    pub fn changed_count(&self) -> usize {
        Bucket::ALL.iter().map(|b| self.bucket(*b).len()).sum()
    }
}

/// The groups of the Changes list, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Conflicted,
    Staged,
    Unstaged,
    Untracked,
}

impl Bucket {
    pub const ALL: [Bucket; 4] = [
        Bucket::Conflicted,
        Bucket::Staged,
        Bucket::Unstaged,
        Bucket::Untracked,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Bucket::Conflicted => "Conflicts",
            Bucket::Staged => "Staged",
            Bucket::Unstaged => "Changes",
            Bucket::Untracked => "Untracked",
        }
    }

    /// Which side of the index a file's diff is read from.
    pub fn side(self) -> wackdepot_core::diff::DiffSide {
        match self {
            Bucket::Staged => wackdepot_core::diff::DiffSide::Staged,
            _ => wackdepot_core::diff::DiffSide::Unstaged,
        }
    }
}
