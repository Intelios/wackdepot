//! Relative ages for commits ("3h", "2 days ago"), std-only.

use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// "now", "5m", "3h", "2d", "4mo", "1y" — compact, for list rows.
pub fn short_age(iso: &str) -> String {
    let Some(t) = parse_iso(iso) else {
        return String::new();
    };
    let s = (now() - t).max(0);
    match s {
        s if s < 60 => "now".into(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s if s < 86_400 * 30 => format!("{}d", s / 86_400),
        s if s < 86_400 * 365 => format!("{}mo", s / (86_400 * 30)),
        s => format!("{}y", s / (86_400 * 365)),
    }
}

/// "just now", "5 minutes ago", "yesterday", "3 weeks ago" — for headings.
pub fn long_age(iso: &str) -> String {
    let Some(t) = parse_iso(iso) else {
        return String::new();
    };
    let s = (now() - t).max(0);
    let plural = |n: i64, unit: &str| {
        if n == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    };
    match s {
        s if s < 60 => "just now".into(),
        s if s < 3600 => plural(s / 60, "minute"),
        s if s < 86_400 => plural(s / 3600, "hour"),
        s if s < 86_400 * 2 => "yesterday".into(),
        s if s < 86_400 * 14 => plural(s / 86_400, "day"),
        s if s < 86_400 * 60 => plural(s / (86_400 * 7), "week"),
        s if s < 86_400 * 365 => plural(s / (86_400 * 30), "month"),
        s => plural(s / (86_400 * 365), "year"),
    }
}

/// Parse git's `%aI` (`2024-01-31T12:34:56+01:00` or `…Z`) to epoch seconds.
pub fn parse_iso(iso: &str) -> Option<i64> {
    let (date, rest) = iso.trim().split_once('T')?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let (time, offset) = match rest.rfind(['+', '-']) {
        Some(p) if rest[p..].contains(':') => (&rest[..p], Some(&rest[p..])),
        _ => (rest.trim_end_matches('Z'), None),
    };
    let mut t = time.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let m: i64 = t.next()?.parse().ok()?;
    let sec: i64 = t.next().unwrap_or("0").parse().ok()?;
    let mut ts = days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60 + sec;
    if let Some(off) = offset {
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let mut o = off[1..].split(':');
        let oh: i64 = o.next()?.parse().ok()?;
        let om: i64 = o.next().unwrap_or("0").parse().ok()?;
        ts -= sign * (oh * 3600 + om * 60);
    }
    Some(ts)
}

/// Days since the epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_offsets() {
        assert_eq!(parse_iso("1970-01-01T00:00:00+00:00"), Some(0));
        assert_eq!(parse_iso("1970-01-01T01:00:00+01:00"), Some(0));
        assert_eq!(parse_iso("1969-12-31T19:00:00-05:00"), Some(0));
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("nonsense"), None);
    }
}
