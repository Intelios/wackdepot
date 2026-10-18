//! wackdepot-syntax — syntax highlighting for diffs. Whole file versions are
//! parsed with syntect (pure-Rust regex) over bat's syntax set (two-face),
//! and every token is reduced to one of WackCode's six code colours, so no
//! real colour ever lives outside the UI theme.
//!
//! Budgets keep big files cheap: oversized files aren't highlighted, long
//! lines stay plain, and a file stops being highlighted after a time budget
//! (the rest renders plain).

use std::ops::Range;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// The kinds of ink code gets (WackCode's `--wc-code-*` tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Plain,
    Keyword,
    String,
    Number,
    Title,
    Meta,
    Comment,
}

/// Highlighted byte ranges of one line (no newline), in order, plain gaps
/// left out.
pub type LineSpans = Vec<(Range<usize>, Kind)>;

/// Files bigger than this aren't highlighted.
pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_LINES: usize = 20_000;
/// Lines longer than this stay plain (and aren't fed to the parser).
pub const MAX_LINE: usize = 2000;
/// Stop highlighting a file after this long; the rest stays plain.
pub const TIME_BUDGET: Duration = Duration::from_millis(150);

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

fn kinds() -> &'static [(Scope, Kind)] {
    static K: OnceLock<Vec<(Scope, Kind)>> = OnceLock::new();
    K.get_or_init(|| {
        let s = |text: &str, k: Kind| (Scope::new(text).expect("valid scope"), k);
        vec![
            s("comment", Kind::Comment),
            s("string", Kind::String),
            s("constant.character", Kind::String),
            s("constant.numeric", Kind::Number),
            s("constant.language", Kind::Number),
            s("constant.other", Kind::Number),
            s("keyword.operator", Kind::Plain),
            s("keyword", Kind::Keyword),
            s("storage", Kind::Keyword),
            s("variable.language", Kind::Keyword),
            s("entity.name", Kind::Title),
            s("support.function", Kind::Title),
            s("support.type", Kind::Title),
            s("support.class", Kind::Title),
            s("entity.other.inherited-class", Kind::Title),
            s("entity.other.attribute-name", Kind::Meta),
            s("meta.attribute", Kind::Meta),
            s("meta.annotation", Kind::Meta),
            s("meta.decorator", Kind::Meta),
            s("meta.preprocessor", Kind::Meta),
            s("punctuation.definition.annotation", Kind::Meta),
            s("markup.heading", Kind::Title),
            s("markup.bold", Kind::Keyword),
            s("markup.raw", Kind::String),
            s("markup.underline.link", Kind::Meta),
        ]
    })
}

/// Load the syntax set now (it takes a moment) — call once at launch on a
/// background thread.
pub fn warm() {
    let _ = syntaxes();
    let _ = kinds();
}

/// The syntax for `path` (by extension, then file name), or `first_line`.
fn syntax_for(path: &str, first_line: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or(name);
    set.find_syntax_by_extension(ext)
        .or_else(|| set.find_syntax_by_extension(name))
        .or_else(|| set.find_syntax_by_first_line(first_line))
        .filter(|s| s.name != "Plain Text")
}

/// The innermost scope on the stack that names a kind.
fn kind_of(stack: &ScopeStack) -> Kind {
    for scope in stack.as_slice().iter().rev() {
        for (prefix, kind) in kinds() {
            if prefix.is_prefix_of(*scope) {
                return *kind;
            }
        }
    }
    Kind::Plain
}

/// Highlight `text` (a whole file) as the language `path` suggests. One
/// entry per line (`str::lines`); `None` when the language is unknown or
/// the file is over budget.
pub fn highlight(path: &str, text: &str) -> Option<Vec<LineSpans>> {
    if text.len() > MAX_BYTES {
        return None;
    }
    let line_count = text.lines().count();
    if line_count > MAX_LINES {
        return None;
    }
    let syntax = syntax_for(path, text.lines().next().unwrap_or(""))?;
    let set = syntaxes();
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Vec::with_capacity(line_count);
    let start = Instant::now();
    let mut buf = String::new();

    for (n, line) in text.lines().enumerate() {
        if n % 256 == 0 && start.elapsed() > TIME_BUDGET {
            out.resize(line_count, Vec::new());
            break;
        }
        if line.len() > MAX_LINE {
            out.push(Vec::new());
            continue;
        }
        buf.clear();
        buf.push_str(line);
        buf.push('\n');
        let Ok(ops) = state.parse_line(&buf, set) else {
            out.resize(line_count, Vec::new());
            break;
        };
        let mut spans: LineSpans = Vec::new();
        let mut at = 0;
        let push = |range: Range<usize>, kind: Kind, spans: &mut LineSpans| {
            let range = range.start.min(line.len())..range.end.min(line.len());
            if range.is_empty() || kind == Kind::Plain {
                return;
            }
            match spans.last_mut() {
                Some((r, k)) if *k == kind && r.end == range.start => r.end = range.end,
                _ => spans.push((range, kind)),
            }
        };
        for (pos, op) in ops {
            if pos > at {
                push(at..pos, kind_of(&stack), &mut spans);
                at = pos;
            }
            let _ = stack.apply(&op);
        }
        if at < line.len() {
            push(at..line.len(), kind_of(&stack), &mut spans);
        }
        out.push(spans);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds_in(line: &LineSpans, text: &str) -> Vec<(String, Kind)> {
        line.iter()
            .map(|(r, k)| (text[r.clone()].to_string(), *k))
            .collect()
    }

    #[test]
    fn rust_gets_keywords_strings_numbers_comments() {
        let src = "fn main() {\n    let x = 42; // answer\n    println!(\"hi\");\n}\n";
        let hl = highlight("src/main.rs", src).unwrap();
        assert_eq!(hl.len(), 4);
        let l0 = kinds_in(&hl[0], "fn main() {");
        assert!(l0.contains(&("fn".into(), Kind::Keyword)));
        assert!(l0.iter().any(|(t, k)| t == "main" && *k == Kind::Title));
        let l1 = kinds_in(&hl[1], "    let x = 42; // answer");
        assert!(l1.contains(&("let".into(), Kind::Keyword)));
        assert!(l1.contains(&("42".into(), Kind::Number)));
        assert!(
            l1.iter()
                .any(|(t, k)| t.contains("answer") && *k == Kind::Comment)
        );
        let l2 = kinds_in(&hl[2], "    println!(\"hi\");");
        assert!(
            l2.iter()
                .any(|(t, k)| t.contains("hi") && *k == Kind::String)
        );
    }

    #[test]
    fn toml_and_typescript_are_known() {
        assert!(highlight("Cargo.toml", "[package]\nname = \"x\"\n").is_some());
        assert!(highlight("app.ts", "const a: number = 1;\n").is_some());
    }

    #[test]
    fn unknown_and_oversized_are_plain() {
        assert!(highlight("notes.unknownext", "hello\n").is_none());
        let huge = "x\n".repeat(MAX_LINES + 1);
        assert!(highlight("a.rs", &huge).is_none());
    }
}
