//! Read-only live GitHub contract check. No write APIs are called.
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
    let full = std::env::args()
        .nth(1)
        .ok_or("Pass owner/repo for a read-only contract check.")?;
    let (owner, name) = full.split_once('/').ok_or("Use owner/repo.")?;
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|_| "Sign in to gh first.".to_string())?;
    if !output.status.success() {
        return Err("Sign in to gh first.".into());
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
    client.repository(&id)?;
    let page = client.list(&id, Filter::Open, "", &user.login, None)?;
    println!(
        "Account, repository and PR search passed ({} results).",
        page.items.len()
    );
    let options = client.options(&id)?;
    println!(
        "Creation options passed ({} branches, {} templates).",
        options.branches.len(),
        options.templates.len()
    );
    if let Some(pull) = page.items.first() {
        let detail = client.detail(&id, pull.number, &user.login)?;
        println!(
            "Files, commits, conversations, review threads, viewed states and checks passed ({} files, {} threads).",
            detail.files.len(),
            detail.reviews.threads.len()
        );
    }
    Ok(())
}
