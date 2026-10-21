//! The commit composer: a solid card at the foot of the Changes column —
//! summary, a description that grows as you type, amend and co-author
//! controls, and the commit pill. Fields clear only when the commit lands.

use gpui::{
    AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement, Render, Styled,
    Window, div, prelude::*, px,
};
use wackdepot_core::commit::CommitOptions;

use crate::components::button::{Variant, button, icon_button};
use crate::components::seam::seam;
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::icons::icon;
use crate::model::{Beat, Bucket, BusyOp, RepoEvent, RepoModel};
use crate::motion::{LINEAR, Motion, POP_STRONG};
use crate::theme::{ActiveTheme, metrics};

pub struct Composer {
    model: Entity<RepoModel>,
    summary: Entity<TextInput>,
    description: Entity<TextInput>,
    coauthor_input: Entity<TextInput>,
    amend: bool,
    co_authors: Vec<(String, String)>,
    adding_coauthor: bool,
    error: Option<String>,
    motion: Motion,
}

impl Composer {
    pub fn new(model: Entity<RepoModel>, cx: &mut Context<Self>) -> Self {
        let summary = cx.new(|cx| TextInput::new("Summary (required)", cx));
        let description = cx.new(|cx| TextInput::new("Description", cx).multiline(2, 8));
        let coauthor_input = cx.new(|cx| TextInput::new("Name <email@example.com>", cx));
        cx.subscribe(&summary, |this: &mut Self, _, e: &TextInputEvent, cx| {
            this.error = None;
            if *e == TextInputEvent::Submit {
                this.commit(cx);
            }
            cx.notify();
        })
        .detach();
        cx.subscribe(&description, |_, _, _: &TextInputEvent, cx| cx.notify())
            .detach();
        cx.subscribe(
            &coauthor_input,
            |this: &mut Self, _, e: &TextInputEvent, cx| {
                if *e == TextInputEvent::Submit {
                    this.add_coauthor(cx);
                }
            },
        )
        .detach();
        cx.subscribe(&model, |this: &mut Self, _, e: &RepoEvent, cx| match e {
            RepoEvent::Beat(Beat::Committed { .. }) => {
                this.reset(cx);
                // The seal: a stamp pops onto the card and the rim flashes.
                this.motion
                    .restart(("seal", 0), 0.0, 1.0, 1200.0, 0.0, LINEAR);
                this.motion
                    .restart(("stamp", 0), 0.4, 1.0, 380.0, 0.0, POP_STRONG);
                cx.notify();
            }
            RepoEvent::Opened => this.reset(cx),
            RepoEvent::Snapshot | RepoEvent::Busy => cx.notify(),
            _ => {}
        })
        .detach();
        Self {
            model,
            summary,
            description,
            coauthor_input,
            amend: false,
            co_authors: Vec::new(),
            adding_coauthor: false,
            error: None,
            motion: Motion::default(),
        }
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.summary.update(cx, |i, cx| i.clear(cx));
        self.description.update(cx, |i, cx| i.clear(cx));
        self.amend = false;
        self.co_authors.clear();
        self.adding_coauthor = false;
        cx.notify();
    }

    /// Files the commit button will pick up — the checked ones.
    fn included(&self, cx: &App) -> usize {
        self.model.read(cx).included_count()
    }

    pub fn can_commit(&self, cx: &App) -> bool {
        let m = self.model.read(cx);
        let has_summary = !self.summary.read(cx).content().trim().is_empty();
        let conflicts = !m.snap.bucket(Bucket::Conflicted).is_empty();
        has_summary && (self.included(cx) > 0 || self.amend) && !conflicts && m.busy.is_none()
    }

    /// ⌘↩ and the commit pill.
    pub fn commit(&mut self, cx: &mut Context<Self>) {
        if self.summary.read(cx).content().trim().is_empty() {
            self.error = Some("Write a summary first.".into());
            cx.notify();
            return;
        }
        if !self.can_commit(cx) {
            return;
        }
        let summary = self.summary.read(cx).content().to_string();
        let description = self.description.read(cx).content().to_string();
        let opts = CommitOptions {
            amend: self.amend,
            co_authors: self.co_authors.clone(),
        };
        self.model
            .update(cx, |m, cx| m.commit(summary, description, opts, cx));
    }

