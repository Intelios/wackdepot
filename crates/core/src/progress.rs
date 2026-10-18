//! Parsing git's progress meters (`Receiving objects:  45% (450/1000)` …)
//! into something the sync pill can show live.

/// One progress reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// git's phase name, e.g. "Receiving objects".
    pub phase: String,
    /// 0–100 when the phase reports a percentage.
    pub percent: Option<u8>,
}

/// Parse one stderr line. `None` for lines that aren't a progress meter
/// (hints, `To <url>` summaries, ref updates).
pub fn parse(line: &str) -> Option<Progress> {
    let line = line.trim();
    let line = line.strip_prefix("remote:").map(str::trim).unwrap_or(line);
    let (phase, rest) = line.split_once(':')?;
    let phase = phase.trim();
    if phase.is_empty() || !phase.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') {
        return None;
    }
    let rest = rest.trim();
    if !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let percent = rest
        .split_whitespace()
        .find_map(|t| t.strip_suffix('%'))
        .and_then(|p| p.parse::<u8>().ok())
        .map(|p| p.min(100));
    Some(Progress {
        phase: phase.to_string(),
        percent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_meters() {
        assert_eq!(
            parse("Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s"),
            Some(Progress {
                phase: "Receiving objects".into(),
                percent: Some(45)
            })
        );
        assert_eq!(
            parse("remote: Counting objects: 100% (12/12), done."),
            Some(Progress {
                phase: "Counting objects".into(),
                percent: Some(100)
            })
        );
        assert_eq!(
            parse("Enumerating objects: 5, done."),
            Some(Progress {
                phase: "Enumerating objects".into(),
                percent: None
            })
        );
    }

    #[test]
    fn ignores_other_lines() {
        assert_eq!(parse("To github.com:me/repo.git"), None);
        assert_eq!(parse("   abc123..def456  main -> main"), None);
        assert_eq!(parse("hint: Updates were rejected"), None);
    }
}
