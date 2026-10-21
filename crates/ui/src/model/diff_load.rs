//! Loading what the content card shows for a selection, off the UI thread.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use wackdepot_core::blob;
use wackdepot_core::commit::LogEntry;
use wackdepot_core::diff::{self, CommitFile, DiffFileStatus, DiffOptions, FileDiff};
use wackdepot_core::words::{self, Ranges};
use wackdepot_syntax::LineSpans;

use super::image::{ImageVersions, pair as image_pair};
use super::selection::Selection;
use super::snapshot::Bucket;

/// A loaded diff, ready to render.
#[derive(Debug)]
pub struct DiffDoc {
    pub selection: Selection,
    pub kind: DocKind,
    /// The file being shown (`None` for an empty commit).
    pub file: Option<FileDiff>,
    /// The ours↔theirs patch of a conflicted file (the merge's combined
    /// view); `None` unless the main file is `Conflicted` with both sides.
    pub sides: Option<SidesDoc>,
    /// Word-level emphasis per hunk, per line (aligned with `hunk.lines`).
    pub words: Vec<Vec<Option<Ranges>>>,
    /// Whitespace was ignored — hunks/lines can't be staged from this diff.
    pub ignore_whitespace: bool,
    /// Syntax spans for the whole old / new file, by 0-based line.
    pub old_hl: Option<Arc<Vec<LineSpans>>>,
    pub new_hl: Option<Arc<Vec<LineSpans>>>,
    /// Decoded images for a binary file: old ↔ new for a plain diff, or
    /// just the worktree copy (as `new`) for a conflicted file's marker
    /// view. `images.any() == false` → the binary stub shows.
    pub images: ImageVersions,
}

impl DiffDoc {
    /// Every decoded image this doc holds, for GPU-tile eviction.
    pub fn all_images(&self) -> impl Iterator<Item = &super::image::SideImage> {
        self.images.iter().chain(
            self.sides
                .as_ref()
                .into_iter()
                .flat_map(|s| s.images.iter()),
        )
    }

    /// Whether hunks and lines of this diff can be staged/discarded.
    /// Conflicted diffs show the file's markers, not a patch — there's
    /// nothing to apply, only "Mark resolved" on the whole file.
    pub fn stageable(&self) -> bool {
        matches!(self.kind, DocKind::Working { .. })
            && !self.ignore_whitespace
            && self
                .file
                .as_ref()
                .is_some_and(|f| !f.is_binary && f.status != DiffFileStatus::Conflicted)
    }

    /// Syntax spans for a diff line, from whichever side it lives on.
    pub fn spans(&self, line: &wackdepot_core::diff::DiffLine) -> Option<&LineSpans> {
        spans_from(&self.old_hl, &self.new_hl, line)
    }
}

/// Syntax spans for a diff line, from whichever side it lives on.
fn spans_from<'a>(
    old_hl: &'a Option<Arc<Vec<LineSpans>>>,
    new_hl: &'a Option<Arc<Vec<LineSpans>>>,
    line: &wackdepot_core::diff::DiffLine,
) -> Option<&'a LineSpans> {
    use wackdepot_core::diff::DiffLineKind;
    let (hl, n) = match line.kind {
        DiffLineKind::Removed => (old_hl.as_ref()?, line.old_lineno?),
        // A conflicted diff's lines all carry worktree numbers.
        DiffLineKind::Added
        | DiffLineKind::Context
        | DiffLineKind::Ours
        | DiffLineKind::Base
        | DiffLineKind::Theirs => (new_hl.as_ref()?, line.new_lineno?),
        DiffLineKind::NoNewline | DiffLineKind::Marker => return None,
    };
    hl.get(n.checked_sub(1)? as usize)
}

/// The ours↔theirs view of a conflicted file, with its own highlighting:
/// `old` is stage 2 (ours), `new` is stage 3 (theirs).
#[derive(Debug)]
pub struct SidesDoc {
    pub file: FileDiff,
    pub words: Vec<Vec<Option<Ranges>>>,
    pub old_hl: Option<Arc<Vec<LineSpans>>>,
    pub new_hl: Option<Arc<Vec<LineSpans>>>,
    /// The two index stages decoded as images, for a conflicted image file.
    pub images: ImageVersions,
}

