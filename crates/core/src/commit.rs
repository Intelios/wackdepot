//! Committing: `git commit -F -`, safe undo of the tip commit, author
//! identity, and `list_log` for the history view.
//!
//! History entries are parsed from `git log` using ASCII unit/record
//! separators (`\x1f` / `\x1e`) so commit bodies can't break the format.

use std::path::Path;

use crate::git::Git;

/// `git log` format: fields separated by `\x1f`, entries by `\x1e`.
const LOG_FMT: &str = "%H%x1f%h%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b%x1e";

/// One commit in the history list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Full SHA.
    pub sha: String,
    /// Abbreviated SHA.
    pub short: String,
    pub author_name: String,
    pub author_email: String,
    /// ISO-8601 author date (`%aI`).
    pub authored_at: String,
    /// Parent SHAs — 2+ means a merge commit.
    pub parents: Vec<String>,
    /// First line of the message.
    pub subject: String,
    /// Everything after the subject's blank line.
    pub body: String,
}

/// Create a commit from the staged changes. `summary` is required; the
/// message sent to git is `summary\n\ndescription`.
pub fn commit(repo: &Path, summary: &str, description: &str) -> Result<(), String> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err("Couldn't commit — the summary is empty.".into());
    }
    let g = Git::new(repo);
    if g.run(["diff", "--cached", "--quiet"]).is_ok() {
        // --quiet exits 1 when there ARE staged changes; 0 means nothing.
        return Err("Couldn't commit — nothing is staged.".into());
    }
    let message = format!("{summary}\n\n{}", description.trim_end());
    g.run_stdin(["commit", "-F", "-"], &message)?;
    Ok(())
}

/// The configured commit author — `(user.name, user.email)`. `Err` when git
/// has no identity configured, since every commit would fail anyway.
pub fn author(repo: &Path) -> Result<(String, String), String> {
    let g = Git::new(repo);
    let name = g
        .run(["config", "user.name"])
        .map(|o| o.stdout.trim().to_string())?;
    let email = g
        .run(["config", "user.email"])
        .map(|o| o.stdout.trim().to_string())?;
    if name.is_empty() || email.is_empty() {
        return Err("Couldn't find a git author — set user.name and user.email.".into());
    }
    Ok((name, email))
}

/// Whether the tip commit hasn't been pushed: `true` when there's no upstream
/// at all, or when `HEAD` is not an ancestor of its upstream. Only then is
/// [`undo_last_commit`] safe — a pushed commit is someone else's history.
pub fn tip_is_unpushed(repo: &Path) -> Result<bool, String> {
    let g = Git::new(repo);
    // Unborn HEAD (fresh repo): nothing to undo.
    let has_head = g
        .try_run(["rev-parse", "--verify", "--quiet", "HEAD"])
        .map(|o| o.ok())
        .unwrap_or(false);
    if !has_head {
        return Ok(false);
    }
    let upstream = g
        .try_run(["rev-parse", "--verify", "--quiet", "@{u}"])
        .map(|o| o.ok())
        .unwrap_or(false);
    if !upstream {
        return Ok(true);
    }
    match g.try_run(["merge-base", "--is-ancestor", "HEAD", "@{u}"]) {
        Ok(out) => Ok(!out.ok()), // exit 1 = HEAD is ahead → unpushed
        Err(e) => Err(e),
    }
}

/// Undo the most recent commit, keeping its changes staged
/// (`git reset --soft HEAD~1`). Refuses when the tip was already pushed —
/// rewriting pushed history is how you lose coworkers' trust.
pub fn undo_last_commit(repo: &Path) -> Result<(), String> {
    if !tip_is_unpushed(repo)? {
        return Err("Couldn't undo — that commit was already pushed.".into());
    }
    Git::new(repo).run(["reset", "--soft", "HEAD~1"])?;
    Ok(())
}

/// Newest-first history, `limit` entries max. An unborn branch (no commits
/// yet) yields an empty list rather than an error.
pub fn list_log(repo: &Path, limit: usize) -> Result<Vec<LogEntry>, String> {
    let g = Git::new(repo);
    let has_head = g
        .try_run(["rev-parse", "--verify", "--quiet", "HEAD"])
        .map(|o| o.ok())
        .unwrap_or(false);
    if !has_head {
        return Ok(Vec::new());
    }
    let limit = limit.max(1).to_string();
    let out = g.run(["log", &format!("--format={LOG_FMT}"), "-n", &limit])?;
    Ok(parse_log(&out.stdout))
}

