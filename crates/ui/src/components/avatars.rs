//! Avatar stacks — the circles of faces GitHub Desktop puts on every
//! commit: author first, then co-authors, then the committer when it
//! differs. Circles overlap at rest and spread on hover (animated,
//! reduce-motion aware); each has a name+email tooltip and falls back to
//! a silhouette when the image can't be fetched.

use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, Context, Entity, FontWeight, Hsla, IntoElement, MouseDownEvent,
    ParentElement, Render, Styled, Window, div, img, prelude::*, px,
};
use wackdepot_core::commit::LogEntry;

use crate::avatar_cache::AvatarStore;
use crate::icons::icon;
use crate::motion::{EASE, id, now};
use crate::theme::{ActiveTheme, Theme};

/// A `gpui` event handler the caller forwards so a stack clicks like the
/// row it sits in (select on left, context menu on right).
pub type Handler<E> = Rc<dyn Fn(&E, &mut Window, &mut App)>;
pub type Click = Handler<gpui::ClickEvent>;
pub type RightClick = Handler<MouseDownEvent>;

/// Faces that fit the resting overlap — up to four people all get a
/// circle; five or more leave three faces and a "+n" wedge behind the
/// last one (Desktop's `avatar-more` rule).
const REST_FACES: usize = 3;
const MAX_SOLO: usize = 4;
/// Hover spread: how far past its neighbours a face slides — the face
/// plus a small gap.
const EXPANDED_STRIDE: f32 = 1.18;
/// Rest overlap: each face slides over ~55% of the one before it.
const REST_STRIDE: f32 = 0.45;
/// Spread/collapse duration in ms (Desktop's is 100ms linear; ours is
/// eased, matching the app's motion).
const EXPAND_MS: f32 = 140.0;

/// One person's place in a commit's stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// The commit's `Author:` (and plain co-authors and committers — the
    /// tooltip names them by name/email alone).
    Author,
    CoAuthor,
    Committer,
}

/// Someone drawn on the stack.
#[derive(Clone, Debug)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub role: Role,
}

/// Everyone credited on `entry`, deduped, in stack order — author,
/// co-authors, then the committer. The committer joins only when it
/// isn't the author or already a co-author, and never when it's the
/// web-flow `GitHub <noreply@github.com>` identity GitHub stamps on
/// merges made on the site (Desktop's `isWebFlowCommitter`).
pub fn participants(entry: &LogEntry) -> Vec<Person> {
    let mut out: Vec<Person> = Vec::new();
    let mut push = |name: &str, email: &str, role: Role| {
        if name.is_empty() && email.is_empty() {
            return;
        }
        if out
            .iter()
            .any(|p| p.name == name && p.email.eq_ignore_ascii_case(email))
        {
            return;
        }
        out.push(Person {
            name: name.to_string(),
            email: email.to_string(),
            role,
        });
    };
    push(&entry.author_name, &entry.author_email, Role::Author);
    for (name, email) in &entry.co_authors {
        push(name, email, Role::CoAuthor);
    }
    let committer_is_author = entry.committer_name == entry.author_name
        && entry
            .committer_email
            .eq_ignore_ascii_case(&entry.author_email);
    let web_flow =
        entry.committer_name == "GitHub" && entry.committer_email == "noreply@github.com";
    if !committer_is_author && !web_flow {
        push(
            &entry.committer_name,
            &entry.committer_email,
            Role::Committer,
        );
    }
    out
}

/// One stack's expand/collapse tween, kept per stack by the owning view
/// (`uniform_list` rows are built during layout — after the view's
/// `Motion::end` has pruned its tracks — so `Motion` can't drive them;
/// this tiny clock instead, like `Popover`'s local tween).
#[derive(Default)]
pub struct StackExpand {
    x: f32,
    from: f32,
    target: f32,
    since: f64,
}

impl StackExpand {
    /// The current expansion `0..1` for `target` (0 = collapsed overlap,
    /// 1 = fully spread). Asks for another frame while still moving.
    pub fn sample(&mut self, target: bool, window: &mut Window, cx: &App) -> f32 {
        if cx.reduce_motion() {
            self.target = if target { 1.0 } else { 0.0 };
            self.x = self.target;
            self.from = self.target;
            self.since = now();
            return self.x;
        }
        let t = if target { 1.0f32 } else { 0.0 };
        if (t - self.target).abs() > f32::EPSILON {
            self.from = self.x;
            self.target = t;
            self.since = now();
        }
        let p = ((now() - self.since) as f32 * 1000.0 / EXPAND_MS).clamp(0.0, 1.0);
        self.x = if p >= 1.0 {
            t
        } else {
            self.from + (t - self.from) * EASE.eval(p)
        };
        if p < 1.0 {
            window.request_animation_frame();
        }
        self.x
    }
}

