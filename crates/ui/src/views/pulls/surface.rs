//! PR diffs use the same row layout, colour mapping, syntax spans and word
//! highlighting as local diffs, with an explicitly separate review gutter.

use super::markdown::markdown;
use crate::app::prefs;
use crate::components::button::button;
use crate::model::pulls::{PullEvent, PullRequestModel};
use crate::theme::{ActiveTheme, Theme};
use crate::views::diff::{Row, build_rows, row_tone, text::styled_line};
use gpui::{
    AnyElement, Context, Entity, IntoElement, ListAlignment, ListState, MouseButton, ParentElement,
    Render, Styled, StyledText, Window, div, list, prelude::*, px,
};
use std::rc::Rc;
use std::sync::Arc;
use wackdepot_core::{
    diff::{DiffFileStatus, DiffLine, DiffLineKind, FileDiff},
    words,
};
use wackdepot_github::{BlobContent, ChangedFile, DiffSide, RepositoryId, ReviewAnchor};

type Highlight = Option<Arc<Vec<wackdepot_syntax::LineSpans>>>;
pub(super) struct DiffRequest {
    pub key: String,
    pub changed: ChangedFile,
    pub target: RepositoryId,
    pub head_repo: RepositoryId,
    pub base: String,
    pub head: String,
    pub reviewable: bool,
    pub local: Option<std::path::PathBuf>,
}
struct Document {
    file: FileDiff,
    words: Vec<Vec<Option<words::Ranges>>>,
    old: Highlight,
    new: Highlight,
    head: String,
    reviewable: bool,
}