    fn toggle_amend(&mut self, cx: &mut Context<Self>) {
        self.amend = !self.amend;
        if self.amend && self.summary.read(cx).is_empty() {
            let repo = self.model.read(cx).repo.clone();
            if let Some(repo) = repo {
                cx.spawn(async move |this, cx| {
                    let msg = cx
                        .background_executor()
                        .spawn(async move { wackdepot_core::commit::head_message(&repo) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Ok((s, d)) = msg
                            && this.amend
                            && this.summary.read(cx).is_empty()
                        {
                            this.summary.update(cx, |i, cx| i.set_content(s, cx));
                            this.description.update(cx, |i, cx| i.set_content(d, cx));
                        }
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }

    fn add_coauthor(&mut self, cx: &mut Context<Self>) {
        let text = self.coauthor_input.read(cx).content().trim().to_string();
        match parse_person(&text) {
            Some(p) => {
                if !self.co_authors.contains(&p) {
                    self.co_authors.push(p);
                }
                self.coauthor_input.update(cx, |i, cx| i.clear(cx));
                self.adding_coauthor = false;
                self.error = None;
            }
            None => self.error = Some("Co-authors look like Name <email@example.com>.".into()),
        }
        cx.notify();
    }
}

use gpui::App;

/// `Name <email>` → `(name, email)`.
fn parse_person(s: &str) -> Option<(String, String)> {
    let (name, rest) = s.split_once('<')?;
    let email = rest.strip_suffix('>')?.trim();
    let name = name.trim();
    (!name.is_empty() && email.contains('@')).then(|| (name.to_string(), email.to_string()))
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.motion.begin(cx);
        let seal = self.motion.value(("seal", 0), 1.0);
        let stamp = self.motion.value(("stamp", 0), 1.0);
        let sealing = seal < 1.0;
        self.motion.end(window);
        let t = cx.theme().clone();
        let model = self.model.read(cx);
        let branch = model.branch_label();
        let committing = model.busy == Some(BusyOp::Commit);
        let tip_unpushed = model.snap.tip_unpushed;
        let has_commits = !model.snap.log.is_empty();
        let included = self.included(cx);
        let conflicts = !model.snap.bucket(Bucket::Conflicted).is_empty();
        let can = self.can_commit(cx);
        let focused = self.summary.read(cx).is_focused(window)
            || self.description.read(cx).is_focused(window)
            || self.coauthor_input.read(cx).is_focused(window);

        let label = if committing {
            "Committing…".to_string()
        } else if self.amend {
            "Amend last commit".to_string()
        } else if conflicts {
            "Resolve conflicts to commit".to_string()
        } else if included == 0 {
            "Check files to commit".to_string()
        } else {
            format!("Commit {included} to {branch}")
        };

        let entity = cx.entity();
        let amend_btn = {
            let e = entity.clone();
            icon_button("amend", "amend")
                .variant(if self.amend {
                    Variant::Lit
                } else {
                    Variant::Ghost
                })
                .disabled(!has_commits || !tip_unpushed)
                .on_click(move |_, _, cx| e.update(cx, |c, cx| c.toggle_amend(cx)))
        };
        let coauthor_btn = {
            let e = entity.clone();
            icon_button("coauthor", "user-plus")
                .variant(if self.adding_coauthor || !self.co_authors.is_empty() {
                    Variant::Lit
                } else {
                    Variant::Ghost
                })
                .on_click(move |_, window, cx| {
                    e.update(cx, |c, cx| {
                        c.adding_coauthor = !c.adding_coauthor;
                        if c.adding_coauthor {
                            c.coauthor_input.update(cx, |i, cx| i.focus(window, cx));
                        }
                        cx.notify();
                    })
                })
        };
        let commit_btn = {
            let e = entity.clone();
            button("commit", label)
                .variant(if can {
                    Variant::Primary
                } else {
                    Variant::Secondary
                })
                .disabled(!can)
                .on_click(move |_, _, cx| e.update(cx, |c, cx| c.commit(cx)))
        };

        let mut chips = div().flex().flex_wrap().gap(px(6.));
        for (i, (name, _)) in self.co_authors.iter().enumerate() {
            let e = entity.clone();
            chips = chips.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(22.))
                    .pl(px(9.))
                    .pr(px(3.))
                    .rounded_full()
                    .bg(t.selected_fill())
                    .shadow(vec![t.rim(t.accent_line_soft)])
                    .text_size(px(11.5))
                    .text_color(t.accent_text)
                    .child(name.clone())
                    .child(icon_button(("coauthor-x", i), "close").small().on_click(
                        move |_, _, cx| {
                            e.update(cx, |c, cx| {
                                c.co_authors.remove(i);
                                cx.notify();
                            })
                        },
                    )),
            );
        }

        let rim = if sealing {
            t.accent.opacity((1.0 - seal).clamp(0.0, 1.0))
        } else if focused {
            t.accent_line_mid
        } else {
            t.border_bright.opacity(0.7)
        };
        let fade = if seal < 0.65 {
            1.0
        } else {
            (1.0 - (seal - 0.65) / 0.35).max(0.0)
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(12.))
            .rounded(px(metrics::RADIUS_COMPOSER))
            .bg(t.composer)
            .border_1()
            .border_color(rim)
            .when(sealing, |el| {
                let size = 46.0 * stamp;
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(fade)
                        .child(
                            div()
                                .size(px(size))
                                .rounded_full()
                                .bg(t.accent)
                                .shadow(t.accent_glow())
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon("check").size(px(size * 0.5)).text_color(t.on_accent)),
                        ),
                )
            })
            .when(focused, |el| {
                el.shadow(vec![gpui::BoxShadow {
                    color: t.focus_halo(),
                    offset: gpui::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(4.),
                    inset: false,
                }])
            })
            .child(
                div()
                    .text_size(px(metrics::TEXT_BASE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.summary.clone()),
            )
            // The same seam the diff card wears, between the two fields.
            .child(div().mx(px(-12.)).my(px(2.)).child(seam()))
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(t.text_body)
                    .child(self.description.clone()),
            )
            .when(!self.co_authors.is_empty(), |el| el.child(chips))
            .when(self.adding_coauthor, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(30.))
                        .px(px(10.))
                        .rounded_full()
                        .bg(t.input)
                        .border_1()
                        .border_color(t.border)
                        .text_size(px(12.5))
                        .child(self.coauthor_input.clone()),
                )
            })
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_size(px(11.5)).text_color(t.sem.danger).child(e)),
            )
            .child(
                div()
                    .mt(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(amend_btn)
                    .child(coauthor_btn)
                    .child(div().flex_1())
                    .child(commit_btn),
            )
    }
}
