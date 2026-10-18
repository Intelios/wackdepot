//! Turning a diff line into styled text: tabs expanded, syntax colours from
//! the theme, and word-level emphasis as a background.

use std::ops::Range;

use gpui::{FontStyle, HighlightStyle, Hsla, SharedString};
use wackdepot_syntax::{Kind, LineSpans};

use crate::theme::Theme;

const TAB: usize = 4;

pub fn kind_color(t: &Theme, k: Kind) -> Option<Hsla> {
    Some(match k {
        Kind::Plain => return None,
        Kind::Keyword => t.code_keyword,
        Kind::String => t.code_string,
        Kind::Number => t.code_number,
        Kind::Title => t.code_title,
        Kind::Meta => t.code_meta,
        Kind::Comment => t.code_comment,
    })
}

/// Expand tabs to spaces, returning the new text and a byte-offset map
/// (old offset → new offset, one entry per old byte plus the end).
fn expand_tabs(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut col = 0;
    for ch in text.chars() {
        let at = out.len();
        for _ in 0..ch.len_utf8() {
            map.push(at);
        }
        if ch == '\t' {
            let n = TAB - col % TAB;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(ch);
            col += 1;
        }
    }
    map.push(out.len());
    (out, map)
}

/// The display text and its highlight runs (sorted, non-overlapping).
pub fn styled_line(
    t: &Theme,
    text: &str,
    spans: Option<&LineSpans>,
    words: Option<&[Range<usize>]>,
    word_bg: Hsla,
) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
    let (shown, map) = expand_tabs(text);
    let remap = |r: &Range<usize>| map[r.start.min(text.len())]..map[r.end.min(text.len())];

    // Cut the line at every span and word boundary, then style each piece.
    let mut cuts = vec![0, shown.len()];
    let spans: Vec<(Range<usize>, Kind)> = spans
        .map(|s| s.iter().map(|(r, k)| (remap(r), *k)).collect())
        .unwrap_or_default();
    let words: Vec<Range<usize>> = words
        .map(|w| w.iter().map(remap).collect())
        .unwrap_or_default();
    for (r, _) in &spans {
        cuts.extend([r.start, r.end]);
    }
    for r in &words {
        cuts.extend([r.start, r.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b || !shown.is_char_boundary(a) || !shown.is_char_boundary(b) {
            continue;
        }
        let kind = spans
            .iter()
            .find(|(r, _)| r.start <= a && b <= r.end)
            .map(|(_, k)| *k);
        let in_word = words.iter().any(|r| r.start <= a && b <= r.end);
        let color = kind.and_then(|k| kind_color(t, k));
        if color.is_none() && !in_word {
            continue;
        }
        let style = HighlightStyle {
            color,
            font_style: (kind == Some(Kind::Comment)).then_some(FontStyle::Italic),
            background_color: in_word.then_some(word_bg),
            ..Default::default()
        };
        match out.last_mut() {
            Some((r, s)) if r.end == a && *s == style => r.end = b,
            _ => out.push((a..b, style)),
        }
    }
    (shown.into(), out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_expand_and_ranges_follow() {
        let (s, map) = expand_tabs("\tab");
        assert_eq!(s, "    ab");
        assert_eq!(map[1], 4);
        assert_eq!(map[3], 6);
    }
}
