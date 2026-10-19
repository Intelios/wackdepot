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

fn pull_json(sha: &str) -> serde_json::Value {
    json!({"number":1,"node_id":"PR_test","title":"Test PR","body":"Description","state":"open","draft":false,"html_url":"https://github.com/a/b/pull/1","user":{"login":"me"},"head":{"ref":"feature","sha":sha,"repo":null},"base":{"ref":"main","sha":"base","repo":null},"mergeable":true,"mergeable_state":"clean"})
}
fn repo_json() -> serde_json::Value {
    json!({"id":1,"name":"b","owner":{"login":"a"},"default_branch":"main","permissions":{"push":true},"allow_squash_merge":true})
}
fn review_responses(state: &str) -> Vec<serde_json::Value> {
    vec![
        json!({"data":{"node":{"mergeStateStatus":state,"reviewDecision":"APPROVED","viewerCanUpdate":true,"reviews":{"nodes":[]},"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}),
        json!({"data":{"node":{"files":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}),
    ]
}
fn repo_id() -> RepositoryId {
    RepositoryId {
        owner: "a".into(),
        name: "b".into(),
    }
}

#[test]
fn conditional_get_reuses_only_the_matching_response() {
    let (client, transport) = client(vec![]);
    transport.responses.lock().unwrap().extend([
        http::Response {
            status: 200,
            headers: std::collections::HashMap::from([("etag".into(), "revision-1".into())]),
            body: json!({"login":"me"}).to_string().into_bytes(),
        },
        http::Response {
            status: 304,
            ..Default::default()
        },
    ]);
    assert_eq!(client.viewer().unwrap().login, "me");
    assert_eq!(client.viewer().unwrap().login, "me");
    assert!(
        transport.requests.lock().unwrap()[1]
            .headers
            .iter()
            .any(|(key, value)| key == "If-None-Match" && value == "revision-1")
    );
}

#[test]
fn creation_survives_metadata_failure_without_creating_another_pr() {
    let (client, transport) = client(vec![json!([]), pull_json("tip")]);
    transport.responses.lock().unwrap().extend([
        http::Response {
            status: 403,
            body: json!({"message":"Reviewers cannot be assigned"})
                .to_string()
                .into_bytes(),
            ..Default::default()
        },
        http::Response {
            status: 200,
            body: json!([pull_json("tip")]).to_string().into_bytes(),
            ..Default::default()
        },
        http::Response {
            status: 200,
            body: json!([]).to_string().into_bytes(),
            ..Default::default()
        },
    ]);
    let input = CreatePullRequestInput {
        title: "Test".into(),
        base: "main".into(),
        head: "me:feature".into(),
        labels: vec!["bug".into()],
        ..Default::default()
    };
    let created = client.create(&repo_id(), &input).unwrap();
    assert_eq!(created.pull.number, 1);
    assert!(created.metadata_error.is_some());
    assert!(
        client
            .create(&repo_id(), &input)
            .unwrap()
            .metadata_error
            .is_none()
    );
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST" && r.url.ends_with("/pulls"))
            .count(),
        1
    );
}

#[test]
fn uncertain_create_is_reconciled_before_retry() {
    let (client, transport) = client(vec![json!([])]);
    let input = CreatePullRequestInput {
        title: "Test".into(),
        base: "main".into(),
        head: "me:feature".into(),
        ..Default::default()
    };
    assert!(
        client
            .create(&repo_id(), &input)
            .unwrap_err()
            .contains("didn't confirm")
    );
    transport
        .responses
        .lock()
        .unwrap()
        .push_back(http::Response {
            status: 200,
            body: json!([pull_json("tip")]).to_string().into_bytes(),
            ..Default::default()
        });
    assert_eq!(client.create(&repo_id(), &input).unwrap().pull.number, 1);
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST" && r.url.ends_with("/pulls"))
            .count(),
        1
    );
}

#[test]
fn merge_revalidates_and_sends_the_reviewed_sha() {
    let sha = "1".repeat(40);
    let mut responses = vec![repo_json(), pull_json(&sha)];
    responses.extend(review_responses("CLEAN"));
    responses.push(json!({"merged":true}));
    let (client, transport) = client(responses);
    client
        .merge(
            &repo_id(),
            1,
            "me",
            &MergeInput {
                method: MergeMethod::Squash,
                expected_head_sha: sha.clone(),
                title: String::new(),
                body: String::new(),
            },
        )
        .unwrap();
    let requests = transport.requests.lock().unwrap();
    let merge = requests.last().unwrap();
    assert_eq!(merge.method, "PUT");
    let body: serde_json::Value = serde_json::from_slice(merge.body.as_ref().unwrap()).unwrap();
    assert_eq!(body["sha"], sha);
    assert_eq!(body["merge_method"], "squash");
    assert!(body.get("commit_title").is_none());
}

#[test]
fn stale_and_blocked_merges_never_send_a_merge_request() {
    let sha = "1".repeat(40);
    let (stale, stale_transport) = client(vec![repo_json(), pull_json(&sha)]);
    assert!(
        stale
            .merge(
                &repo_id(),
                1,
                "me",
                &MergeInput {
                    method: MergeMethod::Squash,
                    expected_head_sha: "0".repeat(40),
                    title: String::new(),
                    body: String::new()
                }
            )
            .is_err()
    );
    assert!(
        !stale_transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.method == "PUT")
    );
    for state in ["BLOCKED", "BEHIND", "DIRTY", "UNKNOWN"] {
        let mut responses = vec![repo_json(), pull_json(&sha)];
        responses.extend(review_responses(state));
        let (client, transport) = client(responses);
        assert!(
            client
                .merge(
                    &repo_id(),
                    1,
                    "me",
                    &MergeInput {
                        method: MergeMethod::Squash,
                        expected_head_sha: sha.clone(),
                        title: String::new(),
                        body: String::new()
                    }
                )
                .is_err()
        );
        assert!(
            !transport
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.method == "PUT")
        );
    }
}

#[test]
fn multiline_left_comment_keeps_old_side_coordinates() {
    let sha = "1".repeat(40);
    let (client, transport) = client(vec![pull_json(&sha), json!({})]);
    client
        .comment_line(
            &repo_id(),
            1,
            &ReviewAnchor {
                path: "renamed file.rs".into(),
                side: DiffSide::Left,
                line: 8,
                start_line: Some(5),
                head_sha: sha.clone(),
            },
            "Review this deletion.",
        )
        .unwrap();
    let requests = transport.requests.lock().unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(requests[1].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["side"], "LEFT");
    assert_eq!(body["start_side"], "LEFT");
    assert_eq!(body["start_line"], 5);
    assert_eq!(body["line"], 8);
    assert_eq!(body["commit_id"], sha);
}

#[test]
fn binary_oversized_and_missing_content_are_distinct() {
    let (client, _) = client(vec![
        json!({"size":3,"encoding":"base64","content":"YQBi"}),
        json!({"size":900000,"encoding":"none"}),
        json!({"size":4,"encoding":"none"}),
        json!({"size":0,"encoding":"base64","content":""}),
    ]);
    assert_eq!(
        client.blob_content(&repo_id(), "tip", "a b.bin").unwrap(),
        BlobContent::Binary
    );
    assert_eq!(
        client.blob_content(&repo_id(), "tip", "large").unwrap(),
        BlobContent::TooLarge
    );
    assert_eq!(
        client.blob_content(&repo_id(), "tip", "missing").unwrap(),
        BlobContent::Unavailable
    );
    assert_eq!(
        client.blob_content(&repo_id(), "tip", "empty").unwrap(),
        BlobContent::Text(String::new())
    );
}

#[test]
fn account_changes_discard_responses_already_in_flight() {
    struct Blocking {
        entered: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl http::Transport for Blocking {
        fn send(&self, _: http::Request) -> Result<http::Response, String> {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            Ok(http::Response {
                status: 200,
                headers: std::collections::HashMap::from([("etag".into(), "old".into())]),
                body: json!({"login":"old-account"}).to_string().into_bytes(),
            })
        }
    }
    let (entered, wait) = std::sync::mpsc::channel();
    let (resume, release) = std::sync::mpsc::channel();
    let store = Arc::new(MemoryStore::default());
    store
        .save(&auth::Tokens {
            access_token: "old".into(),
            refresh_token: None,
            expires_at: None,
            refresh_expires_at: None,
        })
        .unwrap();
    let client = Client::with_transport(
        Arc::new(Blocking {
            entered,
            release: Mutex::new(release),
        }),
        store,
        None,
    );
    let worker = client.clone();
    let pending = std::thread::spawn(move || worker.viewer());
    wait.recv().unwrap();
    client.disconnect().unwrap();
    resume.send(()).unwrap();
    assert!(
        pending
            .join()
            .unwrap()
            .unwrap_err()
            .contains("account changed")
    );
    assert!(!client.connected().unwrap());
}

#[test]
fn unreadable_drafts_are_backed_up_before_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let store = drafts::DraftStore::at(dir.path().into());
    store.save(1, 2, 3, &drafts::Draft::default()).unwrap();
    let path = dir.path().join("1/2/3.json");
    std::fs::write(&path, "incomplete JSON").unwrap();
    assert!(store.load(1, 2, 3).is_err());
    store
        .save(
            1,
            2,
            3,
            &drafts::Draft {
                review_body: "Recovered".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("unreadable"))
    );
    assert_eq!(store.load(1, 2, 3).unwrap().review_body, "Recovered");
}

fn draft_detail() -> PullRequestDetail {
    PullRequestDetail {
        pull: serde_json::from_value(pull_json("tip")).unwrap(),
        files: vec![],
        files_complete: true,
        commits: vec![],
        comments: vec![],
        reviews: ReviewInfo::default(),
        checks: Checks::default(),
        merge_base_sha: "base".into(),
    }
}

#[test]
fn lost_comment_response_reconciles_without_erasing_new_input() {
    let mut detail = draft_detail();
    let comment = |id| {
        serde_json::from_value::<Comment>(json!({"id":id,"node_id":"node","body":"Done","user":{"login":"me"},"created_at":"now"})).unwrap()
    };
    detail.comments.push(comment(1));
    let intent = drafts::PendingWrite::new(drafts::WriteKind::Conversation, "Done".into(), &detail);
    assert!(!intent.confirmed(&detail, "me"));
    detail.comments.push(comment(2));
    assert!(intent.confirmed(&detail, "me"));
    assert!(!intent.confirmed(&detail, "someone-else"));
    let mut draft = drafts::Draft {
        pending_write: Some(intent),
        conversation_body: "A new message typed during posting".into(),
        ..Default::default()
    };
    assert!(draft.reconcile(&detail, "me"));
    assert_eq!(
        draft.conversation_body,
        "A new message typed during posting"
    );
    assert!(draft.pending_write.is_none());
}

#[test]
fn review_recovery_requires_the_same_anchor_and_a_new_comment() {
    let mut detail = draft_detail();
    let anchor = ReviewAnchor {
        path: "renamed.rs".into(),
        side: DiffSide::Left,
        line: 8,
        start_line: Some(3),
        head_sha: "tip".into(),
    };
    let write = drafts::PendingWrite::new(
        drafts::WriteKind::Inline {
            anchor: anchor.clone(),
            immediate: false,
        },
        "Review these lines".into(),
        &detail,
    );
    detail.reviews.threads.push(ReviewThread {
        id: "thread".into(),
        path: anchor.path.clone(),
        line: Some(8),
        start_line: Some(3),
        side: DiffSide::Right,
        comments: vec![ReviewComment {
            id: "new".into(),
            body: write.body.clone(),
            author: "me".into(),
            pending: true,
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(!write.confirmed(&detail, "me"));
    detail.reviews.threads[0].side = DiffSide::Left;
    assert!(write.confirmed(&detail, "me"));
    detail.reviews.threads[0].outdated = true;
    assert!(!write.confirmed(&detail, "me"));
    detail.reviews.threads[0].outdated = false;
    let mut draft = drafts::Draft {
        pending_write: Some(write),
        comments: vec![drafts::LocalComment {
            anchor,
            body: "Review these lines".into(),
        }],
        ..Default::default()
    };
    assert!(draft.reconcile(&detail, "me"));
    assert!(draft.comments.is_empty());
}

#[test]
fn submitted_review_and_thread_reply_recover_after_restart() {
    let mut detail = draft_detail();
    let intent = drafts::PendingWrite::new(
        drafts::WriteKind::Review {
            event: "REQUEST_CHANGES".into(),
        },
        "Please adjust this".into(),
        &detail,
    );
    let draft = drafts::Draft {
        pending_write: Some(intent),
        review_body: "Please adjust this".into(),
        ..Default::default()
    };
    let mut recovered: drafts::Draft =
        serde_json::from_slice(&serde_json::to_vec(&draft).unwrap()).unwrap();
    detail.reviews.submitted.push(SubmittedReview {
        id: 10,
        user: User {
            login: "me".into(),
            ..Default::default()
        },
        body: "Please adjust this".into(),
        state: "CHANGES_REQUESTED".into(),
        submitted_at: Some("now".into()),
        ..Default::default()
    });
    assert!(recovered.reconcile(&detail, "me"));
    assert!(recovered.review_body.is_empty());
    let intent = drafts::PendingWrite::new(
        drafts::WriteKind::Reply {
            thread: "thread".into(),
        },
        "Reply".into(),
        &detail,
    );
    detail.reviews.threads.push(ReviewThread {
        id: "other-thread".into(),
        comments: vec![ReviewComment {
            id: "new".into(),
            body: "Reply".into(),
            author: "me".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(!intent.confirmed(&detail, "me"));
    detail.reviews.threads[0].id = "thread".into();
    assert!(intent.confirmed(&detail, "me"));
}

#[test]
fn partial_metadata_retry_only_repeats_the_failed_assignment() {
    let (client, transport) = client(vec![json!([]), pull_json("tip"), json!([])]);
    transport.responses.lock().unwrap().extend([
        http::Response {
            status: 403,
            body: json!({"message":"Reviewers cannot be assigned"})
                .to_string()
                .into_bytes(),
            ..Default::default()
        },
        http::Response {
            status: 200,
            body: pull_json("tip").to_string().into_bytes(),
            ..Default::default()
        },
        http::Response {
            status: 200,
            body: json!({}).to_string().into_bytes(),
            ..Default::default()
        },
    ]);
    let input = CreatePullRequestInput {
        title: "Test".into(),
        base: "main".into(),
        head: "me:feature".into(),
        labels: vec!["bug".into()],
        reviewers: vec!["reviewer".into()],
        ..Default::default()
    };
    let outcome = client.create(&repo_id(), &input).unwrap();
    let remaining = outcome.metadata_remaining.unwrap();
    assert!(remaining.labels.is_empty());
    assert_eq!(remaining.reviewers, vec!["reviewer"]);
    assert!(
        client
            .retry_creation_metadata(&repo_id(), outcome.pull.number, &remaining)
            .unwrap()
            .metadata_error
            .is_none()
    );
    let requests = transport.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "POST" && r.url.ends_with("/labels"))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "POST" && r.url.ends_with("/pulls"))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "POST" && r.url.ends_with("/requested_reviewers"))
            .count(),
        2
    );
}

#[test]
fn cancelling_during_sign_in_keeps_the_existing_session() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct CancelOnResponse(Arc<AtomicBool>);
    impl http::Transport for CancelOnResponse {
        fn send(&self, _: http::Request) -> Result<http::Response, String> {
            self.0.store(true, Ordering::SeqCst);
            Ok(http::Response {
                status: 200,
                body: json!({"id":2,"login":"new"}).to_string().into_bytes(),
                ..Default::default()
            })
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let store = Arc::new(MemoryStore::default());
    let tokens = |value: &str| auth::Tokens {
        access_token: value.into(),
        refresh_token: None,
        expires_at: None,
        refresh_expires_at: None,
    };
    store.save(&tokens("old")).unwrap();
    let client = Client::with_transport(
        Arc::new(CancelOnResponse(cancelled.clone())),
        store.clone(),
        None,
    );
    assert!(
        client
            .install_tokens_cancellable(tokens("new"), &cancelled)
            .unwrap_err()
            .contains("cancelled")
    );
    assert_eq!(store.load().unwrap().unwrap().access_token, "old");
}

#[test]
fn browsing_reads_credential_storage_once_for_the_entire_session() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counted {
        loads: AtomicUsize,
        denied: bool,
    }
    impl TokenStore for Counted {
        fn load(&self) -> Result<Option<auth::Tokens>, String> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            if self.denied {
                return Err("Reconnect GitHub in Settings.".into());
            }
            Ok(Some(auth::Tokens {
                access_token: "in-memory".into(),
                refresh_token: None,
                expires_at: None,
                refresh_expires_at: None,
            }))
        }
        fn save(&self, _: &auth::Tokens) -> Result<(), String> {
            Ok(())
        }
        fn clear(&self) -> Result<(), String> {
            Ok(())
        }
    }
    let (_, transport) = client((0..20).map(|_| json!({"id":1,"login":"me"})).collect());
    let store = Arc::new(Counted {
        loads: AtomicUsize::new(0),
        denied: false,
    });
    let session = Client::with_transport(transport, store.clone(), None);
    assert!(session.connected().unwrap());
    for _ in 0..20 {
        assert_eq!(session.clone().viewer().unwrap().login, "me");
    }
    assert_eq!(store.loads.load(Ordering::SeqCst), 1);
    let denied = Arc::new(Counted {
        loads: AtomicUsize::new(0),
        denied: true,
    });
    let session = Client::with_transport(Arc::new(Fake::default()), denied.clone(), None);
    assert!(session.connected().is_err());
    for _ in 0..20 {
        assert!(session.viewer().is_err());
    }
    assert_eq!(denied.loads.load(Ordering::SeqCst), 1);
}

#[test]
fn refused_credential_save_keeps_authorized_session_in_memory() {
    struct Unavailable;
    impl TokenStore for Unavailable {
        fn load(&self) -> Result<Option<auth::Tokens>, String> {
            Err("Storage unavailable.".into())
        }
        fn save(&self, _: &auth::Tokens) -> Result<(), String> {
            Err("Storage unavailable.".into())
        }
        fn clear(&self) -> Result<(), String> {
            Ok(())
        }
    }
    let (_, transport) = client(vec![
        json!({"id":1,"login":"me"}),
        json!({"id":1,"login":"me"}),
    ]);
    let session = Client::with_transport(transport, Arc::new(Unavailable), None);
    assert!(session.connected().is_err());
    session
        .install_tokens(auth::Tokens {
            access_token: "authorized".into(),
            refresh_token: None,
            expires_at: None,
            refresh_expires_at: None,
        })
        .unwrap();
    assert!(session.connected().unwrap());
    assert!(
        session
            .persistence_error()
            .unwrap()
            .contains("this session")
    );
    assert_eq!(session.viewer().unwrap().login, "me");
}

#[test]
fn helper_rotation_is_reused_after_a_cached_token_is_rejected() {
    let (_, transport) = client(vec![json!({"id":1,"login":"me"})]);
    let store = Arc::new(MemoryStore::default());
    let tokens = |value: &str, expiry| auth::Tokens {
        access_token: value.into(),
        refresh_token: Some("refresh".into()),
        expires_at: Some(expiry),
        refresh_expires_at: None,
    };
    store.save(&tokens("old", auth::now() + 1000)).unwrap();
    let session = Client::with_transport(transport.clone(), store.clone(), None);
    session.viewer().unwrap();
    store
        .save(&tokens("renewed-by-git", auth::now() + 2000))
        .unwrap();
    transport.responses.lock().unwrap().extend([
        http::Response {
            status: 401,
            ..Default::default()
        },
        http::Response {
            status: 200,
            body: json!({"id":1,"login":"me"}).to_string().into_bytes(),
            ..Default::default()
        },
    ]);
    session.viewer().unwrap();
    let requests = transport.requests.lock().unwrap();
    assert!(
        requests[2]
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer renewed-by-git")
    );
    assert!(requests.iter().all(|r| !r.url.contains("access_token")));
}
