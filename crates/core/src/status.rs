//! `git status --porcelain=v2 -z --branch` → [`RepoStatus`].
//!
//! Porcelain v2 meanings: in an `XY` pair, `X` is index-vs-HEAD (staged) and
//! `Y` is worktree-vs-index (unstaged). Records starting `u` are unmerged
//! paths (merge conflicts), `?` are untracked. The `merge_in_progress` flag is
//! folded in via [`is_merging`]; [`abort_merge`] lives here too so all
//! merge-state handling sits in one module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::git::Git;

/// What kind of change a file carries, derived from the porcelain XY code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    /// Unmerged — both sides of a merge touched it (or worse).
    Unmerged,
}

impl ChangeKind {
    /// Map a single X or Y status column character to a kind.
    fn from_xy(c: char) -> Self {
        match c {
            'A' => Self::Added,
            'M' => Self::Modified,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            'U' => Self::Unmerged,
            _ => Self::Modified,
        }
    }
}

/// One file in the changes list. The same `FileChange` may appear in both
/// `staged` and `unstaged` when both columns are set (`MM`); `kind` then
/// describes the staged side — check `x`/`y` or the bucket to tell sides apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path relative to the repo root.
    pub path: String,
    /// Original path for renames/copies.
    pub orig_path: Option<String>,
    /// Index-vs-HEAD column (`.` when nothing is staged).
    pub x: char,
    /// Worktree-vs-index column (`.` when the worktree matches the index).
    pub y: char,
    pub kind: ChangeKind,
}

impl FileChange {
    /// Whether the staged column reports a change.
    pub fn is_staged(&self) -> bool {
        self.x != '.'
    }

    /// Whether the worktree column reports a change.
    pub fn is_unstaged(&self) -> bool {
        self.y != '.'
    }
}

/// The full changes-view snapshot for one repository.
#[derive(Debug, Clone, Default)]
pub struct RepoStatus {
    /// Current branch name, or `None` when detached.
    pub branch: Option<String>,
    /// Tracking ref, e.g. `origin/main`, when the branch has one.
    pub upstream: Option<String>,
    /// Commits ahead of upstream.
    pub ahead: u32,
    /// Commits behind upstream.
    pub behind: u32,
    /// Commit OID when `HEAD` is detached (`branch` is `None` then).
    pub detached_at: Option<String>,
    /// `.git/MERGE_HEAD` exists — a merge is waiting on resolution/commit.
    pub merge_in_progress: bool,
    /// The multi-step operation waiting on the user, if any (merge, rebase,
    /// cherry-pick, revert) — drives the banner above the changes list.
    pub operation: Option<Operation>,
    /// Unmerged paths from a merge in progress.
    pub conflicted: Vec<FileChange>,
    /// Changes in the index (X column set).
    pub staged: Vec<FileChange>,
    /// Worktree changes not yet staged (Y column set).
    pub unstaged: Vec<FileChange>,
    /// Files git doesn't know about.
    pub untracked: Vec<FileChange>,
}

/// Snapshot a repo's status: branch/ahead-behind info plus every changed file
/// bucketed into `conflicted` / `staged` / `unstaged` / `untracked`.
pub fn status(repo: &Path) -> Result<RepoStatus, String> {
    // --no-optional-locks: a background status must never take index.lock
    // and race a stage/commit the user just clicked.
    let out = Git::new(repo).run([
        "--no-optional-locks",
        "status",
        "--porcelain=v2",
        "-z",
        "--branch",
        "--untracked-files=normal",
    ])?;
    let mut s = parse_status_porcelain(&out.stdout);
    s.operation = operation(repo);
    s.merge_in_progress = s.operation == Some(Operation::Merge);
    Ok(s)
}

/// A git operation that stopped half-way and needs the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl Operation {
    /// The git subcommand that continues or aborts it.
    fn command(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }
}

/// Which operation, if any, is in progress — read from the marker files git
/// leaves in the git dir.
pub fn operation(repo: &Path) -> Option<Operation> {
    let dir = git_dir(repo)?;
    if dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists() {
        Some(Operation::Rebase)
    } else if dir.join("MERGE_HEAD").exists() {
        Some(Operation::Merge)
    } else if dir.join("CHERRY_PICK_HEAD").exists() {
        Some(Operation::CherryPick)
    } else if dir.join("REVERT_HEAD").exists() {
        Some(Operation::Revert)
    } else {
        None
    }
}

/// `git <op> --abort` — back to where things were before it started.
pub fn abort_operation(repo: &Path, op: Operation) -> Result<(), String> {
    Git::new(repo).run([op.command(), "--abort"])?;
    Ok(())
}

