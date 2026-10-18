//! Settings, as a page in the app (WackCode's shape): sections in the left
//! column, and in the card a centred column that opens with a hero and
//! continues in solid blocks. Changes apply live.

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FontWeight, IntoElement, ParentElement,
    Render, RenderOnce, SharedString, Styled, Window, div, prelude::*, px,
};

use crate::app::prefs;
use crate::components::button::{Variant, button};
use crate::components::mark::accent_mark;
use crate::components::mascot::{Pose, mascot};
use crate::components::menu::menu_row;
use crate::components::misc::kicker;
use crate::components::pill_track::pill_track;
use crate::components::slider::slider;
use crate::components::text_input::{TextInput, TextInputEvent};
use crate::settings::{Backdrop, MotionPref};
use crate::theme::presets::PRESETS;
use crate::theme::{self, ActiveTheme, metrics};
use crate::views::dialogs::field;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Git,
    Integrations,
    About,
}

impl Section {
    const ALL: [Section; 4] = [
        Section::Appearance,
        Section::Git,
        Section::Integrations,
        Section::About,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Git => "Git",
            Section::Integrations => "Integrations",
            Section::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Appearance => "palette",
            Section::Git => "branch",
            Section::Integrations => "link",
            Section::About => "spark",
        }
    }
}

/// Things the workspace must act on (it owns the window backdrop).
pub enum SettingsEvent {
    Backdrop,
    Close,
}

pub struct SettingsPage {
    pub section: Section,
    accent: Entity<TextInput>,
    background: Entity<TextInput>,
    name: Entity<TextInput>,
    email: Entity<TextInput>,
    editor: Entity<TextInput>,
    note: Option<(SharedString, bool)>,
}

impl EventEmitter<SettingsEvent> for SettingsPage {}