/// Parse `git log` output in [`LOG_FMT`].
fn parse_log(text: &str) -> Vec<LogEntry> {
    text.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_matches(['\n', '\x1e']);
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.split('\x1f').collect();
            if f.len() < 8 {
                return None;
            }
            Some(LogEntry {
                sha: f[0].to_string(),
                short: f[1].to_string(),
                author_name: f[2].to_string(),
                author_email: f[3].to_string(),
                authored_at: f[4].to_string(),
                parents: f[5].split_whitespace().map(str::to_string).collect(),
                subject: f[6].to_string(),
                body: f[7].trim_end_matches('\n').to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{stage_all, stage_file};
    use crate::status::status;
    use crate::testutil::{temp_repo, write_file};

    #[test]
    fn commit_then_log() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_file(dir.path(), "a.txt").unwrap();
        commit(dir.path(), "add a", "a longer body\nacross lines").unwrap();

        let log = list_log(dir.path(), 10).unwrap();
        assert_eq!(log.len(), 1);
        let e = &log[0];
        assert_eq!(e.subject, "add a");
        assert!(e.body.contains("across lines"));
        assert_eq!(e.sha.len(), 40);
        assert_eq!(e.short.len(), 7);
        assert_eq!(e.author_name, "WackDepot Test");
        assert_eq!(e.author_email, "test@wackdepot.local");
        assert!(e.parents.is_empty());
        assert!(e.authored_at.contains('T'));
    }

    #[test]
    fn commit_requires_staged_changes() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        let err = commit(dir.path(), "nope", "").unwrap_err();
        assert_eq!(err, "Couldn't commit — nothing is staged.");
        let err = commit(dir.path(), "   ", "").unwrap_err();
        assert_eq!(err, "Couldn't commit — the summary is empty.");
    }

    #[test]
    fn author_reads_config() {
        let (dir, _g) = temp_repo();
        let (name, email) = author(dir.path()).unwrap();
        assert_eq!(name, "WackDepot Test");
        assert_eq!(email, "test@wackdepot.local");
    }

    #[test]
    fn list_log_empty_repo_is_empty() {
        let (dir, _g) = temp_repo();
        assert!(list_log(dir.path(), 10).unwrap().is_empty());
    }

    #[test]
    fn undo_last_commit_unpushed() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "one", "").unwrap();
        write_file(dir.path(), "b.txt", "2\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "two", "").unwrap();

        // No upstream → tip is unpushed → undo works.
        assert!(tip_is_unpushed(dir.path()).unwrap());
        undo_last_commit(dir.path()).unwrap();

        let log = list_log(dir.path(), 10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].subject, "one");
        // The undone commit's changes are still staged.
        let s = status(dir.path()).unwrap();
        assert_eq!(
            s.staged.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["b.txt"]
        );

        // First commit (root) can't be undone — HEAD~1 doesn't exist.
        assert!(
            g.try_run(["rev-parse", "--verify", "HEAD~1"])
                .unwrap()
                .status
                != 0
        );
    }

    #[test]
    fn undo_refuses_pushed_tip() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "one", "").unwrap();

        // Bare "remote" on disk; push so HEAD is contained in upstream.
        let scratch = tempfile::tempdir().unwrap();
        let remote = scratch.path().join("remote.git");
        g.run(["init", "--bare", "-b", "main", remote.to_str().unwrap()])
            .unwrap();
        g.run(["remote", "add", "origin", remote.to_str().unwrap()])
            .unwrap();
        g.run(["push", "-u", "origin", "HEAD"]).unwrap();

        assert!(!tip_is_unpushed(dir.path()).unwrap());
        let err = undo_last_commit(dir.path()).unwrap_err();
        assert!(err.contains("already pushed"));

        // A new local commit becomes unpushed again.
        write_file(dir.path(), "b.txt", "2\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "two", "").unwrap();
        assert!(tip_is_unpushed(dir.path()).unwrap());
    }
}
