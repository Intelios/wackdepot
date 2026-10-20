//! Unified-diff engine: parsing git's patch output into [`FileDiff`]s,
//! per-file diffs for the changes list, synthesized whole-file diffs for
//! untracked files, worktree-with-markers diffs for conflicted files, and
//! per-commit diffs.
//!
//! [`FileDiff::header_text`] and [`Hunk::patch_text`] preserve the raw patch
//! bytes so [`crate::stage`] can rebuild a `git apply`-able patch for any
//! single hunk verbatim.

use std::fs;
use std::path::{Path, PathBuf};

use crate::git::Git;

/// Text files larger than this in the working tree aren't diffed — treat them
/// like binary so a giant file can't wedge the UI.
const MAX_TEXT_BYTES: u64 = 512 * 1024;

/// Which side of a file to diff: worktree-vs-index or index-vs-HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    /// `git diff -- <path>` — what's changed since staging.
    Unstaged,
    /// `git diff --cached -- <path>` — what's staged for the next commit.
    Staged,
}

/// What happened to a file in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffFileStatus {
    Added,
    #[default]
    Modified,
    Deleted,
    /// Also covers copies — `old_path`/`new_path` carry the pair.
    Renamed,
    /// A binary file (or one too large to treat as text); `hunks` is empty.
    Binary,
    /// An unmerged path: hunks show the worktree file with its conflict
    /// markers — not an applyable patch.
    Conflicted,
}

/// One line inside a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    /// The `\ No newline at end of file` marker.
    NoNewline,
    /// A `<<<<<<<`/`=======`/`>>>>>>>` (or diff3 `|||||||`) marker line in a
    /// conflicted file.
    Marker,
    /// Lines inside a conflict's `<<<<<<<` section (ours).
    Ours,
    /// Lines inside a diff3 `|||||||` section (the merge base).
    Base,
    /// Lines after `=======` up to `>>>>>>>` (theirs).
    Theirs,
}

/// A single diff line with its gutter coordinates.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// Line text without the leading `+`/`-`/` ` sigil (or the marker text).
    pub text: String,
    /// Line number on the old side; `None` for added lines.
    pub old_lineno: Option<u32>,
    /// Line number on the new side; `None` for removed lines.
    pub new_lineno: Option<u32>,
}

/// One `@@` hunk. `patch_text` is the raw patch text of just this hunk —
/// header included, trailing newline included — ready to feed `git apply`
/// behind a file header.
#[derive(Debug, Clone)]
pub struct Hunk {
    /// The raw `@@ -a,b +c,d @@ context` line.
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
    pub patch_text: String,
}

/// The diff of one file. Canonical path for display: [`FileDiff::path`].
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Path on the `a/` side; `None` for new files.
    pub old_path: Option<String>,
    /// Path on the `b/` side; `None` for deletions.
    pub new_path: Option<String>,
    pub status: DiffFileStatus,
    /// Raw header lines (`diff --git`, `index`, `---`, `+++`, mode/rename
    /// markers) — prefix this to a hunk's `patch_text` for `git apply`.
    pub header_text: String,
    pub hunks: Vec<Hunk>,
    /// Binary or too-large file — there are no hunks to show or apply.
    pub is_binary: bool,
    /// Merge-conflict details; `Some` iff `status` is `Conflicted`.
    pub conflict: Option<ConflictInfo>,
}

/// What a conflicted file looks like beyond its lines — which stages exist
/// and how many marker regions the worktree copy carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConflictInfo {
    /// An `ours` stage exists in the index (`:2:path`) — false when our side
    /// deleted the file.
    pub ours: bool,
    /// A `theirs` stage exists (`:3:path`) — false when their side deleted it.
    pub theirs: bool,
    /// `<<<<<<<`/`>>>>>>>` regions found in the worktree copy. Zero means the
    /// markers were already edited out — or the sides can't have any.
    pub regions: usize,
    /// The file exists in the worktree — false when it's deleted there.
    pub in_worktree: bool,
}

impl FileDiff {
    /// The path the UI should label this diff with.
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }

    /// An entry for a file we know changed but have no patch for (binary,
    /// oversized, or a merge-commit file with no combined diff).
    pub(crate) fn stub(path: impl Into<String>, status: DiffFileStatus) -> Self {
        Self {
            old_path: Some(path.into()),
            new_path: None,
            status,
            header_text: String::new(),
            hunks: Vec::new(),
            is_binary: true,
            conflict: None,
        }
    }
}

/// How a diff is computed. `Default` is git's own: whitespace counts, three
/// lines of context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffOptions {
    /// `-w` — ignore all whitespace. Such a patch no longer matches the file
    /// byte-for-byte, so callers must not stage or discard from it.
    pub ignore_whitespace: bool,
    /// `-U<n>` — lines of context; `None` keeps git's default (3).
    pub context: Option<u32>,
}

impl DiffOptions {
    fn push_args(&self, args: &mut Vec<String>) {
        if self.ignore_whitespace {
            args.push("-w".into());
        }
        if let Some(n) = self.context {
            args.push(format!("-U{n}"));
        }
    }
}

/// The diff for `file` on one `side` — a [`FileDiff`] even when the patch is
/// empty (`status: Modified`, no hunks). For untracked files the whole-file
/// diff is synthesized from disk so it can be rendered and hunk-staged like
/// any other addition; binary or >512KB files come back `is_binary`.
pub fn diff(repo: &Path, file: &str, side: DiffSide) -> Result<FileDiff, String> {
    diff_with(repo, file, side, &DiffOptions::default())
}

