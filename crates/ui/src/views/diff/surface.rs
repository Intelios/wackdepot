//! Shared diff layout and semantic styling for working changes and hosted reviews.
use crate::theme::Theme;
use wackdepot_core::diff::{DiffFileStatus, DiffLineKind, FileDiff};

/// One row of the rendered diff.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Row {
    Hunk {
        hunk: usize,
    },
    Line {
        hunk: usize,
        line: usize,
    },
    /// Split view: the old line on the left, the new one on the right.
    Pair {
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
}

pub(crate) fn row_tone(
    t: &Theme,
    kind: DiffLineKind,
) -> (Option<gpui::Hsla>, Option<gpui::Hsla>, &'static str) {
    match kind {
        DiffLineKind::Added | DiffLineKind::Ours => {
            (Some(t.sem.diff_add_bg), Some(t.sem.diff_add), "+")
        }
        DiffLineKind::Removed | DiffLineKind::Theirs => {
            (Some(t.sem.diff_del_bg), Some(t.sem.diff_del), "−")
        }
        // The `|||||||` base block and the markers themselves stay out of
        // diff colours — they're warnings, not changes.
        DiffLineKind::Base => (Some(t.sem.diff_hunk_bg), Some(t.sem.diff_hunk), ""),
        DiffLineKind::Marker => (
            Some(t.sem.warning_bg.opacity(0.45)),
            Some(t.sem.warning),
            "",
        ),
        DiffLineKind::Context | DiffLineKind::NoNewline => (None, None, ""),
    }
}

/// Rows for a file diff: a band per hunk, then its lines (unified) or
/// paired old/new lines (split; a run of removals beside the run of
/// additions that follows it).
pub(crate) fn build_rows(file: &FileDiff, split: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    // A conflicted diff is a file view, not an old/new pair — unified only.
    let split = split && file.status != DiffFileStatus::Conflicted;
    for (hi, h) in file.hunks.iter().enumerate() {
        rows.push(Row::Hunk { hunk: hi });
        if !split {
            rows.extend((0..h.lines.len()).map(|li| Row::Line { hunk: hi, line: li }));
            continue;
        }
        let lines = &h.lines;
        let mut i = 0;
        while i < lines.len() {
            match lines[i].kind {
                DiffLineKind::Context => {
                    rows.push(Row::Pair {
                        hunk: hi,
                        left: Some(i),
                        right: Some(i),
                    });
                    i += 1;
                }
                DiffLineKind::NoNewline => i += 1,
                _ => {
                    let mut dels = Vec::new();
                    while i < lines.len()
                        && matches!(
                            lines[i].kind,
                            DiffLineKind::Removed | DiffLineKind::Theirs | DiffLineKind::NoNewline
                        )
                    {
                        if matches!(lines[i].kind, DiffLineKind::Removed | DiffLineKind::Theirs) {
                            dels.push(i);
                        }
                        i += 1;
                    }
                    let mut adds = Vec::new();
                    while i < lines.len()
                        && matches!(
                            lines[i].kind,
                            DiffLineKind::Added
                                | DiffLineKind::Ours
                                | DiffLineKind::Base
                                | DiffLineKind::NoNewline
                        )
                    {
                        if lines[i].kind != DiffLineKind::NoNewline {
                            adds.push(i);
                        }
                        i += 1;
                    }
                    if dels.is_empty() && adds.is_empty() {
                        // A kind that fits neither run (e.g. a marker) gets
                        // its own row — and the loop always moves on.
                        rows.push(Row::Pair {
                            hunk: hi,
                            left: None,
                            right: Some(i),
                        });
                        i += 1;
                        continue;
                    }
                    for k in 0..dels.len().max(adds.len()) {
                        rows.push(Row::Pair {
                            hunk: hi,
                            left: dels.get(k).copied(),
                            right: adds.get(k).copied(),
                        });
                    }
                }
            }
        }
    }
    rows
}
