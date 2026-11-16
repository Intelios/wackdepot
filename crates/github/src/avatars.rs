//! Author avatars, the way GitHub Desktop resolves them: every commit
//! author's email goes to `avatars.githubusercontent.com/u/e`, which
//! answers with the account photo server-side — real emails and
//! `users.noreply.github.com` stealth addresses included. Unknown emails
//! get GitHub's generated identicon, so a 200 is always an image; no
//! token, no rate limits, and the same URL serves repos hosted anywhere.
//!
//! One exception, matching Desktop's `botAvatarCache`: accounts whose
//! login ends in `[bot]` are GitHub Apps — `u/e` gives their throwaway
//! user identicon, but GitHub.com shows the *app's* icon. Those emails
//! look up through `api.github.com/users/{login}`, whose `avatar_url`
//! points at `…/in/{integration}` — the same thing Desktop renders.

use crate::http::{Request, Transport};

/// Where a bot's avatar is resolved before the image itself is fetched.
const USERS_API: &str = "https://api.github.com/users/";

/// The image URL for a commit author's email — `None` when there's no
/// email to look up (callers show the silhouette instead). `s=64` keeps
/// every lookup on one cache key while staying sharp at 2× the biggest
/// size we render (20px).
pub fn url_for_email(email: &str) -> Option<String> {
    let email = email.trim();
    if email.is_empty() {
        return None;
    }
    if let Some((_, login)) = stealth(email)
        && login.to_ascii_lowercase().ends_with("[bot]")
    {
        return Some(format!("{USERS_API}{}", crate::api::encode(login)));
    }
    Some(format!(
        "https://avatars.githubusercontent.com/u/e?email={}&s=64",
        crate::api::encode(email)
    ))
}

/// GET the avatar bytes. `Err` is a sentence for logs; any non-2xx, a
/// refused host, or a transport failure lands the caller's fallback.
pub fn fetch(transport: &dyn Transport, url: &str) -> Result<Vec<u8>, String> {
    // Bot lookups land on `/users/{login}` first: GitHub answers with the
    // app's `avatar_url` (`in/{integration}`), which is then fetched like
    // any other avatar. One hop, no recursion.
    let url = match api_user_avatar(transport, url)? {
        Some(resolved) => resolved,
        None => url.to_string(),
    };
    let response = transport.send(Request {
        method: "GET",
        url,
        headers: vec![("User-Agent".into(), "WackDepot/0.1".into())],
        body: None,
    })?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "GitHub returned status {} for this avatar.",
            response.status
        ));
    }
    Ok(response.body)
}

/// When `url` is a `/users/{login}` API lookup, fetch it and return the
/// account's `avatar_url`; anything else passes through unchanged.
fn api_user_avatar(transport: &dyn Transport, url: &str) -> Result<Option<String>, String> {
    let Some(api) = url.strip_prefix("https://api.github.com") else {
        return Ok(None);
    };
    if !api.starts_with("/users/") {
        return Ok(None);
    }
    let response = transport.send(Request {
        method: "GET",
        url: url.to_string(),
        headers: vec![
            ("User-Agent".into(), "WackDepot/0.1".into()),
            ("Accept".into(), "application/vnd.github+json".into()),
        ],
        body: None,
    })?;
    if response.status == 404 {
        return Err("GitHub doesn't know this account.".into());
    }
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "GitHub returned status {} for this account.",
            response.status
        ));
    }
    let avatar_url = serde_json::from_slice::<serde_json::Value>(&response.body)
        .ok()
        .and_then(|v| v["avatar_url"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty());
    match avatar_url {
        Some(url) => Ok(Some(url)),
        None => Err("GitHub didn't return this account's avatar.".into()),
    }
}

/// `id+login@users.noreply.github.com` (modern) or `login@…` (legacy) →
/// `(id?, login)`. Desktop's `parseStealthEmail`, dot-com only.
fn stealth(email: &str) -> Option<(Option<u64>, &str)> {
    let (local, domain) = email.rsplit_once('@')?;
    if !domain.eq_ignore_ascii_case("users.noreply.github.com") {
        return None;
    }
    match local.split_once('+') {
        Some((id, login)) if !login.is_empty() => Some((id.parse::<u64>().ok(), login)),
        _ if !local.is_empty() => Some((None, local)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encodes_the_email() {
        let url = url_for_email("Ada+lovelace@example.com").unwrap();
        assert!(url.starts_with("https://avatars.githubusercontent.com/u/e?email="));
        assert!(url.ends_with("&s=64"));
        assert!(url.contains("Ada%2Blovelace%40example.com"));
        assert!(url_for_email("  ").is_none());
        assert!(url_for_email("").is_none());
    }

    #[test]
    fn stealth_emails_parse() {
        assert_eq!(
            stealth("345377139+wackcode-agent[bot]@users.noreply.github.com"),
            Some((Some(345377139), "wackcode-agent[bot]"))
        );
        assert_eq!(
            stealth("dependabot[bot]@users.noreply.github.com"),
            Some((None, "dependabot[bot]"))
        );
        assert_eq!(stealth("me@example.com"), None);
        assert_eq!(stealth("me@users.noreply.example.com"), None);
    }

    #[test]
    fn bot_emails_resolve_through_the_api() {
        assert_eq!(
            url_for_email("345377139+wackcode-agent[bot]@users.noreply.github.com").as_deref(),
            Some("https://api.github.com/users/wackcode-agent%5Bbot%5D")
        );
        assert_eq!(
            url_for_email("dependabot[bot]@users.noreply.github.com").as_deref(),
            Some("https://api.github.com/users/dependabot%5Bbot%5D")
        );
        // A human's stealth email still goes to `u/e`.
        assert!(
            url_for_email("39053683+Intelios@users.noreply.github.com")
                .unwrap()
                .contains("u/e?email=")
        );
    }
}
