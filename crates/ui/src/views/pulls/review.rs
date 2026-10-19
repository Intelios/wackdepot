use super::markdown::markdown;
use super::workspace::{Field, PullWorkspace};
use crate::app::prefs;
use crate::components::button::button;
use crate::components::pill_track::pill_track;
use crate::model::{BusyOp, Done};
use crate::theme::ActiveTheme;
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, Window, div, prelude::*,
    px,
};
use wackdepot_github::{self as gh, MergeInput, ReviewEvent, ReviewThread};

impl PullWorkspace {
    pub(super) fn overview(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let detail = model.detail.clone().unwrap();
        let busy = model.busy || model.reconciling;
        let target = model.target.clone().unwrap();
        let unsent = model.draft.comments.clone();
        let has_checkout = model.last_checkout.is_some();
        let mut body = div()
            .w_full()
            .max_w(px(920.))
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(16.));
        if let Some(description) = detail.pull.body.as_ref().filter(|b| !b.is_empty()) {
            body = body.child(markdown("pr-description", description, cx));
        } else {
            body = body.child(
                div()
                    .text_size(px(13.))
                    .text_color(t.text_dim)
                    .child("No description provided."),
            );
        }
        let reviewers = detail
            .pull
            .requested_reviewers
            .iter()
            .map(|u| u.login.clone())
            .chain(
                detail
                    .pull
                    .requested_teams
                    .iter()
                    .map(|team| format!("@{}", team.slug)),
            )
            .collect::<Vec<_>>()
            .join(", ");
        let labels = detail
            .pull
            .labels
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        body = body.child(
            div()
                .p(px(12.))
                .rounded(px(10.))
                .bg(t.input)
                .text_size(px(12.))
                .text_color(t.text_dim)
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(format!(
                    "Reviewers: {}",
                    if reviewers.is_empty() {
                        "None requested"
                    } else {
                        &reviewers
                    }
                ))
                .child(format!(
                    "Labels: {}",
                    if labels.is_empty() { "None" } else { &labels }
                ))
                .child(format!(
                    "Review decision: {}",
                    match detail.reviews.decision.as_str() {
                        "APPROVED" => "Approved",
                        "CHANGES_REQUESTED" => "Changes requested",
                        "REVIEW_REQUIRED" => "Review required",
                        _ => "No decision yet",
                    }
                )),
        );
        let mut timeline = Vec::new();
        for comment in &detail.comments {
            timeline.push((
                comment.created_at.clone(),
                comment.user.login.clone(),
                comment.body.clone(),
                "Comment".to_string(),
            ));
        }
        for review in &detail.reviews.submitted {
            timeline.push((
                review.submitted_at.clone().unwrap_or_default(),
                review.user.login.clone(),
                review.body.clone(),
                match review.state.as_str() {
                    "APPROVED" => "Approved",
                    "CHANGES_REQUESTED" => "Requested changes",
                    _ => "Reviewed",
                }
                .to_string(),
            ));
        }
        timeline.sort_by(|a, b| a.0.cmp(&b.0));
        for (i, (when, author, text, state)) in timeline.iter().enumerate() {
            body = body.child(
                div()
                    .p(px(14.))
                    .rounded(px(12.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_dim)
                            .child(format!(
                                "{author} · {state} · {}",
                                crate::views::time::long_age(when)
                            )),
                    )
                    .when(!text.is_empty(), |el| {
                        el.child(markdown(format!("conversation-{i}"), text, cx))
                    }),
            );
        }
        for thread in &detail.reviews.threads {
            body = body.child(self.thread(thread, cx));
        }
        for (i, comment) in unsent.iter().enumerate() {
            let anchor = comment.anchor.clone();
            let restore = self.model.clone();
            body = body.child(
                div()
                    .p(px(12.))
                    .bg(t.input)
                    .rounded(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.sem.warning_text)
                            .child(format!(
                                "Unsent comment · {}:{}{}",
                                anchor.path,
                                anchor.line,
                                if anchor.head_sha != detail.pull.head.sha {
                                    " · Older revision"
                                } else {
                                    ""
                                }
                            )),
                    )
                    .child(markdown(format!("unsent-{i}"), &comment.body, cx))
                    .child(
                        button(("restore-inline", i), "Continue this comment")
                            .small()
                            .on_click(move |_, _, cx| {
                                restore.update(cx, |m, cx| {
                                    m.inline = Some(anchor.clone());
                                    m.draft_changed(cx);
                                })
                            }),
                    ),
            );
        }
        body = body.child(self.text_field("Join the conversation", Field::Conversation, cx));
        let post = self.model.clone();
        body = body.child(
            button("pr-post-comment", "Post comment")
                .disabled(busy || self.content(Field::Conversation, cx).trim().is_empty())
                .on_click(move |_, _, cx| post.update(cx, |m, cx| m.post_conversation(cx))),
        );
        body = body.child(self.review_composer(cx));
        let blocker = detail.merge_blocker(&target);
        let mut merge = div()
            .p(px(16.))
            .rounded(px(12.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .gap(px(10.));
        merge = merge.child(div().font_weight(FontWeight::SEMIBOLD).child(
            if detail.pull.merged_at.is_some() {
                "Pull request merged"
            } else {
                "Merge requirements"
            },
        ));
        if let Some(blocker) = blocker.clone() {
            merge = merge.child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_dim)
                    .child(blocker),
            );
        } else {
            merge = merge.child(
                div()
                    .text_size(px(12.))
                    .text_color(t.sem.success)
                    .child("GitHub's current requirements allow this pull request to merge."),
            );
        }
        if detail.pull.state == "open" && detail.pull.merged_at.is_none() {
            let methods = target.merge_methods();
            let saved = prefs::get(cx)
                .github_merge_methods
                .get(&target.identity().full_name())
                .copied();
            let selected = saved
                .filter(|method| methods.contains(method))
                .or_else(|| methods.first().copied())
                .unwrap_or_default();
            let mut track = pill_track("pr-merge-method");
            for method in &methods {
                track = track.item(method.label());
            }
            let target_name = target.identity().full_name();
            let chosen = methods.clone();
            track = track
                .selected(methods.iter().position(|m| *m == selected).unwrap_or(0))
                .on_select(move |index, _, cx| {
                    if let Some(method) = chosen.get(index) {
                        prefs::update(cx, |p| {
                            p.github_merge_methods.insert(target_name.clone(), *method);
                        });
                        cx.refresh_windows();
                    }
                });
            if !methods.is_empty() {
                merge = merge
                    .child(track)
                    .child(self.text_field("Merge commit title (optional)", Field::MergeTitle, cx))
                    .child(self.text_field(
                        "Merge commit description (optional)",
                        Field::MergeBody,
                        cx,
                    ));
            }
            let confirm = cx.entity();
            merge = merge.child(
                button("pr-merge", selected.label())
                    .primary()
                    .disabled(busy || blocker.is_some())
                    .on_click(move |_, _, cx| {
                        confirm.update(cx, |this, cx| this.request_merge(selected, cx))
                    }),
            );
            if detail.reviews.merge_state == "BEHIND" && detail.reviews.can_update {
                let update = cx.entity();
                merge = merge.child(
                    button("pr-update-branch", "Update branch")
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            update.update(cx, |this, cx| this.request_update(cx))
                        }),
                );
            }
        }
        let mut lifecycle = div().flex().flex_wrap().gap(px(8.));
        if detail.reviews.can_update && detail.pull.merged_at.is_none() {
            let draft = self.model.clone();
            let node = detail.pull.node_id.clone();
            let is_draft = detail.pull.draft;
            let state = cx.entity();
            let open = detail.pull.state == "open";
            if open {
                lifecycle = lifecycle.child(
                    button(
                        "pr-draft-state",
                        if is_draft {
                            "Mark ready for review"
                        } else {
                            "Convert to draft"
                        },
                    )
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        draft.update(cx, |m, cx| {
                            let node = node.clone();
                            m.perform(
                                "Updating draft state…",
                                move |client, _, _| client.set_draft(&node, !is_draft),
                                |m, _, cx| m.refresh_detail(cx),
                                cx,
                            );
                        })
                    }),
                );
            }
            lifecycle = lifecycle.child(
                button(
                    "pr-open-state",
                    if open {
                        "Close pull request"
                    } else {
                        "Reopen pull request"
                    },
                )
                .disabled(busy)
                .on_click(move |_, _, cx| {
                    state.update(cx, |this, cx| this.request_state(!open, cx))
                }),
            );
        }
        if detail.pull.merged_at.is_some() && has_checkout {
            let cleanup = cx.entity();
            lifecycle = lifecycle.child(
                button("pr-cleanup", "Clean up local branch")
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        cleanup.update(cx, |this, cx| this.request_cleanup(cx))
                    }),
            );
        }
        div()
            .id("pr-overview")
            .track_scroll(&self.scroll[0])
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .child(body.child(merge).child(lifecycle))
            .into_any_element()
    }

    fn thread(&self, thread: &ReviewThread, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let mut panel = div()
            .p(px(14.))
            .rounded(px(12.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .gap(px(8.));
        let path = thread.path.clone();
        let goto = cx.entity();
        panel = panel.child(
            button(
                format!("thread-file-{}", thread.id),
                format!(
                    "{}:{}{}{}",
                    thread.path,
                    thread.line.unwrap_or_default(),
                    if thread.outdated { " · Outdated" } else { "" },
                    if thread.resolved { " · Resolved" } else { "" }
                ),
            )
            .small()
            .ghost()
            .on_click(move |_, _, cx| {
                goto.update(cx, |this, cx| {
                    this.section = 1;
                    this.selected_file = Some(path.clone());
                    cx.notify();
                })
            }),
        );
        for comment in &thread.comments {
            let mut actions = div().flex().items_center().gap(px(7.)).child(
                div()
                    .flex_1()
                    .text_size(px(11.))
                    .text_color(t.text_dim)
                    .child(format!(
                        "{} · {}{}",
                        comment.author,
                        crate::views::time::long_age(&comment.created_at),
                        if comment.pending { " · Pending" } else { "" }
                    )),
            );
            if comment.can_update {
                let model = self.model.clone();
                let id = comment.id.clone();
                let text = comment.body.clone();
                actions = actions.child(
                    button(format!("edit-comment-{}", comment.id), "Edit")
                        .small()
                        .ghost()
                        .on_click(move |_, _, cx| {
                            model.update(cx, |m, cx| {
                                let key = format!("edit:{id}");
                                m.active_thread = Some(key.clone());
                                m.draft.replies.insert(key, text.clone());
                                m.draft_changed(cx);
                            })
                        }),
                );
            }
            if comment.can_delete {
                let view = cx.entity();
                let id = comment.id.clone();
                actions = actions.child(
                    button(format!("delete-comment-{}", comment.id), "Delete")
                        .small()
                        .ghost()
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                let model = this.model.clone();
                                let id = id.clone();
                                this.confirm(
                                    "Delete this review comment?".into(),
                                    "The comment will be removed from GitHub.".into(),
                                    "Delete",
                                    true,
                                    move |cx| {
                                        model.update(cx, |m, cx| {
                                            m.perform(
                                                "Deleting review comment…",
                                                move |client, _, _| {
                                                    client.delete_review_comment(&id)
                                                },
                                                |m, _, cx| m.refresh_detail(cx),
                                                cx,
                                            )
                                        })
                                    },
                                    cx,
                                );
                            })
                        }),
                );
            }
            panel = panel.child(actions).child(markdown(
                format!("thread-comment-{}", comment.id),
                &comment.body,
                cx,
            ));
        }
        let model = self.model.clone();
        let id = thread.id.clone();
        let mut actions = div().flex().gap(px(8.)).child(
            button(format!("thread-reply-{}", thread.id), "Reply")
                .small()
                .on_click(move |_, _, cx| {
                    model.update(cx, |m, cx| {
                        m.active_thread = Some(id.clone());
                        m.draft_changed(cx);
                    })
                }),
        );
        if thread.can_resolve && !thread.resolved || thread.can_unresolve && thread.resolved {
            let model = self.model.clone();
            let id = thread.id.clone();
            let resolved = thread.resolved;
            actions = actions.child(
                button(
                    format!("thread-resolve-{}", thread.id),
                    if resolved { "Unresolve" } else { "Resolve" },
                )
                .small()
                .disabled(self.model.read(cx).busy || self.model.read(cx).reconciling)
                .on_click(move |_, _, cx| {
                    model.update(cx, |m, cx| {
                        let id = id.clone();
                        m.perform(
                            "Updating discussion…",
                            move |client, _, _| client.resolve(&id, !resolved),
                            |m, _, cx| m.refresh_detail(cx),
                            cx,
                        );
                    })
                }),
            );
        }
        panel.child(actions).into_any_element()
    }

    fn review_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let busy = model.busy || model.reconciling;
        let detail = model.detail.clone().unwrap();
        let pending = detail.reviews.pending_id.clone();
        let own = detail.pull.user.login.eq_ignore_ascii_case(
            &model
                .session
                .read(cx)
                .user
                .as_ref()
                .map(|u| u.login.clone())
                .unwrap_or_default(),
        );
        let mut panel = div()
            .p(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_color(t.border)
            .bg(t.surface)
            .flex()
            .flex_col()
            .gap(px(10.));
        if !model.draft.review_head.is_empty() && model.draft.review_head != detail.pull.head.sha {
            let head = detail.pull.head.sha.clone();
            let model = self.model.clone();
            panel = panel.child(div().text_size(px(12.)).text_color(t.sem.warning_text)
                .child("New commits arrived. Inspect their changes before continuing this review."))
                .child(button("pr-acknowledge-head", "I've reviewed the latest commits").small().on_click(move |_, _, cx| {
                    model.update(cx, |m, cx| { m.edit_draft(|d| d.review_head=head.clone(), cx); m.draft_changed(cx); });
                }));
        }
        if let Some(anchor) = &model.inline {
            panel = panel
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.text_dim)
                        .child(format!(
                            "{} · {} lines {}–{} · Shift-click a gutter number to select a range",
                            anchor.path,
                            anchor.side.api(),
                            anchor.start_line.unwrap_or(anchor.line),
                            anchor.line
                        )),
                )
                .child(self.text_field("Comment on these lines", Field::Inline, cx));
            let add = self.model.clone();
            let now = self.model.clone();
            let text = self.content(Field::Inline, cx);
            let first = text.clone();
            let stale = anchor.head_sha != detail.pull.head.sha;
            panel = panel.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button("pr-add-review", "Add to review")
                            .primary()
                            .disabled(busy || stale || text.trim().is_empty())
                            .on_click(move |_, _, cx| {
                                add.update(cx, |m, cx| m.add_inline(first.clone(), false, cx))
                            }),
                    )
                    .child(
                        button("pr-comment-now", "Comment now")
                            .disabled(busy || stale || text.trim().is_empty())
                            .on_click(move |_, _, cx| {
                                now.update(cx, |m, cx| m.add_inline(text.clone(), true, cx))
                            }),
                    ),
            );
        }
        if let Some(thread) = model.active_thread.clone() {
            let edit = thread.starts_with("edit:");
            let reply = self.model.clone();
            panel = panel
                .child(self.text_field(
                    if edit {
                        "Edit review comment"
                    } else {
                        "Reply to discussion"
                    },
                    Field::Reply,
                    cx,
                ))
                .child(
                    button(
                        "pr-send-reply",
                        if edit { "Save comment" } else { "Post reply" },
                    )
                    .disabled(busy || self.content(Field::Reply, cx).trim().is_empty())
                    .on_click(move |_, _, cx| {
                        reply.update(cx, |m, cx| m.post_reply(thread.clone(), cx))
                    }),
                );
        }
        panel = panel.child(self.text_field(
            if pending.is_some() {
                "Finish your pending review"
            } else {
                "Review summary"
            },
            Field::Review,
            cx,
        ));
        let mut actions = div().flex().flex_wrap().gap(px(8.));
        for (i, (event, label)) in [
            (ReviewEvent::Comment, "Submit comments"),
            (ReviewEvent::Approve, "Approve"),
            (ReviewEvent::RequestChanges, "Request changes"),
        ]
        .into_iter()
        .enumerate()
        {
            let model = self.model.clone();
            actions = actions.child(
                button(("pr-submit-review", i), label)
                    .disabled(
                        busy || own && event != ReviewEvent::Comment
                            || event == ReviewEvent::RequestChanges
                                && model.read(cx).draft.review_body.trim().is_empty(),
                    )
                    .on_click(move |_, _, cx| model.update(cx, |m, cx| m.submit_review(event, cx))),
            );
        }
        if let Some(pending) = pending {
            let view = cx.entity();
            actions = actions.child(
                button("pr-discard-review", "Discard pending review")
                    .small()
                    .danger()
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            let model = this.model.clone();
                            let pending = pending.clone();
                            this.confirm(
                                "Discard this pending review?".into(),
                                "All unpublished comments in this GitHub review will be removed."
                                    .into(),
                                "Discard review",
                                true,
                                move |cx| {
                                    model.update(cx, |m, cx| {
                                        m.perform(
                                            "Discarding pending review…",
                                            move |client, _, _| client.discard_review(&pending),
                                            |m, _, cx| {
                                                m.draft.review_body.clear();
                                                m.refresh_detail(cx);
                                                m.draft_changed(cx);
                                            },
                                            cx,
                                        )
                                    })
                                },
                                cx,
                            );
                        })
                    }),
            );
        }
        panel.child(actions).into_any_element()
    }

    pub(super) fn files(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let detail = model.detail.clone().unwrap();
        let target = model.target.as_ref().unwrap().identity();
        let head_repo = detail
            .pull
            .head
            .repo
            .as_ref()
            .map(|r| r.identity())
            .unwrap_or_else(|| target.clone());
        if self
            .selected_file
            .as_ref()
            .is_none_or(|path| !detail.files.iter().any(|f| &f.filename == path))
        {
            self.selected_file = detail.files.first().map(|f| f.filename.clone());
        }
        let mut file_list = div()
            .id("pr-files-list")
            .w(px(190.))
            .flex_none()
            .p(px(6.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_r_1()
            .border_color(t.border);
        for (i, file) in detail.files.iter().enumerate() {
            let path = file.filename.clone();
            let selected = self.selected_file.as_ref() == Some(&path);
            file_list = file_list.child(
                div()
                    .id(("pr-file", i))
                    .p(px(8.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(if selected { t.selected_fill() } else { t.bg })
                    .hover(|s| s.bg(t.pill_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_file = Some(path.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .child(file.filename.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(t.text_dim)
                            .child(format!(
                                "+{} −{}{}",
                                file.additions,
                                file.deletions,
                                if detail.reviews.viewed.contains(&file.filename) {
                                    " · Viewed"
                                } else {
                                    ""
                                }
                            )),
                    ),
            );
        }
        let mut pane = div().flex_1().min_w_0().min_h_0().flex().flex_col();
        if let Some(file) = self
            .selected_file
            .as_ref()
            .and_then(|path| detail.files.iter().find(|f| &f.filename == path))
            .cloned()
        {
            let path = file.filename.clone();
            let viewed = detail.reviews.viewed.contains(&path);
            let node = detail.pull.node_id.clone();
            let model = self.model.clone();
            let url = detail.pull.html_url.clone();
            let key = format!(
                "{}:{}:{}:{}:{}",
                target.full_name(),
                detail.pull.number,
                detail.pull.base.sha,
                detail.pull.head.sha,
                path
            );
            let base = detail.merge_base_sha.clone();
            let head = detail.pull.head.sha.clone();
            self.diff.update(cx, |d, cx| {
                d.load(
                    super::surface::DiffRequest {
                        key,
                        changed: file,
                        target,
                        head_repo,
                        base,
                        head,
                        reviewable: true,
                        local: None,
                    },
                    cx,
                )
            });
            pane = pane
                .child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .border_b_1()
                        .border_color(t.border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(t.mono.clone())
                                .text_size(px(12.))
                                .child(path.clone()),
                        )
                        .child(
                            button(
                                "pr-file-viewed",
                                if viewed { "✓ Viewed" } else { "Mark viewed" },
                            )
                            .small()
                            .disabled(self.model.read(cx).busy || self.model.read(cx).reconciling)
                            .on_click(move |_, _, cx| {
                                model.update(cx, |m, cx| {
                                    let path = path.clone();
                                    let node = node.clone();
                                    m.perform(
                                        "Saving file progress…",
                                        move |client, _, _| client.viewed(&node, &path, !viewed),
                                        |m, _, cx| m.refresh_detail(cx),
                                        cx,
                                    );
                                })
                            }),
                        )
                        .child(
                            button("pr-file-web", "Open on GitHub")
                                .small()
                                .ghost()
                                .on_click(move |_, _, cx| cx.open_url(&format!("{url}/files"))),
                        ),
                )
                .child(div().flex_1().min_h_0().child(self.diff.clone()));
        }
        let mut content = div().flex_1().min_h_0().flex().flex_col();
        if !detail.files_complete {
            content=content.child(div().p(px(10.)).bg(t.sem.warning_bg).text_color(t.sem.warning_text).text_size(px(12.)).child("GitHub limited the file list. Open this PR on GitHub to inspect every file."));
        }
        content = content.child(div().flex_1().min_h_0().flex().child(file_list).child(pane));
        if self.model.read(cx).inline.is_some() || self.model.read(cx).active_thread.is_some() {
            content = content.child(
                div()
                    .id("pr-file-comment-editor")
                    .max_h(px(330.))
                    .overflow_y_scroll()
                    .p(px(10.))
                    .child(self.review_composer(cx)),
            );
        }
        content.into_any_element()
    }

    pub(super) fn commits(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let detail = self.model.read(cx).detail.clone().unwrap();
        div()
            .id("pr-commits")
            .track_scroll(&self.scroll[2])
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(detail.commits.iter().enumerate().map(|(i, commit)| {
                let url = commit.html_url.clone();
                div()
                    .p(px(14.))
                    .bg(t.surface)
                    .rounded(px(12.))
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(13.))
                                    .child(
                                        commit
                                            .commit
                                            .message
                                            .lines()
                                            .next()
                                            .unwrap_or_default()
                                            .to_string(),
                                    ),
                            )
                            .child(
                                button(
                                    ("pr-commit-web", i),
                                    commit.sha[..commit.sha.len().min(7)].to_string(),
                                )
                                .small()
                                .mono()
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                            ),
                    )
                    .child(markdown(
                        format!("pr-commit-body-{i}"),
                        &commit
                            .commit
                            .message
                            .lines()
                            .skip(1)
                            .collect::<Vec<_>>()
                            .join("\n"),
                        cx,
                    ))
            }))
            .into_any_element()
    }

    pub(super) fn checks(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let detail = model.detail.clone().unwrap();
        let busy = model.busy || model.reconciling;
        let can_rerun = model
            .target
            .as_ref()
            .is_some_and(|r| r.permissions.push || r.permissions.admin || r.permissions.maintain);
        let mut panel = div()
            .id("pr-checks")
            .track_scroll(&self.scroll[3])
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(detail.checks.state().label()),
            );
        for (i, check) in detail.checks.checks.iter().enumerate() {
            let url = check.url.clone();
            panel = panel.child(
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(t.surface)
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().flex_1().text_size(px(13.)).child(check.name.clone()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(match check.state {
                                gh::CheckState::Passing => t.sem.success,
                                gh::CheckState::Failing => t.sem.danger_text,
                                _ => t.text_dim,
                            })
                            .child(check.detail.clone()),
                    )
                    .when(!url.is_empty(), |el| {
                        el.child(
                            button(("pr-check-details", i), "Details")
                                .small()
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            );
        }
        for run in &detail.checks.workflows {
            let id = run.id;
            let show = self.model.clone();
            let all = cx.entity();
            let failed = cx.entity();
            let name = run.name.clone();
            let second_name = name.clone();
            panel = panel.child(
                div()
                    .p(px(12.))
                    .bg(t.input)
                    .rounded(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(div().text_size(px(13.)).child(format!(
                        "{} · {}",
                        run.name,
                        run.conclusion.as_deref().unwrap_or(&run.status)
                    )))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(
                                button(("pr-workflow-jobs", id), "Show jobs")
                                    .small()
                                    .disabled(busy)
                                    .on_click(move |_, _, cx| {
                                        show.update(cx, |m, cx| {
                                            m.perform(
                                                "Loading workflow jobs…",
                                                move |client, repo, _| client.jobs(&repo, id),
                                                |m, jobs, cx| {
                                                    if let Some(detail) = m.detail.as_mut() {
                                                        std::sync::Arc::make_mut(detail)
                                                            .checks
                                                            .jobs = jobs;
                                                    }
                                                    cx.notify();
                                                },
                                                cx,
                                            )
                                        })
                                    }),
                            )
                            .child(
                                button(("pr-rerun-all", id), "Rerun all")
                                    .small()
                                    .disabled(busy || !can_rerun)
                                    .on_click(move |_, _, cx| {
                                        all.update(cx, |this, cx| {
                                            this.request_rerun(id, false, name.clone(), false, cx)
                                        })
                                    }),
                            )
                            .child(
                                button(("pr-rerun-failed", id), "Rerun failed")
                                    .small()
                                    .disabled(busy || !can_rerun)
                                    .on_click(move |_, _, cx| {
                                        failed.update(cx, |this, cx| {
                                            this.request_rerun(
                                                id,
                                                true,
                                                second_name.clone(),
                                                false,
                                                cx,
                                            )
                                        })
                                    }),
                            ),
                    ),
            );
        }
        for job in &detail.checks.jobs {
            let id = job.id;
            let rerun = cx.entity();
            let name = job.name.clone();
            panel = panel.child(
                div()
                    .p(px(10.))
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().flex_1().text_size(px(12.)).child(format!(
                        "{} · {}",
                        job.name,
                        job.conclusion.as_deref().unwrap_or(&job.status)
                    )))
                    .child(
                        button(("pr-rerun-job", id), "Rerun job")
                            .small()
                            .disabled(busy || !can_rerun)
                            .on_click(move |_, _, cx| {
                                rerun.update(cx, |this, cx| {
                                    this.request_rerun(id, false, name.clone(), true, cx)
                                })
                            }),
                    ),
            );
        }
        panel.into_any_element()
    }

    fn request_merge(&self, method: gh::MergeMethod, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let Some(detail) = model.detail.clone() else {
            return;
        };
        let target = model.target.as_ref().unwrap().identity().full_name();
        let number = detail.pull.number;
        let input = MergeInput {
            method,
            expected_head_sha: detail.pull.head.sha.clone(),
            title: self.content(Field::MergeTitle, cx),
            body: self.content(Field::MergeBody, cx),
        };
        let body = format!(
            "{} into {target}:{}\n\nMethod: {}\nReviewed commit: {}\n\n{}\n{}",
            detail.pull.head.label,
            detail.pull.base.name,
            method.label(),
            input.expected_head_sha,
            if input.title.is_empty() {
                &detail.pull.title
            } else {
                &input.title
            },
            input.body
        );
        let model = self.model.clone();
        self.confirm(
            format!("Merge pull request #{number}?"),
            body,
            "Merge pull request",
            false,
            move |cx| {
                model.update(cx, |m, cx| {
                    m.perform(
                        "Merging pull request…",
                        move |client, id, login| client.merge(&id, number, &login, &input),
                        |m, _, cx| m.refresh(cx),
                        cx,
                    )
                })
            },
            cx,
        );
    }
    fn request_update(&self, cx: &mut Context<Self>) {
        let detail = self.model.read(cx).detail.clone().unwrap();
        let number = detail.pull.number;
        let head = detail.pull.head.sha.clone();
        let model = self.model.clone();
        self.confirm("Update this pull request branch?".into(),format!("Merge {} into {} on GitHub. This creates a commit on the pull request's head branch.",detail.pull.base.name,detail.pull.head.label),"Update branch",false,move |cx|model.update(cx,|m,cx|m.perform("Updating branch…",move |client,id,_|client.update_branch(&id,number,&head),|m,_,cx|m.refresh(cx),cx)),cx);
    }
    fn request_state(&self, open: bool, cx: &mut Context<Self>) {
        let number = self.model.read(cx).selected.unwrap();
        let model = self.model.clone();
        self.confirm(
            format!(
                "{} pull request #{number}?",
                if open { "Reopen" } else { "Close" }
            ),
            "The conversation and commits will stay available on GitHub.".into(),
            if open { "Reopen" } else { "Close pull request" },
            !open,
            move |cx| {
                model.update(cx, |m, cx| {
                    m.perform(
                        "Updating pull request…",
                        move |client, id, _| client.state(&id, number, open),
                        |m, _, cx| m.refresh(cx),
                        cx,
                    )
                })
            },
            cx,
        );
    }
    fn request_cleanup(&self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let Some(branch) = model.last_checkout.clone() else {
            return;
        };
        let base = model.detail.as_ref().unwrap().pull.base.name.clone();
        let git = model.git.clone();
        self.confirm("Clean up the local PR branch?".into(),format!("Switch to {base} and delete {branch} if Git confirms it is fully merged. Your remote branches will stay in place."),"Clean up",false,move |cx|git.update(cx,|git,cx|git.run(BusyOp::Other("Cleaning up"),move |path,_|{wackdepot_core::remote::cleanup(&wackdepot_core::Git::new(path),&branch,&base)?;Ok(Done::toast("Local pull request branch removed."))},cx)),cx);
    }
    fn request_rerun(
        &self,
        id: u64,
        failed: bool,
        name: String,
        job: bool,
        cx: &mut Context<Self>,
    ) {
        let model = self.model.clone();
        self.confirm(
            format!("Rerun {name}?"),
            if job {
                "GitHub Actions will rerun this job and its dependent jobs."
            } else if failed {
                "GitHub Actions will rerun the failed jobs and their dependent jobs."
            } else {
                "GitHub Actions will rerun every job in this workflow."
            }
            .into(),
            "Rerun",
            false,
            move |cx| {
                model.update(cx, |m, cx| {
                    m.perform(
                        "Requesting workflow rerun…",
                        move |client, repo, _| {
                            if job {
                                client.rerun_job(&repo, id)
                            } else {
                                client.rerun(&repo, id, failed)
                            }
                        },
                        |m, _, cx| m.refresh_detail(cx),
                        cx,
                    )
                })
            },
            cx,
        );
    }
}
