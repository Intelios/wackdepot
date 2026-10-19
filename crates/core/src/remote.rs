//! Remote discovery and explicit PR operations, always through system Git.

use crate::Git;
use crate::progress::Progress;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

pub fn list(repo: &Path) -> Result<Vec<Remote>, String> {
    let git = Git::new(repo);
    git.run(["remote"])?
        .stdout
        .lines()
        .map(|name| {
            let fetch_url = git
                .run(["remote", "get-url", "--", name])?
                .stdout
                .trim()
                .to_string();
            let push_url = git
                .run(["remote", "get-url", "--push", "--", name])?
                .stdout
                .trim()
                .to_string();
            Ok(Remote {
                name: name.to_string(),
                fetch_url,
                push_url,
            })
        })
        .collect()
}

pub fn ensure_publishable(git: &Git) -> Result<(), String> {
    let branch = crate::branch::current_branch(git.repo())?
        .ok_or("Create or switch to a branch before publishing commits.")?;
    ensure_branch_publishable(git, &branch)
}

fn ensure_branch_publishable(git: &Git, branch: &str) -> Result<(), String> {
    let out = git.try_run([
        "config",
        "--get",
        &format!("branch.{branch}.wackdepot-review-only"),
    ])?;
    if out.stdout.trim() == "true" {
        return Err("This is a pull request review branch. Create a new branch or check out a branch from your writable fork before publishing.".into());
    }
    Ok(())
}

