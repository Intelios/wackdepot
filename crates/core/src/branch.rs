//! Branch listing and switching — `git branch` plus `checkout`/`delete`.
//! Fields are separated with `%1f` so names containing spaces or punctuation
//! parse cleanly.

use std::collections::HashMap;
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

/// Remote-tracking branches (`git branch -r`): `origin/main` & friends.
/// Symbolic tips like `origin/HEAD` are excluded — they're aliases, not
/// branches you'd merge. Names only; used for merge sources.
pub fn remote_branches(repo: &Path) -> Result<Vec<String>, String> {
    let out = Git::new(repo).run(["branch", "-r", "--format=%(refname:short)"])?;
    Ok(out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.ends_with("/HEAD"))
        .map(str::to_string)
        .collect())
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

/// `git branch -m <old> <new>` — renames, keeping the upstream config.
pub fn rename_branch(repo: &Path, old: &str, new: &str) -> Result<(), String> {
    let new = new.trim();
    if new.is_empty() {
        return Err("Couldn't rename — the new name is empty.".into());
    }
    Git::new(repo).run(["branch", "-m", old, new])?;
    Ok(())
}

/// `git tag [-a -m <message>] <name> <target>` — a lightweight tag, or an
/// annotated one when there's a message.
pub fn create_tag(
    repo: &Path,
    name: &str,
    target: &str,
    message: Option<&str>,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Couldn't create the tag — the name is empty.".into());
    }
    let g = Git::new(repo);
    match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => g.run(["tag", "-a", name, target, "-m", m])?,
        None => g.run(["tag", name, target])?,
    };
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

/// `git for-each-ref` tag layout: tag object SHA, peeled commit SHA, name.
const TAG_FMT: &str = "%(objectname)%1f%(*objectname)%1f%(refname:short)";

/// Every tag keyed by the commit it names — lightweight tags land on their
/// object directly, annotated ones on the peeled `*objectname`.
pub fn tags_by_commit(repo: &Path) -> Result<HashMap<String, Vec<String>>, String> {
    let out = Git::new(repo).run(["for-each-ref", &format!("--format={TAG_FMT}"), "refs/tags"])?;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for line in out.stdout.lines() {
        let f: Vec<&str> = line.split('\x1f').collect();
        if f.len() < 3 || f[2].is_empty() {
            continue;
        }
        // Annotated: the peeled field is the tagged commit; lightweight
        // tags carry the commit (or other object) in %(objectname).
        let sha = if f[1].is_empty() { f[0] } else { f[1] };
        map.entry(sha.to_string())
            .or_default()
            .push(f[2].to_string());
    }
    for tags in map.values_mut() {
        tags.sort();
    }
    Ok(map)
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
    fn remote_branches_lists_tracking_refs() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let remote = scratch.path().join("r.git");
        g.run(["init", "--bare", "-b", "main", remote.to_str().unwrap()])
            .unwrap();
        g.run(["remote", "add", "origin", remote.to_str().unwrap()])
            .unwrap();
        g.run(["push", "-u", "origin", "HEAD"]).unwrap();
        let names = remote_branches(dir.path()).unwrap();
        assert!(names.contains(&"origin/main".to_string()));
        assert!(!names.iter().any(|n| n.ends_with("/HEAD")));

        // No remotes at all → empty, not an error.
        let (solo, _g) = temp_repo();
        assert!(remote_branches(solo.path()).unwrap().is_empty());
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

    #[test]
    fn rename_and_tag() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        create_branch(dir.path(), "topic", "HEAD").unwrap();
        rename_branch(dir.path(), "topic", "feature").unwrap();
        let names: Vec<String> = list_branches(dir.path())
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert!(names.contains(&"feature".to_string()) && !names.contains(&"topic".to_string()));

        create_tag(dir.path(), "v1", "HEAD", None).unwrap();
        create_tag(dir.path(), "v2", "HEAD", Some("Release two")).unwrap();
        let tags = g.run(["tag", "-n1"]).unwrap().stdout;
        assert!(tags.contains("v1") && tags.contains("Release two"));
        assert!(create_tag(dir.path(), "v1", "HEAD", None).is_err());
    }

    #[test]
    fn tags_keyed_by_commit() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "init", "").unwrap();
        let first = g
            .run(["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        write_file(dir.path(), "a.txt", "2\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "second", "").unwrap();
        let second = g
            .run(["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();

        // One of each flavour on the release commit.
        create_tag(dir.path(), "v2.0", "HEAD", Some("Release 2.0")).unwrap();
        create_tag(dir.path(), "latest", "HEAD", None).unwrap();
        create_tag(dir.path(), "v1.0", &first, None).unwrap();

        let map = tags_by_commit(dir.path()).unwrap();
        assert_eq!(map[&second], vec!["latest", "v2.0"]);
        assert_eq!(map[&first], vec!["v1.0"]);
        assert!(map.get("").is_none());
    }
}
