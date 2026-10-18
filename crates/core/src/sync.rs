//! Network ops — fetch/pull/push/clone — plus local repo creation.
//!
//! git writes progress to stderr even on success, so these return the stderr
//! tail for the UI's progress line rather than discarding it.

use std::fs;
use std::path::{Path, PathBuf};

use crate::git::{Git, failure_message};
use crate::progress::Progress;

/// Receives live progress from a network op.
pub type OnProgress<'a> = &'a mut dyn FnMut(Progress);

/// Feed parsed progress meters from git's stderr into `on`.
fn meter<'a>(on: OnProgress<'a>) -> impl FnMut(&str) + 'a {
    move |line: &str| {
        if let Some(p) = crate::progress::parse(line) {
            on(p);
        }
    }
}

/// `git fetch --all --prune`. Returns the last lines of progress output.
pub fn fetch(repo: &Path) -> Result<String, String> {
    fetch_with_progress(repo, &mut |_| {})
}

/// [`fetch`], reporting progress as it goes.
pub fn fetch_with_progress(repo: &Path, on: OnProgress) -> Result<String, String> {
    let out = Git::new(repo)
        .run_streaming(["fetch", "--all", "--prune", "--progress"], &mut meter(on))?;
    Ok(progress_tail(&out.stdout, &out.stderr))
}

/// `git pull` — fetch + merge the tracked upstream. Returns the output tail.
pub fn pull(repo: &Path) -> Result<String, String> {
    pull_with_progress(repo, &mut |_| {})
}

/// [`pull`], reporting progress as it goes.
pub fn pull_with_progress(repo: &Path, on: OnProgress) -> Result<String, String> {
    let out = Git::new(repo).run_streaming(["pull", "--progress"], &mut meter(on))?;
    Ok(progress_tail(&out.stdout, &out.stderr))
}

/// `git push`, or `git push -u origin HEAD` when the branch has no upstream
/// yet — so first pushes wire up tracking automatically.
pub fn push(repo: &Path) -> Result<String, String> {
    push_with_progress(repo, &mut |_| {})
}

/// [`push`], reporting progress as it goes.
pub fn push_with_progress(repo: &Path, on: OnProgress) -> Result<String, String> {
    let g = Git::new(repo);
    let has_upstream = g
        .try_run(["rev-parse", "--verify", "--quiet", "@{u}"])
        .map(|o| o.ok())
        .unwrap_or(false);
    let mut sink = meter(on);
    let out = if has_upstream {
        g.run_streaming(["push", "--progress"], &mut sink)?
    } else {
        g.run_streaming(["push", "--progress", "-u", "origin", "HEAD"], &mut sink)?
    };
    Ok(progress_tail(&out.stdout, &out.stderr))
}

/// `git clone --progress <url> [<dir>]` into `dest_parent`, returning the new
/// repo's path. `name` overrides the directory name git would derive from the
/// URL; otherwise the `Cloning into '…'` line (or the URL tail) decides.
pub fn clone(url: &str, dest_parent: &Path, name: Option<&str>) -> Result<PathBuf, String> {
    clone_with_progress(url, dest_parent, name, &mut |_| {})
}

/// [`clone`], reporting progress as it goes.
pub fn clone_with_progress(
    url: &str,
    dest_parent: &Path,
    name: Option<&str>,
    on: OnProgress,
) -> Result<PathBuf, String> {
    let mut args = vec![
        "clone".to_string(),
        "--progress".to_string(),
        url.to_string(),
    ];
    if let Some(n) = name {
        args.push(n.to_string());
    }
    let out = run_global_with_cwd(dest_parent, &args, &mut meter(on))?;
    if let Some(n) = name {
        return Ok(dest_parent.join(n));
    }
    if let Some(dir) = cloning_into(&out.stderr) {
        return Ok(dest_parent.join(dir));
    }
    if let Some(n) = name_from_url(url) {
        return Ok(dest_parent.join(n));
    }
    Err("Cloned, but couldn't tell where it landed.".into())
}

/// The directory name from git's `Cloning into 'X'…` progress line.
fn cloning_into(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        if let Some(rest) = line.trim().strip_prefix("Cloning into '") {
            let dir = rest.trim_end_matches('.').trim_end_matches('\'');
            if !dir.is_empty() {
                return Some(dir.to_string());
            }
        }
    }
    None
}

/// `run_global` inside a specific working directory — needed for clone, which
/// has no repo yet.
fn run_global_with_cwd(
    cwd: &Path,
    args: &[String],
    on_line: &mut dyn FnMut(&str),
) -> Result<crate::git::GitOutput, String> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null());
    let out = crate::git::stream(cmd, on_line)?;
    if out.ok() {
        return Ok(out);
    }
    let msg = failure_message(&out);
    Err(if msg.is_empty() {
        format!("git clone exited with status {}.", out.status)
    } else {
        msg
    })
}

