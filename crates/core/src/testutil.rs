//! Shared helpers for the test suite — real git in `tempfile` dirs.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use crate::git::{Git, GitOutput};

/// An initialized repo in a temp dir with a deterministic author.
pub fn temp_repo() -> (tempfile::TempDir, Git) {
    let dir = tempfile::tempdir().unwrap();
    let g = Git::new(dir.path());
    g.run(["init", "-b", "main"]).unwrap();
    g.run(["config", "user.name", "WackDepot Test"]).unwrap();
    g.run(["config", "user.email", "test@wackdepot.local"])
        .unwrap();
    (dir, g)
}

/// Run git without unwrapping — for expecting failures.
pub fn git_out<I, S>(g: &Git, args: I) -> GitOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    g.try_run(args).unwrap()
}

/// Write `contents` to `repo/rel`, creating parent dirs.
pub fn write_file(repo: &Path, rel: &str, contents: &str) {
    let p = repo.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

pub fn read_file(repo: &Path, rel: &str) -> String {
    fs::read_to_string(repo.join(rel)).unwrap()
}