/// [`diff`] with explicit [`DiffOptions`].
pub fn diff_with(
    repo: &Path,
    file: &str,
    side: DiffSide,
    opts: &DiffOptions,
) -> Result<FileDiff, String> {
    let g = Git::new(repo);
    let mut args: Vec<String> = vec!["diff".into(), "--no-color".into()];
    if side == DiffSide::Staged {
        args.push("--cached".into());
    }
    opts.push_args(&mut args);
    args.extend(["--".into(), file.to_string()]);
    let out = g.run(args)?;
    if let Some(d) = parse_diff(&out.stdout).into_iter().next() {
        return Ok(d);
    }

    // An unmerged path produces no two-way patch (`git diff` emits a
    // combined `diff --cc`, or nothing at all). Show the worktree file with
    // its conflict markers instead of an empty diff.
    if let Some((ours, theirs)) = unmerged_sides(&g, file) {
        return conflicted_diff(repo, file, ours, theirs);
    }

    // Empty patch: either no changes on this side, or the file isn't tracked
    // yet (git diff ignores untracked files) — synthesize the addition.
    if !is_tracked(repo, file) {
        let path = repo.join(file);
        if path.exists() {
            return untracked_diff(&path, file);
        }
        return Err(format!("Couldn't diff {file} — the file isn't there."));
    }
    Ok(FileDiff {
        old_path: Some(file.to_string()),
        new_path: Some(file.to_string()),
        status: DiffFileStatus::Modified,
        header_text: String::new(),
        hunks: Vec::new(),
        is_binary: false,
        conflict: None,
    })
}

/// `git ls-files -u -- <file>` → `Some((ours, theirs))` stage presence when
/// the path is unmerged. A file deleted on both sides has stage 1 only.
pub(crate) fn unmerged_sides(g: &Git, file: &str) -> Option<(bool, bool)> {
    let out = g.try_run(["ls-files", "-u", "--", file]).ok()?;
    if !out.ok() || out.stdout.trim().is_empty() {
        return None;
    }
    // Lines look like "<mode> <sha> <stage>\t<path>".
    let has = |stage: &str| {
        out.stdout.lines().any(|l| {
            l.split_once('\t')
                .is_some_and(|(head, _)| head.ends_with(stage))
        })
    };
    Some((has(" 2"), has(" 3")))
}

/// The combined-diff view for a conflicted file: `git diff :2:file :3:file`,
/// our version against theirs — an ordinary patch whose removals are ours
/// and additions are theirs, so what each side holds is obvious at a glance.
/// `None` when either stage is absent (a side deleted the file — there's
/// nothing to compare) or the path isn't unmerged.
pub fn sides_diff(repo: &Path, file: &str) -> Result<Option<FileDiff>, String> {
    let g = Git::new(repo);
    let Some((true, true)) = unmerged_sides(&g, file) else {
        return Ok(None);
    };
    let out = g.run([
        "diff",
        "--no-color",
        &format!(":2:{file}"),
        &format!(":3:{file}"),
    ])?;
    let mut d = parse_diff(&out.stdout)
        .into_iter()
        .next()
        .unwrap_or_else(|| FileDiff {
            // Same content on both sides — nothing differs.
            old_path: Some(file.to_string()),
            new_path: Some(file.to_string()),
            status: DiffFileStatus::Modified,
            header_text: String::new(),
            hunks: Vec::new(),
            is_binary: false,
            conflict: None,
        });
    // Stage paths are the file's path — keep the label stable no matter
    // what blob-pair header git printed.
    if d.old_path.is_none() {
        d.old_path = Some(file.to_string());
    }
    if d.new_path.is_none() {
        d.new_path = Some(file.to_string());
    }
    Ok(Some(d))
}

/// Context lines kept on either side of a conflict region.
const CONFLICT_CONTEXT: usize = 3;

/// The diff view of a conflicted file: the worktree copy with its conflict
/// sections classified (`<<<<<<<`/`=======`/`>>>>>>>`, diff3's `|||||||`
/// included). Hunks group the regions with a few lines of context each, or
/// the whole file when there are no markers. These hunks are a view, not a
/// patch — `header_text`/`patch_text` stay empty and nothing can apply them.
fn conflicted_diff(repo: &Path, file: &str, ours: bool, theirs: bool) -> Result<FileDiff, String> {
    let path = repo.join(file);
    let bytes = fs::read(&path).unwrap_or_default();
    let too_big = fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_TEXT_BYTES;
    let mut info = ConflictInfo {
        ours,
        theirs,
        regions: 0,
        in_worktree: path.exists(),
    };
    if too_big || bytes.iter().take(8192).any(|&b| b == 0) {
        let mut d = FileDiff::stub(file, DiffFileStatus::Conflicted);
        d.new_path = Some(file.to_string());
        d.conflict = Some(info);
        return Ok(d);
    }

    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let (regions, kinds) = classify_conflicts(&lines);
    info.regions = regions.len();

    let mut hunks = Vec::new();
    if regions.is_empty() {
        // No markers left (already edited, or a side can't conflict) — show
        // the whole file as it stands.
        if !lines.is_empty() {
            hunks.push(conflict_hunk(0, &lines, &kinds, "the file as it stands"));
        }
    } else {
        // Merge regions closer than twice the context into one hunk.
        let last = lines.len() - 1;
        let mut span = (
            regions[0].0.saturating_sub(CONFLICT_CONTEXT),
            (regions[0].1 + CONFLICT_CONTEXT).min(last),
        );
        for r in regions.iter().skip(1) {
            if r.0.saturating_sub(CONFLICT_CONTEXT) <= span.1 + 1 {
                span.1 = (r.1 + CONFLICT_CONTEXT).min(last);
            } else {
                hunks.push(conflict_hunk(
                    span.0,
                    &lines[span.0..=span.1],
                    &kinds[span.0..=span.1],
                    "conflict",
                ));
                span = (
                    r.0.saturating_sub(CONFLICT_CONTEXT),
                    (r.1 + CONFLICT_CONTEXT).min(last),
                );
            }
        }
        hunks.push(conflict_hunk(
            span.0,
            &lines[span.0..=span.1],
            &kinds[span.0..=span.1],
            "conflict",
        ));
    }

    Ok(FileDiff {
        old_path: Some(file.to_string()),
        new_path: Some(file.to_string()),
        status: DiffFileStatus::Conflicted,
        header_text: String::new(),
        hunks,
        is_binary: false,
        conflict: Some(info),
    })
}