/// `git <op> --continue` once conflicts are resolved and staged. The editor
/// is pinned to `true` so git keeps its prepared message instead of opening
/// one (there's no tty in a GUI).
pub fn continue_operation(repo: &Path, op: Operation) -> Result<(), String> {
    let g = Git::new(repo);
    if op == Operation::Merge {
        g.run(["-c", "core.editor=true", "commit", "--no-edit"])?;
    } else {
        g.run(["-c", "core.editor=true", op.command(), "--continue"])?;
    }
    Ok(())
}

/// Lines added and removed per path, for the `+n −m` on each changes row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineStats {
    /// Worktree vs index, plus untracked files counted from disk.
    pub unstaged: HashMap<String, (u32, u32)>,
    /// Index vs HEAD.
    pub staged: HashMap<String, (u32, u32)>,
}

/// `git diff --numstat` for both sides. Binary files are left out. Untracked
/// text files (under 512KB) count every line as added.
pub fn line_stats(repo: &Path, untracked: &[FileChange]) -> Result<LineStats, String> {
    let g = Git::new(repo);
    let unstaged = g.run(["--no-optional-locks", "diff", "--numstat", "-z", "-M"])?;
    let staged = g.run([
        "--no-optional-locks",
        "diff",
        "--cached",
        "--numstat",
        "-z",
        "-M",
    ])?;
    let mut stats = LineStats {
        unstaged: parse_numstat(&unstaged.stdout),
        staged: parse_numstat(&staged.stdout),
    };
    for f in untracked {
        let path = repo.join(&f.path);
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() || meta.len() > 512 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.iter().take(8192).any(|&b| b == 0) {
            continue;
        }
        // Same count as `str::lines`: every newline, plus an unterminated tail.
        let lines = bytes.iter().filter(|&&b| b == b'\n').count()
            + usize::from(bytes.last().is_some_and(|&b| b != b'\n'));
        stats.unstaged.insert(f.path.clone(), (lines as u32, 0));
    }
    Ok(stats)
}

/// Parse `--numstat -z`: `a\tr\tpath\0`, or `a\tr\t\0old\0new\0` for renames.
/// Binary entries (`-\t-`) are skipped.
fn parse_numstat(raw: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    let mut parts = raw.split('\0');
    while let Some(head) = parts.next() {
        if head.is_empty() {
            continue;
        }
        let mut cols = head.splitn(3, '\t');
        let added = cols.next().and_then(|a| a.parse().ok());
        let removed = cols.next().and_then(|r| r.parse().ok());
        let mut path = cols.next().unwrap_or("").to_string();
        if path.is_empty() {
            let _old = parts.next();
            path = parts.next().unwrap_or("").to_string();
        }
        if let (Some(a), Some(r)) = (added, removed) {
            map.insert(path, (a, r));
        }
    }
    map
}

/// Parse the raw `-z` output of `git status --porcelain=v2 --branch`.
/// Pure — no repo access; `merge_in_progress` stays `false` (see [`status`]).
pub fn parse_status_porcelain(raw: &str) -> RepoStatus {
    let mut s = RepoStatus::default();
    let mut oid: Option<String> = None;

    // `-z` terminates every record with NUL; for `2 `/`C ` rename/copy records
    // the origPath follows as its own NUL-terminated field.
    let mut records = raw.split('\0').peekable();
    while let Some(rec) = records.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(header) = rec.strip_prefix("# ") {
            parse_branch_header(header, &mut s, &mut oid);
            continue;
        }
        match rec.as_bytes()[0] {
            b'?' => {
                s.untracked.push(FileChange {
                    path: rec[2..].to_string(),
                    orig_path: None,
                    x: '?',
                    y: '?',
                    kind: ChangeKind::Added,
                });
            }
            b'!' => {} // ignored files aren't requested; skip anyway
            b'u' => {
                // u <xy> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
                let fields: Vec<&str> = rec.splitn(11, ' ').collect();
                if fields.len() == 11 {
                    let (x, y) = xy(fields[1]);
                    s.conflicted.push(FileChange {
                        path: fields[10].to_string(),
                        orig_path: None,
                        x,
                        y,
                        kind: ChangeKind::Unmerged,
                    });
                }
            }
            b'1' | b'2' => {
                // 1 <xy> <sub> <mH> <mI> <mW> <hH> <hI> <path>   → path is field 8
                // 2 <xy> <sub> <mH> <mI> <mW> <hH> <hI> <R100> <path>\0<origPath>  → field 9
                let path_at = if rec.as_bytes()[0] == b'1' { 8 } else { 9 };
                let fields: Vec<&str> = rec.splitn(path_at + 1, ' ').collect();
                if fields.len() <= path_at {
                    continue;
                }
                let (x, y) = xy(fields[1]);
                let orig_path = if rec.as_bytes()[0] == b'2' {
                    records.next().map(str::to_string)
                } else {
                    None
                };
                let change = FileChange {
                    path: fields[path_at].to_string(),
                    orig_path,
                    x,
                    y,
                    kind: ChangeKind::from_xy(if x != '.' { x } else { y }),
                };
                if change.is_staged() {
                    s.staged.push(change.clone());
                }
                if change.is_unstaged() {
                    s.unstaged.push(change);
                }
            }
            _ => {}
        }
    }

    if s.branch.is_none() {
        s.detached_at = oid;
    }
    s
}

