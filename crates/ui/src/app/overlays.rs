//! What floats above the window: one dialog at a time (over a scrim) and a
//! short stack of frosted toasts at the bottom centre.

use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement, Render, Styled,
    Window, div, prelude::*, px,
};
use wackdepot_core::diff::{FileDiff, Hunk};
use wackdepot_core::stage::LineSel;

use crate::components::button::{button, icon_button};
use crate::components::frost::frosted;
use crate::components::mark::accent_mark;
use crate::icons::icon;
use crate::model::{ConfirmAction, Notice, NoticeAction, NoticeKind, RepoModel};
use crate::motion::{EASE, Motion, POP, id as mid};
use crate::theme::{ActiveTheme, metrics};
use crate::views::dialogs::{CloneDialog, NameDialog, NameKind, NewRepoDialog};

/// A discard waiting on its confirm, carrying the diff it applies to.
pub enum Pending {
    Hunk(Box<FileDiff>, Box<Hunk>),
    Lines(Box<FileDiff>, LineSel),
}

pub enum Dialog {
    Task(TaskConfirmation),
    Confirm {
        action: ConfirmAction,
        pending: Option<Pending>,
    },
    Clone(Entity<CloneDialog>),
    NewRepo(Entity<NewRepoDialog>),
    Name(Entity<NameDialog>),
}

pub struct TaskConfirmation {
    pub title: String,
    pub body: String,
    pub label: String,
    pub danger: bool,
    pub action: Box<dyn FnOnce(&mut gpui::App)>,
}

struct Toast {
    id: u64,
    notice: Notice,
}

pub struct Overlays {
    model: Entity<RepoModel>,
    pub dialog: Option<Dialog>,
    toasts: Vec<Toast>,
    next_id: u64,
    motion: Motion,
}

impl Overlays {
    pub fn new(model: Entity<RepoModel>) -> Self {
        Self {
            model,
            dialog: None,
            toasts: Vec::new(),
            next_id: 0,
            motion: Motion::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn confirm_task(&mut self, confirmation: TaskConfirmation, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Task(confirmation));
        cx.notify();
    }

    pub fn confirm(
        &mut self,
        action: ConfirmAction,
        pending: Option<Pending>,
        cx: &mut Context<Self>,
    ) {
        self.dialog = Some(Dialog::Confirm { action, pending });
        cx.notify();
    }

    pub fn open_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.model.clone();
        let this = cx.entity().downgrade();
        let d = cx.new(|cx| CloneDialog::new(model, this, window, cx));
        self.dialog = Some(Dialog::Clone(d));
        cx.notify();
    }

    pub fn open_new_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.model.clone();
        let this = cx.entity().downgrade();
        let d = cx.new(|cx| NewRepoDialog::new(model, this, window, cx));
        self.dialog = Some(Dialog::NewRepo(d));
        cx.notify();
    }

    pub fn open_name(&mut self, kind: NameKind, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.model.clone();
        let this = cx.entity().downgrade();
        let d = cx.new(|cx| NameDialog::new(kind, model, this, window, cx));
        self.dialog = Some(Dialog::Name(d));
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        cx.notify();
    }

    /// Esc: close the dialog. Returns whether it handled anything.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dialog.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    pub fn toast(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.next_id += 1;
        let id = self.next_id;
        // The same message twice in a row just refreshes the existing one.
        self.toasts.retain(|t| t.notice.text != notice.text);
        self.toasts.push(Toast { id, notice });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
        cx.notify();
        let secs = if self
            .toasts
            .last()
            .is_some_and(|t| t.notice.kind == NoticeKind::Error)
        {
            9
        } else {
            5
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(secs))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.toasts.retain(|t| t.id != id);
                cx.notify();
            });
        })
        .detach();
    }

    fn confirm_card(&mut self, action: &ConfirmAction, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let entity = cx.entity();
        let cancel = {
            let e = entity.clone();
            button("confirm-cancel", "Cancel")
                .on_click(move |_, _, cx| e.update(cx, |o, cx| o.close(cx)))
        };
        let go = {
            let e = entity.clone();
            button("confirm-go", action.confirm_label())
                .danger()
                .on_click(move |_, _, cx| {
                    e.update(cx, |o, cx| o.run_confirmed(cx));
                })
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .items_start()
                    .child(
                        div()
                            .size(px(30.))
                            .flex_none()
                            .rounded_full()
                            .bg(t.sem.danger_bg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("alert").size(px(16.)).text_color(t.sem.danger)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(metrics::TEXT_TITLE))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(action.title()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(t.text_soft)
                                    .child(action.body()),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(cancel)
                    .child(go),
            )
    }

    fn run_confirmed(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Confirm { action, pending }) = self.dialog.take() else {
            return;
        };
        self.model.update(cx, |m, cx| match pending {
            Some(Pending::Hunk(file, hunk)) => m.discard_hunk_now(*file, *hunk, cx),
            Some(Pending::Lines(file, sel)) => m.discard_lines_now(*file, sel, cx),
            None => m.perform(action, cx),
        });
        cx.notify();
    }

    fn toast_el(&self, toast: &Toast, enter: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let entity = cx.entity();
        let model = self.model.clone();
        let error = toast.notice.kind == NoticeKind::Error;
        let id = toast.id;
        let mut card = div()
            .id(("toast", id))
            .flex()
            .items_center()
            .gap(px(10.))
            .min_h(px(40.))
            .max_w(px(520.))
            .pl(px(10.))
            .pr(px(6.))
            .py(px(6.))
            .rounded(px(14.))
            .bg(if error {
                t.sem.danger_bg.opacity(0.94)
            } else {
                t.frost()
            })
            .border_1()
            .border_color(if error {
                t.sem.danger_line
            } else {
                t.border_bright.opacity(0.7)
            })
            .shadow(t.shadow_pop())
            .text_size(px(12.5))
            .child(if error {
                div()
                    .size(px(22.))
                    .flex_none()
                    .rounded_full()
                    .bg(t.sem.danger.opacity(0.16))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("alert").size(px(13.)).text_color(t.sem.danger))
                    .into_any_element()
            } else {
                accent_mark("check").into_any_element()
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(if error { t.sem.danger_text } else { t.text })
                    .child(toast.notice.text.clone()),
            );
        if let Some(action) = toast.notice.action {
            let label = match action {
                NoticeAction::Pull => "Pull",
                NoticeAction::Retry(_) => "Retry",
            };
            let e = entity.clone();
            card = card.child(button(("toast-act", id), label).small().lit(true).on_click(
                move |_, _, cx| {
                    model.update(cx, |m, cx| match action {
                        NoticeAction::Pull => m.pull(cx),
                        NoticeAction::Retry(op) => m.retry(op, cx),
                    });
                    e.update(cx, |o, cx| {
                        o.toasts.retain(|t| t.id != id);
                        cx.notify();
                    });
                },
            ));
        }
        let e = entity.clone();
        card = card.child(icon_button(("toast-x", id), "close").small().on_click(
            move |_, _, cx| {
                e.update(cx, |o, cx| {
                    o.toasts.retain(|t| t.id != id);
                    cx.notify();
                });
            },
        ));
        div()
            .relative()
            .top(px((1.0 - enter) * 12.0))
            .opacity(enter.clamp(0.0, 1.0))
            .child(frosted(14.0, metrics::FROST_BLUR, card))
    }
}