impl SidesDoc {
    pub fn spans(&self, line: &wackdepot_core::diff::DiffLine) -> Option<&LineSpans> {
        spans_from(&self.old_hl, &self.new_hl, line)
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

fn highlight_versions(path: &str, v: blob::Versions) -> (Option<Highlight>, Option<Highlight>) {
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
            let old_path = file.old_path.as_deref().filter(|o| *o != path.as_str());
            let (old_hl, new_hl) = if file.is_binary {
                (None, None)
            } else {
                highlight_versions(
                    path,
                    blob::file_versions(repo, path, old_path, bucket.side()),
                )
            };
            // Binary files carry raw bytes instead of hunks — try decoding
            // them as images. A conflicted file's two-way diff is index
            // stage vs. worktree, so its marker view shows what's on disk;
            // ours↔theirs belongs to the sides doc below.
            let conflicted = file.status == DiffFileStatus::Conflicted;
            let images = if file.is_binary {
                let v = if conflicted {
                    blob::worktree_byte_versions(repo, path)
                } else {
                    blob::file_byte_versions(repo, path, old_path, bucket.side())
                };
                image_pair(&v)
            } else {
                ImageVersions::default()
            };
            // A conflicted file's other view: ours↔theirs straight from the
            // index stages, highlighted (or decoded) from the stage blobs.
            let sides = if conflicted {
                diff::sides_diff(repo, path)?.map(|d| {
                    let (old, new) = highlight_versions(path, blob::conflict_versions(repo, path));
                    let images = if d.is_binary {
                        image_pair(&blob::conflict_byte_versions(repo, path))
                    } else {
                        ImageVersions::default()
                    };
                    SidesDoc {
                        words: word_ranges(&d),
                        file: d,
                        old_hl: old,
                        new_hl: new,
                        images,
                    }
                })
            } else {
                None
            };
            Ok(DiffDoc {
                sides,
                words: word_ranges(&file),
                selection: sel.clone(),
                kind: DocKind::Working { bucket: *bucket },
                file: Some(file),
                ignore_whitespace: opts.ignore_whitespace,
                old_hl,
                new_hl,
                images,
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
            let images =
                match (pick, &file) {
                    (Some(c), Some(f)) if f.is_binary => image_pair(
                        &blob::commit_file_byte_versions(repo, sha, &c.path, c.old_path.as_deref()),
                    ),
                    _ => ImageVersions::default(),
                };
            Ok(DiffDoc {
                sides: None,
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
                images,
            })
        }
    }
}

fn word_ranges(file: &FileDiff) -> Vec<Vec<Option<Ranges>>> {
    file.hunks.iter().map(words::hunk_word_ranges).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::selection::Selection;
    use wackdepot_core::Git;

    /// A real repo in a tempdir, like core's `testutil` (crate-private).
    fn temp_repo() -> (tempfile::TempDir, Git) {
        let dir = tempfile::tempdir().unwrap();
        let g = Git::new(dir.path());
        g.run(["init", "-b", "main"]).unwrap();
        g.run(["config", "user.name", "WackDepot Test"]).unwrap();
        g.run(["config", "user.email", "test@wackdepot.local"])
            .unwrap();
        (dir, g)
    }

    fn png(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(w, h, image::Rgba(c)))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn a_changed_png_loads_as_an_image_diff() {
        let (dir, g) = temp_repo();
        std::fs::write(dir.path().join("pic.png"), png(8, 8, [255, 0, 0, 255])).unwrap();
        g.run(["add", "pic.png"]).unwrap();
        wackdepot_core::commit::commit(dir.path(), "add pic", "").unwrap();
        std::fs::write(dir.path().join("pic.png"), png(16, 4, [0, 0, 255, 255])).unwrap();

        let doc = load(
            dir.path(),
            &Selection::File {
                path: "pic.png".into(),
                bucket: Bucket::Unstaged,
            },
            &[],
            DiffOptions::default(),
        )
        .unwrap();
        let file = doc.file.as_ref().unwrap();
        assert!(file.is_binary);
        assert!(doc.images.any());
        assert_eq!(doc.images.old.as_ref().unwrap().dims, (8, 8));
        assert_eq!(doc.images.new.as_ref().unwrap().dims, (16, 4));

        // The same file seen through history diffs the same way.
        let log = wackdepot_core::commit::list_log(dir.path(), 5).unwrap();
        let sha = log[0].sha.clone();
        let doc = load(
            dir.path(),
            &Selection::Commit { sha, file: None },
            &log,
            DiffOptions::default(),
        )
        .unwrap();
        assert!(doc.images.any());
        assert!(doc.images.old.is_none()); // added in this commit
        assert_eq!(doc.images.new.as_ref().unwrap().dims, (8, 8));
    }

    #[test]
    fn an_untrackable_binary_stays_a_stub() {
        let (dir, _g) = temp_repo();
        // NUL bytes → binary, and nothing that decodes to an image.
        std::fs::write(dir.path().join("data.bin"), [0u8, 1, 2, 0, 3, 7]).unwrap();
        let doc = load(
            dir.path(),
            &Selection::File {
                path: "data.bin".into(),
                bucket: Bucket::Untracked,
            },
            &[],
            DiffOptions::default(),
        )
        .unwrap();
        assert!(doc.file.as_ref().unwrap().is_binary);
        assert!(!doc.images.any());
    }
}