/// `# branch.<key> <value>` headers. `oid` is squirrelled away separately
/// because `branch.head (detached)` only makes it interesting then.
fn parse_branch_header(header: &str, s: &mut RepoStatus, oid: &mut Option<String>) {
    let Some((key, value)) = header.split_once(' ') else {
        return;
    };
    match key {
        "branch.oid" => *oid = Some(value.to_string()),
        "branch.head" => {
            if value != "(detached)" {
                s.branch = Some(value.to_string());
            }
        }
        "branch.upstream" => s.upstream = Some(value.to_string()),
        "branch.ab" => {
            for part in value.split(' ') {
                if let Some(n) = part.strip_prefix('+') {
                    s.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    s.behind = n.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

fn xy(field: &str) -> (char, char) {
    let mut chars = field.chars();
    (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'))
}

/// Whether `repo` is mid-merge (`MERGE_HEAD` exists). Resolves the real path
/// through `git rev-parse --absolute-git-dir` so linked worktrees (where
/// `.git` is a file and the common dir is shared) work.
pub fn is_merging(repo: &Path) -> bool {
    merge_head_path(repo).is_some_and(|p| p.exists())
}

/// The repo-specific `MERGE_HEAD` path, or `None` if it can't be resolved.
fn merge_head_path(repo: &Path) -> Option<PathBuf> {
    Some(git_dir(repo)?.join("MERGE_HEAD"))
}

/// This worktree's own git dir (`.git`, or `.git/worktrees/<name>` for a
/// linked worktree).
fn git_dir(repo: &Path) -> Option<PathBuf> {
    let out = Git::new(repo)
        .try_run(["rev-parse", "--absolute-git-dir"])
        .ok()?;
    if !out.ok() {
        return None;
    }
    Some(PathBuf::from(out.stdout.trim()))
}

/// Bail out of an in-progress merge — `git merge --abort`.
pub fn abort_merge(repo: &Path) -> Result<(), String> {
    Git::new(repo).run(["merge", "--abort"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::stage::mark_resolved;
    use crate::testutil::{git_out, temp_repo, write_file};

    const FIXTURE: &str = "# branch.oid abcd1234abcd1234abcd1234abcd1234abcd1234\0\
        # branch.head main\0\
        # branch.upstream origin/main\0\
        # branch.ab +2 -1\0\
        1 M. N... 100644 100644 100644 h1 h1 staged.txt\0\
        1 .M N... 100644 100644 100644 h1 h1 unstaged.txt\0\
        1 MM N... 100644 100644 100644 h1 h1 both.txt\0\
        2 R. N... 100644 100644 100644 h1 h1 R100 new name.txt\0old name.txt\0\
        u AA N... 000000 100644 100644 100644 h0 h1 h2 conflict.txt\0\
        ? brand new.txt\0";

    #[test]
    fn parses_all_record_kinds() {
        let s = parse_status_porcelain(FIXTURE);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert!(s.detached_at.is_none());

        assert_eq!(
            s.staged.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["staged.txt", "both.txt", "new name.txt"]
        );
        assert_eq!(
            s.unstaged
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["unstaged.txt", "both.txt"]
        );
        let rename = &s.staged[2];
        assert_eq!(rename.kind, ChangeKind::Renamed);
        assert_eq!(rename.orig_path.as_deref(), Some("old name.txt"));

        assert_eq!(s.conflicted.len(), 1);
        assert_eq!(s.conflicted[0].kind, ChangeKind::Unmerged);
        assert_eq!(s.conflicted[0].path, "conflict.txt");

        assert_eq!(s.untracked.len(), 1);
        assert_eq!(s.untracked[0].path, "brand new.txt");
        assert_eq!(s.untracked[0].kind, ChangeKind::Added);
    }

    #[test]
    fn parses_detached_head() {
        let s = parse_status_porcelain(
            "# branch.oid ebc181c09da1606761a22ffc31da30739ef36e04\0# branch.head (detached)\0",
        );
        assert!(s.branch.is_none());
        assert_eq!(
            s.detached_at.as_deref(),
            Some("ebc181c09da1606761a22ffc31da30739ef36e04")
        );
    }

    #[test]
    fn live_status_flow() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "hello.txt", "hi\n");

        let s = status(dir.path()).unwrap();
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.untracked.len(), 1);
        assert_eq!(s.untracked[0].path, "hello.txt");

        g.run(["add", "--", "hello.txt"]).unwrap();
        let s = status(dir.path()).unwrap();
        assert!(s.untracked.is_empty());
        assert_eq!(s.staged.len(), 1);
        assert_eq!(s.staged[0].kind, ChangeKind::Added);

        commit(dir.path(), "add hello", "").unwrap();
        write_file(dir.path(), "hello.txt", "hi\nmore\n");
        let s = status(dir.path()).unwrap();
        assert_eq!(s.unstaged.len(), 1);
        assert_eq!(s.unstaged[0].kind, ChangeKind::Modified);
    }

    #[test]
    fn merge_conflict_flow() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "c.txt", "base\n");
        g.run(["add", "c.txt"]).unwrap();
        commit(dir.path(), "base", "").unwrap();

        g.run(["checkout", "-b", "side"]).unwrap();
        write_file(dir.path(), "c.txt", "side\n");
        g.run(["add", "c.txt"]).unwrap();
        commit(dir.path(), "side", "").unwrap();

        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "c.txt", "main\n");
        g.run(["add", "c.txt"]).unwrap();
        commit(dir.path(), "main", "").unwrap();

        // The merge fails — git reports the conflict and leaves MERGE_HEAD.
        assert!(git_out(&g, ["merge", "side"]).status != 0);

        let s = status(dir.path()).unwrap();
        assert!(s.merge_in_progress);
        assert!(is_merging(dir.path()));
        assert_eq!(s.conflicted.len(), 1);
        assert_eq!(s.conflicted[0].path, "c.txt");

        // Resolve by picking a side, then git add marks it resolved.
        write_file(dir.path(), "c.txt", "resolved\n");
        mark_resolved(dir.path(), "c.txt").unwrap();
        let s = status(dir.path()).unwrap();
        assert!(s.conflicted.is_empty());
        assert_eq!(s.staged.len(), 1);

        abort_merge(dir.path()).unwrap();
        let s = status(dir.path()).unwrap();
        assert!(!s.merge_in_progress);
        assert!(s.staged.is_empty() && s.unstaged.is_empty() && s.conflicted.is_empty());
    }

    #[test]
    fn is_merging_handles_worktrees() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "x\n");
        g.run(["add", "f.txt"]).unwrap();
        commit(dir.path(), "init", "").unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let wt = scratch.path().join("wt-linked");
        g.run(["worktree", "add", wt.to_str().unwrap(), "-b", "wtb"])
            .unwrap();
        assert!(!is_merging(&wt));
        // A merge in the linked worktree sets MERGE_HEAD in its private git dir.
        let wg = Git::new(&wt);
        write_file(&wt, "c.txt", "one\n");
        wg.run(["add", "c.txt"]).unwrap();
        crate::commit::commit(&wt, "wt commit", "").unwrap();
        wg.run(["checkout", "-b", "other"]).unwrap();
        write_file(&wt, "c.txt", "two\n");
        wg.run(["add", "c.txt"]).unwrap();
        crate::commit::commit(&wt, "other", "").unwrap();
        wg.run(["checkout", "wtb"]).unwrap();
        write_file(&wt, "c.txt", "three\n");
        wg.run(["add", "c.txt"]).unwrap();
        crate::commit::commit(&wt, "wtb", "").unwrap();
        assert!(git_out(&wg, ["merge", "other"]).status != 0);
        assert!(is_merging(&wt));
        wg.run(["merge", "--abort"]).unwrap();
    }

    #[test]
    fn line_stats_cover_both_sides_and_untracked() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n2\n");
        g.run(["add", "a.txt"]).unwrap();
        commit(dir.path(), "init", "").unwrap();
        write_file(dir.path(), "a.txt", "1\nTWO\n3\n");
        g.run(["add", "a.txt"]).unwrap();
        write_file(dir.path(), "a.txt", "1\nTWO\n3\n4\n");
        write_file(dir.path(), "new.txt", "x\ny\nz");
        let st = status(dir.path()).unwrap();
        let stats = line_stats(dir.path(), &st.untracked).unwrap();
        assert_eq!(stats.staged.get("a.txt"), Some(&(2, 1)));
        assert_eq!(stats.unstaged.get("a.txt"), Some(&(1, 0)));
        assert_eq!(stats.unstaged.get("new.txt"), Some(&(3, 0)));
    }
}
