//! Links out to the repository's page on GitHub — the repo itself, a
//! branch, a commit. The base URL comes from the remote the user fetches
//! from (origin, then upstream, then the first remote), which is also the
//! one GitHub Desktop views.

use std::path::Path;

/// What "View on GitHub" points at.
#[derive(Clone, Debug)]
pub enum GitHubTarget {
    /// The repository's front page.
    Repository,
    /// A branch's tree page (`/tree/<name>`).
    Branch(String),
    /// A commit page (`/commit/<sha>`).
    Commit(String),
}

/// The repository's web URL (`https://github.com/owner/name`), when any
/// remote points at github.com.
pub fn repository_url(repo: &Path) -> Option<String> {
    let mut remotes = wackdepot_core::remote::list(repo).ok()?;
    // The clone source first: that's the fork (or the repo itself), not an
    // upstream the user probably can't push to. Then the rest, stable.
    remotes.sort_by_key(|r| {
        if r.name == "origin" {
            0
        } else if r.name == "upstream" {
            1
        } else {
            2
        }
    });
    remotes
        .iter()
        .find_map(|r| wackdepot_github::RepositoryId::from_remote(&r.fetch_url))
        .map(|id| format!("https://github.com/{}", id.full_name()))
}

impl GitHubTarget {
    /// The full URL under `base` (as [`repository_url`] returns it).
    pub fn url(&self, base: &str) -> String {
        match self {
            Self::Repository => base.to_string(),
            Self::Branch(name) => format!("{base}/tree/{}", url_path(name)),
            Self::Commit(sha) => format!("{base}/commit/{sha}"),
        }
    }
}

/// Percent-encodes `name` for a URL path, keeping `/` intact: GitHub
/// resolves `…/tree/feat/thing` to the branch `feat/thing`, while `%2F`
/// breaks that lookup. Everything else outside the URL-safe set is
/// escaped so unusual names still land on a valid URL.
fn url_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> (tempfile::TempDir, wackdepot_core::Git) {
        let dir = tempfile::tempdir().unwrap();
        let git = wackdepot_core::Git::new(dir.path());
        git.run(["init", "-b", "main"]).unwrap();
        (dir, git)
    }

    #[test]
    fn prefers_origin_then_upstream_then_any_remote() {
        let (dir, git) = repo();
        git.run(["remote", "add", "fork", "git@github.com:me/fork.git"])
            .unwrap();
        git.run([
            "remote",
            "add",
            "upstream",
            "https://github.com/them/project.git",
        ])
        .unwrap();
        assert_eq!(
            repository_url(dir.path()).as_deref(),
            Some("https://github.com/them/project")
        );
        git.run(["remote", "add", "origin", "https://github.com/me/project"])
            .unwrap();
        assert_eq!(
            repository_url(dir.path()).as_deref(),
            Some("https://github.com/me/project")
        );
    }

    #[test]
    fn skips_non_github_remotes_and_reports_none() {
        let (dir, git) = repo();
        assert_eq!(repository_url(dir.path()), None);
        git.run([
            "remote",
            "add",
            "origin",
            "https://gitlab.example.com/a/b.git",
        ])
        .unwrap();
        assert_eq!(repository_url(dir.path()), None);
    }

    #[test]
    fn target_urls() {
        let base = "https://github.com/a/b";
        assert_eq!(GitHubTarget::Repository.url(base), base);
        assert_eq!(
            GitHubTarget::Commit("abc123".into()).url(base),
            "https://github.com/a/b/commit/abc123"
        );
        assert_eq!(
            GitHubTarget::Branch("feat/thing".into()).url(base),
            "https://github.com/a/b/tree/feat/thing"
        );
        assert_eq!(
            GitHubTarget::Branch("fix this?".into()).url(base),
            "https://github.com/a/b/tree/fix%20this%3F"
        );
    }
}
