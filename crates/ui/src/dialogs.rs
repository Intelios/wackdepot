//! Centered dialogs (clone/new-repo forms + confirm cards) and the toast
//! layer, all frosted cards over the dimmed window.

use gpui::{AnyElement, Context, Div, Styled, div, prelude::*, px};

use crate::frost::{self, MENU_BLUR};
use crate::icons::icon;
use crate::ops::PickTarget;
use crate::shell::Shell;
use crate::state::{ConfirmAction, Dialog, Notice};
use crate::theme::Theme;

/// A dialog action button; `danger` paints it in the danger color.
fn button(theme: &Theme, id: &'static str, label: &str, kind: BtnKind) -> gpui::Stateful<Div> {
    let (bg, color) = match kind {
        BtnKind::Primary => (theme.accent, theme.on_accent),
        BtnKind::Secondary => (theme.elevated, theme.text_soft),
        BtnKind::Danger => (theme.danger, theme.on_accent),
        BtnKind::Ghost => (gpui::hsla(0., 0., 0., 0.), theme.text_dim),
    };
    let hover = match kind {
        BtnKind::Primary => theme.accent.opacity(0.85),
        BtnKind::Danger => theme.danger.opacity(0.85),
        _ => theme.accent.opacity(0.12),
    };
    div()
        .id(id)
        .h_7()
        .px_3()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .bg(bg)
        .text_color(color)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .occlude()
        .child(label.to_string())
}

#[derive(Clone, Copy)]
enum BtnKind {
    Primary,
    Secondary,
    Danger,
    #[allow(dead_code)]
    Ghost,
}

impl Shell {
    /// The dim + centered frosted card for `dialog`.
    pub fn dialog_layer(
        &self,
        cx: &mut Context<Self>,
        theme: &Theme,
        dialog: Dialog,
    ) -> impl IntoElement {
        let card: AnyElement = match &dialog {
            Dialog::CloneRepo => self.clone_card(cx, theme).into_any_element(),
            Dialog::NewRepo => self.new_repo_card(cx, theme).into_any_element(),
            Dialog::Confirm(action) => self.confirm_card(cx, theme, action).into_any_element(),
        };

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::hsla(0., 0., 0., 0.35))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    // Click on the dim, outside the card, dismisses.
                    this.dialog = None;
                    cx.notify();
                }),
            )
            .child(frost::frosted(
                14.0,
                MENU_BLUR,
                theme.frost,
                div()
                    .w(px(380.))
                    .rounded_xl()
                    .bg(theme.menu)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Swallow clicks — the dim only dismisses on misses.
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(card),
            ))
    }

    fn clone_card(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .text_color(theme.text)
                    .child("Clone a repository"),
            )
            .child(field(theme, "Repository URL", self.clone_url.clone()))
            .child(field_row(
                theme,
                "Local folder",
                self.clone_dir.clone(),
                "Choose…",
                cx.listener(|this, _, _, cx| this.pick_dir(PickTarget::CloneDir, cx)),
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        button(theme, "clone-cancel", "Cancel", BtnKind::Secondary).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.dialog = None;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button(theme, "clone-go", "Clone", BtnKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| this.do_clone(cx))),
                    ),
            )
    }

    fn new_repo_card(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .text_color(theme.text)
                    .child("Create a repository"),
            )
            .child(field(theme, "Name", self.new_repo_name.clone()))
            .child(field_row(
                theme,
                "Local folder",
                self.new_repo_dir.clone(),
                "Choose…",
                cx.listener(|this, _, _, cx| this.pick_dir(PickTarget::NewRepoDir, cx)),
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        button(theme, "new-cancel", "Cancel", BtnKind::Secondary).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.dialog = None;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button(theme, "new-go", "Create", BtnKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| this.do_new_repo(cx))),
                    ),
            )
    }

    fn confirm_card(&self, cx: &mut Context<Self>, theme: &Theme, action: &ConfirmAction) -> Div {
        let action = action.clone();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon("warning").text_color(theme.danger))
                    .child(div().text_lg().text_color(theme.text).child(action.title())),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_soft)
                    .child(action.body()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        button(theme, "confirm-cancel", "Cancel", BtnKind::Secondary).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.dialog = None;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button(theme, "confirm-go", "Do it", BtnKind::Danger).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.confirm(action.clone(), cx);
                            }),
                        ),
                    ),
            )
    }

    /// The bottom-right toast.
    pub fn notice_layer(
        &self,
        cx: &mut Context<Self>,
        theme: &Theme,
        notice: Notice,
    ) -> impl IntoElement {
        div().absolute().bottom_4().right_4().child(frost::frosted(
            10.0,
            MENU_BLUR,
            theme.frost,
            div()
                .id("notice")
                .max_w(px(360.))
                .rounded_lg()
                .bg(theme.menu)
                .border_1()
                .border_color(if notice.is_error {
                    theme.danger
                } else {
                    theme.accent
                })
                .shadow_lg()
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .child(
                    icon(if notice.is_error { "warning" } else { "check" }).text_color(
                        if notice.is_error {
                            theme.danger
                        } else {
                            theme.accent
                        },
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(theme.text)
                        .child(notice.text.clone()),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                })),
        ))
    }

    /// A faint accent pulse in the titlebar while an op runs (paint-only).
    pub fn busy_layer(&self, theme: &Theme) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h_px()
            .bg(theme.accent.opacity(0.6))
    }
}

/// Label + input field.
fn field(theme: &Theme, label: &str, input: gpui::Entity<crate::input::TextInput>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(theme.text_dim)
                .child(label.to_string()),
        )
        .child(
            div()
                .rounded_md()
                .bg(theme.input)
                .border_1()
                .border_color(theme.border)
                .px_2()
                .py_1()
                .text_sm()
                .child(input),
        )
}

/// Label + input + a trailing "Choose…" button.
fn field_row(
    theme: &Theme,
    label: &str,
    input: gpui::Entity<crate::input::TextInput>,
    btn_label: &str,
    on_choose: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(theme.text_dim)
                .child(label.to_string()),
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
                        .py_1()
                        .text_sm()
                        .child(input),
                )
                .child(
                    div()
                        .id(format!("pick-{label}"))
                        .h_7()
                        .px_2()
                        .rounded_md()
                        .flex()
                        .items_center()
                        .text_sm()
                        .bg(theme.elevated)
                        .text_color(theme.text_soft)
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.accent.opacity(0.12)))
                        .occlude()
                        .child(btn_label.to_string())
                        .on_click(on_choose),
                ),
        )
}
