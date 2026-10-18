//! Branch listing and switching — `git branch` plus `checkout`/`delete`.
//! Fields are separated with `%1f` so names containing spaces or punctuation
//! parse cleanly.

use std::path::Path;

use crate::git::Git;

/// `git branch --format` layout: name, upstream, short sha, date, HEAD marker.
const BRANCH_FMT: &str = "%(refname:short)%1f%(upstream:short)%1f%(objectname:short)%1f%(committerdate:iso8601)%1f%(HEAD)";

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    /// Tracking branch, e.g. `origin/main`, when set.
    pub upstream: Option<String>,
    /// Abbreviated tip SHA.
    pub sha: String,
    /// Whether this is the checked-out branch.
    pub is_current: bool,
}

/// All local branches, checked-out one first (git's own ordering keeps `*`
/// marked rows in name order; `is_current` flags it).
pub fn list_branches(repo: &Path) -> Result<Vec<BranchInfo>, String> {
    let out = Git::new(repo).run(["branch", &format!("--format={BRANCH_FMT}")])?;
    Ok(parse_branches(&out.stdout))
}

fn parse_branches(text: &str) -> Vec<BranchInfo> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\x1f').collect();
            if f.len() < 5 || f[0].is_empty() {
                return None;
            }
            Some(BranchInfo {
                name: f[0].to_string(),
                upstream: (!f[1].is_empty()).then(|| f[1].to_string()),
                sha: f[2].to_string(),
                is_current: f[4].trim() == "*",
            })
        })
        .collect()
}

/// The current branch name, or `None` when `HEAD` is detached.
pub fn current_branch(repo: &Path) -> Result<Option<String>, String> {
    let out = Git::new(repo).try_run(["symbolic-ref", "--short", "-q", "HEAD"])?;
    if !out.ok() {
        return Ok(None); // detached HEAD
    }
    Ok(Some(out.stdout.trim().to_string()))
}

/// `git branch <name> <base>` — creates without switching.
pub fn create_branch(repo: &Path, name: &str, base: &str) -> Result<(), String> {
    Git::new(repo).run(["branch", name, base])?;
    Ok(())
}

/// `git checkout <name>`. When the switch would clobber local changes, git's
/// own message ("Your local changes … would be overwritten") is already the
/// right user-facing error and comes through verbatim.
pub fn checkout(repo: &Path, name: &str) -> Result<(), String> {
    Git::new(repo).run(["checkout", name])?;
    Ok(())
}

/// `git branch -d`/`-D <name>`. `force` is required when the branch isn't
/// fully merged — git refuses otherwise, with its reason in the error.
pub fn delete_branch(repo: &Path, name: &str, force: bool) -> Result<(), String> {
    Git::new(repo).run(["branch", if force { "-D" } else { "-d" }, name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::stage::stage_all;
    use crate::testutil::{temp_repo, write_file};

    #[test]
    fn branch_lifecycle() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();

        assert_eq!(current_branch(dir.path()).unwrap().as_deref(), Some("main"));

        create_branch(dir.path(), "feature", "main").unwrap();
        let branches = list_branches(dir.path()).unwrap();
        assert_eq!(branches.len(), 2);
        let main = branches.iter().find(|b| b.name == "main").unwrap();
        assert!(main.is_current);
        assert_eq!(main.sha.len(), 7);

        checkout(dir.path(), "feature").unwrap();
        assert_eq!(
            current_branch(dir.path()).unwrap().as_deref(),
            Some("feature")
        );
        let branches = list_branches(dir.path()).unwrap();
        assert!(
            branches
                .iter()
                .find(|b| b.name == "feature")
                .unwrap()
                .is_current
        );

        checkout(dir.path(), "main").unwrap();
        delete_branch(dir.path(), "feature", false).unwrap();
        assert_eq!(list_branches(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn checkout_surfaces_clobber_error() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        create_branch(dir.path(), "other", "main").unwrap();

        // Diverge `other`, then dirty the same file on main.
        checkout(dir.path(), "other").unwrap();
        write_file(dir.path(), "a.txt", "other\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "other change", "").unwrap();
        checkout(dir.path(), "main").unwrap();
        write_file(dir.path(), "a.txt", "dirty local\n");

        let err = checkout(dir.path(), "other").unwrap_err();
        assert!(err.contains("overwritten") || err.contains("commit your changes"));
    }

    #[test]
    fn delete_unmerged_needs_force() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();

        create_branch(dir.path(), "wip", "main").unwrap();
        checkout(dir.path(), "wip").unwrap();
        write_file(dir.path(), "b.txt", "2\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "wip work", "").unwrap();
        checkout(dir.path(), "main").unwrap();

        assert!(delete_branch(dir.path(), "wip", false).is_err());
        delete_branch(dir.path(), "wip", true).unwrap();
        assert_eq!(list_branches(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn detached_head_has_no_branch() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        g.run(["checkout", "--detach", "HEAD"]).unwrap();
        assert_eq!(current_branch(dir.path()).unwrap(), None);
    }
}
