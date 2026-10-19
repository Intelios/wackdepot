//! Shared diff layout and semantic styling for working changes and hosted reviews.
use crate::theme::Theme;
use wackdepot_core::diff::{DiffLineKind, FileDiff};

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
        DiffLineKind::Added => (Some(t.sem.diff_add_bg), Some(t.sem.diff_add), "+"),
        DiffLineKind::Removed => (Some(t.sem.diff_del_bg), Some(t.sem.diff_del), "−"),
        DiffLineKind::Context | DiffLineKind::NoNewline => (None, None, ""),
    }
}

/// Rows for a file diff: a band per hunk, then its lines (unified) or
/// paired old/new lines (split; a run of removals beside the run of
/// additions that follows it).
pub(crate) fn build_rows(file: &FileDiff, split: bool) -> Vec<Row> {
    let mut rows = Vec::new();
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
                            DiffLineKind::Removed | DiffLineKind::NoNewline
                        )
                    {
                        if lines[i].kind == DiffLineKind::Removed {
                            dels.push(i);
                        }
                        i += 1;
                    }
                    let mut adds = Vec::new();
                    while i < lines.len()
                        && matches!(lines[i].kind, DiffLineKind::Added | DiffLineKind::NoNewline)
                    {
                        if lines[i].kind == DiffLineKind::Added {
                            adds.push(i);
                        }
                        i += 1;
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
