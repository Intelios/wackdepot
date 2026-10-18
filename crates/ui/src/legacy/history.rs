//! The History tab: commit list in the rail (subject, author, age), and the
//! right-pane commit detail (message, author, files-changed diff sections).

use gpui::{AnyElement, Context, Div, Stateful, Styled, div, prelude::*, px, uniform_list};

use crate::icons::icon;
use crate::legacy::shell::Shell;
use crate::legacy::state::Selected;
use crate::legacy::theme::Theme;

impl Shell {
    pub fn history_view(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        if self.repo.log.is_empty() {
            return self.empty_rail_hist(
                theme,
                if self.repo.status.is_none() {
                    "Reading the repository…"
                } else {
                    "No commits yet — your first one starts the story."
                },
            );
        }
        let n = self.repo.log.len();
        let selected_ix = match self.selected {
            Some(Selected::Commit { index }) => Some(index),
            _ => None,
        };
        div().flex_1().min_h_0().child(
            uniform_list(
                "history",
                n,
                cx.processor(move |this, range, window, cx| {
                    let theme = this.theme(window);
                    let mut items: Vec<AnyElement> = Vec::new();
                    for ix in range {
                        items.push(
                            this.history_row(cx, &theme, ix, selected_ix == Some(ix))
                                .into_any_element(),
                        );
                    }
                    items
                }),
            )
            .h_full(),
        )
    }

    fn empty_rail_hist(&self, theme: &Theme, text: &str) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .text_sm()
            .text_color(theme.text_dim)
            .child(text.to_string())
    }

    fn history_row(
        &self,
        cx: &mut Context<Self>,
        theme: &Theme,
        index: usize,
        selected: bool,
    ) -> Stateful<Div> {
        let entry = &self.repo.log[index];
        let age = rel_age(&entry.authored_at);
        let mut el = div()
            .id(("hist", index as u64))
            .px_3()
            .py_1p5()
            .mx_1()
            .rounded_md()
            .flex()
            .flex_col()
            .gap_0p5()
            .cursor_pointer()
            .text_sm();
        if selected {
            el = el.bg(theme.accent.opacity(0.16));
        } else {
            el = el.hover(|s| s.bg(theme.accent.opacity(0.08)));
        }
        el.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w_4()
                        .h_4()
                        .flex_none()
                        .rounded_full()
                        .bg(theme.elevated)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_dim)
                        .child(icon("commit").size_2p5().text_color(theme.text_dim)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(if selected {
                            theme.text
                        } else {
                            theme.text_soft
                        })
                        .child(entry.subject.clone()),
                )
                .child(div().text_xs().text_color(theme.text_faint).child(age)),
        )
        .child(
            div()
                .pl(px(24.))
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(theme.text_dim)
                .child(entry.author_name.clone())
                .child(
                    div()
                        .font_family("SF Mono")
                        .text_color(theme.text_faint)
                        .child(entry.short.clone()),
                ),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selected = Some(Selected::Commit { index });
            this.refresh_selected_diff(cx);
            cx.notify();
        }))
    }

    /// Right pane for a selected commit: header card + per-file diffs.
    pub fn commit_detail(&self, cx: &mut Context<Self>, theme: &Theme) -> Div {
        let Some(sel) = &self.selected_diff else {
            return self.empty_commit(theme, "Pick a commit to see what it changed.");
        };
        let Some(entry) = sel.commit.clone() else {
            return self.empty_commit(theme, "Pick a commit to see what it changed.");
        };
        let files_n = sel.all.len();
        let header = div()
            .flex_none()
            .p_4()
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_lg()
                    .text_color(theme.text)
                    .child(entry.subject.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.text_dim)
                    .child(format!(
                        "{} · {}",
                        entry.author_name,
                        rel_age(&entry.authored_at)
                    ))
                    .child(
                        div()
                            .font_family("SF Mono")
                            .text_color(theme.text_faint)
                            .child(entry.short.clone()),
                    )
                    .child(div().text_color(theme.text_faint).child(format!(
                        "{files_n} file{}",
                        if files_n == 1 { "" } else { "s" }
                    ))),
            )
            .when(!entry.body.trim().is_empty(), |el| {
                el.child(
                    div()
                        .text_sm()
                        .text_color(theme.text_soft)
                        .mt_1()
                        .child(entry.body.trim().to_string()),
                )
            });

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(self.diff_view(cx, theme))
    }

    fn empty_commit(&self, theme: &Theme, text: &str) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(theme.text_dim)
            .child(text.to_string())
    }
}

/// "3h", "2d", "4mo" — compact relative age for list rows.
fn rel_age(iso: &str) -> String {
    let Ok(t) = chrono_parse(iso) else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let secs = (now - t).max(0);
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("{}d", secs / 86400)
    } else if secs < 86400 * 365 {
        format!("{}mo", secs / (86400 * 30))
    } else {
        format!("{}y", secs / (86400 * 365))
    }
}

/// Parse the `%aI` ISO-8601 timestamp into epoch seconds (std-only).
fn chrono_parse(iso: &str) -> Result<i64, ()> {
    // Format: 2024-01-31T12:34:56+hh:mm or Z.
    let iso = iso.trim();
    let (date, rest) = iso.split_once('T').ok_or(())?;
    let mut d = date.split('-');
    let year: i64 = d.next().ok_or(())?.parse().map_err(|_| ())?;
    let month: i64 = d.next().ok_or(())?.parse().map_err(|_| ())?;
    let day: i64 = d.next().ok_or(())?.parse().map_err(|_| ())?;
    let (time, tz) = if let Some(p) = rest.rfind(['+', '-']) {
        if rest[p..].contains(':') {
            rest.split_at(p)
        } else {
            (rest, "")
        }
    } else {
        (rest.trim_end_matches('Z'), "+00:00")
    };
    let mut t = time.split(':');
    let hour: i64 = t.next().ok_or(())?.parse().map_err(|_| ())?;
    let min: i64 = t.next().ok_or(())?.parse().map_err(|_| ())?;
    let sec: i64 = t.next().unwrap_or("0").parse().map_err(|_| ())?;
    let days = days_from_civil(year, month as u32, day as u32);
    let mut ts = days * 86400 + hour * 3600 + min * 60 + sec;
    // Apply tz offset (+hh:mm means local = ts + off, so utc = ts - off).
    if !tz.is_empty() && tz != "+00:00" {
        let (sign, off) = if let Some(o) = tz.strip_prefix('+') {
            (1i64, o)
        } else if let Some(o) = tz.strip_prefix('-') {
            (-1i64, o)
        } else {
            (0, "")
        };
        let mut o = off.split(':');
        let oh: i64 = o.next().unwrap_or("0").parse().unwrap_or(0);
        let om: i64 = o.next().unwrap_or("0").parse().unwrap_or(0);
        ts -= sign * (oh * 3600 + om * 60);
    }
    Ok(ts)
}

/// Days since epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + (d as i64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_epoch() {
        assert_eq!(chrono_parse("1970-01-01T00:00:00+00:00"), Ok(0));
        assert_eq!(chrono_parse("1970-01-01T01:00:00+01:00"), Ok(0));
        assert_eq!(chrono_parse("1970-01-01T00:00:00Z"), Ok(0));
        assert!(chrono_parse("2024-01-31T12:00:00+00:00").unwrap() > 0);
    }
}
