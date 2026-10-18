//! Anchored popovers: repo picker, branch picker, sync menu. All render
//! through `popover::menu`/`row` so they share the frosted card anatomy.

use gpui::{AnyElement, Context, Div, Styled, div, prelude::*};

use crate::icons::icon;
use crate::legacy::popover::{menu, row};
use crate::legacy::shell::Shell;
use crate::legacy::state::Dialog;

impl Shell {
    /// The repo switcher: list of added repos + add/clone/create actions.
    pub fn repo_picker(
        &self,
        cx: &mut Context<Self>,
        theme: &crate::legacy::theme::Theme,
    ) -> impl IntoElement {
        let mut items: Vec<AnyElement> = Vec::new();
        for path in self.settings.repos.clone() {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            let selected = self.active.as_ref() == Some(&path);
            let p2 = path.clone();
            let p3 = path.clone();
            let item = row(
                theme,
                format!("repo-{}", path.display()),
                Some(icon("folder").text_color(if selected {
                    theme.accent
                } else {
                    theme.text_dim
                })),
                name,
                Some(
                    div()
                        .id(format!("rm-{}", path.display()))
                        .w_5()
                        .h_5()
                        .text_color(theme.danger)
                        .occlude()
                        .child(icon("x").size_4().text_color(theme.danger))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_repo(&p3, cx);
                            cx.stop_propagation();
                        })),
                ),
                selected,
                cx.listener(move |this, _, _, cx| this.open_repo(p2.clone(), cx)),
            );
            items.push(item.into_any_element());
        }
        items.push(separator(theme).into_any_element());
        items.push(
            row(
                theme,
                "add-existing",
                Some(icon("repo-add").text_color(theme.accent)),
                "Add existing repository…",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| this.add_existing(cx)),
            )
            .into_any_element(),
        );
        items.push(
            row(
                theme,
                "clone-repo",
                Some(icon("cloud").text_color(theme.accent)),
                "Clone repository…",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.dialog = Some(Dialog::CloneRepo);
                    cx.notify();
                }),
            )
            .into_any_element(),
        );
        items.push(
            row(
                theme,
                "new-repo",
                Some(icon("plus").text_color(theme.accent)),
                "Create new repository…",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.dialog = Some(Dialog::NewRepo);
                    cx.notify();
                }),
            )
            .into_any_element(),
        );

        menu(theme, 280.0, self.dismiss_popover(cx), items)
    }

    /// Branch list + create form.
    pub fn branch_picker(
        &self,
        cx: &mut Context<Self>,
        theme: &crate::legacy::theme::Theme,
    ) -> impl IntoElement {
        let mut items: Vec<AnyElement> = Vec::new();
        let current = self.repo.status.as_ref().and_then(|s| s.branch.clone());
        for b in self.repo.branches.clone() {
            let name = b.name.clone();
            let name2 = b.name.clone();
            let name3 = b.name.clone();
            let is_current = current.as_deref() == Some(b.name.as_str());
            let trailing = if is_current {
                None
            } else {
                Some(
                    div()
                        .id(format!("del-{}", b.name))
                        .w_5()
                        .h_5()
                        .occlude()
                        .child(icon("trash").size_4().text_color(theme.danger))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask_delete_branch(name2.clone(), false, cx);
                            cx.stop_propagation();
                        })),
                )
            };
            items.push(
                row(
                    theme,
                    format!("br-{}", b.name),
                    Some(icon("git-branch").text_color(if is_current {
                        theme.accent
                    } else {
                        theme.text_dim
                    })),
                    name,
                    trailing,
                    is_current,
                    cx.listener(move |this, _, _, cx| {
                        if !is_current {
                            this.checkout(name3.clone(), cx);
                        } else {
                            this.popover = None;
                            cx.notify();
                        }
                    }),
                )
                .into_any_element(),
            );
        }
        items.push(separator(theme).into_any_element());
        // Inline create form: name + base inputs and a Create button.
        items.push(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_dim)
                        .child("New branch"),
                )
                .child(
                    div()
                        .rounded_md()
                        .bg(theme.input)
                        .border_1()
                        .border_color(theme.border)
                        .px_2()
                        .py_0p5()
                        .text_sm()
                        .child(self.branch_name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .rounded_md()
                                .bg(theme.input)
                                .border_1()
                                .border_color(theme.border)
                                .px_2()
                                .py_0p5()
                                .text_sm()
                                .child(self.branch_base.clone()),
                        )
                        .child(
                            div()
                                .id("create-branch-btn")
                                .h_6()
                                .px_2()
                                .rounded_md()
                                .flex()
                                .items_center()
                                .text_xs()
                                .bg(theme.accent.opacity(0.2))
                                .text_color(theme.accent)
                                .cursor_pointer()
                                .hover(|s| s.bg(theme.accent.opacity(0.3)))
                                .occlude()
                                .child("Create")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.create_branch(cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .into_any_element(),
        );

        menu(theme, 280.0, self.dismiss_popover(cx), items)
    }

    /// The "…" menu beside the sync button: fetch/pull/push, stash, open-in.
    pub fn sync_menu(
        &self,
        cx: &mut Context<Self>,
        theme: &crate::legacy::theme::Theme,
    ) -> impl IntoElement {
        let mut items: Vec<AnyElement> = vec![
            row(
                theme,
                "m-fetch",
                Some(icon("fetch").text_color(theme.text_soft)),
                "Fetch",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.fetch(cx);
                }),
            )
            .into_any_element(),
            row(
                theme,
                "m-pull",
                Some(icon("pull").text_color(theme.text_soft)),
                "Pull",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.pull(cx);
                }),
            )
            .into_any_element(),
            row(
                theme,
                "m-push",
                Some(icon("push").text_color(theme.text_soft)),
                "Push",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.push(cx);
                }),
            )
            .into_any_element(),
            separator(theme).into_any_element(),
            row(
                theme,
                "m-stash",
                Some(icon("stash").text_color(theme.text_soft)),
                "Stash all changes",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| this.stash_push(cx)),
            )
            .into_any_element(),
        ];
        for s in self.stashes.clone() {
            let label = if s.message.is_empty() {
                s.name.clone()
            } else {
                format!("{} — {}", s.name, s.message)
            };
            let n = s.name.clone();
            items.push(
                row(
                    theme,
                    format!("stash-{}", s.name),
                    Some(icon("stash").text_color(theme.warning)),
                    format!("Apply {label}"),
                    Some(
                        div()
                            .id(format!("stash-drop-{}", s.name))
                            .occlude()
                            .child(icon("trash").size_4().text_color(theme.danger))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ask_drop_stash(n.clone(), cx);
                                cx.stop_propagation();
                            })),
                    ),
                    false,
                    cx.listener(|this, _, _, cx| this.stash_apply(cx)),
                )
                .into_any_element(),
            );
        }
        items.push(separator(theme).into_any_element());
        items.push(
            row(
                theme,
                "m-editor",
                Some(icon("external").text_color(theme.text_soft)),
                "Open in Editor",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| this.open_in_editor(cx)),
            )
            .into_any_element(),
        );
        items.push(
            row(
                theme,
                "m-terminal",
                Some(icon("external").text_color(theme.text_soft)),
                "Open in Terminal",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| this.open_in_terminal(cx)),
            )
            .into_any_element(),
        );
        items.push(
            row(
                theme,
                "m-finder",
                Some(icon("folder").text_color(theme.text_soft)),
                "Show in Finder",
                None::<Div>,
                false,
                cx.listener(|this, _, _, cx| this.open_in_finder(cx)),
            )
            .into_any_element(),
        );

        menu(theme, 260.0, self.dismiss_popover(cx), items)
    }

    /// Popover-layer dismiss: clears only the popover.
    fn dismiss_popover(&self, cx: &mut Context<Self>) -> impl Fn(&mut Window, &mut App) + 'static {
        let shell = cx.weak_entity();
        move |_, cx| {
            let _ = shell.update(cx, |this, cx| {
                this.popover = None;
                cx.notify();
            });
        }
    }
}

fn separator(theme: &crate::legacy::theme::Theme) -> Div {
    div().h_px().mx_1().my_1().bg(theme.border)
}

use gpui::{App, Window};