/// The stack itself: a fixed-width relative slot (at rest `faces`
/// overlap; hovering spreads them over whatever sits right — like
/// Desktop's `position: absolute` stack, no reflow) plus an invisible
/// zone that spans the *expanded* extent so hovering a spread face still
/// counts as hovering the stack, and so gap clicks behave like the row.
///
/// `key` namespaces the element ids (use `motion::id(&sha)`).
#[allow(clippy::too_many_arguments)]
pub fn avatar_stack(
    people: &[Person],
    key: u64,
    size: f32,
    ring: Hsla,
    x: f32,
    t: &Theme,
    avatars: &Entity<AvatarStore>,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
    on_click: Option<Click>,
    on_right_click: Option<RightClick>,
) -> impl IntoElement {
    let wedge = people.len() > MAX_SOLO;
    let shown = if wedge {
        REST_FACES
    } else {
        people.len().min(MAX_SOLO)
    };
    let stride = REST_STRIDE * size + (EXPANDED_STRIDE * size - REST_STRIDE * size) * x;
    let width = (shown.max(1) - 1) as f32 * REST_STRIDE * size + size;
    let extent = ((people.len().max(1) - 1) as f32 * EXPANDED_STRIDE * size + size)
        .max(width + if wedge { size * 0.75 } else { 0.0 });

    // Wedge + faces paint in reverse so the author (index 0) lands on
    // top — gpui has no z-index.
    let mut children: Vec<AnyElement> = Vec::new();
    if wedge {
        children.push(more_chip(key, people, shown, width, size, x, t));
    }
    for (i, person) in people.iter().enumerate().rev() {
        children.push(face(
            key,
            i,
            person,
            stride,
            size,
            ring,
            t,
            avatars,
            on_click.clone(),
        ));
    }

    let mut zone = div()
        .id(("avatar-zone", key))
        .absolute()
        .left(px(0.))
        .top(px(0.))
        .w(px(extent))
        .h(px(size))
        .on_hover(on_hover);
    if let Some(click) = on_click.clone() {
        zone = zone.on_click(move |e, w, cx| click(e, w, cx));
    }
    if let Some(right) = on_right_click {
        zone = zone.on_mouse_down(gpui::MouseButton::Right, move |e, w, cx| right(e, w, cx));
    }

    div()
        .flex_none()
        .relative()
        .w(px(width))
        .h(px(size))
        .child(zone)
        .children(children)
}

/// The narrow "more people" sliver that peeks out behind the last face —
/// Desktop's `avatar-more` wedge. Fades out as the stack opens.
fn more_chip(
    key: u64,
    people: &[Person],
    shown: usize,
    width: f32,
    size: f32,
    x: f32,
    t: &Theme,
) -> AnyElement {
    let hidden: Vec<Person> = people[shown..].to_vec();
    // Peek out just past the last resting face — the sliver reads as
    // "someone else" and fades as the stack opens.
    div()
        .absolute()
        .left(px(width + size * 0.12))
        .top(px(size * 0.28))
        .h(px(size * 0.44))
        .w(px(size * 0.5))
        .rounded(px(size * 0.22))
        .bg(t.hover())
        .border_1()
        .border_color(t.border_bright.opacity(0.5))
        .opacity(1.0 - x)
        .id(("avatar-more", key))
        .flex_none()
        .tooltip(move |_, cx| {
            cx.new(|_| NamesTip {
                title: format!("{} more", hidden.len()),
                names: hidden.iter().map(|p| p.name.clone()).collect(),
            })
            .into()
        })
        .into_any_element()
}

/// One circle in the stack: border ring in the surface colour so faces
/// separate cleanly when overlapped, an avatar image on the
/// `AvatarStore` cache, the silhouette when there's no URL or the fetch
/// failed, and a name+email tooltip.
#[allow(clippy::too_many_arguments)]
fn face(
    key: u64,
    i: usize,
    person: &Person,
    stride: f32,
    size: f32,
    ring: Hsla,
    t: &Theme,
    avatars: &Entity<AvatarStore>,
    on_click: Option<Click>,
) -> AnyElement {
    let url = wackdepot_github::avatar_url(&person.email);
    let person_for_tip = person.clone();
    let tip = move |_: &mut Window, cx: &mut App| -> AnyView {
        cx.new(|_| PersonTip {
            person: person_for_tip.clone(),
        })
        .into()
    };
    let mut el = div()
        .id(("avatar", id((key, i))))
        .absolute()
        .left(px(i as f32 * stride))
        .top(px(0.))
        .size(px(size))
        .rounded_full()
        .border_1()
        .border_color(ring)
        .bg(t.pill());
    el = match url {
        Some(url) => {
            let glyph = t.text_faint;
            el.child(
                img(url)
                    .with_fallback(move || fallback_glyph(size, glyph))
                    .image_cache(avatars)
                    .id(("avatar-img", id((key, i, "img"))))
                    .size(px(size))
                    .rounded_full()
                    .object_fit(gpui::ObjectFit::Cover),
            )
        }
        None => el.child(fallback_glyph(size, t.text_faint)),
    };
    let mut el = el.tooltip(tip);
    if let Some(click) = on_click {
        el = el.on_click(move |e, w, cx| click(e, w, cx));
    }
    el.into_any_element()
}

