//! Word-level highlights inside changed lines — the changed span of a
//! modified line is emphasized on top of the line's add/remove tint.
//!
//! Pairing follows GitHub Desktop: inside a hunk, a run of `k` removed lines
//! directly followed by `k` added lines pairs them up line by line. Unequal
//! runs aren't paired — there's no honest way to say which old line became
//! which new one.

use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::diff::{DiffLineKind, Hunk};

/// Lines longer than this (in bytes) aren't word-diffed.
pub const MAX_LINE: usize = 1000;
/// Below this share of unchanged text the two lines are effectively
/// different — highlighting nearly everything would be noise.
const MIN_UNCHANGED: f32 = 0.4;

/// Byte ranges into a line's text to emphasize.
pub type Ranges = Vec<Range<usize>>;

/// Word ranges for every line of `hunk`, index-aligned with `hunk.lines`.
/// `None` for context lines and for changed lines that have no pair or
/// whose pair is too different. Inside a conflicted file, `Ours` runs pair
/// with the `Theirs` run that follows the `=======` marker instead.
pub fn hunk_word_ranges(hunk: &Hunk) -> Vec<Option<Ranges>> {
    let lines = &hunk.lines;
    let mut out = vec![None; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        match lines[i].kind {
            DiffLineKind::Removed => {
                let del_start = i;
                while i < lines.len() && lines[i].kind == DiffLineKind::Removed {
                    i += 1;
                }
                // A "\ No newline" marker may sit between the runs.
                let mut add_start = i;
                while add_start < lines.len() && lines[add_start].kind == DiffLineKind::NoNewline {
                    add_start += 1;
                }
                let mut j = add_start;
                while j < lines.len() && lines[j].kind == DiffLineKind::Added {
                    j += 1;
                }
                let dels = i - del_start;
                let adds = j - add_start;
                if dels == adds {
                    for k in 0..dels {
                        let (old, new) = (del_start + k, add_start + k);
                        if let Some((a, b)) = intraline(&lines[old].text, &lines[new].text) {
                            out[old] = Some(a);
                            out[new] = Some(b);
                        }
                    }
                }
                i = j.max(i);
            }
            DiffLineKind::Ours => {
                let our_start = i;
                while i < lines.len() && lines[i].kind == DiffLineKind::Ours {
                    i += 1;
                }
                // Skip the `=======` marker (and any diff3 `|||||||` base
                // block) to land on the theirs section.
                let mut j = i;
                while j < lines.len()
                    && matches!(lines[j].kind, DiffLineKind::Marker | DiffLineKind::Base)
                {
                    j += 1;
                }
                let mut k = j;
                while k < lines.len() && lines[k].kind == DiffLineKind::Theirs {
                    k += 1;
                }
                let (ours, theirs) = (i - our_start, k - j);
                if ours == theirs {
                    for n in 0..ours {
                        let (a_idx, b_idx) = (our_start + n, j + n);
                        if let Some((a, b)) = intraline(&lines[a_idx].text, &lines[b_idx].text) {
                            out[a_idx] = Some(a);
                            out[b_idx] = Some(b);
                        }
                    }
                }
                i = k.max(i);
            }
            _ => i += 1,
        }
    }
    out
}

/// The changed spans of `old` and `new`: removed tokens in `old`, inserted
/// tokens in `new`. `None` when either line is too long or the lines share
/// too little to make emphasis meaningful.
pub fn intraline(old: &str, new: &str) -> Option<(Ranges, Ranges)> {
    if old.len() > MAX_LINE || new.len() > MAX_LINE || old == new {
        return None;
    }
    let a = tokens(old);
    let b = tokens(new);
    let a_text: Vec<&str> = a.iter().map(|r| &old[r.clone()]).collect();
    let b_text: Vec<&str> = b.iter().map(|r| &new[r.clone()]).collect();

    let mut del = Vec::new();
    let mut ins = Vec::new();
    let mut unchanged = 0usize;
    for op in capture_diff_slices(Algorithm::Myers, &a_text, &b_text) {
        match op {
            DiffOp::Equal { old_index, len, .. } => {
                unchanged += a[old_index..old_index + len]
                    .iter()
                    .map(|r| r.len())
                    .sum::<usize>();
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                push_span(&mut del, &a[old_index..old_index + old_len]);
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                push_span(&mut ins, &b[new_index..new_index + new_len]);
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                push_span(&mut del, &a[old_index..old_index + old_len]);
                push_span(&mut ins, &b[new_index..new_index + new_len]);
            }
        }
    }
    let longest = old.len().max(new.len()).max(1);
    if (unchanged as f32) / (longest as f32) < MIN_UNCHANGED {
        return None;
    }
    Some((trim_ws(old, del), trim_ws(new, ins)))
}

/// Append the byte span covering `toks`, merging with an adjacent span.
fn push_span(spans: &mut Ranges, toks: &[Range<usize>]) {
    let (Some(first), Some(last)) = (toks.first(), toks.last()) else {
        return;
    };
    let span = first.start..last.end;
    if let Some(prev) = spans.last_mut()
        && prev.end == span.start
    {
        prev.end = span.end;
        return;
    }
    spans.push(span);
}

/// Drop spans that are only whitespace — re-indentation reads as noise.
fn trim_ws(text: &str, spans: Ranges) -> Ranges {
    spans
        .into_iter()
        .filter(|r| !text[r.clone()].trim().is_empty())
        .collect()
}

/// Split into runs of word characters, runs of whitespace, and single
/// punctuation characters, as byte ranges.
fn tokens(s: &str) -> Vec<Range<usize>> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Punct,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Punct
        }
    };
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut prev: Option<Class> = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        let joins = k != Class::Punct && prev.as_ref() == Some(&k);
        match out.last_mut() {
            Some(last) if joins => last.end = i + c.len_utf8(),
            _ => out.push(i..i + c.len_utf8()),
        }
        prev = Some(k);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse_diff;

    #[test]
    fn highlights_the_changed_word() {
        let (a, b) = intraline("let row_h = 20.0;", "let row_h = ROW_H;").unwrap();
        assert_eq!(a, vec![12..16]);
        assert_eq!(b, vec![12..17]);
    }

    #[test]
    fn unrelated_lines_get_nothing() {
        assert!(intraline("fn main() {}", "struct Thing;").is_none());
        assert!(intraline(&"a".repeat(MAX_LINE + 1), "b").is_none());
    }

    #[test]
    fn whitespace_only_changes_are_dropped() {
        let (a, b) = intraline("    foo(bar)", "  foo(bar)").unwrap();
        assert!(a.is_empty() && b.is_empty());
    }

    #[test]
    fn pairs_equal_runs_only() {
        let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,4 +1,4 @@\n-let a = 1;\n-let b = 2;\n+let a = 10;\n+let b = 20;\n ctx\n-solo one\n+solo two\n+extra\n";
        let d = &parse_diff(patch)[0];
        let r = hunk_word_ranges(&d.hunks[0]);
        assert!(r[0].is_some() && r[1].is_some() && r[2].is_some() && r[3].is_some());
        assert!(r[4].is_none());
        // 1 removed vs 2 added: unpaired.
        assert!(r[5].is_none() && r[6].is_none() && r[7].is_none());
    }
}