impl SettingsPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let s = prefs::get(cx).clone();
        let mk = |placeholder: &str, value: Option<String>, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut i = TextInput::new(placeholder.to_string(), cx);
                if let Some(v) = value {
                    i.set_content(v, cx);
                }
                i
            })
        };
        let (gname, gemail) = wackdepot_core::config::global_identity();
        let accent = mk("#c2ee4a", s.accent_hex.clone(), cx);
        let background = mk("#111310", s.background_hex.clone(), cx);
        let name = mk("Your Name", gname, cx);
        let email = mk("you@example.com", gemail, cx);
        let editor = mk("WackCode", s.editor_app.clone(), cx);
        for input in [&accent, &background] {
            cx.subscribe(input, |this: &mut Self, _, e: &TextInputEvent, cx| {
                if *e == TextInputEvent::Submit {
                    this.apply_hex(cx);
                }
            })
            .detach();
        }
        for input in [&name, &email] {
            cx.subscribe(input, |this: &mut Self, _, e: &TextInputEvent, cx| {
                if *e == TextInputEvent::Submit {
                    this.save_identity(cx);
                }
            })
            .detach();
        }
        cx.subscribe(&editor, |this: &mut Self, input, e: &TextInputEvent, cx| {
            let v = input.read(cx).content().trim().to_string();
            prefs::update(cx, |s| s.editor_app = (!v.is_empty()).then_some(v));
            if *e == TextInputEvent::Submit {
                this.note = Some(("Editor saved.".into(), false));
                cx.notify();
            }
        })
        .detach();
        Self {
            section: Section::Appearance,
            accent,
            background,
            name,
            email,
            editor,
            note: None,
        }
    }

    fn set_theme(
        &mut self,
        accent: Option<String>,
        background: Option<String>,
        cx: &mut Context<Self>,
    ) {
        prefs::update(cx, |s| {
            s.accent_hex = accent.clone();
            s.background_hex = background.clone();
        });
        self.accent
            .update(cx, |i, cx| i.set_content(accent.unwrap_or_default(), cx));
        self.background.update(cx, |i, cx| {
            i.set_content(background.unwrap_or_default(), cx)
        });
        let input = prefs::theme_input(prefs::get(cx));
        theme::apply(cx, &input);
    }

    fn apply_hex(&mut self, cx: &mut Context<Self>) {
        let norm = |s: &str| {
            let s = s.trim();
            if s.is_empty() {
                return Ok(None);
            }
            let s = if s.starts_with('#') {
                s.to_string()
            } else {
                format!("#{s}")
            };
            theme::color::parse_hex(&s)
                .map(|_| Some(s.to_lowercase()))
                .ok_or(())
        };
        let a = norm(self.accent.read(cx).content());
        let b = norm(self.background.read(cx).content());
        match (a, b) {
            (Ok(a), Ok(b)) => {
                self.note = None;
                self.set_theme(a, b, cx);
            }
            _ => self.note = Some(("Colours look like #c2ee4a.".into(), true)),
        }
        cx.notify();
    }

    fn save_identity(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).content().to_string();
        let email = self.email.read(cx).content().to_string();
        self.note = Some(
            match wackdepot_core::config::set_global_identity(&name, &email) {
                Ok(()) => ("Saved — new commits use this identity.".into(), false),
                Err(e) => (e.into(), true),
            },
        );
        cx.notify();
    }

    // ── Pieces ─────────────────────────────────────────────────────────

    fn hero(&self, title: &str, body: &str, art: impl IntoElement, cx: &App) -> impl IntoElement {
        let t = cx.theme().clone();
        div()
            .flex()
            .items_center()
            .gap(px(18.))
            .p(px(20.))
            .rounded(px(metrics::RADIUS_CARD))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .child(art)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(kicker("Settings", cx))
                    .child(
                        div()
                            .text_size(px(metrics::TEXT_HERO))
                            .font_weight(FontWeight::BOLD)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(t.text_dim)
                            .child(body.to_string()),
                    ),
            )
    }

    fn block(
        title: &str,
        body: Option<&str>,
        content: impl IntoElement,
        cx: &App,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(18.))
            .rounded(px(metrics::RADIUS_CARD))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(metrics::TEXT_ROW))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string()),
                    )
                    .children(body.map(|b| {
                        div()
                            .text_size(px(12.))
                            .text_color(t.text_dim)
                            .child(b.to_string())
                    })),
            )
            .child(content)
    }

    fn appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = prefs::get(cx).clone();
        let current = (s.accent_hex.clone(), s.background_hex.clone());
        let entity = cx.entity();
        let mut presets = div().flex().flex_wrap().gap(px(10.));
        for (i, p) in PRESETS.iter().enumerate() {
            let is_default = i == 0;
            let selected = if is_default {
                current.0.is_none() && current.1.is_none()
            } else {
                current.0.as_deref() == Some(p.accent) && current.1.as_deref() == Some(p.background)
            };
            let (swatch_bg, swatch_accent) = p.swatch();
            let e = entity.clone();
            let (a, b) = (p.accent.to_string(), p.background.to_string());
            presets = presets.child(
                div()
                    .id(("preset", i))
                    .w(px(118.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(8.))
                    .rounded(px(12.))
                    .bg(t.input)
                    .border_1()
                    .border_color(if selected { t.accent_line } else { t.border })
                    .when(selected, |el| el.shadow(vec![t.rim(t.accent_line)]))
                    .cursor_pointer()
                    .hover(|st| st.bg(t.surface_hover))
                    .on_click(move |_, _, cx| {
                        e.update(cx, |page, cx| {
                            if is_default {
                                page.set_theme(None, None, cx);
                            } else {
                                page.set_theme(Some(a.clone()), Some(b.clone()), cx);
                            }
                        })
                    })
                    .child(
                        div()
                            .h(px(46.))
                            .rounded(px(8.))
                            .bg(swatch_bg)
                            .border_1()
                            .border_color(t.border)
                            .flex()
                            .items_end()
                            .gap(px(4.))
                            .p(px(6.))
                            .child(div().size(px(12.)).rounded_full().bg(swatch_accent))
                            .child(
                                div()
                                    .w(px(28.))
                                    .h(px(4.))
                                    .mb(px(4.))
                                    .rounded_full()
                                    .bg(swatch_accent.opacity(0.4)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(p.name),
                    ),
            );
        }
        let custom_apply = {
            let e = entity.clone();
            button("hex-apply", "Apply")
                .small()
                .on_click(move |_, _, cx| e.update(cx, |p, cx| p.apply_hex(cx)))
        };
        let tint = f32::from(s.glass_tint) / 90.0;
        let e_tint = entity.clone();
        let glass = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                pill_track("backdrop")
                    .item("Liquid Glass")
                    .item("Solid")
                    .selected(usize::from(s.backdrop == Backdrop::Solid))
                    .on_select(move |i, _, cx| {
                        prefs::update(cx, |s| {
                            s.backdrop = if i == 1 {
                                Backdrop::Solid
                            } else {
                                Backdrop::Glass
                            }
                        });
                        e_tint.update(cx, |_, cx| cx.emit(SettingsEvent::Backdrop));
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .opacity(if s.backdrop == Backdrop::Solid {
                        0.48
                    } else {
                        1.0
                    })
                    .child(
                        div()
                            .w(px(80.))
                            .text_size(px(12.5))
                            .text_color(t.text_soft)
                            .child("Tint"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(slider("tint", tint).on_change(|v, _, cx| {
                                let pct = (v * 90.0).round() as u8;
                                if pct != prefs::get(cx).glass_tint {
                                    prefs::update(cx, |s| s.glass_tint = pct);
                                    let input = prefs::theme_input(prefs::get(cx));
                                    theme::apply(cx, &input);
                                }
                            })),
                    )
                    .child(
                        div()
                            .w(px(40.))
                            .text_size(px(12.))
                            .text_color(t.text_dim)
                            .child(format!("{}%", s.glass_tint)),
                    ),
            );
        let motion = pill_track("motion")
            .item("Follow macOS")
            .item("Always reduce")
            .selected(usize::from(s.reduce_motion == MotionPref::Always))
            .on_select(|i, _, cx| {
                prefs::update(cx, |s| {
                    s.reduce_motion = if i == 1 {
                        MotionPref::Always
                    } else {
                        MotionPref::System
                    }
                });
                crate::app::backdrop::sync_reduce_motion(cx);
                cx.refresh_windows();
            });
        let adjusted = cx.theme().accent_adjusted || cx.theme().background_adjusted;
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.hero("Appearance", "Pick a theme, tune the glass, and choose how much things move.", accent_mark("palette").size(52.), cx))
            .child(Self::block("Theme", Some("Every colour is derived from an accent and a background, the same way WackCode does it."), presets, cx))
            .child(Self::block(
                "Custom colours",
                Some(if adjusted { "Adjusted slightly so text stays readable." } else { "Hex values, like #c2ee4a. Leave empty for the default." }),
                div()
                    .flex()
                    .items_end()
                    .gap(px(10.))
                    .child(div().flex_1().child(field("Accent", &self.accent, None, window, cx)))
                    .child(div().flex_1().child(field("Background", &self.background, None, window, cx)))
                    .child(custom_apply),
                cx,
            ))
            .child(Self::block("Glass", Some("What sits behind the window. The window turns solid while it isn't focused."), glass, cx))
            .child(Self::block("Motion", Some("Springs, glides and celebrations. Reduce turns them off."), motion, cx))
    }

    fn git(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let save = button("identity-save", "Save identity")
            .small()
            .primary()
            .on_click(move |_, _, cx| {
                entity.update(cx, |p, cx| p.save_identity(cx));
            });
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.hero("Git", "Who your commits come from, and where repositories open.", accent_mark("branch").size(52.), cx))
            .child(Self::block(
                "Identity",
                Some("Written to your global git config (user.name and user.email)."),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(field("Name", &self.name, None, window, cx))
                    .child(field("Email", &self.email, None, window, cx))
                    .child(div().flex().justify_end().child(save)),
                cx,
            ))
            .child(Self::block(
                "Editor",
                Some("Open in Editor uses this app; WackCode, Zed, then VS Code are tried when it's empty."),
                field("App name", &self.editor, None, window, cx),
                cx,
            ))
    }

    fn integrations(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.hero("Integrations", "Services WackDepot can talk to.", accent_mark("link").size(52.), cx))
            .child(Self::block(
                "GitHub",
                Some("Pull requests in the branch picker, checks on the branch pill, and publishing new repositories."),
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(button("gh-connect", "Connect GitHub").icon("pull-request").disabled(true))
                    .child(div().text_size(px(12.)).text_color(t.text_faint).child("Coming soon.")),
                cx,
            ))
    }

    fn about(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let git = wackdepot_core::repo::git_version().unwrap_or_else(|_| "not found".into());
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.hero(
                "WackDepot",
                &format!("Version {} · git {git}", env!("CARGO_PKG_VERSION")),
                mascot("about-mascot").size(64.).pose(Pose::Celebrate),
                cx,
            ))
            .child(Self::block(
                "Made with",
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(t.text_soft)
                    .child("gpui (zeronsh/zui) — Apache-2.0")
                    .child("Glass and frost patterns from zeron — Apache-2.0 / MIT")
                    .child("Inter by The Inter Project Authors — SIL Open Font License 1.1")
                    .child("syntect and bat's syntax set (two-face) — MIT")
                    .child("similar — Apache-2.0"),
                cx,
            ))
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let page = match self.section {
            Section::Appearance => self.appearance(window, cx).into_any_element(),
            Section::Git => self.git(window, cx).into_any_element(),
            Section::Integrations => self.integrations(cx).into_any_element(),
            Section::About => self.about(cx).into_any_element(),
        };
        div()
            .id("settings-page")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .py(px(28.))
                    .px(px(24.))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(680.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(page)
                            .children(self.note.clone().map(|(n, err)| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(if err { t.sem.danger } else { t.accent_text })
                                    .child(n)
                            })),
                    ),
            )
    }
}

/// The section list shown in the left column while Settings is open.
#[derive(IntoElement)]
pub struct SettingsNav {
    pub page: Entity<SettingsPage>,
}

impl RenderOnce for SettingsNav {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let current = self.page.read(cx).section;
        let back = {
            let p = self.page.clone();
            button("settings-back", "Back")
                .icon("back")
                .variant(Variant::Ghost)
                .on_click(move |_, _, cx| {
                    p.update(cx, |_, cx| cx.emit(SettingsEvent::Close));
                })
        };
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(8.))
            .child(div().pb(px(8.)).child(back));
        for (i, s) in Section::ALL.iter().enumerate() {
            let p = self.page.clone();
            let s = *s;
            col = col.child(
                menu_row(("section", i), s.title())
                    .icon(s.icon())
                    .selected(s == current)
                    .on_click(move |_, _, cx| {
                        p.update(cx, |page, cx| {
                            page.section = s;
                            page.note = None;
                            cx.notify();
                        });
                    }),
            );
        }
        col
    }
}
