//! `impl Render for Shell`: the titlebar strip, glass rail, content pane,
//! dialogs and notices composed over the window blur.

use gpui::{
    AnyElement, Context, Div, ExternalPaths, Render, Stateful, Styled, Window, div, prelude::*, px,
};

use crate::icons::icon;
use crate::motion;
use crate::shell::Shell;
use crate::state::{BusyOp, Dialog, PopoverKind, Selected, Tab};
use crate::theme::{self, Theme};

/// A small pill button in the titlebar or rail.
pub fn pill_button(theme: &Theme, el: Stateful<Div>, active: bool) -> Stateful<Div> {
    let mut b = el
        .flex()
        .items_center()
        .gap_1p5()
        .h_6()
        .px_2()
        .rounded_full()
        .text_sm()
        .cursor_pointer()
        .occlude()
        .text_color(if active { theme.text } else { theme.text_soft });
    if active {
        b = b.bg(theme.accent.opacity(0.25));
    } else {
        b = b.hover(|s| s.bg(theme.accent.opacity(0.12)));
    }
    b
}

/// Titlebar cluster button: icon + label, opens a popover.
fn cluster(
    theme: &Theme,
    id: &'static str,
    icon_name: &'static str,
    label: impl IntoElement,
    open: PopoverKind,
    current: Option<PopoverKind>,
) -> Stateful<Div> {
    let is_open = current == Some(open);
    let mut el = pill_button(theme, div().id(id), is_open);
    el = el.child(icon(icon_name).size_4());
    el = el.child(label);
    el.child(icon("chevron-down").size_3().text_color(theme.text_dim))
}

