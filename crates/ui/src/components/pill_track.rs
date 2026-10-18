//! A recessed pill track whose accent fill glides between options on a
//! spring (Changes·History, Unified·Split, Branches·Pull requests).

use std::rc::Rc;

use gpui::{
    App, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    TextRun, Window, div, prelude::*, px, relative,
};

use crate::motion::{Motion, spring};
use crate::theme::ActiveTheme;

type OnSelect = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PillTrack {
    id: ElementId,
    items: Vec<(SharedString, Option<usize>)>,
    selected: usize,
    /// Fill the width with equal segments (else sized to labels).
    equal: bool,
    height: f32,
    on_select: Option<OnSelect>,
}

pub fn pill_track(id: impl Into<ElementId>) -> PillTrack {
    PillTrack {
        id: id.into(),
        items: Vec::new(),
        selected: 0,
        equal: false,
        height: 28.0,
        on_select: None,
    }
}

impl PillTrack {
    pub fn item(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push((label.into(), None));
        self
    }

    /// An option with a count badge.
    pub fn item_count(mut self, label: impl Into<SharedString>, count: usize) -> Self {
        self.items.push((label.into(), Some(count)));
        self
    }

    pub fn selected(mut self, i: usize) -> Self {
        self.selected = i;
        self
    }

    pub fn equal(mut self) -> Self {
        self.equal = true;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

const PAD_X: f32 = 12.0;
const LABEL_SIZE: f32 = 12.5;

impl RenderOnce for PillTrack {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let n = self.items.len().max(1);
        let motion = window.use_keyed_state(self.id.clone(), cx, |_, _| Motion::default());

        // Segment widths: measured labels (plus badges), or equal fractions.
        let widths: Vec<f32> = if self.equal {
            Vec::new()
        } else {
            let font = window.text_style().font();
            self.items
                .iter()
                .map(|(label, count)| {
                    let run = TextRun {
                        len: label.len(),
                        font: gpui::Font {
                            weight: FontWeight::MEDIUM,
                            ..font.clone()
                        },
                        color: t.text,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let w = window
                        .text_system()
                        .shape_line(label.clone(), px(LABEL_SIZE), &[run], None)
                        .width();
                    f32::from(w) + PAD_X * 2.0 + if count.is_some() { 24.0 } else { 0.0 }
                })
                .collect()
        };
        let sel = self.selected.min(n - 1);
        let (target_x, target_w) = if self.equal {
            (sel as f32 / n as f32, 1.0 / n as f32)
        } else {
            (widths[..sel].iter().sum::<f32>(), widths[sel])
        };
        let mut frame = false;
        let (x, w) = motion.update(cx, |m, cx| {
            m.begin(cx);
            let x = m.spring(("x", 0), target_x, spring::GLIDE_PILL);
            let w = m.spring(("w", 0), target_w, spring::GLIDE_PILL);
            frame = m.end_frame();
            (x, w)
        });
        if frame {
            window.request_animation_frame();
        }

        let fill = div()
            .absolute()
            .top(px(2.))
            .bottom(px(2.))
            .rounded_full()
            .bg(t.selected_fill())
            .shadow(vec![t.rim(t.accent_line)]);
        let fill = if self.equal {
            fill.left(relative(x)).w(relative(w))
        } else {
            fill.left(px(x + 2.0)).w(px(w))
        };

        let on_select = self.on_select.clone();
        let mut track = div()
            .id(self.id)
            .relative()
            .flex()
            .h(px(self.height))
            .p(px(2.))
            .rounded_full()
            .bg(t.recessed())
            .shadow(vec![t.rim(t.pill())])
            .child(fill);
        if self.equal {
            track = track.w_full();
        }
        for (i, (label, count)) in self.items.into_iter().enumerate() {
            let active = i == sel;
            let cb = on_select.clone();
            let mut seg = div()
                .id(i)
                .relative()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .px(px(PAD_X))
                .rounded_full()
                .text_size(px(LABEL_SIZE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if active { t.text } else { t.text_dim })
                .cursor_pointer()
                .child(label);
            if self.equal {
                seg = seg.flex_1();
            } else {
                seg = seg.w(px(widths[i]));
            }
            if !active {
                seg = seg.hover(|s| s.text_color(t.text_soft));
            }
            if let Some(c) = count {
                seg = seg.child(crate::components::misc::count_pill(c, cx));
            }
            if let Some(cb) = cb {
                seg = seg.on_click(move |_, w, cx| cb(i, w, cx));
            }
            track = track.child(seg);
        }
        if self.equal {
            track.into_any_element()
        } else {
            // Sized to its labels: don't let a column parent stretch it.
            div().flex().child(track).into_any_element()
        }
    }
}
