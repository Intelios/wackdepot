//! What's selected, keyed by path or SHA (never by list index, which shifts
//! every time a file is staged), and how it survives a refresh.

use super::snapshot::{Bucket, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// A working-tree file in one bucket of the Changes list.
    File { path: String, bucket: Bucket },
    /// A commit in History, and optionally one of its files.
    Commit { sha: String, file: Option<String> },
}

/// Changes / History.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Changes,
    History,
    PullRequests,
}

impl Tab {
    pub fn index(self) -> usize {
        match self {
            Tab::Changes => 0,
            Tab::History => 1,
            Tab::PullRequests => 2,
        }
    }
}

/// Carry `sel` from `prev` into `next`:
/// - a file still in its bucket stays put;
/// - a file that moved buckets (staged, unstaged) is followed;
/// - a file that vanished hands the selection to its neighbour in the same
///   bucket, then to the first file anywhere;
/// - a commit stays while its SHA is in the log.
pub fn repair(sel: &Selection, prev: &Snapshot, next: &Snapshot) -> Option<Selection> {
    match sel {
        Selection::File { path, bucket } => {
            if next.bucket(*bucket).iter().any(|f| &f.path == path) {
                return Some(sel.clone());
            }
            for b in follow_order(*bucket) {
                if next.bucket(b).iter().any(|f| &f.path == path) {
                    return Some(Selection::File {
                        path: path.clone(),
                        bucket: b,
                    });
                }
            }
            let old_index = prev.bucket(*bucket).iter().position(|f| &f.path == path);
            let same = next.bucket(*bucket);
            if let (Some(i), false) = (old_index, same.is_empty()) {
                let f = &same[i.min(same.len() - 1)];
                return Some(Selection::File {
                    path: f.path.clone(),
                    bucket: *bucket,
                });
            }
            first_file(next)
        }
        Selection::Commit { sha, .. } => {
            next.log.iter().any(|c| &c.sha == sha).then(|| sel.clone())
        }
    }
}

/// The first file in display order, if any.
pub fn first_file(s: &Snapshot) -> Option<Selection> {
    Bucket::ALL.iter().find_map(|b| {
        s.bucket(*b).first().map(|f| Selection::File {
            path: f.path.clone(),
            bucket: *b,
        })
    })
}

/// Where a file most likely went: staging moves Unstaged/Untracked → Staged,
/// unstaging moves Staged → Unstaged (or Untracked for a new file).
fn follow_order(from: Bucket) -> [Bucket; 3] {
    match from {
        Bucket::Staged => [Bucket::Unstaged, Bucket::Untracked, Bucket::Conflicted],
        Bucket::Unstaged => [Bucket::Staged, Bucket::Untracked, Bucket::Conflicted],
        Bucket::Untracked => [Bucket::Staged, Bucket::Unstaged, Bucket::Conflicted],
        Bucket::Conflicted => [Bucket::Staged, Bucket::Unstaged, Bucket::Untracked],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wackdepot_core::status::{ChangeKind, FileChange, RepoStatus};

    fn fc(path: &str) -> FileChange {
        FileChange {
            path: path.into(),
            orig_path: None,
            x: 'M',
            y: '.',
            kind: ChangeKind::Modified,
        }
    }

    fn snap(staged: &[&str], unstaged: &[&str]) -> Snapshot {
        Snapshot {
            status: Some(RepoStatus {
                staged: staged.iter().map(|p| fc(p)).collect(),
                unstaged: unstaged.iter().map(|p| fc(p)).collect(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn file(path: &str, bucket: Bucket) -> Selection {
        Selection::File {
            path: path.into(),
            bucket,
        }
    }

    #[test]
    fn follows_a_file_across_buckets() {
        let prev = snap(&[], &["a", "b"]);
        let next = snap(&["a"], &["b"]);
        assert_eq!(
            repair(&file("a", Bucket::Unstaged), &prev, &next),
            Some(file("a", Bucket::Staged))
        );
    }

    #[test]
    fn hands_off_to_the_neighbour_then_anything() {
        let prev = snap(&[], &["a", "b", "c"]);
        let next = snap(&[], &["a", "c"]);
        assert_eq!(
            repair(&file("b", Bucket::Unstaged), &prev, &next),
            Some(file("c", Bucket::Unstaged))
        );
        let next = snap(&["z"], &[]);
        assert_eq!(
            repair(&file("b", Bucket::Unstaged), &prev, &next),
            Some(file("z", Bucket::Staged))
        );
        assert_eq!(
            repair(&file("b", Bucket::Unstaged), &prev, &snap(&[], &[])),
            None
        );
    }
}
