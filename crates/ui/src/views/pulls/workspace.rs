use super::surface::PullDiff;
use crate::app::overlays::{Overlays, TaskConfirmation};
use crate::components::button::{button, icon_button};
use crate::components::misc::empty_state;
use crate::components::pill_track::pill_track;
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::model::pulls::{PullEvent, PullRequestModel};
use crate::theme::{ActiveTheme, metrics};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FontWeight, IntoElement, ParentElement, Render,
    ScrollHandle, Styled, Window, div, prelude::*, px,
};
use wackdepot_github as gh;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Title,
    Body,
    Base,
    Head,
    Reviewers,
    Teams,
    Labels,
    Review,
    Conversation,
    Inline,
    Reply,
    MergeTitle,
    MergeBody,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Picker {
    Source,
    Base,
    Head,
    Reviewers,
    Teams,
    Labels,
    Template,
}

pub struct PullWorkspace {
    pub(super) model: Entity<PullRequestModel>,
    pub(super) overlays: Entity<Overlays>,
    fields: Vec<(Field, Entity<TextInput>)>,
    pub(super) diff: Entity<PullDiff>,
    pub(super) section: usize,
    pub(super) selected_file: Option<String>,
    pub(super) scroll: [ScrollHandle; 4],
    pub(super) picker: Option<Picker>,
    pub(super) editing: bool,
    sync_key: String,
    syncing: bool,
}
impl PullWorkspace {
    pub fn new(
        model: Entity<PullRequestModel>,
        overlays: Entity<Overlays>,
        cx: &mut Context<Self>,
    ) -> Self {
        let specs = [
            (Field::Title, "Give this pull request a title", false),
            (Field::Body, "Describe the change", true),
            (Field::Base, "Base branch", false),
            (Field::Head, "owner:head-branch", false),
            (
                Field::Reviewers,
                "Reviewer logins, separated by commas",
                false,
            ),
            (Field::Teams, "Team slugs, separated by commas", false),
            (Field::Labels, "Labels, separated by commas", false),
            (Field::Review, "Summarize your review", true),
            (Field::Conversation, "Leave a comment", true),
            (Field::Inline, "Comment on the selected lines", true),
            (Field::Reply, "Reply to this discussion", true),
            (Field::MergeTitle, "Commit title", false),
            (Field::MergeBody, "Commit description", true),
        ];
        let fields = specs
            .into_iter()
            .map(|(kind, placeholder, multi)| {
                let input = cx.new(|cx| {
                    let input = TextInput::new(placeholder, cx);
                    if multi { input.multiline(3, 10) } else { input }
                });
                cx.subscribe(
                    &input,
                    move |this: &mut Self, input, event: &TextInputEvent, cx| {
                        if *event != TextInputEvent::Changed || this.syncing {
                            return;
                        }
                        let value = input.read(cx).content().to_string();
                        let anchor = this.model.read(cx).inline.clone();
                        let thread = this.model.read(cx).active_thread.clone();
                        this.model.update(cx, |m, cx| {
                            m.edit_draft(
                                |draft| match kind {
                                    Field::Title => draft.creation.title = value,
                                    Field::Body => draft.creation.body = value,
                                    Field::Base => draft.creation.base = value,
                                    Field::Head => draft.creation.head = value,
                                    Field::Reviewers => {
                                        draft.creation.reviewers = comma_list(&value)
                                    }
                                    Field::Teams => draft.creation.teams = comma_list(&value),
                                    Field::Labels => draft.creation.labels = comma_list(&value),
                                    Field::Review => {
                                        draft.review_body = value;
                                        draft.review_body_initialized = true;
                                    }
                                    Field::Conversation => draft.conversation_body = value,
                                    Field::MergeTitle => draft.merge_title = value,
                                    Field::MergeBody => draft.merge_body = value,
                                    Field::Reply => {
                                        if let Some(thread) = thread {
                                            draft.replies.insert(thread, value);
                                        }
                                    }
                                    Field::Inline => {
                                        if let Some(anchor) = anchor {
                                            if let Some(comment) = draft
                                                .comments
                                                .iter_mut()
                                                .find(|c| c.anchor == anchor)
                                            {
                                                comment.body = value;
                                            } else if !value.is_empty() {
                                                draft.comments.push(gh::drafts::LocalComment {
                                                    anchor,
                                                    body: value,
                                                });
                                            }
                                        }
                                    }
                                },
                                cx,
                            )
                        });
                        if matches!(kind, Field::Base | Field::Head) {
                            this.model.update(cx, |m, cx| {
                                m.invalidate_preview(cx);
                            });
                        }
                    },
                )
                .detach();
                (kind, input)
            })
            .collect();
        cx.subscribe(&model, |this: &mut Self, _, event: &PullEvent, cx| {
            if matches!(event, PullEvent::Draft) {
                this.sync(cx);
            }
            cx.notify();
        })
        .detach();
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        let diff = cx.new(|cx| PullDiff::new(model.clone(), cx));
        Self {
            model,
            overlays,
            fields,
            diff,
            section: 0,
            selected_file: None,
            scroll: std::array::from_fn(|_| ScrollHandle::new()),
            picker: None,
            editing: false,
            sync_key: String::new(),
            syncing: false,
        }
    }
    pub(super) fn input(&self, field: Field) -> Entity<TextInput> {
        self.fields
            .iter()
            .find(|(kind, _)| *kind == field)
            .unwrap()
            .1
            .clone()
    }
    pub fn sync_prefs(&mut self, cx: &mut Context<Self>) {
        self.diff.update(cx, |_, cx| cx.notify());
        cx.notify();
    }
    pub(super) fn content(&self, field: Field, cx: &App) -> String {
        self.input(field).read(cx).content().to_string()
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let key = format!(
            "{}-{:?}-{}-{}",
            model.generation,
            model.target.as_ref().map(|r| r.id),
            model.selected.unwrap_or(0),
            model.creating
        );
        if key != self.sync_key {
            self.sync_key = key;
            self.selected_file = None;
            self.section = 0;
            self.editing = false;
            self.picker = None;
            self.scroll = std::array::from_fn(|_| ScrollHandle::new());
        }
        let draft = model.draft.clone();
        let anchor = model.inline.clone();
        let thread = model.active_thread.clone();
        let values = [
            (Field::Title, draft.creation.title),
            (Field::Body, draft.creation.body),
            (Field::Base, draft.creation.base),
            (Field::Head, draft.creation.head),
            (Field::Reviewers, draft.creation.reviewers.join(", ")),
            (Field::Teams, draft.creation.teams.join(", ")),
            (Field::Labels, draft.creation.labels.join(", ")),
            (Field::Review, draft.review_body),
            (Field::Conversation, draft.conversation_body),
            (Field::MergeTitle, draft.merge_title),
            (Field::MergeBody, draft.merge_body),
            (
                Field::Reply,
                thread
                    .and_then(|t| draft.replies.get(&t).cloned())
                    .unwrap_or_default(),
            ),
            (
                Field::Inline,
                anchor
                    .and_then(|a| {
                        draft
                            .comments
                            .iter()
                            .find(|c| c.anchor == a)
                            .map(|c| c.body.clone())
                    })
                    .unwrap_or_default(),
            ),
        ];
        self.syncing = true;
        for (field, value) in values {
            let input = self.input(field);
            if input.read(cx).content() != value {
                input.update(cx, |i, cx| i.set_content(value, cx));
            }
        }
        self.syncing = false;
    }
    pub(super) fn confirm(
        &self,
        title: String,
        body: String,
        label: &str,
        danger: bool,
        action: impl FnOnce(&mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.overlays.update(cx, |o, cx| {
            o.confirm_task(
                TaskConfirmation {
                    title,
                    body,
                    label: label.into(),
                    danger,
                    action: Box::new(action),
                },
                cx,
            )
        });
    }
    pub(super) fn picker_button(
        &self,
        id: &'static str,
        label: &str,
        picker: Picker,
        cx: &mut Context<Self>,
    ) -> crate::components::button::Button {
        button(id, label.to_string())
            .small()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.picker = if this.picker == Some(picker) {
                    None
                } else {
                    Some(picker)
                };
                cx.notify();
            }))
    }
    fn header(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let Some(detail) = model.detail.clone() else {
            return div().into_any_element();
        };
        let pull = &detail.pull;
        let number = pull.number;
        let busy = model.busy;
        let loading = model.detail_loading;
        let url = pull.html_url.clone();
        let refresh = self.model.clone();
        let checkout = cx.entity();
        let edit = cx.entity();
        div()
            .p(px(18.))
            .rounded_tl(px(metrics::RADIUS_CARD_INNER))
            .rounded_tr(px(metrics::RADIUS_CARD_INNER))
            .bg(t.surface)
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .items_start()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(19.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{}  #{number}", pull.title)),
                    )
                    .child(
                        icon_button("pr-open-web", "external")
                            .small()
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                    .child(
                        icon_button("pr-reload", "refresh")
                            .small()
                            .disabled(loading || busy)
                            .on_click(move |_, _, cx| {
                                refresh.update(cx, |m, cx| m.refresh_detail(cx))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(11.5))
                    .text_color(t.text_dim)
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(3.))
                            .rounded_full()
                            .bg(t.input)
                            .text_color(if pull.merged_at.is_some() {
                                t.sem.renamed.ink
                            } else if pull.state == "closed" {
                                t.sem.danger_text
                            } else {
                                t.sem.success
                            })
                            .child(pull.state_label()),
                    )
                    .child(format!(
                        "{} wants to merge {} → {}",
                        pull.user.login, pull.head.label, pull.base.name
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(7.))
                    .child(
                        button("pr-checkout", "Check out")
                            .small()
                            .disabled(busy)
                            .on_click(move |_, _, cx| {
                                checkout.update(cx, |this, cx| this.request_checkout(cx))
                            }),
                    )
                    .when(detail.reviews.can_update, |el| {
                        el.child(
                            button("pr-edit", "Edit details")
                                .small()
                                .disabled(busy)
                                .on_click(move |_, _, cx| {
                                    edit.update(cx, |this, cx| this.begin_edit(cx))
                                }),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(match detail.checks.state() {
                                gh::CheckState::Failing => t.sem.danger_text,
                                gh::CheckState::Passing => t.sem.success,
                                _ => t.text_dim,
                            })
                            .child(detail.checks.state().label()),
                    ),
            )
            .child(
                pill_track("pr-sections")
                    .item("Overview")
                    .item_count("Files", detail.files.len())
                    .item_count("Commits", detail.commits.len())
                    .item("Checks")
                    .selected(self.section)
                    .on_select({
                        let this = cx.entity();
                        move |i, _, cx| {
                            this.update(cx, |this, cx| {
                                this.section = i;
                                cx.notify();
                            })
                        }
                    }),
            )
            .into_any_element()
    }
    fn request_checkout(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let dirty = model.git.read(cx).snap.changed_count() > 0;
        if dirty {
            let model = self.model.clone();
            self.confirm("Stash changes and check out this PR?".into(),"Your tracked and untracked changes will be saved in a stash. Restore them from the sync menu when you return to your branch.".into(),"Stash and Check Out",false,move |cx|model.update(cx,|m,cx|m.checkout(true,cx)),cx);
        } else {
            self.model.update(cx, |m, cx| m.checkout(false, cx));
        }
    }
    fn begin_edit(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.model.read(cx).detail.clone() else {
            return;
        };
        self.editing = true;
        self.model.update(cx, |m, cx| {
            m.edit_draft(
                |d| {
                    d.creation.title = detail.pull.title.clone();
                    d.creation.body = detail.pull.body.clone().unwrap_or_default();
                    d.creation.reviewers = detail
                        .pull
                        .requested_reviewers
                        .iter()
                        .map(|u| u.login.clone())
                        .collect();
                    d.creation.teams = detail
                        .pull
                        .requested_teams
                        .iter()
                        .map(|t| t.slug.clone())
                        .collect();
                    d.creation.labels = detail.pull.labels.iter().map(|l| l.name.clone()).collect();
                },
                cx,
            );
            m.perform(
                "Loading repository options…",
                |client, id, _| client.options(&id),
                |m, options, cx| {
                    m.options = options;
                    m.draft_changed(cx);
                },
                cx,
            );
            m.draft_changed(cx);
        });
        cx.notify();
    }
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.picker.take().is_some() || std::mem::take(&mut self.editing) {
            cx.notify();
            true
        } else {
            false
        }
    }
}
impl Render for PullWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let error = model
            .error
            .clone()
            .or_else(|| model.session.read(cx).error.clone());
        let message = model.message.clone();
        let creating = model.creating;
        let connected = model.session.read(cx).user.is_some();
        let has_detail = model.detail.is_some();
        let loading = model.detail_loading || model.loading;
        let mut root = div().size_full().min_w_0().min_h_0().flex().flex_col();
        if let Some(error) = error {
            root = root.child(
                div()
                    .p(px(12.))
                    .bg(t.sem.danger_bg)
                    .text_color(t.sem.danger_text)
                    .text_size(px(12.))
                    .child(error),
            );
        }
        if let Some(message) = message {
            root = root.child(
                div()
                    .px(px(14.))
                    .py(px(8.))
                    .bg(t.input)
                    .text_size(px(12.))
                    .text_color(t.text_dim)
                    .child(message),
            );
        }
        if !connected {
            let session = self.model.read(cx).session.clone();
            return root.child(div().flex_1().flex().flex_col().items_center().justify_center().gap(px(16.)).p(px(30.)).child(empty_state("pull-request","Your pull requests, at home").body("Connect GitHub to create, review, and merge pull requests in WackDepot.")).child(button("pr-connect","Connect GitHub").primary().on_click(move |_,_,cx|session.update(cx,|s,cx|s.connect(cx))))).into_any_element();
        }
        if creating || self.editing {
            return root.child(self.creation(window, cx)).into_any_element();
        }
        if !has_detail {
            return root.child(div().flex_1().flex().items_center().justify_center().p(px(24.)).child(empty_state("pull-request",if loading{"Loading pull request…"}else{"Choose a pull request"}).body("Browse changes, join the conversation, or start a new pull request."))).into_any_element();
        }
        root = root.child(self.header(cx));
        let body = match self.section {
            1 => self.files(cx),
            2 => self.commits(cx),
            3 => self.checks(cx),
            _ => self.overview(window, cx),
        };
        root.child(body).into_any_element()
    }
}

pub(super) fn comma_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