/// A hunk over `lines[start..]` — worktree lines only, so line numbers live
/// on the new side. The header names the sides when marker labels allow
/// (`@@ +4,9 @@ HEAD vs side`), else `tail` is the trailing context.
fn conflict_hunk(start: usize, lines: &[&str], kinds: &[DiffLineKind], tail: &str) -> Hunk {
    let mut ours = None;
    let mut theirs = None;
    for (text, kind) in lines.iter().zip(kinds) {
        if *kind != DiffLineKind::Marker {
            continue;
        }
        if ours.is_none() {
            ours = conflict_marker(text, b'<');
        }
        if theirs.is_none() {
            theirs = conflict_marker(text, b'>');
        }
    }
    let tail = match (ours, theirs) {
        (Some(o), Some(t)) if !o.is_empty() && !t.is_empty() => {
            format!("{} vs {}", o.replace('@', " "), t.replace('@', " "))
        }
        _ => tail.to_string(),
    };
    let mut h = Hunk {
        header: format!("@@ +{},{} @@ {}", start + 1, lines.len(), tail),
        old_start: 0,
        old_lines: 0,
        new_start: start as u32 + 1,
        new_lines: lines.len() as u32,
        lines: Vec::with_capacity(lines.len()),
        patch_text: String::new(),
    };
    for (i, (text, kind)) in lines.iter().zip(kinds).enumerate() {
        h.lines.push(DiffLine {
            kind: *kind,
            text: (*text).to_string(),
            old_lineno: None,
            new_lineno: Some(start as u32 + i as u32 + 1),
        });
    }
    h
}

/// `(kind, label)` when `line` opens/closes/marks a conflict section.
/// Markers need at least seven of their character then end of line or a
/// space + label — anything shorter is just text.
fn conflict_marker(line: &str, ch: u8) -> Option<&str> {
    let b = line.as_bytes();
    if b.len() < 7 || b[..7].iter().any(|&c| c != ch) {
        return None;
    }
    match b.get(7) {
        None => Some(""),
        Some(&b' ') => Some(line[8..].trim()),
        _ => None,
    }
}

/// The `<<<<<<<`…`>>>>>>>` regions (line indices) plus a per-line kind for
/// the whole file. Unterminated markers run to EOF; nested or stray markers
/// degrade to whatever section they're sitting in.
fn classify_conflicts(lines: &[&str]) -> (Vec<(usize, usize)>, Vec<DiffLineKind>) {
    let mut regions = Vec::new();
    let mut kinds = vec![DiffLineKind::Context; lines.len()];
    let mut state = DiffLineKind::Context;
    let mut open = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if state == DiffLineKind::Context {
            if conflict_marker(line, b'<').is_some() {
                state = DiffLineKind::Ours;
                kinds[i] = DiffLineKind::Marker;
                open = i;
                continue;
            }
        } else if conflict_marker(line, b'>').is_some() {
            kinds[i] = DiffLineKind::Marker;
            regions.push((open, i));
            state = DiffLineKind::Context;
            continue;
        } else if conflict_marker(line, b'|').is_some() {
            state = DiffLineKind::Base;
            kinds[i] = DiffLineKind::Marker;
            continue;
        } else if conflict_marker(line, b'=').is_some() {
            state = DiffLineKind::Theirs;
            kinds[i] = DiffLineKind::Marker;
            continue;
        }
        kinds[i] = state;
    }
    // A `<<<<<<<` never closed still counts as a region.
    if state != DiffLineKind::Context {
        regions.push((open, lines.len() - 1));
    }
    (regions, kinds)
}

/// Whether `file` is in the index (tracked — even if deleted in the worktree).
fn is_tracked(repo: &Path, file: &str) -> bool {
    Git::new(repo)
        .try_run(["ls-files", "--error-unmatch", "--", file])
        .map(|o| o.ok())
        .unwrap_or(false)
}

/// Build a whole-file `Added` diff for an untracked path by reading it.
/// A NUL byte in the first 8KB or a size over `MAX_TEXT_BYTES` → binary stub.
/// The patch is emitted in git's own shape and run back through
/// [`parse_diff`], so staging a hunk of an untracked file just works.
fn untracked_diff(path: &Path, rel: &str) -> Result<FileDiff, String> {
    let meta = fs::metadata(path).map_err(|e| format!("Couldn't read {rel} — {e}."));
    let too_big = meta
        .as_ref()
        .map(|m| m.len() > MAX_TEXT_BYTES)
        .unwrap_or(false);
    let bytes = fs::read(path).map_err(|e| format!("Couldn't read {rel} — {e}."))?;
    if too_big || bytes.iter().take(8192).any(|&b| b == 0) {
        return Ok(FileDiff::stub(rel, DiffFileStatus::Binary));
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut patch = String::new();
    patch.push_str(&format!(
        "diff --git a/{0} b/{0}\nnew file mode 100644\n",
        git_quote(rel)
    ));
    patch.push_str("--- /dev/null\n");
    patch.push_str(&format!("+++ b/{}\n", git_quote(rel)));
    let n = text.lines().count();
    if n > 0 {
        patch.push_str(&format!("@@ -0,0 +1,{n} @@\n"));
        for line in text.lines() {
            patch.push('+');
            patch.push_str(line);
            patch.push('\n');
        }
        if !text.ends_with('\n') {
            patch.push_str("\\ No newline at end of file\n");
        }
    }
    parse_diff(&patch)
        .into_iter()
        .next()
        .ok_or_else(|| format!("Couldn't build a diff for {rel}."))
}

/// Parse unified-diff text (`git diff`, `git show --patch`) into file diffs.
/// Pure — no repo access; anything it can't parse is skipped.
pub fn parse_diff(text: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut hunk: Option<Hunk> = None;
    let mut binary_body = false;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush(&mut files, &mut cur, &mut hunk);
            let (a, b) = split_ab_paths(rest);
            cur = Some(FileDiff {
                old_path: a,
                new_path: b,
                status: DiffFileStatus::Modified,
                header_text: format!("{line}\n"),
                hunks: Vec::new(),
                is_binary: false,
                conflict: None,
            });
            binary_body = false;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };

        if let Some(h) = hunk.as_mut() {
            if line.starts_with("@@") {
                f.hunks.push(hunk.take().unwrap());
                hunk = Some(parse_hunk_header(line));
            } else {
                match line.as_bytes().first() {
                    Some(b'+') => push_line(h, DiffLineKind::Added, &line[1..]),
                    Some(b'-') => push_line(h, DiffLineKind::Removed, &line[1..]),
                    Some(b' ') => push_line(h, DiffLineKind::Context, &line[1..]),
                    Some(b'\\') => h.lines.push(DiffLine {
                        kind: DiffLineKind::NoNewline,
                        text: line.trim_start_matches('\\').trim().to_string(),
                        old_lineno: None,
                        new_lineno: None,
                    }),
                    // Not a body line — the hunk ended. Keep the line out of
                    // patch_text and reprocess it as a header line below.
                    _ => {
                        f.hunks.push(hunk.take().unwrap());
                        if !consume_header_line(f, line) {
                            binary_body = true;
                        }
                        continue;
                    }
                }
                hunk.as_mut().unwrap().patch_text.push_str(line);
                hunk.as_mut().unwrap().patch_text.push('\n');
            }
            continue;
        }

        if binary_body {
            // Literal data of a "GIT binary patch" — not text, not applyable.
            continue;
        }
        if line.starts_with("@@") {
            hunk = Some(parse_hunk_header(line));
        } else if !consume_header_line(f, line) {
            binary_body = true;
        }
    }
    flush(&mut files, &mut cur, &mut hunk);
    files
}