pub struct PullDiff {
    model: Entity<PullRequestModel>,
    document: Option<Arc<Document>>,
    rows: Rc<Vec<Row>>,
    list: ListState,
    pub key: String,
    loading: bool,
    error: Option<String>,
    split: bool,
    generation: u64,
}
impl PullDiff {
    pub fn new(model: Entity<PullRequestModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            document: None,
            rows: Rc::new(Vec::new()),
            list: ListState::new(0, ListAlignment::Top, px(500.)),
            key: String::new(),
            loading: false,
            error: None,
            split: prefs::get(cx).diff_split,
            generation: 0,
        }
    }
    pub fn load(&mut self, request: DiffRequest, cx: &mut Context<Self>) {
        let DiffRequest {
            key,
            changed,
            target,
            head_repo,
            base,
            head,
            reviewable,
            local,
        } = request;
        if self.key == key {
            return;
        }
        self.key = key;
        self.generation += 1;
        let generation = self.generation;
        self.document = None;
        self.loading = true;
        self.error = None;
        let client = self.model.read(cx).session.read(cx).client.clone();
        cx.spawn(async move |this,cx| {
            let result=cx.background_executor().spawn(async move {
                if !changed.patch_complete() {
                    let kind = if changed.patch.as_ref().is_some_and(|p|p.len()>512*1024) {
                        BlobContent::TooLarge
                    } else if local.is_some() && changed.patch.is_none() {
                        BlobContent::Binary
                    } else if changed.status == "removed" {
                        client.blob_content(&target, &base, changed.previous_filename.as_deref().unwrap_or(&changed.filename)).unwrap_or(BlobContent::Unavailable)
                    } else {
                        client.blob_content(&head_repo, &head, &changed.filename).unwrap_or(BlobContent::Unavailable)
                    };
                    return Err(match kind {
                        BlobContent::Binary => "This binary file has no text diff. Open it on GitHub to inspect the change.",
                        BlobContent::TooLarge => "This file exceeds WackDepot's text budget. Open the complete change on GitHub.",
                        BlobContent::Text(_) => "GitHub supplied an incomplete patch for this file. Open it on GitHub to inspect the complete change.",
                        BlobContent::Unavailable => "GitHub hasn't supplied this file's text patch. Open it on GitHub to inspect the complete change.",
                    }.to_string());
                }
                let patch=changed.patch.as_deref().unwrap_or_default();
                let parsed=wackdepot_core::diff::parse_diff(&format!("diff --git a/file b/file\n--- a/file\n+++ b/file\n{patch}\n"));
                let mut file=parsed.into_iter().next().unwrap_or(FileDiff {old_path:None,new_path:None,status:DiffFileStatus::Modified,header_text:String::new(),hunks:Vec::new(),is_binary:false});
                file.old_path=if changed.status=="added"{None}else{Some(changed.previous_filename.clone().unwrap_or_else(||changed.filename.clone()))};
                file.new_path=if changed.status=="removed"{None}else{Some(changed.filename.clone())};
                file.status=match changed.status.as_str(){"added"=>DiffFileStatus::Added,"removed"=>DiffFileStatus::Deleted,"renamed"|"copied"=>DiffFileStatus::Renamed,_=>DiffFileStatus::Modified};
                let versions=if let Some(path)=local{wackdepot_core::blob::versions_between(&path,&base,&head,file.old_path.as_deref(),file.new_path.as_deref())}else{wackdepot_core::blob::Versions {old:if base.is_empty(){None}else{file.old_path.as_deref().and_then(|path|client.blob(&target,&base,path).ok().flatten())},new:file.new_path.as_deref().and_then(|path|client.blob(&head_repo,&head,path).ok().flatten())}};
                let old=versions.old.and_then(|text|wackdepot_syntax::highlight(&changed.filename,&text)).map(Arc::new);
                let new=versions.new.and_then(|text|wackdepot_syntax::highlight(&changed.filename,&text)).map(Arc::new);
                let words=file.hunks.iter().map(words::hunk_word_ranges).collect();
                Ok(Document {file,words,old,new,head,reviewable})
            }).await;
            let _=this.update(cx,|this,cx|{if this.generation!=generation{return;}this.loading=false;match result{Ok(document)=>{this.rows=Rc::new(build_rows(&document.file,this.split));this.list.reset(this.rows.len());this.document=Some(Arc::new(document));},Err(error)=>this.error=Some(error)}cx.notify();});
        }).detach();
        cx.notify();
    }
    fn gutter(
        &self,
        number: Option<u32>,
        side: DiffSide,
        key: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.theme().clone();
        let Some(number) = number else {
            return div().w(px(44.)).flex_none().into_any_element();
        };
        let doc = self.document.as_ref().unwrap();
        let path = doc.file.path().to_string();
        let head = doc.head.clone();
        let reviewable = doc.reviewable;
        let picked = self.model.read(cx).inline.as_ref().is_some_and(|a| {
            a.path == path
                && a.side == side
                && number >= a.start_line.unwrap_or(a.line)
                && number <= a.line
        });
        div()
            .id((
                if side == DiffSide::Left {
                    "pr-old"
                } else {
                    "pr-new"
                },
                key,
            ))
            .w(px(44.))
            .h(px(22.))
            .flex_none()
            .pr(px(8.))
            .text_size(px(11.))
            .text_color(if picked { t.accent_text } else { t.text_faint })
            .bg(if picked { t.selected_fill() } else { t.bg })
            .flex()
            .items_center()
            .justify_end()
            .child(number.to_string())
            .when(reviewable, |el| {
                el.cursor_pointer()
                    .hover(|s| s.text_color(t.accent_text))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            let previous = this.model.read(cx).inline.clone();
                            let range = previous
                                .filter(|a| {
                                    event.modifiers.shift
                                        && a.path == path
                                        && a.side == side
                                        && a.head_sha == head
                                })
                                .map(|a| {
                                    (
                                        a.start_line.unwrap_or(a.line).min(number),
                                        a.line.max(number),
                                    )
                                });
                            let (start, end) = range.unwrap_or((number, number));
                            let contiguous = this.document.as_ref().is_some_and(|doc| {
                                doc.file.hunks.iter().any(|h| {
                                    h.lines.iter().any(|l| {
                                        if side == DiffSide::Left {
                                            l.old_lineno == Some(start)
                                        } else {
                                            l.new_lineno == Some(start)
                                        }
                                    }) && h.lines.iter().any(|l| {
                                        if side == DiffSide::Left {
                                            l.old_lineno == Some(end)
                                        } else {
                                            l.new_lineno == Some(end)
                                        }
                                    })
                                })
                            });
                            this.model.update(cx, |model, cx| {
                                model.inline = Some(ReviewAnchor {
                                    path: path.clone(),
                                    side,
                                    line: if contiguous { end } else { number },
                                    start_line: (contiguous && start < end).then_some(start),
                                    head_sha: head.clone(),
                                });
                                cx.emit(PullEvent::Draft);
                                cx.notify();
                            });
                        }),
                    )
            })
            .into_any_element()
    }
    fn code(&self, line: &DiffLine, hunk: usize, index: usize, t: &Theme) -> AnyElement {
        let doc = self.document.as_ref().unwrap();
        let (highlight, number) = if line.kind == DiffLineKind::Removed {
            (&doc.old, line.old_lineno)
        } else {
            (&doc.new, line.new_lineno)
        };
        let spans = highlight
            .as_ref()
            .and_then(|all| number.and_then(|n| all.get(n.saturating_sub(1) as usize)));
        let words = doc
            .words
            .get(hunk)
            .and_then(|all| all.get(index))
            .and_then(|w| w.as_deref());
        let word_bg = if line.kind == DiffLineKind::Removed {
            t.sem.diff_del_word
        } else {
            t.sem.diff_add_word
        };
        let (text, highlights) = styled_line(t, &line.text, spans, words, word_bg);
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(t.code_text)
            .child(StyledText::new(text).with_highlights(highlights))
            .into_any_element()
    }
    fn row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let doc = self.document.clone().unwrap();
        let row = self.rows[index];
        let (hunk, left, right) = match row {
            Row::Hunk { hunk } => {
                return div()
                    .h(px(32.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .font_family(t.mono.clone())
                    .text_size(px(11.))
                    .text_color(t.sem.diff_hunk)
                    .bg(t.sem.diff_hunk_bg)
                    .child(doc.file.hunks[hunk].header.clone())
                    .into_any_element();
            }
            Row::Line { hunk, line } => (hunk, Some(line), None),
            Row::Pair { hunk, left, right } => (hunk, left, right),
        };
        let lines = &doc.file.hunks[hunk].lines;
        let mut body = div()
            .flex()
            .w_full()
            .min_h(px(22.))
            .font_family(t.mono.clone())
            .text_size(px(12.));
        let mut anchors = Vec::new();
        if self.split {
            for (side, line_index) in [(DiffSide::Left, left), (DiffSide::Right, right)] {
                let mut cell = div().flex().flex_1().min_w_0().h(px(22.)).items_center();
                if let Some(li) = line_index {
                    let line = &lines[li];
                    let number = if side == DiffSide::Left {
                        line.old_lineno
                    } else {
                        line.new_lineno
                    };
                    let (bg, _, _) = row_tone(&t, line.kind);
                    if let Some(bg) = bg {
                        cell = cell.bg(bg);
                    }
                    cell = cell
                        .child(self.gutter(
                            number,
                            side,
                            index * 2 + usize::from(side == DiffSide::Right),
                            cx,
                        ))
                        .child(self.code(line, hunk, li, &t));
                    if let Some(n) = number {
                        anchors.push((side, n));
                    }
                }
                if side == DiffSide::Left {
                    cell = cell.border_r_1().border_color(t.border);
                }
                body = body.child(cell);
            }
        } else if let Some(li) = left {
            let line = &lines[li];
            let (bg, _, sigil) = row_tone(&t, line.kind);
            if let Some(bg) = bg {
                body = body.bg(bg);
            }
            body = body
                .child(self.gutter(line.old_lineno, DiffSide::Left, index, cx))
                .child(self.gutter(line.new_lineno, DiffSide::Right, index, cx))
                .child(
                    div()
                        .w(px(16.))
                        .flex_none()
                        .text_color(t.text_faint)
                        .child(sigil),
                )
                .child(self.code(line, hunk, li, &t));
            if let Some(n) = line.old_lineno {
                anchors.push((DiffSide::Left, n));
            }
            if let Some(n) = line.new_lineno {
                anchors.push((DiffSide::Right, n));
            }
        }
        let threads = self
            .model
            .read(cx)
            .detail
            .as_ref()
            .map(|d| {
                d.reviews
                    .threads
                    .iter()
                    .filter(|thread| {
                        !thread.outdated
                            && thread.path == doc.file.path()
                            && thread
                                .line
                                .is_some_and(|n| anchors.contains(&(thread.side, n)))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut row = div().flex().flex_col().w_full().child(body);
        for thread in threads {
            let model = self.model.clone();
            let id = thread.id.clone();
            let mut bubble = div()
                .m(px(8.))
                .p(px(12.))
                .rounded(px(10.))
                .bg(t.surface)
                .border_1()
                .border_color(t.border)
                .flex()
                .flex_col()
                .gap(px(8.));
            for (ci, comment) in thread.comments.iter().enumerate() {
                bubble = bubble
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.text_dim)
                            .child(format!(
                                "{}{}",
                                comment.author,
                                if comment.pending {
                                    " · Pending review"
                                } else {
                                    ""
                                }
                            )),
                    )
                    .child(markdown(
                        format!("inline-{}-{ci}", thread.id),
                        &comment.body,
                        cx,
                    ));
            }
            bubble = bubble.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(t.text_dim)
                            .child(if thread.resolved {
                                "Resolved"
                            } else {
                                "Discussion"
                            }),
                    )
                    .child(button(("reply-inline", index), "Reply").small().on_click(
                        move |_, _, cx| {
                            model.update(cx, |m, cx| {
                                m.active_thread = Some(id.clone());
                                m.draft_changed(cx);
                            })
                        },
                    )),
            );
            row = row.child(bubble);
        }
        row.into_any_element()
    }
}
impl Render for PullDiff {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let split = prefs::get(cx).diff_split;
        if self.split != split {
            self.split = split;
            if let Some(doc) = &self.document {
                self.rows = Rc::new(build_rows(&doc.file, split));
                self.list.reset(self.rows.len());
            }
        }
        let t = cx.theme().clone();
        if let Some(error) = &self.error {
            return div()
                .p(px(18.))
                .text_color(t.text_dim)
                .text_size(px(13.))
                .child(error.clone())
                .into_any_element();
        }
        if self.loading {
            return div()
                .p(px(18.))
                .text_color(t.text_dim)
                .child("Loading this file…")
                .into_any_element();
        }
        if self.document.is_none() {
            return div()
                .p(px(18.))
                .text_color(t.text_dim)
                .child("Choose a file to inspect its changes.")
                .into_any_element();
        }
        if self.rows.is_empty() {
            return div()
                .p(px(18.))
                .text_color(t.text_dim)
                .child("This file has no text changes.")
                .into_any_element();
        }
        list(
            self.list.clone(),
            cx.processor(|this, ix, _, cx| this.row(ix, cx)),
        )
        .size_full()
        .into_any_element()
    }
}