pub fn publish(
    git: &Git,
    remote: &str,
    branch: &str,
    on: &mut dyn FnMut(Progress),
) -> Result<(), String> {
    ensure_branch_publishable(git, branch)?;
    validate_branch(git, branch)?;
    if remote.starts_with('-') || remote.is_empty() {
        return Err("Choose a remote before publishing this branch.".into());
    }
    git.run_streaming(
        [
            "push",
            "--progress",
            "--set-upstream",
            "--",
            remote,
            &format!("refs/heads/{branch}:refs/heads/{branch}"),
        ],
        &mut |line| {
            if let Some(p) = crate::progress::parse(line) {
                on(p);
            }
        },
    )?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CheckoutPullRequest {
    pub source: String,
    pub number: u64,
    pub expected_sha: String,
    pub local_name: String,
    pub tracking: Option<(String, String)>,
}

pub fn checkout_pull_request(
    git: &Git,
    input: &CheckoutPullRequest,
    on: &mut dyn FnMut(Progress),
) -> Result<String, String> {
    validate_branch(git, &input.local_name)?;
    if input.number == 0
        || !matches!(input.expected_sha.len(), 40 | 64)
        || !input.expected_sha.bytes().all(|c| c.is_ascii_hexdigit())
        || input.source.starts_with('-')
        || input.source.is_empty()
    {
        return Err("The pull request's checkout information isn't valid. Refresh its details and try again.".into());
    }
    let status = crate::status::status(git.repo())?;
    if status.operation.is_some() {
        return Err(
            "Finish or abort the Git operation in progress before checking out a pull request."
                .into(),
        );
    }
    if !status.staged.is_empty()
        || !status.unstaged.is_empty()
        || !status.untracked.is_empty()
        || !status.conflicted.is_empty()
    {
        return Err("Stash or commit your local changes before checking out this pull request. Your changes have been preserved.".into());
    }
    let private_ref = format!("refs/wackdepot/pull-requests/{}", input.number);
    let spec = format!("+refs/pull/{}/head:{private_ref}", input.number);
    git.run_streaming(
        [
            "fetch",
            "--no-tags",
            "--progress",
            "--",
            &input.source,
            &spec,
        ],
        &mut |line| {
            if let Some(p) = crate::progress::parse(line) {
                on(p);
            }
        },
    )?;
    let actual = git
        .run(["rev-parse", "--verify", &private_ref])?
        .stdout
        .trim()
        .to_string();
    if actual != input.expected_sha {
        return Err(
            "This pull request has new commits. Refresh its details before checking it out.".into(),
        );
    }
    let mut local = input.local_name.clone();
    let mut index = 1;
    loop {
        let existing = git.try_run([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{local}"),
        ])?;
        if !existing.ok() {
            break;
        }
        let marked = git
            .try_run([
                "config",
                "--get",
                &format!("branch.{local}.wackdepot-review-only"),
            ])?
            .stdout
            .trim()
            == "true";
        if existing.stdout.trim() == actual && (input.tracking.is_some() || marked) {
            git.run(["checkout", "--no-guess", &local])?;
            return Ok(local);
        }
        index += 1;
        local = format!("{}-{index}", input.local_name);
    }
    git.run(["checkout", "--no-track", "-b", &local, &private_ref])?;
    git.run([
        "config",
        &format!("branch.{local}.wackdepot-review-only"),
        "true",
    ])?;
    if let Some((remote, branch)) = &input.tracking {
        validate_branch(git, branch)?;
        if remote.starts_with('-') {
            return Err("The head remote's name isn't valid.".into());
        }
        let target = format!("refs/remotes/{remote}/{branch}");
        git.run([
            "fetch",
            "--no-tags",
            "--",
            remote,
            &format!("+refs/heads/{branch}:{target}"),
        ])?;
        git.run([
            "branch",
            "--set-upstream-to",
            &format!("{remote}/{branch}"),
            &local,
        ])?;
        git.run([
            "config",
            "--unset",
            &format!("branch.{local}.wackdepot-review-only"),
        ])?;
    }
    Ok(local)
}

pub fn cleanup(git: &Git, branch: &str, base: &str) -> Result<(), String> {
    validate_branch(git, branch)?;
    validate_branch(git, base)?;
    if branch == base {
        return Err("The base branch can't be deleted during pull request cleanup.".into());
    }
    let status = crate::status::status(git.repo())?;
    if status.operation.is_some()
        || !status.staged.is_empty()
        || !status.unstaged.is_empty()
        || !status.untracked.is_empty()
        || !status.conflicted.is_empty()
    {
        return Err("Commit or stash your changes and finish the current Git operation before cleaning up this branch.".into());
    }
    git.run(["checkout", "--no-guess", base])?;
    git.run(["branch", "-d", branch])?;
    Ok(())
}

fn validate_branch(git: &Git, branch: &str) -> Result<(), String> {
    if branch.is_empty() || branch.starts_with('-') || {
        let checked = git.try_run(["check-ref-format", "--branch", branch])?;
        !checked.ok() || checked.stdout.trim() != branch
    } {
        return Err("That branch name isn't valid.".into());
    }
    Ok(())
}

pub struct BranchComparison {
    pub files: Vec<crate::diff::FileDiff>,
    pub commits: Vec<(String, String)>,
    pub base_sha: String,
    pub head_sha: String,
}

/// Preview committed changes without publishing or modifying the index.
pub fn compare(repo: &Path, base: &str, head: &str) -> Result<BranchComparison, String> {
    let git = Git::new(repo);
    let resolve = |revision: &str| -> Result<String, String> {
        if revision.starts_with('-') || revision.is_empty() {
            return Err("Choose valid base and head branches for the comparison.".into());
        }
        Ok(git
            .run([
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{revision}^{{commit}}"),
            ])?
            .stdout
            .trim()
            .to_string())
    };
    let base = resolve(base)?;
    let head_sha = resolve(head)?;
    let base_sha = git
        .run(["merge-base", &base, &head_sha])?
        .stdout
        .trim()
        .to_string();
    let patch = git
        .run([
            "diff",
            "--no-color",
            "--find-renames",
            &base_sha,
            &head_sha,
            "--",
        ])?
        .stdout;
    if patch.len() > 16 * 1024 * 1024 {
        return Err("This local comparison is too large to preview safely. Publish the branch and inspect its changes on GitHub.".into());
    }
    let log = git
        .run([
            "log",
            "--reverse",
            "--format=%H%x1f%B%x1e",
            &format!("{base}..{head_sha}"),
            "--",
        ])?
        .stdout;
    let commits = log
        .split('\x1e')
        .filter_map(|entry| {
            entry
                .trim()
                .split_once('\x1f')
                .map(|(sha, message)| (sha.to_string(), message.trim().to_string()))
        })
        .collect();
    Ok(BranchComparison {
        files: crate::diff::parse_diff(&patch),
        commits,
        base_sha,
        head_sha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{temp_repo, write_file};

    #[test]
    fn local_preview_excludes_uncommitted_changes_and_preserves_head() {
        let (dir, git, _) = source();
        write_file(dir.path(), "file", "uncommitted work\n");
        let before = git.run(["rev-parse", "HEAD"]).unwrap().stdout;
        let preview = compare(dir.path(), "main", "feature").unwrap();
        assert_eq!(preview.commits.len(), 1);
        assert_eq!(preview.files.len(), 1);
        assert!(preview.files[0].hunks[0].patch_text.contains("+feature"));
        assert!(!preview.files[0].hunks[0].patch_text.contains("uncommitted"));
        assert_eq!(git.run(["rev-parse", "HEAD"]).unwrap().stdout, before);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file")).unwrap(),
            "uncommitted work\n"
        );
    }
    fn source() -> (tempfile::TempDir, Git, String) {
        let (dir, git) = temp_repo();
        write_file(dir.path(), "file", "base\n");
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-m", "base"]).unwrap();
        git.run(["checkout", "-b", "feature"]).unwrap();
        write_file(dir.path(), "file", "feature\n");
        git.run(["commit", "-am", "feature"]).unwrap();
        let sha = git
            .run(["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        git.run(["update-ref", "refs/pull/1/head", &sha]).unwrap();
        (dir, git, sha)
    }
    #[test]
    fn discovers_fetch_and_push_urls() {
        let (dir, git) = temp_repo();
        git.run(["remote", "add", "origin", "https://github.com/a/b.git"])
            .unwrap();
        git.run([
            "remote",
            "set-url",
            "--push",
            "origin",
            "git@github.com:a/b.git",
        ])
        .unwrap();
        let remotes = list(dir.path()).unwrap();
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].push_url, "git@github.com:a/b.git");
    }
    #[test]
    fn checkout_uses_pull_ref_and_prevents_accidental_push() {
        let (source, _, sha) = source();
        let (dest, git) = temp_repo();
        let input = CheckoutPullRequest {
            source: source.path().to_string_lossy().into(),
            number: 1,
            expected_sha: sha.clone(),
            local_name: "pr/1".into(),
            tracking: None,
        };
        assert_eq!(
            checkout_pull_request(&git, &input, &mut |_| {}).unwrap(),
            "pr/1"
        );
        assert_eq!(git.run(["rev-parse", "HEAD"]).unwrap().stdout.trim(), sha);
        assert!(ensure_publishable(&git).is_err());
        assert_eq!(
            std::fs::read_to_string(dest.path().join("file")).unwrap(),
            "feature\n"
        );
    }
    #[test]
    fn dirty_checkout_keeps_everything() {
        let (source, _, sha) = source();
        let (dest, git) = temp_repo();
        write_file(dest.path(), "untracked", "keep me");
        let input = CheckoutPullRequest {
            source: source.path().to_string_lossy().into(),
            number: 1,
            expected_sha: sha,
            local_name: "pr/1".into(),
            tracking: None,
        };
        assert!(
            checkout_pull_request(&git, &input, &mut |_| {})
                .unwrap_err()
                .contains("Stash or commit")
        );
        assert_eq!(
            std::fs::read_to_string(dest.path().join("untracked")).unwrap(),
            "keep me"
        );
    }
    #[test]
    fn stale_head_never_switches() {
        let (source, _, _) = source();
        let (_dest, git) = temp_repo();
        let input = CheckoutPullRequest {
            source: source.path().to_string_lossy().into(),
            number: 1,
            expected_sha: "0".repeat(40),
            local_name: "pr/1".into(),
            tracking: None,
        };
        assert!(checkout_pull_request(&git, &input, &mut |_| {}).is_err());
        assert_eq!(
            crate::branch::current_branch(git.repo())
                .unwrap()
                .as_deref(),
            Some("main")
        );
    }
    #[test]
    fn collisions_never_reset_local_work() {
        let (source, _, sha) = source();
        let (dir, git) = temp_repo();
        write_file(dir.path(), "local", "work");
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-m", "local"]).unwrap();
        git.run(["branch", "pr/1"]).unwrap();
        let old = git.run(["rev-parse", "pr/1"]).unwrap().stdout;
        let input = CheckoutPullRequest {
            source: source.path().to_string_lossy().into(),
            number: 1,
            expected_sha: sha,
            local_name: "pr/1".into(),
            tracking: None,
        };
        assert_eq!(
            checkout_pull_request(&git, &input, &mut |_| {}).unwrap(),
            "pr/1-2"
        );
        assert_eq!(git.run(["rev-parse", "pr/1"]).unwrap().stdout, old);
    }
    #[test]
    fn helper_is_command_scoped_and_host_scoped() {
        let (dir, git) = temp_repo();
        let helper = git.with_github_credentials(Path::new(
            "/Applications/A 'quoted' App.app/Contents/MacOS/app",
        ));
        let cmd = helper.command_for_test();
        assert!(cmd.contains("credential.https://github.com.helper"));
        assert!(cmd.contains("'\\''"));
        assert!(
            !std::fs::read_to_string(dir.path().join(".git/config"))
                .unwrap()
                .contains("credential")
        );
    }
}