impl Shell {
    fn titlebar(&self, window: &Window, cx: &mut Context<Self>, theme: &Theme) -> Div {
        let _ = window;
        // Rail side: repo picker, inset past the traffic lights.
        let rail_side = self.drag_strip(
            "titlebar-rail",
            div()
                .h_full()
                .flex()
                .items_center()
                .pl(px(theme::TRAFFIC_LIGHT_INSET))
                .pr_2()
                .child(
                    cluster(
                        theme,
                        "repo-cluster",
                        "folder",
                        div()
                            .max_w(px(190.))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.active_name()),
                        PopoverKind::RepoPicker,
                        self.popover,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.popover = match this.popover {
                            Some(PopoverKind::RepoPicker) => None,
                            _ => Some(PopoverKind::RepoPicker),
                        };
                        cx.notify();
                    }))
                    .when(self.popover == Some(PopoverKind::RepoPicker), |el| {
                        el.child(self.repo_picker(cx, theme))
                    }),
                ),
            cx,
        );

        // Content side: branch + sync controls + settings gear.
        let status = self.repo.status.as_ref();
        let ahead = status.map(|s| s.ahead).unwrap_or(0);
        let behind = status.map(|s| s.behind).unwrap_or(0);
        let has_upstream = status.map(|s| s.upstream.is_some()).unwrap_or(false);

        let sync_label = match self.busy {
            Some(ref op) => op.label().to_string(),
            None => {
                if !has_upstream && self.active.is_some() {
                    "Publish branch".to_string()
                } else if behind > 0 {
                    format!("Pull ↓{behind}")
                } else if ahead > 0 {
                    format!("Push ↑{ahead}")
                } else {
                    "Fetch".to_string()
                }
            }
        };

        let branch_chip = div()
            .flex()
            .items_center()
            .gap_1p5()
            .h_6()
            .px_2()
            .rounded_full()
            .text_sm()
            .cursor_pointer()
            .occlude()
            .text_color(theme.text_soft)
            .hover(|s| s.bg(theme.accent.opacity(0.12)))
            .child(icon("git-branch").size_4().text_color(theme.text_soft))
            .child(self.branch_label());

        let gear = pill_button(theme, div().id("settings-gear"), false)
            .child(icon("settings").size_4().text_color(theme.text_soft))
            .on_click(cx.listener(|this, _, _, cx| {
                this.dialog = Some(Dialog::Settings);
                cx.notify();
            }));

        let mut content_side = div()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .border_b_1()
            .border_color(theme.border);
        if self.active.is_some() {
            content_side = content_side
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            branch_chip
                                .id("branch-chip")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.popover = match this.popover {
                                        Some(PopoverKind::BranchPicker) => None,
                                        _ => Some(PopoverKind::BranchPicker),
                                    };
                                    cx.notify();
                                }))
                                .when(self.popover == Some(PopoverKind::BranchPicker), |el| {
                                    el.child(self.branch_picker(cx, theme))
                                }),
                        )
                        .when(ahead > 0 || behind > 0, |el| {
                            el.child(div().text_xs().text_color(theme.text_dim).child(format!(
                                "{}{}",
                                if ahead > 0 {
                                    format!("↑{ahead} ")
                                } else {
                                    String::new()
                                },
                                if behind > 0 {
                                    format!("↓{behind}")
                                } else {
                                    String::new()
                                }
                            )))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .when_some(self.progress.clone(), |el, p| {
                            el.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_dim)
                                    .max_w(px(220.))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(p),
                            )
                        })
                        .child(
                            pill_button(theme, div().id("sync-btn"), false)
                                .child(
                                    icon(match self.busy {
                                        Some(BusyOp::Push) => "push",
                                        Some(BusyOp::Pull) | Some(BusyOp::Fetch) => "fetch",
                                        _ => "fetch",
                                    })
                                    .size_4(),
                                )
                                .child(sync_label)
                                .on_click(cx.listener(|this, _, _, cx| this.sync(cx))),
                        )
                        .child(
                            pill_button(theme, div().id("sync-menu"), false)
                                .child(icon("ellipsis").size_4().text_color(theme.text_soft))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.popover = match this.popover {
                                        Some(PopoverKind::SyncMenu) => None,
                                        _ => Some(PopoverKind::SyncMenu),
                                    };
                                    if this.popover == Some(PopoverKind::SyncMenu) {
                                        this.refresh_stashes(cx);
                                    }
                                    cx.notify();
                                }))
                                .when(self.popover == Some(PopoverKind::SyncMenu), |el| {
                                    el.child(self.sync_menu(cx, theme))
                                }),
                        )
                        .child(gear),
                );
        } else {
            content_side = content_side.child(div()).child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_faint)
                            .child("WackDepot"),
                    )
                    .child(gear),
            );
        }

        div()
            .h(px(theme::TITLEBAR_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .child(
                div()
                    .w(px(theme::SIDEBAR_WIDTH))
                    .h_full()
                    .flex_none()
                    .child(rail_side),
            )
            .child(self.drag_strip(
                "titlebar-content",
                div().flex_1().h_full().child(content_side),
                cx,
            ))
    }

    fn rail(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        let tabs = div()
            .flex()
            .gap_1()
            .px_2()
            .py_1p5()
            .flex_none()
            .child(
                pill_button(theme, div().id("tab-changes"), self.tab == Tab::Changes)
                    .flex_1()
                    .justify_center()
                    .child("Changes")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tab = Tab::Changes;
                        this.selected = None;
                        this.selected_diff = None;
                        cx.notify();
                    })),
            )
            .child(
                pill_button(theme, div().id("tab-history"), self.tab == Tab::History)
                    .flex_1()
                    .justify_center()
                    .child("History")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tab = Tab::History;
                        this.selected = None;
                        this.selected_diff = None;
                        cx.notify();
                    })),
            );

        let body: AnyElement = match self.tab {
            Tab::Changes => motion::slide_in("changes-pane", self.changes_view(cx, theme), -10.0)
                .into_any_element(),
            Tab::History => motion::slide_in("history-pane", self.history_view(cx, theme), -10.0)
                .into_any_element(),
        };

        div()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(tabs)
            .child(
                // Repo folders dropped on the rail open as repositories.
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        for p in paths.paths() {
                            if wackdepot_core::repo::is_repo(p) {
                                this.open_repo(p.clone(), cx);
                            } else {
                                this.notify_error(
                                    format!("{} isn't a git repository.", p.display()),
                                    cx,
                                );
                            }
                        }
                    }))
                    .child(body),
            )
    }

    fn content(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        let inner: Div = if self.active.is_none() {
            self.welcome_view(cx, theme)
        } else {
            match &self.selected {
                Some(Selected::Commit { .. }) => self.commit_detail(cx, theme),
                _ => self.diff_view(cx, theme),
            }
        };
        div()
            .flex_1()
            .h_full()
            .bg(theme.bg)
            .flex()
            .flex_col()
            .child(inner)
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme(window);

        let mut root = div()
            // App actions — keymap dispatch lands on the root's path.
            .on_action(cx.listener(|this, _: &crate::Dismiss, _, cx| this.dismiss(cx)))
            .on_action(cx.listener(|this, _: &crate::ShowChanges, _, cx| {
                this.tab = Tab::Changes;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::ShowHistory, _, cx| {
                this.tab = Tab::History;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::ToggleBranches, _, cx| {
                this.popover = match this.popover {
                    Some(PopoverKind::BranchPicker) => None,
                    _ => Some(PopoverKind::BranchPicker),
                };
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::DoFetch, _, cx| this.fetch(cx)))
            .on_action(cx.listener(|this, _: &crate::DoPull, _, cx| this.pull(cx)))
            .on_action(cx.listener(|this, _: &crate::DoPush, _, cx| this.push(cx)))
            .on_action(cx.listener(|this, _: &crate::DoCommit, _, cx| this.do_commit(cx)))
            .on_action(cx.listener(|this, _: &crate::Refresh, _, cx| {
                this.request_refresh(cx);
            }))
            .on_action(cx.listener(|this, _: &crate::OpenSettings, _, cx| {
                this.dialog = Some(Dialog::Settings);
                cx.notify();
            }))
            .size_full()
            .font_family(".SystemUIFont")
            .text_color(theme.text)
            .flex()
            .flex_col()
            .child(self.titlebar(window, cx, &theme))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(self.rail(cx, &theme))
                    .child(self.content(cx, &theme)),
            );

        // Dialog layer (centered, dim wash, frosted card).
        if let Some(dialog) = self.dialog.clone() {
            root = root.child(self.dialog_layer(cx, &theme, dialog));
        }

        // Toast bottom-right.
        if let Some(notice) = &self.notice {
            root = root.child(self.notice_layer(cx, &theme, notice.clone()));
        }

        // Busy scrim for long ops (subtle, non-blocking visual only).
        if self.busy.is_some() {
            root = root.child(self.busy_layer(&theme));
        }

        root
    }
}
