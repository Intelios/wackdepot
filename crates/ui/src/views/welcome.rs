//! No repository open: the mascot's stage, the three ways in, and recent
//! repositories. Folders dropped anywhere on the window open too.

use gpui::{
    App, Entity, FontWeight, IntoElement, ParentElement, RenderOnce, Styled, Window, div,
    prelude::*, px,
};

use crate::app::overlays::Overlays;
use crate::app::prefs;
use crate::components::mark::{accent_mark, monogram};
use crate::components::mascot::mascot;
use crate::components::misc::kbd;
use crate::model::RepoModel;
use crate::theme::{ActiveTheme, metrics};

#[derive(IntoElement)]
pub struct Welcome {
    pub model: Entity<RepoModel>,
    pub overlays: Entity<Overlays>,
}

fn way_card(
    id: &'static str,
    glyph: &'static str,
    title: &'static str,
    body: &'static str,
    keys: &'static str,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let t = cx.theme().clone();
    let (hover_bg, hover_line) = (t.surface_raised, t.accent_line_soft);
    div()
        .id(id)
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(16.))
        .rounded(px(metrics::RADIUS_CARD))
        .bg(t.surface)
        .border_1()
        .border_color(t.border)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).border_color(hover_line))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(accent_mark(glyph).size(30.))
                .child(kbd(keys, cx)),
        )
        .child(
            div()
                .text_size(px(metrics::TEXT_ROW))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_size(px(metrics::TEXT_META))
                .text_color(t.text_dim)
                .child(body),
        )
}

impl RenderOnce for Welcome {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let (m1, m2) = (self.model.clone(), self.model.clone());
        let (o1, o2) = (self.overlays.clone(), self.overlays.clone());
        let recent: Vec<_> = prefs::get(cx).repos.iter().rev().take(5).cloned().collect();
        let mut recents = div().flex().flex_col().gap(px(2.));
        for (i, path) in recent.into_iter().enumerate() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let shown = path.display().to_string();
            let m = m2.clone();
            recents = recents.child(
                div()
                    .id(("recent", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(36.))
                    .px(px(10.))
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover()))
                    .on_click(move |_, _, cx| m.update(cx, |m, cx| m.open(path.clone(), cx)))
                    .child(monogram(name.clone()).size(22.))
                    .child(div().text_size(px(metrics::TEXT_ROW)).child(name))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(t.text_faint)
                            .child(shown),
                    ),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(28.))
            .p(px(32.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(14.))
                    .child(mascot("welcome-mascot").size(92.))
                    .child(div().text_size(px(metrics::TEXT_HERO)).font_weight(FontWeight::BOLD).child("Welcome to WackDepot"))
                    .child(div().text_size(px(metrics::TEXT_ROW)).text_color(t.text_dim).child("Pick a repository to get started — or drop a folder anywhere on the window.")),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(640.))
                    .flex()
                    .gap(px(12.))
                    .child(way_card("way-add", "folder-plus", "Add existing", "A repository already on this Mac.", "⌘O", cx).on_click(move |_, _, cx| m1.update(cx, |m, cx| m.add_existing(cx))))
                    .child(way_card("way-clone", "cloud", "Clone", "Copy one from GitHub or any URL.", "⇧⌘O", cx).on_click(move |_, window, cx| o1.update(cx, |o, cx| o.open_clone(window, cx))))
                    .child(way_card("way-new", "plus", "Create new", "Start fresh on main.", "⌘N", cx).on_click(move |_, window, cx| o2.update(cx, |o, cx| o.open_new_repo(window, cx)))),
            )
            .when(!prefs::get(cx).repos.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .max_w(px(640.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(div().px(px(10.)).text_size(px(10.5)).font_weight(FontWeight::BOLD).text_color(t.text_faint).child("RECENT"))
                        .child(recents),
                )
            })
    }
}
