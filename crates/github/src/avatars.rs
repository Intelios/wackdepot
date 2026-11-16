//! Author avatars, the way GitHub Desktop resolves them: every commit
//! author's email goes straight to `avatars.githubusercontent.com/u/e`,
//! which answers with the account photo server-side — real emails,
//! `id+login@users.noreply.github.com`, and bots included. Unknown emails
//! get GitHub's generated identicon, so a 200 is always an image. That
//! means no API calls, no token, and no rate limits; the same URL serves
//! repos hosted anywhere (and nowhere), exactly like Desktop.

use crate::http::{Request, Transport};

/// The image URL for a commit author's email — `None` when there's no
/// email to look up (callers show the silhouette instead). `s=64` keeps
/// every lookup on one cache key while staying sharp at 2× the biggest
/// size we render (20px).
pub fn url_for_email(email: &str) -> Option<String> {
    let email = email.trim();
    if email.is_empty() {
        return None;
    }
    Some(format!(
        "https://avatars.githubusercontent.com/u/e?email={}&s=64",
        crate::api::encode(email)
    ))
}

/// GET the avatar bytes. `Err` is a sentence for logs; any non-2xx, a
/// refused host, or a transport failure lands the caller's fallback.
pub fn fetch(transport: &dyn Transport, url: &str) -> Result<Vec<u8>, String> {
    let response = transport.send(Request {
        method: "GET",
        url: url.into(),
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
}