impl Render for Overlays {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.motion.begin(cx);
        let t = cx.theme().clone();
        let mut root = div().absolute().inset_0().flex().flex_col();

        if self.dialog.is_some() {
            let shown = self
                .motion
                .tween_from(("dialog", 0), 0.0, 1.0, 180.0, 0.0, EASE);
            let rise = self
                .motion
                .tween_from(("dialog-rise", 0), 10.0, 0.0, 260.0, 0.0, POP);
            let entity = cx.entity();
            let body = match self.dialog.as_ref().expect("checked") {
                Dialog::Task(task) => {
                    let (title, body, label, danger) = (
                        task.title.clone(),
                        task.body.clone(),
                        task.label.clone(),
                        task.danger,
                    );
                    let cancel = cx.entity();
                    let confirm = cx.entity();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(17.))
                                .child(title),
                        )
                        .child(
                            div()
                                .id("task-confirm-body")
                                .max_h(px(360.))
                                .overflow_y_scroll()
                                .text_size(px(13.))
                                .text_color(t.text_soft)
                                .child(body),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(button("task-cancel", "Cancel").on_click(move |_, _, cx| {
                                    cancel.update(cx, |o, cx| o.close(cx))
                                }))
                                .child(
                                    button("task-confirm", label)
                                        .variant(if danger {
                                            crate::components::button::Variant::Danger
                                        } else {
                                            crate::components::button::Variant::Primary
                                        })
                                        .on_click(move |_, _, cx| {
                                            let task = confirm.update(cx, |o, cx| {
                                                let task = o.dialog.take();
                                                cx.notify();
                                                task
                                            });
                                            if let Some(Dialog::Task(task)) = task {
                                                (task.action)(cx);
                                            }
                                        }),
                                ),
                        )
                        .into_any_element()
                }
                Dialog::Confirm { action, .. } => {
                    let action = action.clone();
                    self.confirm_card(&action, cx).into_any_element()
                }
                Dialog::Clone(d) => d.clone().into_any_element(),
                Dialog::NewRepo(d) => d.clone().into_any_element(),
                Dialog::Name(d) => d.clone().into_any_element(),
            };
            let card = div()
                .id("dialog-card")
                .w(px(440.))
                .p(px(20.))
                .rounded(px(metrics::RADIUS_CARD + 2.0))
                .bg(t.surface)
                .border_1()
                .border_color(t.border_bright.opacity(0.8))
                .shadow(t.shadow_pop())
                .occlude()
                .child(body);
            root = root.child(
                div()
                    .id("scrim")
                    .absolute()
                    .inset_0()
                    .bg(t.scrim().opacity(t.scrim().a * shown))
                    .occlude()
                    .on_click(move |_, _, cx| entity.update(cx, |o, cx| o.close(cx)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .relative()
                            .top(px(rise))
                            .opacity(shown.clamp(0.0, 1.0))
                            .child(card),
                    ),
            );
        }

        if !self.toasts.is_empty() {
            let list: Vec<Toast> = self
                .toasts
                .iter()
                .map(|t| Toast {
                    id: t.id,
                    notice: t.notice.clone(),
                })
                .collect();
            let toasts: Vec<_> = list
                .iter()
                .map(|toast| {
                    let enter =
                        self.motion
                            .tween_from(("toast", mid(toast.id)), 0.0, 1.0, 260.0, 0.0, POP);
                    self.toast_el(toast, enter, cx).into_any_element()
                })
                .collect();
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(22.))
                    .left_0()
                    .right_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .children(toasts),
            );
        }
        self.motion.end(window);
        root
    }
}
