//! Opt-in live contract check. Authentication comes from the developer's gh
//! session and stays in memory. Writes are restricted to named test repos.
use std::sync::Arc;
use wackdepot_github::{
    auth::{EphemeralTokens, Tokens},
    http::HttpTransport,
    *,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let exercise = true;
    let name=args.first().ok_or("Pass owner/repo, optionally preceded by --exercise for a dedicated wackdepot-pr-test-* repository.")?;
    let (owner, name) = name
        .split_once('/')
        .ok_or("Use owner/repo for the integration repository.")?;
    if exercise && !name.starts_with("wackdepot-pr-test-") {
        return Err(
            "Live writes are restricted to dedicated wackdepot-pr-test-* repositories.".into(),
        );
    }
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|_| {
            "Install and sign in to gh to run this opt-in integration check.".to_string()
        })?;
    if !output.status.success() {
        return Err("Sign in to gh before running this integration check.".into());
    }
    let token = String::from_utf8(output.stdout)
        .map_err(|_| "Couldn't read the integration credential.".to_string())?
        .trim()
        .to_string();
    let store = Arc::new(EphemeralTokens::default());
    store.save(&Tokens {
        access_token: token,
        refresh_token: None,
        expires_at: None,
        refresh_expires_at: None,
    })?;
    let client = Client::with_transport(Arc::new(HttpTransport::default()), store, None);
    let user = client.viewer()?;
    let id = RepositoryId {
        owner: owner.into(),
        name: name.into(),
    };
    let repository = client.repository(&id)?;
    let page = client.list(&id, Filter::Open, "", &user.login, None)?;
    println!(
        "Account, repository, and paginated PR search passed ({} results).",
        page.items.len()
    );
    let options = client.options(&id)?;
    println!(
        "Branches and creation options passed ({} branches, {} templates).",
        options.branches.len(),
        options.templates.len()
    );
    if !exercise {
        if let Some(pull) = page.items.first() {
            let detail = client.detail(&id, pull.number, &user.login)?;
            println!(
                "PR details, files, commits, discussions, and checks passed ({} files, {} threads).",
                detail.files.len(),
                detail.reviews.threads.len()
            );
        }
        return Ok(());
    }
    let input = CreatePullRequestInput {
        title: "WackDepot native PR integration check".into(),
        body: "Created by the explicitly invoked WackDepot integration test.".into(),
        base: repository.default_branch.clone(),
        head: format!("{}:feature", user.login),
        head_repo: name.into(),
        draft: true,
        ..Default::default()
    };
    let outcome = client.create(&id, &input)?;
    if let Some(error) = outcome.metadata_error {
        return Err(error);
    }
    let number = outcome.pull.number;
    println!("Create draft PR passed (#{number}).");
    client.set_draft(&outcome.pull.node_id, false)?;
    client.edit(
        &id,
        number,
        &input.title,
        "Native description, Markdown, and lifecycle integration check.",
    )?;
    let detail = client.detail(&id, number, &user.login)?;
    let file = detail
        .files
        .iter()
        .find(|f| f.patch_complete() && f.patch.is_some())
        .ok_or("The test branch needs a text change.")?;
    let parsed = wackdepot_core::diff::parse_diff(&format!(
        "diff --git a/file b/file\n--- a/file\n+++ b/file\n{}\n",
        file.patch.as_deref().unwrap()
    ));
    let line = parsed
        .iter()
        .flat_map(|f| &f.hunks)
        .flat_map(|h| &h.lines)
        .find_map(|line| line.new_lineno)
        .ok_or("The test change needs a right-side line.")?;
    let anchor = ReviewAnchor {
        path: file.filename.clone(),
        side: DiffSide::Right,
        line,
        start_line: None,
        head_sha: detail.pull.head.sha.clone(),
    };
    let review = client.begin_review(&detail.pull.node_id, &detail.pull.head.sha)?;
    client.add_thread(
        &detail.pull.node_id,
        Some(&review),
        &anchor,
        &detail.pull.head.sha,
        "Inline comment from the native integration check.",
    )?;
    let pending = client.review_info(&detail.pull.node_id, &user.login)?;
    if pending.pending_id.as_deref() != Some(&review) {
        return Err("The pending review wasn't recovered from GitHub.".into());
    }
    let thread = pending
        .threads
        .last()
        .ok_or("The pending review thread wasn't returned.")?;
    if let Some(comment) = thread.comments.last() {
        client.edit_review_comment(
            &comment.id,
            "Edited inline comment from the native integration check.",
        )?;
    }
    client.submit_review(
        &id,
        number,
        &review,
        &detail.pull.head.sha,
        "Native review submission passed.",
        ReviewEvent::Comment,
    )?;
    client.reply(&thread.id, "Thread reply from WackDepot.")?;
    client.resolve(&thread.id, true)?;
    client.resolve(&thread.id, false)?;
    client.viewed(&detail.pull.node_id, &file.filename, true)?;
    client.viewed(&detail.pull.node_id, &file.filename, false)?;
    client.comment(&id, number, "Conversation comment from WackDepot.")?;
    println!(
        "Pending review recovery, inline editing, submit, replies, resolution, viewed files, and conversation passed."
    );
    client.state(&id, number, false)?;
    client.state(&id, number, true)?;
    println!("Close and reopen passed.");
    let mut detail = client.detail(&id, number, &user.login)?;
    for _ in 0..5 {
        if detail.merge_blocker(&repository).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        client.invalidate();
        detail = client.detail(&id, number, &user.login)?;
    }
    let stale = MergeInput {
        method: MergeMethod::Squash,
        expected_head_sha: "0".repeat(40),
        title: String::new(),
        body: String::new(),
    };
    if client.merge(&id, number, &user.login, &stale).is_ok() {
        return Err("The stale merge guard unexpectedly allowed a merge.".into());
    }
    client.merge(
        &id,
        number,
        &user.login,
        &MergeInput {
            method: repository
                .merge_methods()
                .first()
                .copied()
                .ok_or("Enable a merge method in the test repository.")?,
            expected_head_sha: detail.pull.head.sha.clone(),
            title: "WackDepot PR integration passed".into(),
            body: "Created and reviewed through the native API service.".into(),
        },
    )?;
    println!(
        "Stale-head rejection and confirmed merge passed. {}",
        detail.pull.html_url
    );
    Ok(())
}