/// The silhouette shown while an avatar can't be loaded.
fn fallback_glyph(size: f32, color: Hsla) -> AnyElement {
    div()
        .size(px(size))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .child(icon("user").size(px(size * 0.62)).text_color(color))
        .into_any_element()
}

/// Name + email tooltip card (first `Render` tooltip in the app).
struct PersonTip {
    person: Person,
}

impl Render for PersonTip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let mut lines = vec![
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.text)
                .child(if self.person.name.is_empty() {
                    self.person.email.clone()
                } else {
                    self.person.name.clone()
                })
                .into_any_element(),
        ];
        if !self.person.name.is_empty() && !self.person.email.is_empty() {
            lines.push(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_dim)
                    .child(self.person.email.clone())
                    .into_any_element(),
            );
        }
        let role = match self.person.role {
            Role::Author => None,
            Role::CoAuthor => Some("Co-authored this commit"),
            Role::Committer => Some("Committed this change"),
        };
        if let Some(role) = role {
            lines.push(
                div()
                    .text_size(px(11.))
                    .text_color(t.text_faint)
                    .child(role)
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(9.))
            .py(px(7.))
            .rounded(px(10.))
            .bg(t.frost())
            .border_1()
            .border_color(t.border_bright.opacity(0.7))
            .shadow(t.shadow_pop())
            .children(lines)
    }
}

/// Simple "n more people" tooltip for the wedge.
struct NamesTip {
    title: String,
    names: Vec<String>,
}

impl Render for NamesTip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(9.))
            .py(px(7.))
            .rounded(px(10.))
            .bg(t.frost())
            .border_1()
            .border_color(t.border_bright.opacity(0.7))
            .shadow(t.shadow_pop())
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text)
                    .child(self.title.clone()),
            )
            .children(self.names.iter().map(|n| {
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_dim)
                    .child(n.clone())
                    .into_any_element()
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wackdepot_core::commit::LogEntry;

    fn entry() -> LogEntry {
        LogEntry {
            sha: "a".into(),
            short: "a".into(),
            author_name: "Ada".into(),
            author_email: "ada@x".into(),
            authored_at: String::new(),
            parents: vec![],
            subject: String::new(),
            body: String::new(),
            co_authors: vec![("Claude".into(), "c@x".into())],
            committer_name: "Ada".into(),
            committer_email: "ada@x".into(),
        }
    }

    #[test]
    fn author_then_co_authors() {
        let people = participants(&entry());
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].role, Role::Author);
        assert_eq!(people[1].role, Role::CoAuthor);
    }

    #[test]
    fn distinct_committer_joins_but_web_flow_does_not() {
        let mut e = entry();
        e.committer_name = "Sam".into();
        e.committer_email = "sam@x".into();
        let people = participants(&e);
        assert_eq!(people.len(), 3);
        assert_eq!(people[2].role, Role::Committer);

        // Rebase by the author themselves: no extra face (case-insensitive).
        e.committer_name = "Ada".into();
        e.committer_email = "ADA@x".into();
        assert_eq!(participants(&e).len(), 2);

        // Web merges keep the real author, not the "GitHub" bot.
        e.committer_name = "GitHub".into();
        e.committer_email = "noreply@github.com".into();
        assert_eq!(participants(&e).len(), 2);
    }

    #[test]
    fn dedupes_by_name_and_email() {
        let mut e = entry();
        e.co_authors.push(("Ada".into(), "ada@x".into()));
        assert_eq!(participants(&e).len(), 2);

        let mut empty = LogEntry {
            author_name: String::new(),
            author_email: String::new(),
            committer_name: String::new(),
            committer_email: String::new(),
            ..entry()
        };
        empty.co_authors.clear();
        assert!(participants(&empty).is_empty());
    }
}
