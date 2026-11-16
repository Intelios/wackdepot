//! Shared diff layout and semantic styling for working changes and hosted reviews.
use crate::theme::{ActiveTheme, Theme, metrics};
use gpui::{App, Pixels, TextRun, Window, px};
use wackdepot_core::diff::{DiffFileStatus, DiffLine, DiffLineKind, FileDiff};

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

/// How many lines per side get measured exactly — the rest are ruled out
/// by a cheap estimate that can't under-report by much (see `Pool`).
const WIDE_CANDIDATES: usize = 24;

/// The widest code-cell text in a diff: unified width, and the two split
/// cell widths. Lines never wrap, so this sets the horizontal scroll
/// range — without it a minified file's single huge line is unseeable
/// past the viewport edge.
///
/// Shaping every line costs too much on a large diff, so a byte-length
/// estimate (tabs expand) nominates a handful of candidates per pool and
/// only those get shaped. Byte length over-rates multi-byte characters
/// and never under-rates anything except a tab, which its padding covers,
/// so the true widest lines always survive nomination.
pub(crate) fn code_widths(file: &FileDiff, window: &Window, cx: &App) -> (Pixels, Pixels, Pixels) {
    let mut all = Pool::new();
    let mut left = Pool::new();
    let mut right = Pool::new();
    for h in &file.hunks {
        for l in &h.lines {
            all.consider(l);
            match l.kind {
                // Split pairs context on both cells; removals land on the
                // left and everything else on the right.
                DiffLineKind::Context | DiffLineKind::Removed | DiffLineKind::Theirs => {
                    left.consider(l)
                }
                _ => right.consider(l),
            }
        }
    }
    let t = cx.theme();
    let font = gpui::font(t.mono.clone());
    let size = px(metrics::TEXT_CODE);
    let ts = window.text_system();
    let measure = |l: &DiffLine| {
        let text = super::text::expand_tabs(&l.text).0;
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: t.code_text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        ts.layout_line(&text, size, &[run], None).width
    };
    (
        all.widest(&measure),
        left.widest(&measure),
        right.widest(&measure),
    )
}

/// The `WIDE_CANDIDATES` longest-estimated lines of one cell pool.
struct Pool<'a> {
    lines: Vec<(&'a DiffLine, usize)>,
}

impl<'a> Pool<'a> {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Keep `line` if its column estimate ranks in the top candidates.
    /// The estimate is the byte length plus the columns tabs expand to —
    /// multi-byte characters over-report, so genuinely wide lines can't
    /// be squeezed out of nomination.
    fn consider(&mut self, line: &'a DiffLine) {
        let mut cols = line.text.len();
        for (col, b) in line.text.bytes().enumerate() {
            if b == b'\t' {
                cols += super::text::TAB - col % super::text::TAB;
            }
        }
        let i = self.lines.partition_point(|(_, est)| *est > cols);
        if i < WIDE_CANDIDATES {
            self.lines.insert(i, (line, cols));
            self.lines.truncate(WIDE_CANDIDATES);
        }
    }

    /// The measured width of the widest nominated line.
    fn widest(&self, measure: &dyn Fn(&DiffLine) -> Pixels) -> Pixels {
        self.lines
            .iter()
            .map(|(l, _)| measure(l))
            .fold(px(0.), |a, w| a.max(w))
    }
}