/// Every file touched by `sha` — `git show --format= --patch` for the diffs.
/// Merge commits produce no patch from `git show`, so the file list is topped
/// up from `git diff-tree --name-only`, leaving binary-style stubs for files
/// with no patch.
pub fn diff_commit(repo: &Path, sha: &str) -> Result<Vec<FileDiff>, String> {
    diff_commit_with(repo, sha, &DiffOptions::default())
}

/// [`diff_commit`] with explicit [`DiffOptions`].
pub fn diff_commit_with(
    repo: &Path,
    sha: &str,
    opts: &DiffOptions,
) -> Result<Vec<FileDiff>, String> {
    let g = Git::new(repo);
    let mut args: Vec<String> = [
        "show",
        "--format=",
        "--patch",
        "--no-color",
        "--diff-merges=first-parent",
    ]
    .map(String::from)
    .to_vec();
    opts.push_args(&mut args);
    args.push(sha.to_string());
    let show = g.run(args)?;
    let mut diffs = parse_diff(&show.stdout);

    let names = g.run(["diff-tree", "--no-commit-id", "--name-only", "-r", sha])?;
    for name in names.stdout.lines() {
        let name = name.trim();
        if name.is_empty() || diffs.iter().any(|d| d.path() == name) {
            continue;
        }
        diffs.push(FileDiff::stub(name, DiffFileStatus::Binary));
    }
    Ok(diffs)
}

/// The diff of one file inside commit `sha` (against its first parent; the
/// empty tree for a root commit). `path` is the new-side path; for renames
/// pass `old_path` so git pairs them. A merge-commit file with no patch, or a
/// binary file, comes back as a binary stub.
pub fn diff_commit_file(
    repo: &Path,
    sha: &str,
    path: &str,
    old_path: Option<&str>,
    opts: &DiffOptions,
) -> Result<FileDiff, String> {
    let g = Git::new(repo);
    let mut args: Vec<String> = [
        "show",
        "--format=",
        "--patch",
        "--no-color",
        "-M",
        "--diff-merges=first-parent",
    ]
    .map(String::from)
    .to_vec();
    opts.push_args(&mut args);
    args.extend([sha.to_string(), "--".into()]);
    if let Some(old) = old_path.filter(|o| *o != path) {
        args.push(old.to_string());
    }
    args.push(path.to_string());
    let out = g.run(args)?;
    Ok(parse_diff(&out.stdout)
        .into_iter()
        .find(|d| d.path() == path || d.old_path.as_deref() == Some(path))
        .unwrap_or_else(|| FileDiff::stub(path, DiffFileStatus::Binary)))
}

/// One file touched by a commit, with its line counts — the commit view's
/// file strip, loaded without fetching any patch text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFile {
    pub path: String,
    /// Source path for renames and copies.
    pub old_path: Option<String>,
    pub status: DiffFileStatus,
    /// Lines added; `None` for binary files.
    pub added: Option<u32>,
    /// Lines removed; `None` for binary files.
    pub removed: Option<u32>,
}

