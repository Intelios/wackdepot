//! Stash operations — `git stash` list/push/apply/pop/drop.
//! `apply`/`pop`/`drop` take an optional stash ref (`stash@{0}` …); `None`
//! means the most recent stash.

use std::path::Path;

use crate::git::Git;

/// One stash entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    /// The ref name, e.g. `stash@{0}` — pass it to `stash_apply`/`stash_pop`.
    pub name: String,
    /// The reflog subject — "WIP on main: <sha> <subject>" or a custom message.
    pub message: String,
    /// The stash commit — stable while indices shift as stashes come and go.
    pub sha: String,
}

/// All stashes, newest first. An empty stash list is `Ok(vec![])`.
pub fn stash_list(repo: &Path) -> Result<Vec<StashEntry>, String> {
    let out = Git::new(repo).run(["stash", "list", "--format=%gd%x1f%H%x1f%gs"])?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| {
            let mut f = line.splitn(3, '\x1f');
            let name = f.next()?;
            let sha = f.next()?;
            let message = f.next().unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            Some(StashEntry {
                name: name.to_string(),
                message: message.to_string(),
                sha: sha.to_string(),
            })
        })
        .collect())
}

/// What to do with a stash picked from the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StashAction {
    Apply,
    Pop,
    Drop,
}

/// Apply, pop or drop the stash whose commit is `sha`, wherever it sits in
/// the list now — a stash pushed since the list was read shifts every
/// `stash@{n}`, and acting on a stale index would hit the wrong one.
pub fn stash_entry_action(repo: &Path, sha: &str, action: StashAction) -> Result<(), String> {
    let entry = stash_list(repo)?
        .into_iter()
        .find(|e| e.sha == sha)
        .ok_or_else(|| "That stash is gone — it may have been dropped already.".to_string())?;
    let verb = match action {
        StashAction::Apply => "apply",
        StashAction::Pop => "pop",
        StashAction::Drop => "drop",
    };
    Git::new(repo).run(["stash", verb, entry.name.as_str()])?;
    Ok(())
}

/// Stash tracked changes (`git stash push`; `-m` when a message is given).
/// Untracked files stay put — pass `include_untracked` to take them along.
pub fn stash_push(
    repo: &Path,
    message: Option<&str>,
    include_untracked: bool,
) -> Result<(), String> {
    let mut args = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    if let Some(m) = message {
        args.push("-m");
        args.push(m);
    }
    Git::new(repo).run(args)?;
    Ok(())
}

/// `git stash apply [stash]` — restores without dropping the entry.
pub fn stash_apply(repo: &Path, stash: Option<&str>) -> Result<(), String> {
    let mut args = vec!["stash", "apply"];
    if let Some(s) = stash {
        args.push(s);
    }
    Git::new(repo).run(args)?;
    Ok(())
}

/// `git stash pop [stash]` — restores and drops on a clean apply.
pub fn stash_pop(repo: &Path, stash: Option<&str>) -> Result<(), String> {
    let mut args = vec!["stash", "pop"];
    if let Some(s) = stash {
        args.push(s);
    }
    Git::new(repo).run(args)?;
    Ok(())
}

/// `git stash drop [stash]`.
pub fn stash_drop(repo: &Path, stash: Option<&str>) -> Result<(), String> {
    let mut args = vec!["stash", "drop"];
    if let Some(s) = stash {
        args.push(s);
    }
    Git::new(repo).run(args)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::stage::stage_all;
    use crate::status::status;
    use crate::testutil::{read_file, temp_repo, write_file};

    fn repo_with_commit() -> (tempfile::TempDir, Git) {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        (dir, g)
    }

    #[test]
    fn push_apply_drop() {
        let (dir, _g) = repo_with_commit();
        write_file(dir.path(), "a.txt", "dirty\n");

        stash_push(dir.path(), Some("save the dirt"), false).unwrap();
        assert_eq!(read_file(dir.path(), "a.txt"), "1\n");

        let list = stash_list(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "stash@{0}");
        assert!(list[0].message.contains("save the dirt"));

        stash_apply(dir.path(), Some("stash@{0}")).unwrap();
        assert_eq!(read_file(dir.path(), "a.txt"), "dirty\n");
        // Apply keeps the entry.
        assert_eq!(stash_list(dir.path()).unwrap().len(), 1);

        // Applying left a worktree change — put it back before dropping.
        write_file(dir.path(), "a.txt", "1\n");
        stash_drop(dir.path(), Some("stash@{0}")).unwrap();
        assert!(stash_list(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn push_and_pop() {
        let (dir, _g) = repo_with_commit();
        write_file(dir.path(), "a.txt", "dirty\n");
        stash_push(dir.path(), None, false).unwrap();
        assert_eq!(read_file(dir.path(), "a.txt"), "1\n");

        stash_pop(dir.path(), None).unwrap();
        assert_eq!(read_file(dir.path(), "a.txt"), "dirty\n");
        assert!(stash_list(dir.path()).unwrap().is_empty());
        assert_eq!(status(dir.path()).unwrap().unstaged.len(), 1);
    }

    #[test]
    fn push_including_untracked() {
        let (dir, _g) = repo_with_commit();
        write_file(dir.path(), "new.txt", "n\n");
        stash_push(dir.path(), None, true).unwrap();
        assert!(!dir.path().join("new.txt").exists());
        stash_pop(dir.path(), None).unwrap();
        assert!(dir.path().join("new.txt").exists());
    }

    #[test]
    fn act_on_an_older_stash_by_sha() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "f.txt", "base\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();

        write_file(dir.path(), "f.txt", "first\n");
        stash_push(dir.path(), Some("first"), false).unwrap();
        write_file(dir.path(), "f.txt", "second\n");
        stash_push(dir.path(), Some("second"), false).unwrap();

        let list = stash_list(dir.path()).unwrap();
        let older = list
            .iter()
            .find(|e| e.message.contains("first"))
            .unwrap()
            .clone();
        assert_eq!(older.name, "stash@{1}");
        stash_entry_action(dir.path(), &older.sha, StashAction::Pop).unwrap();
        assert_eq!(crate::testutil::read_file(dir.path(), "f.txt"), "first\n");

        let left = stash_list(dir.path()).unwrap();
        assert_eq!(left.len(), 1);
        assert!(left[0].message.contains("second"));
        assert!(stash_entry_action(dir.path(), &older.sha, StashAction::Drop).is_err());
    }
}
