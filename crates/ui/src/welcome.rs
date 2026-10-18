//! The no-repo content pane: a centered stage with the three ways in —
//! add an existing folder, clone a URL, create a new repo. The mascot slot
//! waits on the designer's mark; for now an accent dot placeholder.

use gpui::{Context, Div, Styled, div, prelude::*, px};

use crate::icons::icon;
use crate::shell::Shell;
use crate::state::Dialog;
use crate::theme::Theme;

impl Shell {
    pub fn welcome_view(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        type CardAction = Box<dyn Fn(&mut Shell, &mut Context<Shell>)>;
        let card = |icon_name: &'static str,
                    title: &'static str,
                    body: &'static str,
                    id: &'static str,
                    action: CardAction| {
            div()
                .id(id)
                .w(px(220.))
                .rounded_lg()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .p_4()
                .flex()
                .flex_col()
                .gap_2()
                .cursor_pointer()
                .hover(|s| s.border_color(theme.accent.opacity(0.5)))
                .occlude()
                .child(
                    div()
                        .w_8()
                        .h_8()
                        .rounded_full()
                        .bg(theme.accent.opacity(0.15))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(icon_name).text_color(theme.accent)),
                )
                .child(div().text_sm().text_color(theme.text).child(title))
                .child(div().text_xs().text_color(theme.text_dim).child(body))
                .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_6()
            .child(
                // Mascot slot — designer's mark lands here; accent dot for now.
                div()
                    .w(px(64.))
                    .h(px(64.))
                    .rounded_full()
                    .bg(theme.accent.opacity(0.2))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(28.)).h(px(28.)).rounded_full().bg(theme.accent)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .text_color(theme.text)
                            .child("Welcome to WackDepot"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_dim)
                            .child("A friendlier face for git — pick a repository to begin."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(card(
                        "folder",
                        "Add existing",
                        "Open a folder that's already a git repository.",
                        "welcome-add",
                        Box::new(|this, cx| this.add_existing(cx)),
                    ))
                    .child(card(
                        "cloud",
                        "Clone",
                        "Pull a repository down from a URL.",
                        "welcome-clone",
                        Box::new(|this, cx| {
                            this.dialog = Some(Dialog::CloneRepo);
                            cx.notify();
                        }),
                    ))
                    .child(card(
                        "plus",
                        "Create new",
                        "Start a fresh repository in a new folder.",
                        "welcome-new",
                        Box::new(|this, cx| {
                            this.dialog = Some(Dialog::NewRepo);
                            cx.notify();
                        }),
                    )),
            )
    }
}
