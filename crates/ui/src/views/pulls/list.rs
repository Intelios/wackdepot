use crate::components::button::{button, icon_button};
use crate::components::menu::{menu_row, popover};
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::icons::icon;
use crate::model::pulls::PullRequestModel;
use crate::theme::ActiveTheme;
use gpui::{
    AnyElement, AppContext, Context, Entity, FocusHandle, IntoElement, ParentElement, Render,
    ScrollStrategy, Styled, UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use wackdepot_github::Filter;

pub struct PullList {
    model: Entity<PullRequestModel>,
    search: Entity<TextInput>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    filter_open: bool,
    repositories_open: bool,
}
impl PullList {
    pub fn new(model: Entity<PullRequestModel>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new("Search title, author, or #number", cx));
        cx.subscribe(
            &search,
            |this: &mut Self, input, event: &TextInputEvent, cx| {
                if *event == TextInputEvent::Changed {
                    let text = input.read(cx).content().to_string();
                    let filter = this.model.read(cx).filter;
                    this.model.update(cx, |m, cx| m.search(text, filter, cx));
                }
            },
        )
        .detach();
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            search,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            filter_open: false,
            repositories_open: false,
        }
    }
    pub fn step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        if model.list.is_empty() || model.busy {
            return;
        }
        let current = model
            .selected
            .and_then(|n| model.list.iter().position(|p| p.number == n));
        let index = current
            .map(|i| (i as isize + delta).clamp(0, model.list.len() as isize - 1) as usize)
            .unwrap_or(0);
        let number = model.list[index].number;
        self.model.update(cx, |m, cx| m.select(number, cx));
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        self.focus.focus(window, cx);
    }
    fn row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let Some(pull) = model.list.get(index) else {
            return div().into_any_element();
        };
        let number = pull.number;
        let selected = model.selected == Some(number) && !model.creating;
        let entity = self.model.clone();
        div()
            .id(("pull-request", number as usize))
            .h(px(76.))
            .p(px(10.))
            .rounded(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .cursor_pointer()
            .bg(if selected {
                t.selected_fill()
            } else {
                t.column(1.)
            })
            .hover(|s| s.bg(t.pill_hover()))
            .on_click(move |_, _, cx| entity.update(cx, |m, cx| m.select(number, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(icon("pull-request").size(px(14.)).text_color(
                        if pull.merged_at.is_some() {
                            t.sem.renamed.ink
                        } else if pull.state == "closed" {
                            t.sem.danger
                        } else if pull.draft {
                            t.text_faint
                        } else {
                            t.sem.success
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(t.text)
                            .child(pull.title.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(t.text_dim)
                    .child(format!("#{number}"))
                    .child(div().flex_1().truncate().child(pull.user.login.clone()))
                    .child(pull.state_label()),
            )
            .into_any_element()
    }
}
impl Render for PullList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let filter = model.filter;
        let count = model.list.len();
        let loading = model.loading;
        let next = model.next_cursor.is_some();
        let incomplete = model.incomplete;
        let repository = model.target.as_ref().map(|r| r.identity().full_name());
        let connected = model.session.read(cx).user.is_some();
        let can_create = repository.is_some() && !model.busy;
        let refresh = self.model.clone();
        let create = self.model.clone();
        let mut root = div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .track_focus(&self.focus)
            .key_context("PullRequestList")
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        button(
                            "pr-repository",
                            repository
                                .clone()
                                .unwrap_or_else(|| "GitHub repository".into()),
                        )
                        .small()
                        .ghost()
                        .disabled(repository.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.repositories_open = !this.repositories_open;
                            cx.notify();
                        })),
                    )
                    .child(div().flex_1())
                    .child(
                        icon_button("pr-refresh", "refresh")
                            .small()
                            .disabled(loading)
                            .on_click(move |_, _, cx| refresh.update(cx, |m, cx| m.refresh(cx))),
                    ),
            );
        if self.repositories_open {
            let model = self.model.read(cx);
            let mut choices = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for source in &model.sources {
                for repo in
                    std::iter::once(&source.repository).chain(source.repository.parent.as_deref())
                {
                    let id = repo.identity();
                    if seen.insert(id.full_name()) {
                        choices.push(id);
                    }
                }
            }
            root = root.child(div().flex().flex_col().gap(px(2.)).children(
                choices.into_iter().enumerate().map(|(i, id)| {
                    let model = self.model.clone();
                    button(("target-repo", i), id.full_name())
                        .small()
                        .on_click(move |_, _, cx| {
                            model.update(cx, |m, cx| m.choose_repository(id.clone(), cx))
                        })
                }),
            ));
        }
        root = root
            .child(
                div()
                    .p(px(8.))
                    .rounded(px(10.))
                    .bg(t.input)
                    .border_1()
                    .border_color(t.border)
                    .text_size(px(12.))
                    .child(self.search.clone()),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        button("pr-filter", filter.label())
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.filter_open = !this.filter_open;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        button("pr-new", "New PR")
                            .small()
                            .primary()
                            .disabled(!can_create)
                            .on_click(move |_, _, cx| {
                                create.update(cx, |m, cx| m.start_create(cx))
                            }),
                    ),
            );
        if self.filter_open {
            let menu = popover("pr-filters").width(210.).children(
                Filter::ALL.into_iter().enumerate().map(|(i, filter)| {
                    let this = cx.entity();
                    menu_row(("pr-filter-item", i), filter.label())
                        .selected(filter == self.model.read(cx).filter)
                        .on_click(move |_, _, cx| {
                            this.update(cx, |this, cx| {
                                let query = this.model.read(cx).query.clone();
                                this.model.update(cx, |m, cx| m.search(query, filter, cx));
                                this.filter_open = false;
                                cx.notify();
                            })
                        })
                }),
            );
            root = root.child(menu);
        }
        if count > 0 {
            root = root.child(
                uniform_list(
                    "pr-list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|i| this.row(i, cx)).collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .min_h_0(),
            );
        } else {
            root = root.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(12.))
                    .text_size(px(12.))
                    .text_color(t.text_dim)
                    .child(if loading {
                        "Loading pull requests…"
                    } else if !connected {
                        "Connect GitHub to browse pull requests."
                    } else if repository.is_none() {
                        "Choose a GitHub repository to begin."
                    } else {
                        "No pull requests match this view."
                    }),
            );
        }
        if next {
            let model = self.model.clone();
            root = root.child(
                button("pr-more", if loading { "Loading…" } else { "Load more" })
                    .small()
                    .disabled(loading)
                    .on_click(move |_, _, cx| model.update(cx, |m, cx| m.refresh_list(true, cx))),
            );
        }
        if incomplete {
            root = root.child(
                div()
                    .text_size(px(11.))
                    .text_color(t.text_dim)
                    .child("GitHub limits this search. Narrow the query to see every match."),
            );
        }
        root
    }
}
