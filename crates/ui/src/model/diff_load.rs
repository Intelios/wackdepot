//! Loading what the content card shows for a selection, off the UI thread.

use std::path::Path;
use std::sync::Arc;

use wackdepot_core::commit::LogEntry;
use wackdepot_core::diff::{self, CommitFile, DiffOptions, FileDiff};
use wackdepot_core::words::{self, Ranges};

use super::selection::Selection;
use super::snapshot::Bucket;

/// A loaded diff, ready to render.
#[derive(Debug)]
pub struct DiffDoc {
    pub selection: Selection,
    pub kind: DocKind,
    /// The file being shown (`None` for an empty commit).
    pub file: Option<FileDiff>,
    /// Word-level emphasis per hunk, per line (aligned with `hunk.lines`).
    pub words: Vec<Vec<Option<Ranges>>>,
    /// Whitespace was ignored — hunks/lines can't be staged from this diff.
    pub ignore_whitespace: bool,
}

#[derive(Debug)]
pub enum DocKind {
    Working {
        bucket: Bucket,
    },
    Commit {
        entry: Box<LogEntry>,
        files: Vec<CommitFile>,
    },
}

/// What's in the content card.
#[derive(Debug, Clone, Default)]
pub enum DiffState {
    #[default]
    Empty,
    Loading,
    Ready(Arc<DiffDoc>),
    Error(String),
}

pub fn load(
    repo: &Path,
    sel: &Selection,
    log: &[LogEntry],
    opts: DiffOptions,
) -> Result<DiffDoc, String> {
    match sel {
        Selection::File { path, bucket } => {
            let file = diff::diff_with(repo, path, bucket.side(), &opts)?;
            Ok(DiffDoc {
                words: word_ranges(&file),
                selection: sel.clone(),
                kind: DocKind::Working { bucket: *bucket },
                file: Some(file),
                ignore_whitespace: opts.ignore_whitespace,
            })
        }
        Selection::Commit { sha, file } => {
            let entry =
                log.iter().find(|c| &c.sha == sha).cloned().ok_or_else(|| {
                    "That commit isn't in the loaded history any more.".to_string()
                })?;
            let files = diff::commit_files(repo, sha)?;
            let pick = file
                .as_ref()
                .and_then(|f| files.iter().find(|c| &c.path == f))
                .or_else(|| files.first());
            let file = match pick {
                Some(c) => Some(diff::diff_commit_file(
                    repo,
                    sha,
                    &c.path,
                    c.old_path.as_deref(),
                    &opts,
                )?),
                None => None,
            };
            let selection = Selection::Commit {
                sha: sha.clone(),
                file: pick.map(|c| c.path.clone()),
            };
            Ok(DiffDoc {
                words: file.as_ref().map(word_ranges).unwrap_or_default(),
                selection,
                kind: DocKind::Commit {
                    entry: Box::new(entry),
                    files,
                },
                file,
                ignore_whitespace: opts.ignore_whitespace,
            })
        }
    }
}

fn word_ranges(file: &FileDiff) -> Vec<Vec<Option<Ranges>>> {
    file.hunks.iter().map(words::hunk_word_ranges).collect()
}
