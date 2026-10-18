//! Loading what the content card shows for a selection, off the UI thread.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use wackdepot_core::blob;
use wackdepot_core::commit::LogEntry;
use wackdepot_core::diff::{self, CommitFile, DiffOptions, FileDiff};
use wackdepot_core::words::{self, Ranges};
use wackdepot_syntax::LineSpans;

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
    /// Syntax spans for the whole old / new file, by 0-based line.
    pub old_hl: Option<Arc<Vec<LineSpans>>>,
    pub new_hl: Option<Arc<Vec<LineSpans>>>,
}

impl DiffDoc {
    /// Whether hunks and lines of this diff can be staged/discarded.
    pub fn stageable(&self) -> bool {
        matches!(self.kind, DocKind::Working { .. })
            && !self.ignore_whitespace
            && self.file.as_ref().is_some_and(|f| !f.is_binary)
    }

    /// Syntax spans for a diff line, from whichever side it lives on.
    pub fn spans(&self, line: &wackdepot_core::diff::DiffLine) -> Option<&LineSpans> {
        use wackdepot_core::diff::DiffLineKind;
        let (hl, n) = match line.kind {
            DiffLineKind::Removed => (self.old_hl.as_ref()?, line.old_lineno?),
            DiffLineKind::Added | DiffLineKind::Context => {
                (self.new_hl.as_ref()?, line.new_lineno?)
            }
            DiffLineKind::NoNewline => return None,
        };
        hl.get(n.checked_sub(1)? as usize)
    }
}

/// A whole file's syntax spans, shared between diffs.
type Highlight = Arc<Vec<LineSpans>>;

/// Recently highlighted texts, keyed by content hash and file name.
fn highlight_cached(path: &str, text: &str) -> Option<Arc<Vec<LineSpans>>> {
    static CACHE: OnceLock<Mutex<Vec<(u64, Highlight)>>> = OnceLock::new();
    let mut h = DefaultHasher::new();
    path.rsplit('.').next().hash(&mut h);
    text.hash(&mut h);
    let key = h.finish();
    let cache = CACHE.get_or_init(Mutex::default);
    if let Ok(c) = cache.lock()
        && let Some((_, hl)) = c.iter().find(|(k, _)| *k == key)
    {
        return Some(hl.clone());
    }
    let hl = Arc::new(wackdepot_syntax::highlight(path, text)?);
    if let Ok(mut c) = cache.lock() {
        c.push((key, hl.clone()));
        if c.len() > 32 {
            c.remove(0);
        }
    }
    Some(hl)
}

fn highlight_versions(
    path: &str,
    v: blob::Versions,
) -> (Option<Highlight>, Option<Highlight>) {
    (
        v.old.as_deref().and_then(|t| highlight_cached(path, t)),
        v.new.as_deref().and_then(|t| highlight_cached(path, t)),
    )
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
            let (old_hl, new_hl) = if file.is_binary {
                (None, None)
            } else {
                let old_path = file.old_path.as_deref().filter(|o| *o != path.as_str());
                highlight_versions(
                    path,
                    blob::file_versions(repo, path, old_path, bucket.side()),
                )
            };
            Ok(DiffDoc {
                words: word_ranges(&file),
                selection: sel.clone(),
                kind: DocKind::Working { bucket: *bucket },
                file: Some(file),
                ignore_whitespace: opts.ignore_whitespace,
                old_hl,
                new_hl,
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
            let (old_hl, new_hl) = match (pick, &file) {
                (Some(c), Some(f)) if !f.is_binary => highlight_versions(
                    &c.path,
                    blob::commit_file_versions(repo, sha, &c.path, c.old_path.as_deref()),
                ),
                _ => (None, None),
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
                old_hl,
                new_hl,
            })
        }
    }
}

fn word_ranges(file: &FileDiff) -> Vec<Vec<Option<Ranges>>> {
    file.hunks.iter().map(words::hunk_word_ranges).collect()
}