/// Every file `sha` changed against its first parent (root commits: against
/// the empty tree), from `--name-status` + `--numstat`.
pub fn commit_files(repo: &Path, sha: &str) -> Result<Vec<CommitFile>, String> {
    let g = Git::new(repo);
    let base = [
        "diff-tree",
        "--no-commit-id",
        "-r",
        "-M",
        "--root",
        "-z",
        "--diff-merges=first-parent",
    ];
    let status = g.run(base.iter().copied().chain(["--name-status", sha]))?;
    let numstat = g.run(base.iter().copied().chain(["--numstat", sha]))?;

    let mut files = Vec::new();
    let mut fields = status.stdout.split('\0').filter(|f| !f.is_empty());
    while let Some(code) = fields.next() {
        let kind = code.chars().next().unwrap_or('M');
        let (old_path, path) = if matches!(kind, 'R' | 'C') {
            let old = fields.next().unwrap_or_default().to_string();
            let new = fields.next().unwrap_or_default().to_string();
            (Some(old), new)
        } else {
            (None, fields.next().unwrap_or_default().to_string())
        };
        let status = match kind {
            'A' => DiffFileStatus::Added,
            'D' => DiffFileStatus::Deleted,
            'R' | 'C' => DiffFileStatus::Renamed,
            _ => DiffFileStatus::Modified,
        };
        files.push(CommitFile {
            path,
            old_path,
            status,
            added: None,
            removed: None,
        });
    }

    // numstat -z: "<a>\t<r>\t<path>\0" or, for renames, "<a>\t<r>\t\0<old>\0<new>\0".
    let mut parts = numstat.stdout.split('\0');
    while let Some(head) = parts.next() {
        if head.is_empty() {
            continue;
        }
        let mut cols = head.splitn(3, '\t');
        let added = cols.next().unwrap_or("-");
        let removed = cols.next().unwrap_or("-");
        let mut path = cols.next().unwrap_or("").to_string();
        if path.is_empty() {
            let _old = parts.next();
            path = parts.next().unwrap_or("").to_string();
        }
        if let Some(f) = files.iter_mut().find(|f| f.path == path) {
            f.added = added.parse().ok();
            f.removed = removed.parse().ok();
            if f.added.is_none() && f.status == DiffFileStatus::Modified {
                f.status = DiffFileStatus::Binary;
            }
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------- internals

fn flush(files: &mut Vec<FileDiff>, cur: &mut Option<FileDiff>, hunk: &mut Option<Hunk>) {
    if let Some(h) = hunk.take()
        && let Some(f) = cur.as_mut()
    {
        f.hunks.push(h);
    }
    if let Some(f) = cur.take() {
        files.push(f);
    }
}

/// Fold one non-hunk line into the file header (`index`, `---`, mode/rename
/// markers, `Binary files` …). Returns `false` for lines that belong to a
/// literal "GIT binary patch" body instead.
fn consume_header_line(f: &mut FileDiff, line: &str) -> bool {
    if line == "GIT binary patch" {
        f.is_binary = true;
        f.status = DiffFileStatus::Binary;
        return false;
    }
    if line.starts_with("Binary files ") {
        f.is_binary = true;
        f.status = DiffFileStatus::Binary;
    } else if line.starts_with("new file mode") {
        f.status = DiffFileStatus::Added;
    } else if line.starts_with("deleted file mode") {
        f.status = DiffFileStatus::Deleted;
    } else if let Some(p) = line.strip_prefix("rename from ") {
        f.status = DiffFileStatus::Renamed;
        f.old_path = Some(unquote_path(p));
    } else if let Some(p) = line.strip_prefix("rename to ") {
        f.new_path = Some(unquote_path(p));
    } else if let Some(p) = line.strip_prefix("copy from ") {
        f.status = DiffFileStatus::Renamed;
        f.old_path = Some(unquote_path(p));
    } else if let Some(p) = line.strip_prefix("copy to ") {
        f.new_path = Some(unquote_path(p));
    } else if let Some(p) = line.strip_prefix("--- ") {
        f.old_path = marker_path(p);
    } else if let Some(p) = line.strip_prefix("+++ ") {
        f.new_path = marker_path(p);
    }
    // Everything else (`index`, old/new mode, similarity, blanks) is header
    // text too — it's all fed back to git apply verbatim.
    f.header_text.push_str(line);
    f.header_text.push('\n');
    true
}

/// `---`/`+++` marker → path: `/dev/null` → `None`, else strip `a/`/`b/`.
fn marker_path(raw: &str) -> Option<String> {
    let p = unquote_path(raw.trim_end());
    if p == "/dev/null" {
        None
    } else {
        let stripped = p
            .strip_prefix("a/")
            .or_else(|| p.strip_prefix("b/"))
            .unwrap_or(p.as_str());
        Some(stripped.to_string())
    }
}

/// Parse `@@ -<old>[,<n>] +<new>[,<n>] @@ context` into a fresh `Hunk` whose
/// `patch_text` starts at the header line.
fn parse_hunk_header(line: &str) -> Hunk {
    let (old_start, old_lines, new_start, new_lines) = parse_hunk_ranges(line);
    Hunk {
        header: line.to_string(),
        old_start,
        old_lines,
        new_start,
        new_lines,
        lines: Vec::new(),
        patch_text: format!("{line}\n"),
    }
}

fn parse_hunk_ranges(line: &str) -> (u32, u32, u32, u32) {
    // @@ -10,3 +12,4 @@ optional context
    let mut old = (0, 1);
    let mut new = (0, 1);
    let inner = line.trim_start_matches('@').trim_end_matches('@');
    for token in inner.split_whitespace() {
        if let Some(range) = token.strip_prefix('-') {
            old = parse_range(range);
        } else if let Some(range) = token.strip_prefix('+') {
            new = parse_range(range);
        }
    }
    (old.0, old.1, new.0, new.1)
}

fn parse_range(range: &str) -> (u32, u32) {
    match range.split_once(',') {
        Some((s, n)) => (s.parse().unwrap_or(0), n.parse().unwrap_or(0)),
        // `@@ -12 +12 @@` means one line.
        None => (range.parse().unwrap_or(0), 1),
    }
}

fn push_line(h: &mut Hunk, kind: DiffLineKind, text: &str) {
    let (old_lineno, new_lineno) = match kind {
        DiffLineKind::Context => (
            Some(h.old_start + body_len_minus(h, DiffLineKind::Added)),
            Some(h.new_start + body_len_minus(h, DiffLineKind::Removed)),
        ),
        DiffLineKind::Added => (
            None,
            Some(h.new_start + body_len_minus(h, DiffLineKind::Removed)),
        ),
        DiffLineKind::Removed => (
            Some(h.old_start + body_len_minus(h, DiffLineKind::Added)),
            None,
        ),
        DiffLineKind::NoNewline => (None, None),
        // Conflict lines only exist on synthesized conflicted diffs, where
        // line numbers are assigned directly — these arms are never hit.
        _ => (None, None),
    };
    h.lines.push(DiffLine {
        kind,
        text: text.to_string(),
        old_lineno,
        new_lineno,
    });
}

/// How many lines on `h`'s old/new side have been emitted so far — the count
/// of lines that aren't `skip` kind.
fn body_len_minus(h: &Hunk, skip: DiffLineKind) -> u32 {
    h.lines
        .iter()
        .filter(|l| l.kind != skip && l.kind != DiffLineKind::NoNewline)
        .count() as u32
}

/// `diff --git` rest-of-line → `(a-path, b-path)`. Paths with spaces aren't
/// quoted by git, so split at the last ` b/`; quoted paths are unquoted.
/// The `a/`/`b/` prefixes are stripped — the `---`/`+++` markers are the
/// authoritative source anyway; this is the fallback shape.
fn split_ab_paths(rest: &str) -> (Option<String>, Option<String>) {
    let rest = rest.trim_end();
    let strip = |p: String| {
        p.strip_prefix("a/")
            .or_else(|| p.strip_prefix("b/"))
            .unwrap_or(p.as_str())
            .to_string()
    };
    // Quoted form: "a/x y" "b/x y" — parse the first quoted string, then the rest.
    if let Some((a, tail)) = take_quoted(rest) {
        let b = take_quoted(tail.trim_start())
            .map(|(b, _)| strip(b))
            .unwrap_or_else(|| strip(tail.trim().to_string()));
        return (Some(strip(a)), Some(b));
    }
    match rest.rfind(" b/") {
        Some(i) => (
            Some(strip(unquote_path(&rest[..i]))),
            Some(strip(unquote_path(&rest[i + 1..]))),
        ),
        None => (Some(strip(unquote_path(rest))), None),
    }
}

/// If `s` starts with `"`, parse the C-style quoted string; returns the
/// decoded value and the remainder of `s`.
fn take_quoted(s: &str) -> Option<(String, &str)> {
    if !s.starts_with('"') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 1;
    let mut out: Vec<u8> = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                return Some((String::from_utf8_lossy(&out).into_owned(), &s[i + 1..]));
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                i += decode_escape(&bytes[i..], &mut out);
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    None
}

/// Decode one escape at the start of `s` (after the backslash), append raw
/// bytes to `out`, return bytes consumed.
fn decode_escape(s: &[u8], out: &mut Vec<u8>) -> usize {
    match s[0] {
        b'n' => out.push(b'\n'),
        b't' => out.push(b'\t'),
        b'b' => out.push(8),
        b'f' => out.push(12),
        b'v' => out.push(11),
        b'\\' => out.push(b'\\'),
        b'"' => out.push(b'"'),
        d @ b'0'..=b'7' => {
            let mut val = (d - b'0') as u32;
            let mut n = 1;
            while n < 3 && n < s.len() && s[n].is_ascii_digit() && s[n] < b'8' {
                val = val * 8 + (s[n] - b'0') as u32;
                n += 1;
            }
            out.push(val as u8);
            return n;
        }
        c => out.push(c),
    }
    1
}

/// Remove git's `"…"` quoting if present, else return `s` unchanged.
fn unquote_path(s: &str) -> String {
    take_quoted(s.trim())
        .map(|(p, _)| p)
        .unwrap_or_else(|| s.trim().to_string())
}

/// Quote a path the way git does when it must (control chars, `"`, `\`,
/// non-ASCII under default `core.quotePath`) — otherwise verbatim.
pub(crate) fn git_quote(p: &str) -> String {
    if p.bytes()
        .any(|b| !(0x20..0x80).contains(&b) || b == b'"' || b == b'\\')
    {
        let mut out = String::from("\"");
        for &b in p.as_bytes() {
            match b {
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                b'\n' => out.push_str("\\n"),
                b'\t' => out.push_str("\\t"),
                0x20..=0x7e => out.push(b as char),
                _ => out.push_str(&format!("\\{b:03o}")),
            }
        }
        out.push('"');
        out
    } else {
        p.to_string()
    }
}

/// Repo-relative path helper for tests and callers holding absolute paths.
pub fn rel_path(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The on-disk absolute path for a repo-relative path — just `repo.join`,
/// named so callers read naturally.
pub fn work_path(repo: &Path, rel: &str) -> PathBuf {
    repo.join(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::commit;
    use crate::stage::{stage_all, stage_file, stage_hunk};
    use crate::testutil::{git_out, temp_repo, write_file};

    const SAMPLE: &str = "\
diff --git a/src/a.rs b/src/a.rs
index 1111111..2222222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,4 @@ fn main() {
 context
-old
+new
+extra
 tail
@@ -10,2 +11,2 @@
-kept
+kept2
 same
diff --git a/new file.txt b/new file.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/new file.txt
@@ -0,0 +1,2 @@
+one
+two
\\ No newline at end of file
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 4444444..0000000
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/moved.txt b/renamed file.txt
similarity index 90%
rename from moved.txt
rename to renamed file.txt
diff --git a/pic.png b/pic.png
index 5555555..6666666 100644
Binary files a/pic.png and b/pic.png differ
";

    #[test]
    fn parses_all_file_kinds() {
        let diffs = parse_diff(SAMPLE);
        assert_eq!(diffs.len(), 5);

        let d = &diffs[0];
        assert_eq!(d.status, DiffFileStatus::Modified);
        assert_eq!(d.old_path.as_deref(), Some("src/a.rs"));
        assert_eq!(d.new_path.as_deref(), Some("src/a.rs"));
        assert_eq!(d.hunks.len(), 2);
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 3, 1, 4)
        );
        assert!(h.header.contains("fn main()"));
        let kinds: Vec<DiffLineKind> = h.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![
                DiffLineKind::Context,
                DiffLineKind::Removed,
                DiffLineKind::Added,
                DiffLineKind::Added,
                DiffLineKind::Context
            ]
        );
        assert_eq!(h.lines[1].old_lineno, Some(2));
        assert_eq!(h.lines[2].new_lineno, Some(2));
        // patch_text re-feeds exactly this hunk to git apply.
        assert!(h.patch_text.starts_with("@@ -1,3 +1,4 @@"));
        assert!(h.patch_text.contains("+extra\n"));
        assert!(d.header_text.contains("index 1111111..2222222"));

        let added = &diffs[1];
        assert_eq!(added.status, DiffFileStatus::Added);
        assert_eq!(added.old_path, None);
        assert_eq!(added.new_path.as_deref(), Some("new file.txt"));
        assert_eq!(
            added.hunks[0].lines.last().unwrap().kind,
            DiffLineKind::NoNewline
        );

        assert_eq!(diffs[2].status, DiffFileStatus::Deleted);
        assert_eq!(diffs[2].new_path, None);

        let renamed = &diffs[3];
        assert_eq!(renamed.status, DiffFileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("moved.txt"));
        assert_eq!(renamed.new_path.as_deref(), Some("renamed file.txt"));

        assert_eq!(diffs[4].status, DiffFileStatus::Binary);
        assert!(diffs[4].is_binary);
    }

    #[test]
    fn quoted_paths_roundtrip() {
        let (a, b) = split_ab_paths("a/with space.txt b/with space.txt");
        assert_eq!(a.as_deref(), Some("with space.txt"));
        assert_eq!(b.as_deref(), Some("with space.txt"));

        let (a, b) = split_ab_paths("\"a/we\\303\\244rd\" \"b/we\\303\\244rd\"");
        assert_eq!(a.as_deref().unwrap(), "weärd");
        assert_eq!(b.as_deref().unwrap(), "weärd");
    }

    #[test]
    fn file_diff_unstaged_and_staged() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "a\nb\nc\n");
        g.run(["add", "f.txt"]).unwrap();
        commit(dir.path(), "init", "").unwrap();

        write_file(dir.path(), "f.txt", "a\nB\nc\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Modified);
        assert_eq!(d.hunks.len(), 1);
        assert!(d.hunks[0].lines.iter().any(|l| l.text == "B"));

        // Nothing staged yet — staged diff is an empty Modified.
        let staged = diff(dir.path(), "f.txt", DiffSide::Staged).unwrap();
        assert!(staged.hunks.is_empty());

        stage_file(dir.path(), "f.txt").unwrap();
        let staged = diff(dir.path(), "f.txt", DiffSide::Staged).unwrap();
        assert_eq!(staged.hunks.len(), 1);
    }

    #[test]
    fn untracked_diff_reads_whole_file() {
        let (dir, _g) = temp_repo();
        write_file(dir.path(), "fresh.txt", "one\ntwo\nthree");
        let d = diff(dir.path(), "fresh.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Added);
        assert_eq!(d.hunks.len(), 1);
        assert_eq!(d.hunks[0].new_lines, 3);
        assert_eq!(
            d.hunks[0].lines.last().unwrap().kind,
            DiffLineKind::NoNewline
        );

        // The synthesized patch is applyable — stage the file via its hunk.
        stage_hunk(dir.path(), &d, &d.hunks[0]).unwrap();
        let out = Git::new(dir.path())
            .run(["diff", "--cached", "--name-only"])
            .unwrap();
        assert_eq!(out.stdout.trim(), "fresh.txt");
    }

    #[test]
    fn untracked_binary_is_binary() {
        let (dir, _g) = temp_repo();
        std::fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 0, 3]).unwrap();
        let d = diff(dir.path(), "blob.bin", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Binary);
        assert!(d.is_binary);
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn diff_commit_lists_files() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "a.txt", "1\n");
        write_file(dir.path(), "b.txt", "x\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "add both", "").unwrap();
        let sha = g
            .run(["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();

        write_file(dir.path(), "a.txt", "1\n2\n");
        g.run(["add", "a.txt"]).unwrap();
        g.run(["rm", "b.txt"]).unwrap();
        commit(dir.path(), "change", "").unwrap();
        let sha2 = g
            .run(["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();

        let diffs = diff_commit(dir.path(), &sha2).unwrap();
        assert_eq!(diffs.len(), 2);
        let a = diffs.iter().find(|d| d.path() == "a.txt").unwrap();
        assert_eq!(a.status, DiffFileStatus::Modified);
        let b = diffs.iter().find(|d| d.path() == "b.txt").unwrap();
        assert_eq!(b.status, DiffFileStatus::Deleted);

        let first = diff_commit(dir.path(), &sha).unwrap();
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|d| d.status == DiffFileStatus::Added));
    }

    #[test]
    fn ignore_whitespace_hides_reindent() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "a\n  b\n");
        g.run(["add", "f.txt"]).unwrap();
        commit(dir.path(), "init", "").unwrap();
        write_file(dir.path(), "f.txt", "a\n    b\n");
        let plain = diff_with(
            dir.path(),
            "f.txt",
            DiffSide::Unstaged,
            &DiffOptions::default(),
        )
        .unwrap();
        assert_eq!(plain.hunks.len(), 1);
        let ws = DiffOptions {
            ignore_whitespace: true,
            context: None,
        };
        let quiet = diff_with(dir.path(), "f.txt", DiffSide::Unstaged, &ws).unwrap();
        assert!(quiet.hunks.is_empty());
    }

    /// A repo left mid-merge with `f.txt` conflicted (diverging edits on
    /// main and a `side` branch).
    fn conflicted_repo() -> (tempfile::TempDir, Git) {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "top\nmiddle\nbottom\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "side"]).unwrap();
        write_file(dir.path(), "f.txt", "top\nSIDE\nbottom\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "f.txt", "top\nMAIN\nbottom\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main", "").unwrap();
        assert!(git_out(&g, ["merge", "side"]).status != 0);
        (dir, g)
    }

    #[test]
    fn conflicted_file_shows_markers() {
        let (dir, _g) = conflicted_repo();
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Conflicted);
        assert!(!d.is_binary);
        let info = d.conflict.as_ref().unwrap();
        assert!(info.ours && info.theirs && info.in_worktree);
        assert_eq!(info.regions, 1);
        assert_eq!(d.hunks.len(), 1);
        let h = &d.hunks[0];
        assert_eq!(h.header, "@@ +1,7 @@ HEAD vs side");
        let kinds: Vec<DiffLineKind> = h.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![
                DiffLineKind::Context,
                DiffLineKind::Marker,
                DiffLineKind::Ours,
                DiffLineKind::Marker,
                DiffLineKind::Theirs,
                DiffLineKind::Marker,
                DiffLineKind::Context,
            ]
        );
        // Line numbers sit on the new side — the worktree file.
        assert_eq!(h.lines[2].new_lineno, Some(3));
        assert_eq!(h.lines[2].old_lineno, None);
        assert_eq!(h.lines[4].text, "SIDE");
    }

    #[test]
    fn conflicted_diff3_shows_the_base() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "x\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "side"]).unwrap();
        write_file(dir.path(), "f.txt", "theirs\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "f.txt", "ours\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main", "").unwrap();
        assert!(git_out(&g, ["-c", "merge.conflictStyle=diff3", "merge", "side"]).status != 0);

        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        let kinds: Vec<DiffLineKind> = d.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&DiffLineKind::Base));
        // 7 lines: <<<<<<< ours ||||||| base ======= theirs >>>>>>>
        assert_eq!(kinds.len(), 7);
    }

    #[test]
    fn conflicted_file_with_no_markers_still_shows() {
        let (dir, _g) = conflicted_repo();
        // The user edited the markers out — still unmerged until staged.
        write_file(dir.path(), "f.txt", "top\nresolved\nbottom\n");
        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Conflicted);
        assert_eq!(d.conflict.as_ref().unwrap().regions, 0);
        assert_eq!(d.hunks.len(), 1);
        assert!(
            d.hunks[0]
                .lines
                .iter()
                .all(|l| l.kind == DiffLineKind::Context)
        );
        assert!(d.hunks[0].lines.iter().any(|l| l.text == "resolved"));
    }

    #[test]
    fn modify_delete_conflict_isnt_silent() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "f.txt", "keep me\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "side"]).unwrap();
        g.run(["rm", "-q", "f.txt"]).unwrap();
        commit(dir.path(), "delete", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "f.txt", "changed\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "modify", "").unwrap();
        assert!(git_out(&g, ["merge", "side"]).status != 0);

        let d = diff(dir.path(), "f.txt", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Conflicted);
        let info = d.conflict.as_ref().unwrap();
        assert!(info.ours && !info.theirs);
        // No markers are possible — the file shows whole as context.
        assert_eq!(info.regions, 0);
        assert!(d.hunks[0].lines.iter().any(|l| l.text == "changed"));
    }

    #[test]
    fn binary_conflict_is_a_conflict_not_a_mystery() {
        let (dir, g) = temp_repo();
        std::fs::write(dir.path().join("b.bin"), [0u8, 1, 2]).unwrap();
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "side"]).unwrap();
        std::fs::write(dir.path().join("b.bin"), [0u8, 9, 9]).unwrap();
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "side", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        std::fs::write(dir.path().join("b.bin"), [3u8, 4, 0, 5]).unwrap();
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "main", "").unwrap();
        assert!(git_out(&g, ["merge", "side"]).status != 0);

        let d = diff(dir.path(), "b.bin", DiffSide::Unstaged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Conflicted);
        assert!(d.is_binary);
        assert!(d.hunks.is_empty());
        assert!(d.conflict.is_some());
    }

    #[test]
    fn sides_diff_pairs_ours_against_theirs() {
        let (dir, _g) = conflicted_repo();
        let d = sides_diff(dir.path(), "f.txt").unwrap().unwrap();
        // Ours is the old side, theirs the new: -MAIN +SIDE.
        let h = &d.hunks[0];
        let kinds: Vec<_> = h
            .lines
            .iter()
            .filter_map(|l| match l.kind {
                DiffLineKind::Removed => Some(format!("-{}", l.text)),
                DiffLineKind::Added => Some(format!("+{}", l.text)),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec!["-MAIN", "+SIDE"]);
        assert_eq!(d.old_path.as_deref(), Some("f.txt"));
        assert_eq!(d.new_path.as_deref(), Some("f.txt"));

        // A side that deleted the file has nothing to compare.
        let (dir, g) = temp_repo();
        write_file(dir.path(), "g.txt", "x\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();
        g.run(["checkout", "-b", "dside"]).unwrap();
        g.run(["rm", "-q", "g.txt"]).unwrap();
        commit(dir.path(), "del", "").unwrap();
        g.run(["checkout", "main"]).unwrap();
        write_file(dir.path(), "g.txt", "changed\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "mod", "").unwrap();
        assert!(!git_out(&g, ["merge", "dside"]).ok());
        assert!(sides_diff(dir.path(), "g.txt").unwrap().is_none());

        // A merged (non-conflicted) path has no stages to pair.
        assert!(sides_diff(dir.path(), "absent.txt").unwrap().is_none());
    }

    #[test]
    fn resolved_conflict_diffs_normally() {
        let (dir, g) = conflicted_repo();
        write_file(dir.path(), "f.txt", "top\nresolved\nbottom\n");
        g.run(["add", "f.txt"]).unwrap();
        // Resolved → it leaves the Conflicted bucket and diffs staged
        // against the merge heads like a normal staged file.
        let d = diff(dir.path(), "f.txt", DiffSide::Staged).unwrap();
        assert_eq!(d.status, DiffFileStatus::Modified);
        assert_eq!(d.conflict, None);
        assert!(!d.hunks.is_empty());
    }

    #[test]
    fn classify_conflicts_handles_zdiff3_and_strays() {
        let lines: Vec<&str> = vec![
            "pre",
            "<<<<<<< HEAD",
            "ours",
            "||||||| base",
            "was",
            "=======",
            "theirs",
            ">>>>>>> side",
            "post",
        ];
        let (regions, kinds) = classify_conflicts(&lines);
        assert_eq!(regions, vec![(1, 7)]);
        assert_eq!(kinds[3], DiffLineKind::Marker);
        assert_eq!(kinds[4], DiffLineKind::Base);
        assert_eq!(kinds[6], DiffLineKind::Theirs);

        // A `<<<<<<<` never closed runs to the end of the file.
        let lines: Vec<&str> = vec!["a", "<<<<<<< HEAD", "ours"];
        let (regions, kinds) = classify_conflicts(&lines);
        assert_eq!(regions, vec![(1, 2)]);
        assert_eq!(kinds[2], DiffLineKind::Ours);

        // Fewer than seven chars or no separator isn't a marker.
        let lines: Vec<&str> = vec!["<<<<<", "<<<<<<<nospace", "plain"];
        let (regions, kinds) = classify_conflicts(&lines);
        assert!(regions.is_empty());
        assert!(kinds.iter().all(|k| *k == DiffLineKind::Context));
    }

    #[test]
    fn commit_files_counts_and_renames() {
        let (dir, g) = temp_repo();
        write_file(dir.path(), "keep.txt", "1\n2\n");
        write_file(dir.path(), "old name.txt", "same\ncontent\nhere\n");
        write_file(dir.path(), "gone.txt", "x\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "base", "").unwrap();

        write_file(dir.path(), "keep.txt", "1\n2\n3\n");
        g.run(["mv", "old name.txt", "new name.txt"]).unwrap();
        g.run(["rm", "-q", "gone.txt"]).unwrap();
        write_file(dir.path(), "added.txt", "a\nb\n");
        stage_all(dir.path()).unwrap();
        commit(dir.path(), "change", "").unwrap();

        let files = commit_files(dir.path(), "HEAD").unwrap();
        let get = |p: &str| files.iter().find(|f| f.path == p).unwrap().clone();
        assert_eq!(get("keep.txt").added, Some(1));
        assert_eq!(get("keep.txt").status, DiffFileStatus::Modified);
        let renamed = get("new name.txt");
        assert_eq!(renamed.status, DiffFileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old name.txt"));
        assert_eq!(get("gone.txt").status, DiffFileStatus::Deleted);
        assert_eq!(get("added.txt").added, Some(2));

        let one = diff_commit_file(
            dir.path(),
            "HEAD",
            "keep.txt",
            None,
            &DiffOptions::default(),
        )
        .unwrap();
        assert_eq!(one.hunks.len(), 1);
        let mv = diff_commit_file(
            dir.path(),
            "HEAD",
            "new name.txt",
            Some("old name.txt"),
            &DiffOptions::default(),
        )
        .unwrap();
        assert_eq!(mv.status, DiffFileStatus::Renamed);

        // Root commits diff against the empty tree.
        let root = g
            .run(["rev-list", "--max-parents=0", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        assert_eq!(commit_files(dir.path(), &root).unwrap().len(), 3);
    }
}