/// `git init -b main` inside `path` (created if missing) — every repo
/// WackDepot makes starts on `main`. Falls back to `init` + `symbolic-ref`
/// for git < 2.28 which lacks `-b`.
pub fn create_repo(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Couldn't create {} — {e}.", path.display()))?;
    let g = Git::new(path);
    if g.try_run(["init", "-b", "main"])
        .map(|o| o.ok())
        .unwrap_or(false)
    {
        return Ok(());
    }
    g.run(["init"])?;
    g.run(["symbolic-ref", "HEAD", "refs/heads/main"])?;
    Ok(())
}

/// The last meaningful lines of a successful command — progress bars and
/// `To <url> …` summary lines land on stderr.
fn progress_tail(stdout: &str, stderr: &str) -> String {
    let tail: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if !tail.is_empty() {
        return tail[tail.len().saturating_sub(3)..].join("\n");
    }
    let out: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    out[out.len().saturating_sub(3)..].join("\n")
}

/// Name git would give a cloned directory: last URL component minus `.git`,
/// minus a trailing slash. `None` for empty/`.` results.
/// Repo folder name derived from a clone URL (`…/name.git` → `name`).
pub fn name_from_url(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit('/').next()?;
    let stem = last.strip_suffix(".git").unwrap_or(last);
    (!stem.is_empty() && stem != ".").then(|| stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{commit, list_log};
    use crate::stage::stage_all;
    use crate::testutil::{temp_repo, write_file};

    /// A "remote": a bare repo on disk + an upstream-tracking local clone.
    fn remote_pair() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let remote = base.path().join("remote.git");
        let (seed, g) = temp_repo();
        write_file(seed.path(), "a.txt", "1\n");
        stage_all(seed.path()).unwrap();
        commit(seed.path(), "init", "").unwrap();
        g.run(["init", "--bare", "-b", "main", remote.to_str().unwrap()])
            .unwrap();
        g.run(["remote", "add", "origin", remote.to_str().unwrap()])
            .unwrap();
        g.run(["push", "-u", "origin", "HEAD"]).unwrap();
        (base, seed, remote)
    }

    #[test]
    fn push_pull_fetch_roundtrip() {
        let (base, seed, remote) = remote_pair();

        // Clone the remote into the scratch dir.
        let dest = clone(remote.to_str().unwrap(), base.path(), Some("copy")).unwrap();
        assert!(dest.join("a.txt").exists());
        assert_eq!(dest, base.path().join("copy"));

        // Push a new commit from the seed…
        write_file(seed.path(), "b.txt", "2\n");
        stage_all(seed.path()).unwrap();
        commit(seed.path(), "two", "").unwrap();
        let out = push(seed.path()).unwrap();
        assert!(out.contains("origin") || out.contains("->") || !out.is_empty());

        // …and pull it into the clone.
        pull(&dest).unwrap();
        assert_eq!(list_log(&dest, 10).unwrap().len(), 2);
        assert!(dest.join("b.txt").exists());

        // Fetch is quiet but fine.
        fetch(&dest).unwrap();
        let _ = base;
    }

    #[test]
    fn first_push_sets_upstream() {
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

        // No upstream → push() picks `push -u origin HEAD`.
        push(dir.path()).unwrap();
        let out = g.run(["rev-parse", "--abbrev-ref", "@{u}"]).unwrap();
        assert_eq!(out.stdout.trim(), "origin/main");

        // Second push takes the plain `git push` path.
        write_file(dir.path(), "b.txt", "2\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "two", "").unwrap();
        push(dir.path()).unwrap();
    }

    #[test]
    fn clone_derives_dir_name() {
        let (_base, seed, remote) = remote_pair();
        let scratch = tempfile::tempdir().unwrap();
        let dest = clone(remote.to_str().unwrap(), scratch.path(), None).unwrap();
        assert_eq!(dest.file_name().unwrap(), "remote");
        assert!(dest.join("a.txt").exists());
        let _ = seed;
    }

    #[test]
    fn create_repo_makes_main() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("fresh");
        create_repo(&path).unwrap();
        assert!(path.join(".git").exists());
        let out = Git::new(&path)
            .run(["symbolic-ref", "--short", "HEAD"])
            .unwrap();
        assert_eq!(out.stdout.trim(), "main");
    }

    #[test]
    fn url_names() {
        assert_eq!(
            name_from_url("https://github.com/a/b.git").as_deref(),
            Some("b")
        );
        assert_eq!(
            name_from_url("https://github.com/a/b").as_deref(),
            Some("b")
        );
        assert_eq!(name_from_url("git@x:a/c.git/").as_deref(), Some("c"));
        assert_eq!(name_from_url("https://x/").as_deref(), Some("x"));
    }
}
