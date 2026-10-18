//! Instant feedback for staging: move a file between buckets in the
//! snapshot right away, so the row starts gliding on the click frame. The
//! real refresh after the op settles the truth (and corrects any guess).

use wackdepot_core::status::{ChangeKind, FileChange, RepoStatus};

use super::snapshot::Bucket;

/// `status` as it will look once `path` (in `from`) is staged or unstaged.
pub fn toggle(status: &RepoStatus, path: &str, from: Bucket) -> RepoStatus {
    let mut s = status.clone();
    let take = |list: &mut Vec<FileChange>| -> Option<FileChange> {
        let i = list.iter().position(|f| f.path == path)?;
        Some(list.remove(i))
    };
    match from {
        Bucket::Unstaged | Bucket::Untracked | Bucket::Conflicted => {
            let list = match from {
                Bucket::Unstaged => &mut s.unstaged,
                Bucket::Untracked => &mut s.untracked,
                _ => &mut s.conflicted,
            };
            if let Some(mut f) = take(list) {
                if from == Bucket::Untracked {
                    f.kind = ChangeKind::Added;
                    f.x = 'A';
                } else if f.x == '.' {
                    f.x = f.y;
                }
                f.y = '.';
                // A file both staged and unstaged merges into one staged row.
                s.staged.retain(|g| g.path != f.path);
                let at = s.staged.partition_point(|g| g.path < f.path);
                s.staged.insert(at, f);
            }
        }
        Bucket::Staged => {
            if let Some(mut f) = take(&mut s.staged) {
                if f.x == 'A' {
                    f.x = '?';
                    f.y = '?';
                    let at = s.untracked.partition_point(|g| g.path < f.path);
                    s.untracked.insert(at, f);
                } else {
                    f.y = f.x;
                    f.x = '.';
                    if !s.unstaged.iter().any(|g| g.path == f.path) {
                        let at = s.unstaged.partition_point(|g| g.path < f.path);
                        s.unstaged.insert(at, f);
                    }
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fc(path: &str, x: char, y: char) -> FileChange {
        FileChange {
            path: path.into(),
            orig_path: None,
            x,
            y,
            kind: ChangeKind::Modified,
        }
    }

    #[test]
    fn stage_and_unstage_move_rows() {
        let s = RepoStatus {
            unstaged: vec![fc("a", '.', 'M'), fc("b", '.', 'M')],
            ..Default::default()
        };
        let staged = toggle(&s, "b", Bucket::Unstaged);
        assert_eq!(staged.unstaged.len(), 1);
        assert_eq!(staged.staged[0].path, "b");
        assert_eq!(staged.staged[0].x, 'M');
        let back = toggle(&staged, "b", Bucket::Staged);
        assert_eq!(
            back.unstaged
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        let u = RepoStatus {
            untracked: vec![fc("n", '?', '?')],
            ..Default::default()
        };
        let s = toggle(&u, "n", Bucket::Untracked);
        assert_eq!(s.staged[0].x, 'A');
        assert_eq!(toggle(&s, "n", Bucket::Staged).untracked.len(), 1);
    }
}
