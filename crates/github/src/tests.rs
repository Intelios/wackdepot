use super::*;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MemoryStore(Mutex<Option<auth::Tokens>>);
impl TokenStore for MemoryStore {
    fn load(&self) -> Result<Option<auth::Tokens>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, tokens: &auth::Tokens) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(tokens.clone());
        Ok(())
    }
    fn clear(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
#[derive(Default)]
struct Fake {
    responses: Mutex<VecDeque<http::Response>>,
    requests: Mutex<Vec<http::Request>>,
}
impl http::Transport for Fake {
    fn send(&self, request: http::Request) -> Result<http::Response, String> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or("The test has no response left.".into())
    }
}
fn client(values: Vec<serde_json::Value>) -> (Client, Arc<Fake>) {
    let transport = Arc::new(Fake::default());
    transport
        .responses
        .lock()
        .unwrap()
        .extend(values.into_iter().map(|value| http::Response {
            status: 200,
            body: value.to_string().into_bytes(),
            ..Default::default()
        }));
    let store = Arc::new(MemoryStore::default());
    store
        .save(&auth::Tokens {
            access_token: "test-session".into(),
            refresh_token: None,
            expires_at: None,
            refresh_expires_at: None,
        })
        .unwrap();
    (
        Client::with_transport(transport.clone(), store, None),
        transport,
    )
}

#[test]
fn github_remote_identity() {
    for url in [
        "https://github.com/owner/repo.git",
        "git@github.com:owner/repo.git",
        "ssh://git@github.com/owner/repo.git",
    ] {
        assert_eq!(
            RepositoryId::from_remote(url).unwrap().full_name(),
            "owner/repo"
        );
    }
    for url in [
        "https://github.com.attacker.test/owner/repo",
        "http://github.com/a/b",
        "https://github.com/a/b/c",
        "/a/b",
        "git@other.com:a/b",
        "https://github.com/a/../x",
    ] {
        assert!(RepositoryId::from_remote(url).is_none(), "{url}");
    }
}
#[test]
fn pkce_and_callback_validation() {
    assert_eq!(
        auth::pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert_eq!(
        auth::parse_callback("/github/callback?code=hello&state=expected", "expected").unwrap(),
        "hello"
    );
    assert!(auth::parse_callback("/github/callback?code=hello&state=wrong", "expected").is_err());
    assert!(
        auth::parse_callback(
            "/github/callback?error=access_denied&state=expected",
            "expected"
        )
        .is_err()
    );
    assert!(auth::parse_callback("/favicon.ico", "expected").is_err());
}
#[test]
fn truncated_patch_is_not_reviewable() {
    let mut file = ChangedFile {
        filename: "a.rs".into(),
        previous_filename: None,
        status: "modified".into(),
        additions: 2,
        deletions: 1,
        patch: Some("@@ -1 +1,2 @@\n-old\n+new".into()),
        blob_url: String::new(),
    };
    assert!(!file.patch_complete());
    file.patch.as_mut().unwrap().push_str("\n+second");
    assert!(file.patch_complete());
    file.patch = None;
    assert!(!file.patch_complete());
}
#[test]
fn anchors_preserve_revision_and_side() {
    let anchor = ReviewAnchor {
        path: "file.rs".into(),
        side: DiffSide::Left,
        line: 5,
        start_line: Some(2),
        head_sha: "old".into(),
    };
    assert!(anchor.validate("old").is_ok());
    assert!(anchor.validate("new").is_err());
    assert!(
        ReviewAnchor {
            start_line: Some(6),
            ..anchor
        }
        .validate("old")
        .is_err()
    );
}
#[test]
fn drafts_are_account_scoped_and_forward_compatible() {
    let dir = tempfile::tempdir().unwrap();
    let store = drafts::DraftStore::at(dir.path().into());
    let draft = drafts::Draft {
        review_body: "Keep this review".into(),
        ..Default::default()
    };
    store.save(1, 2, 3, &draft).unwrap();
    assert_eq!(store.load(1, 2, 3).unwrap().review_body, "Keep this review");
    assert!(store.load(2, 2, 3).unwrap().review_body.is_empty());
    assert!(serde_json::from_str::<drafts::Draft>("{\"future\":true}").is_ok());
}
#[test]
fn search_uses_repository_filter_and_cursor() {
    let (client, transport) = client(vec![
        json!({"data":{"search":{"nodes":[],"issueCount":1001,"pageInfo":{"hasNextPage":true,"endCursor":"next"}}}}),
    ]);
    let page = client
        .list(
            &RepositoryId {
                owner: "a".into(),
                name: "b".into(),
            },
            Filter::ReviewRequested,
            "fix",
            "me",
            Some("previous"),
        )
        .unwrap();
    assert!(page.incomplete && page.has_next);
    assert_eq!(page.next_cursor.as_deref(), Some("next"));
    let requests = transport.requests.lock().unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(requests[0].body.as_ref().unwrap()).unwrap();
    assert!(
        body["variables"]["query"]
            .as_str()
            .unwrap()
            .contains("repo:a/b is:pr is:open review-requested:me fix")
    );
    assert_eq!(body["variables"]["after"], "previous");
}
#[test]
fn refresh_tokens_are_saved_before_api_use() {
    let (_, transport) = client(vec![
        json!({"access_token":"rotated","refresh_token":"next","expires_in":28800}),
        json!({"id":7,"login":"me"}),
    ]);
    let store = Arc::new(MemoryStore::default());
    store
        .save(&auth::Tokens {
            access_token: "expired".into(),
            refresh_token: Some("refresh".into()),
            expires_at: Some(1),
            refresh_expires_at: None,
        })
        .unwrap();
    let client = Client::with_transport(
        transport.clone(),
        store.clone(),
        Some(OAuthConfig {
            client_id: "id".into(),
            client_secret: "public-native-config".into(),
        }),
    );
    assert_eq!(client.viewer().unwrap().login, "me");
    assert_eq!(store.load().unwrap().unwrap().access_token, "rotated");
    assert!(
        transport.requests.lock().unwrap()[1]
            .headers
            .iter()
            .any(|(key, value)| key == "Authorization" && value == "Bearer rotated")
    );
}
#[test]
fn rate_limit_stops_followup_network_requests() {
    let (client, transport) = client(vec![]);
    transport
        .responses
        .lock()
        .unwrap()
        .push_back(http::Response {
            status: 429,
            headers: std::collections::HashMap::from([("retry-after".into(), "60".into())]),
            ..Default::default()
        });
    assert!(client.viewer().is_err());
    assert!(client.viewer().is_err());
    assert_eq!(transport.requests.lock().unwrap().len(), 1);
}
#[test]
fn optional_checks_do_not_turn_into_required_failures() {
    assert_eq!(api::check_state("neutral"), CheckState::Passing);
    assert_eq!(api::check_state("skipped"), CheckState::Passing);
    assert_eq!(api::check_state("action_required"), CheckState::Failing);
    assert_eq!(Checks::default().state(), CheckState::None);
}
