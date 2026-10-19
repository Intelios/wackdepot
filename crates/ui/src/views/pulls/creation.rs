use super::workspace::{Field, Picker, PullWorkspace};
use crate::components::button::button;
use crate::theme::ActiveTheme;
use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, ParentElement, Styled, Window, div,
    prelude::*, px,
};

impl PullWorkspace {
    pub(super) fn text_field(&self, label: &str, kind: Field, cx: &App) -> AnyElement {
        let t = cx.theme();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_dim)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .min_h(px(36.))
                    .px(px(11.))
                    .py(px(9.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.input)
                    .text_size(px(13.))
                    .child(self.input(kind)),
            )
            .into_any_element()
    }
    pub(super) fn creation(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let busy = model.busy || model.reconciling;
        let input = model.draft.creation.clone();
        let created = model.draft.created_pr;
        let comparison = model.comparison.clone();
        let options = model.options.clone();
        let target = model
            .target
            .as_ref()
            .map(|r| r.identity().full_name())
            .unwrap_or_default();
        let editing = self.editing;
        let mut content = div()
            .w_full()
            .max_w(px(820.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(16.))
            .child(
                div()
                    .text_size(px(21.))
                    .font_weight(FontWeight::BOLD)
                    .child(if editing {
                        "Edit pull request"
                    } else {
                        "New pull request"
                    }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_dim)
                    .child(format!(
                        "Into {target} · Your draft is saved automatically."
                    )),
            );
        if !editing {
            content = content
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(self.picker_button(
                            "pr-pick-source",
                            "Head repository",
                            Picker::Source,
                            cx,
                        ))
                        .child(self.picker_button("pr-pick-base", "Base branch", Picker::Base, cx))
                        .child(self.picker_button("pr-pick-head", "Head branch", Picker::Head, cx)),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .gap(px(12.))
                        .child(div().flex_1().child(self.text_field(
                            "Base branch",
                            Field::Base,
                            cx,
                        )))
                        .child(div().flex_1().child(self.text_field(
                            "Head (owner:branch)",
                            Field::Head,
                            cx,
                        ))),
                );
        }
        content = content
            .child(self.text_field("Title", Field::Title, cx))
            .child(self.text_field("Description", Field::Body, cx));
        let mut controls = div().flex().flex_wrap().gap(px(8.));
        if !options.templates.is_empty() && !editing {
            controls = controls.child(self.picker_button(
                "pr-pick-template",
                "Use template",
                Picker::Template,
                cx,
            ));
        }
        controls = controls
            .child(self.picker_button(
                "pr-pick-reviewers",
                "Choose reviewers",
                Picker::Reviewers,
                cx,
            ))
            .child(self.picker_button("pr-pick-teams", "Choose teams", Picker::Teams, cx))
            .child(self.picker_button("pr-pick-labels", "Choose labels", Picker::Labels, cx));
        content = content
            .child(controls)
            .child(self.picker_choices(cx))
            .child(self.text_field("Reviewers", Field::Reviewers, cx))
            .child(self.text_field("Teams", Field::Teams, cx))
            .child(self.text_field("Labels", Field::Labels, cx));
        if !editing {
            let model = self.model.clone();
            content=content.child(button("pr-create-draft",if input.draft{"✓ Create as a draft"}else{"Create as a draft"}).small().lit(input.draft).disabled(busy).on_click(move |_,_,cx|model.update(cx,|m,cx|{m.edit_draft(|d|d.creation.draft= !d.creation.draft,cx);m.draft_changed(cx);})))
                .child(div().text_size(px(12.)).text_color(t.text_dim).child("The preview includes committed changes. Uncommitted and untracked work stays on your computer."));
            let preview = self.model.clone();
            content = content.child(
                button("pr-preview", "Preview changes")
                    .disabled(busy)
                    .on_click(move |_, _, cx| preview.update(cx, |m, cx| m.preview(cx))),
            );
            if let Some(comparison) = comparison {
                content =
                    content.child(div().text_size(px(13.)).text_color(t.text).child(format!(
                        "{} commits · {} files",
                        comparison.commits.len(),
                        comparison.files.len()
                    )));
                if comparison.local {
                    content=content.child(div().text_size(px(12.)).text_color(t.text_dim).child("Previewing local commits against the last fetched base branch. Fetch to update the base before publishing."));
                }
                if !comparison.complete {
                    content=content.child(div().text_size(px(12.)).text_color(t.sem.warning_text).child("GitHub limited this comparison. Open the comparison on GitHub to inspect every change."));
                }
                for (i, commit) in comparison.commits.iter().enumerate() {
                    content = content.child(div().text_size(px(12.)).text_color(t.text_dim).child(
                        format!(
                            "{}  {}",
                            &commit.sha[..commit.sha.len().min(7)],
                            commit.commit.message.lines().next().unwrap_or_default()
                        ),
                    ));
                    let _ = i;
                }
                for (i, file) in comparison.files.iter().enumerate() {
                    let path = file.filename.clone();
                    content = content.child(
                        button(
                            ("preview-file", i),
                            format!("{}  +{} −{}", file.filename, file.additions, file.deletions),
                        )
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_file = Some(path.clone());
                            cx.notify();
                        })),
                    );
                }
                if let Some(file) = self
                    .selected_file
                    .as_ref()
                    .and_then(|path| comparison.files.iter().find(|f| &f.filename == path))
                    .cloned()
                {
                    let model = self.model.read(cx);
                    let target = model.target.as_ref().unwrap().identity();
                    let head_id = model
                        .sources
                        .iter()
                        .find(|s| {
                            s.repository.name == input.head_repo
                                && input
                                    .head
                                    .starts_with(&format!("{}:", s.repository.owner.login))
                        })
                        .map(|s| s.repository.identity())
                        .unwrap_or_else(|| target.clone());
                    let key = format!(
                        "preview:{}:{}:{}",
                        target.full_name(),
                        file.filename,
                        comparison.head_sha
                    );
                    let head = comparison.head_sha.clone();
                    let base = comparison.base_sha.clone();
                    let local = if comparison.local {
                        self.model.read(cx).git.read(cx).repo.clone()
                    } else {
                        None
                    };
                    self.diff.update(cx, |diff, cx| {
                        diff.load(
                            super::surface::DiffRequest {
                                key,
                                changed: file,
                                target,
                                head_repo: head_id,
                                base,
                                head,
                                reviewable: false,
                                local,
                            },
                            cx,
                        )
                    });
                    content = content.child(
                        div()
                            .w_full()
                            .h(px(360.))
                            .min_h(px(200.))
                            .border_1()
                            .border_color(t.border)
                            .rounded(px(10.))
                            .overflow_hidden()
                            .child(self.diff.clone()),
                    );
                }
            }
        }
        let create = self.model.clone();
        let publish = self.model.clone();
        let save = cx.entity();
        let cancel = cx.entity();
        let mut footer = div()
            .p(px(14.))
            .border_t_1()
            .border_color(t.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(
                button(
                    "pr-create-cancel",
                    if editing {
                        "Cancel"
                    } else {
                        "Back to pull request"
                    },
                )
                .ghost()
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.editing = false;
                        this.model.update(cx, |m, cx| {
                            m.creating = false;
                            m.draft_changed(cx);
                        });
                        cx.notify();
                    })
                }),
            )
            .child(div().flex_1());
        if editing {
            footer = footer.child(
                button("pr-save-edit", "Save changes")
                    .primary()
                    .disabled(busy || input.title.trim().is_empty())
                    .on_click(move |_, _, cx| save.update(cx, |this, cx| this.save_edit(cx))),
            );
        } else {
            if let Some(number) = created {
                let open = self.model.clone();
                footer = footer.child(
                    button("pr-created-open", format!("Open #{number}"))
                        .on_click(move |_, _, cx| open.update(cx, |m, cx| m.select(number, cx))),
                );
            }
            if created.is_none() {
                footer = footer.child(
                    button("pr-publish-create", "Publish and Create PR")
                        .disabled(busy || input.title.trim().is_empty())
                        .on_click(move |_, _, cx| publish.update(cx, |m, cx| m.create(true, cx))),
                );
            }
            footer = footer.child(
                button(
                    "pr-create-submit",
                    if created.is_some() {
                        "Retry reviewers and labels"
                    } else if input.draft {
                        "Create draft PR"
                    } else {
                        "Create pull request"
                    },
                )
                .primary()
                .disabled(busy || input.title.trim().is_empty())
                .on_click(move |_, _, cx| create.update(cx, |m, cx| m.create(false, cx))),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("pr-creation-scroll")
                    .track_scroll(&self.scroll[0])
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(content),
            )
            .child(footer)
            .into_any_element()
    }

    fn picker_choices(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let Some(picker) = self.picker else {
            return div().into_any_element();
        };
        let model = self.model.read(cx);
        let draft = model.draft.creation.clone();
        let choices: Vec<(String, String)> = match picker {
            Picker::Source => model
                .sources
                .iter()
                .map(|s| {
                    (
                        s.repository.identity().full_name(),
                        s.repository.identity().full_name(),
                    )
                })
                .collect(),
            Picker::Base => model
                .options
                .branches
                .iter()
                .map(|b| (b.clone(), b.clone()))
                .collect(),
            Picker::Head => model
                .git
                .read(cx)
                .snap
                .branches
                .iter()
                .map(|b| (b.name.clone(), b.name.clone()))
                .collect(),
            Picker::Reviewers => model
                .options
                .reviewers
                .iter()
                .map(|u| (u.login.clone(), u.login.clone()))
                .collect(),
            Picker::Teams => model
                .options
                .teams
                .iter()
                .map(|team| (team.slug.clone(), format!("{} ({})", team.name, team.slug)))
                .collect(),
            Picker::Labels => model
                .options
                .labels
                .iter()
                .map(|label| (label.name.clone(), label.name.clone()))
                .collect(),
            Picker::Template => model
                .options
                .templates
                .iter()
                .enumerate()
                .map(|(i, template)| (i.to_string(), template.name.clone()))
                .collect(),
        };
        let mut panel = div()
            .w_full()
            .id("pr-picker-options")
            .max_h(px(220.))
            .overflow_y_scroll()
            .p(px(8.))
            .bg(t.input)
            .rounded(px(10.))
            .flex()
            .flex_col()
            .gap(px(4.));
        if choices.is_empty() {
            panel = panel.child(div().text_size(px(12.)).text_color(t.text_dim).child(
                "No available options for this account. You can enter an exact name in the field.",
            ));
        }
        for (i, (value, label)) in choices.into_iter().enumerate() {
            let selected = match picker {
                Picker::Reviewers => draft.reviewers.contains(&value),
                Picker::Teams => draft.teams.contains(&value),
                Picker::Labels => draft.labels.contains(&value),
                _ => false,
            };
            panel = panel.child(
                button(("pr-picker-choice", i), label)
                    .small()
                    .lit(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let template = this
                            .model
                            .read(cx)
                            .options
                            .templates
                            .get(value.parse::<usize>().unwrap_or(usize::MAX))
                            .cloned();
                        this.model.update(cx, |m, cx| {
                            m.edit_draft(
                                |draft| match picker {
                                    Picker::Source => {
                                        if let Some((owner, name)) = value.split_once('/') {
                                            let branch = draft
                                                .creation
                                                .head
                                                .split_once(':')
                                                .map(|(_, b)| b)
                                                .unwrap_or_default();
                                            draft.creation.head = format!("{owner}:{branch}");
                                            draft.creation.head_repo = name.into();
                                        }
                                    }
                                    Picker::Base => draft.creation.base = value.clone(),
                                    Picker::Head => {
                                        let owner = draft
                                            .creation
                                            .head
                                            .split_once(':')
                                            .map(|(o, _)| o)
                                            .unwrap_or_default();
                                        draft.creation.head = format!("{owner}:{value}");
                                    }
                                    Picker::Reviewers => {
                                        toggle(&mut draft.creation.reviewers, &value)
                                    }
                                    Picker::Teams => toggle(&mut draft.creation.teams, &value),
                                    Picker::Labels => toggle(&mut draft.creation.labels, &value),
                                    Picker::Template => {
                                        if let Some(template) = template {
                                            draft.creation.body = template.body;
                                        }
                                    }
                                },
                                cx,
                            );
                            m.draft_changed(cx);
                        });
                        if !matches!(picker, Picker::Reviewers | Picker::Teams | Picker::Labels) {
                            this.picker = None;
                        }
                        cx.notify();
                    })),
            );
        }
        panel.into_any_element()
    }
    fn save_edit(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        let Some(number) = model.selected else {
            return;
        };
        let input = model.draft.creation.clone();
        let view = cx.entity().downgrade();
        self.model.update(cx, |m, cx| {
            m.perform(
                "Saving pull request…",
                move |client, id, _| {
                    client.edit(&id, number, &input.title, &input.body)?;
                    client.replace_metadata(&id, number, &input)
                },
                move |m, _, cx| {
                    m.refresh_detail(cx);
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |v, cx| {
                            v.editing = false;
                            cx.notify();
                        });
                    }
                },
                cx,
            )
        });
        cx.notify();
    }
}
fn toggle(values: &mut Vec<String>, value: &str) {
    if values.iter().any(|v| v == value) {
        values.retain(|v| v != value);
    } else {
        values.push(value.into());
    }
}
