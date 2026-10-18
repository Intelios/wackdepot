//! The one place git is invoked. [`Git`] wraps `std::process::Command`,
//! always runs `git -C <repo>`, and always sets `GIT_TERMINAL_PROMPT=0` so a
//! GUI-launched git can never block waiting on a tty credential prompt.
//!
//! Error strings are user-facing sentences: [`failure_message`] prefers the
//! last non-empty stderr line (git's own words are usually the best UI copy),
//! stripped of `fatal:`/`error:` prefixes, capitalized and punctuated.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The captured result of a git invocation.
#[derive(Debug)]
pub struct GitOutput {
    /// Exit code, or -1 when the process was killed by a signal.
    pub status: i32,
    /// stdout, lossily decoded (git output is not always UTF-8).
    pub stdout: String,
    /// stderr — progress output lands here even on success.
    pub stderr: String,
}

impl GitOutput {
    /// Whether git exited with status 0.
    pub fn ok(&self) -> bool {
        self.status == 0
    }
}

/// Runs the system `git` inside one repository directory.
///
/// Cheap to clone — it only carries the repo path.
#[derive(Clone, Debug)]
pub struct Git {
    repo: PathBuf,
}

impl Git {
    /// A runner anchored at `repo` — any path inside the working tree. Every
    /// command is invoked as `git -C <repo> <args>`.
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self { repo: repo.into() }
    }

    /// The directory commands run in.
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .arg("-C")
            .arg(&self.repo)
            .stdin(Stdio::null());
        cmd
    }

    /// Run `git <args>`, capturing output. Never `Err`s on a non-zero exit —
    /// check [`GitOutput::ok`] — only when git can't be spawned. Use
    /// [`Git::run`] for operations that must succeed.
    pub fn try_run<I, S>(&self, args: I) -> Result<GitOutput, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = self.command();
        cmd.args(args);
        finish(cmd.output())
    }

    /// Like [`Git::try_run`] but pipes `input` into git's stdin —
    /// for `git apply -`, `git commit -F -`, etc.
    pub fn try_run_stdin<I, S>(&self, args: I, input: &str) -> Result<GitOutput, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = self.command();
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(spawn_err)?;
        if let Some(mut stdin) = child.stdin.take() {
            // git may exit before reading all of stdin (e.g. a patch it can't
            // apply); its real error then arrives via wait_with_output.
            let _ = stdin.write_all(input.as_bytes());
        }
        finish(child.wait_with_output())
    }

    /// Run `git <args>`; `Err` when the exit code is non-zero. The message is
    /// [`failure_message`], or `git <args> exited with status N` when git
    /// printed nothing.
    pub fn run<I, S>(&self, args: I) -> Result<GitOutput, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<S> = args.into_iter().collect();
        let out = self.try_run(&args)?;
        if out.ok() {
            return Ok(out);
        }
        Err(run_failure(&out, &args))
    }

    /// Like [`Git::run`] but pipes `input` into git's stdin.
    pub fn run_stdin<I, S>(&self, args: I, input: &str) -> Result<GitOutput, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<S> = args.into_iter().collect();
        let out = self.try_run_stdin(&args, input)?;
        if out.ok() {
            return Ok(out);
        }
        Err(run_failure(&out, &args))
    }
}

/// Run `git <args>` outside any repository (e.g. `git --version`), still with
/// `GIT_TERMINAL_PROMPT=0`.
pub fn run_global<I, S>(args: I) -> Result<GitOutput, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<S> = args.into_iter().collect();
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(&args)
        .stdin(Stdio::null());
    let out = finish(cmd.output())?;
    if out.ok() {
        return Ok(out);
    }
    Err(run_failure(&out, &args))
}

/// The user-facing sentence for a failed git call: the last non-empty stderr
/// line (or stdout — git occasionally reports errors there, e.g. "nothing to
/// commit"), with `fatal:`/`error:` prefixes stripped, capitalized and
/// punctuated. Bare "Aborting" trailers are skipped — the line above them
/// carries the actual reason. Empty when git printed nothing at all.
pub fn failure_message(out: &GitOutput) -> String {
    // Prefer explicit error/fatal lines — advice (`hint:` …) trails the real
    // reason and reads badly as a toast. Fall back to the last meaningful line.
    let lines = || {
        out.stderr
            .lines()
            .chain(out.stdout.lines())
            .map(str::trim)
            .filter(|l| {
                !l.is_empty() && l.trim_end_matches('.') != "Aborting" && !l.starts_with("hint:")
            })
    };
    for line in lines().rev() {
        if line.starts_with("error:") || line.starts_with("fatal:") {
            return polish(line);
        }
    }
    lines().next_back().map(polish).unwrap_or_default()
}

fn run_failure<S: AsRef<OsStr>>(out: &GitOutput, args: &[S]) -> String {
    let msg = failure_message(out);
    if !msg.is_empty() {
        return msg;
    }
    let display = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    format!("git {display} exited with status {}.", out.status)
}

/// Turn a raw git stderr line into a sentence: drop the `fatal:`/`error:`
/// prefix, capitalize the first letter, ensure terminal punctuation.
fn polish(line: &str) -> String {
    let mut s = line.trim();
    for prefix in ["fatal:", "error:"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start();
            break;
        }
    }
    let mut out = String::with_capacity(s.len() + 2);
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    if !out.is_empty() && !matches!(out.chars().last(), Some('.' | '!' | '?')) {
        out.push('.');
    }
    out
}

fn spawn_err(e: std::io::Error) -> String {
    format!("Couldn't launch git — {e}. Is it installed?")
}

fn finish(result: std::io::Result<Output>) -> Result<GitOutput, String> {
    let out = result.map_err(spawn_err)?;
    Ok(GitOutput {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_repo;

    #[test]
    fn run_captures_output() {
        let (_dir, g) = temp_repo();
        let out = g.run(["rev-parse", "--is-inside-work-tree"]).unwrap();
        assert_eq!(out.stdout.trim(), "true");
    }

    #[test]
    fn error_is_last_stderr_line_cleaned() {
        let (_dir, g) = temp_repo();
        let err = g.run(["checkout", "no-such-branch"]).unwrap_err();
        assert_eq!(
            err,
            "Pathspec 'no-such-branch' did not match any file(s) known to git."
        );
    }

    #[test]
    fn error_falls_back_to_stdout() {
        let (_dir, g) = temp_repo();
        // `git commit` with nothing staged reports on stdout, not stderr.
        let out = g.try_run(["commit", "-m", "x"]).unwrap();
        assert!(!out.ok());
        assert!(
            failure_message(&out)
                .to_lowercase()
                .contains("nothing to commit")
        );
    }
}
