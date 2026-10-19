//! Native Markdown: formatting and links, with no HTML execution or remote
//! image fetching. The parsed representation is inexpensive to cache.

use crate::theme::ActiveTheme;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, FontStyle, FontWeight, HighlightStyle, InteractiveText, IntoElement, ParentElement,
    Styled, StyledText, div, px,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Default)]
struct Block {
    text: String,
    styles: Vec<(Range<usize>, Inline)>,
    links: Vec<(Range<usize>, String)>,
    kind: Kind,
    table_header: bool,
}
#[derive(Clone, Copy, Default)]
struct Inline {
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
    strike: bool,
}
#[derive(Clone, Copy, Default)]
enum Kind {
    #[default]
    Paragraph,
    Heading,
    Code,
    Quote,
    Table,
}

fn parse(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Inline::default();
    let mut link = None::<(usize, String)>;
    let mut list_depth: usize = 0;
    let mut ordered = Vec::new();
    let mut table = false;
    let append = |block: &mut Block, text: &str, style: Inline| {
        let start = block.text.len();
        block.text.push_str(text);
        if start < block.text.len() {
            block.styles.push((start..block.text.len(), style));
        }
    };
    let flush = |blocks: &mut Vec<Block>, block: &mut Block| {
        if !block.text.is_empty() {
            blocks.push(std::mem::take(block));
        }
    };
    for event in Parser::new_ext(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    ) {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                flush(&mut blocks, &mut block);
                block.kind = Kind::Heading;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut blocks, &mut block);
                block.kind = Kind::Code;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                block.kind = Kind::Quote;
            }
            Event::Start(Tag::Table(_)) => {
                flush(&mut blocks, &mut block);
                table = true;
                block.kind = Kind::Table;
            }
            Event::Start(Tag::TableHead) => {
                block.kind = Kind::Table;
                block.table_header = true;
            }
            Event::Start(Tag::TableRow) => {
                block.kind = Kind::Table;
            }
            Event::Start(Tag::TableCell) => {
                if table {
                    block.kind = Kind::Table;
                }
                if !block.text.is_empty() {
                    append(&mut block, "\u{001f}", style);
                }
            }
            Event::Start(Tag::List(start)) => {
                list_depth += 1;
                ordered.push(start);
            }
            Event::Start(Tag::Item) => {
                flush(&mut blocks, &mut block);
                let prefix = match ordered.last_mut() {
                    Some(Some(n)) => {
                        let text = format!("{n}. ");
                        *n += 1;
                        text
                    }
                    _ => "• ".into(),
                };
                append(
                    &mut block,
                    &format!("{}{prefix}", "  ".repeat(list_depth.saturating_sub(1))),
                    style,
                );
            }
            Event::Start(Tag::Strong) => style.bold = true,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::Start(Tag::Link { dest_url, .. }) => {
                style.link = true;
                link = Some((block.text.len(), dest_url.into_string()));
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                append(&mut block, "[Image: ", style);
                link = Some((block.text.len(), dest_url.into_string()));
            }
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::End(TagEnd::Link) => {
                style.link = false;
                if let Some((start, url)) = link.take() {
                    block.links.push((start..block.text.len(), url));
                }
            }
            Event::End(TagEnd::Image) => {
                append(&mut block, "]", style);
                if let Some((start, url)) = link.take() {
                    block.links.push((start..block.text.len(), url));
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                ordered.pop();
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::CodeBlock
                | TagEnd::Item
                | TagEnd::TableRow
                | TagEnd::TableHead,
            ) => flush(&mut blocks, &mut block),
            Event::End(TagEnd::Table) => {
                flush(&mut blocks, &mut block);
                table = false;
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                append(&mut block, &text, style)
            }
            Event::Code(text) => append(
                &mut block,
                &text,
                Inline {
                    code: true,
                    ..style
                },
            ),
            Event::SoftBreak | Event::HardBreak => append(&mut block, "\n", style),
            Event::TaskListMarker(checked) => {
                append(&mut block, if checked { "☑ " } else { "☐ " }, style)
            }
            Event::Rule => {
                flush(&mut blocks, &mut block);
                append(&mut block, "────────", style);
                flush(&mut blocks, &mut block);
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut block);
    blocks
}

pub fn markdown<T: Into<gpui::SharedString>>(
    id: T,
    source: &str,
    cx: &App,
) -> impl IntoElement + use<T> {
    let t = cx.theme().clone();
    let id = id.into();
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_size(px(13.))
        .text_color(t.text_soft)
        .children(parse(source).into_iter().enumerate().map(|(i, block)| {
            if matches!(block.kind, Kind::Table) {
                return div()
                    .w_full()
                    .max_w(px(720.))
                    .flex()
                    .border_b_1()
                    .border_color(t.border)
                    .children(block.text.split('\u{001f}').map(|cell| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .px(px(10.))
                            .py(px(7.))
                            .text_size(px(12.))
                            .when(block.table_header, |el| {
                                el.font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text)
                                    .bg(t.input)
                            })
                            .child(cell.to_string())
                    }))
                    .into_any_element();
            }
            let highlights = block.styles.into_iter().map(|(range, style)| {
                (
                    range,
                    HighlightStyle {
                        font_weight: style.bold.then_some(FontWeight::BOLD),
                        font_style: style.italic.then_some(FontStyle::Italic),
                        color: if style.link {
                            Some(t.accent_text)
                        } else if style.code {
                            Some(t.code_text)
                        } else {
                            None
                        },
                        background_color: style.code.then_some(t.input),
                        strikethrough: style.strike.then_some(gpui::StrikethroughStyle::default()),
                        ..Default::default()
                    },
                )
            });
            let ranges = block.links.iter().map(|(range, _)| range.clone()).collect();
            let urls: Vec<_> = block.links.into_iter().map(|(_, url)| url).collect();
            let text = InteractiveText::new(
                (id.clone(), i),
                StyledText::new(block.text).with_highlights(highlights),
            )
            .on_click(ranges, move |index, _, cx| {
                if let Some(url) = urls.get(index)
                    && (url.starts_with("https://") || url.starts_with("http://"))
                {
                    cx.open_url(url);
                }
            });
            let mut row = div().min_w_0().child(text);
            match block.kind {
                Kind::Heading => {
                    row = row
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(16.))
                        .text_color(t.text)
                }
                Kind::Code => {
                    row = row
                        .p(px(10.))
                        .rounded(px(8.))
                        .bg(t.input)
                        .font_family(t.mono.clone())
                        .text_size(px(12.))
                }
                Kind::Quote => row = row.pl(px(12.)).border_l_2().border_color(t.border_bright),
                Kind::Table => row = row.font_family(t.mono.clone()).text_size(px(12.)),
                _ => {}
            }
            row.into_any_element()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_markdown_content_and_safe_links() {
        let blocks = parse(
            "# Title\n\n**Bold** and [link](https://github.com)\n\n- [x] Done\n\n```rs\nlet x = 1;\n```\n\n<script>alert(1)</script>",
        );
        let text = blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("Title")
                && text.contains("Bold")
                && text.contains("☑ Done")
                && text.contains("let x = 1;")
                && text.contains("<script>")
        );
        assert!(blocks.iter().any(|b| !b.links.is_empty()));
    }
}
