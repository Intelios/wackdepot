//! One GitHub account per app. All Keychain and network work runs off-thread.

use gpui::{App, AppContext, Context, Entity, Global};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use wackdepot_github::auth::Authorization;
use wackdepot_github::{Client, OAuthConfig, User};

pub struct GitHubSession {
    pub client: Client,
    pub user: Option<User>,
    pub connecting: bool,
    pub error: Option<String>,
    pub generation: u64,
    cancellation: Option<Arc<AtomicBool>>,
}
struct SharedSession(Entity<GitHubSession>);
impl Global for SharedSession {}

pub fn init(cx: &mut App) {
    let restore = !crate::app::prefs::get(cx).github_disconnected;
    let entity = cx.new(|cx: &mut Context<GitHubSession>| {
        let qa =
            cfg!(debug_assertions) && std::env::var("WACKDEPOT_GITHUB_QA").as_deref() == Ok("1");
        let client = if qa {
            Client::with_transport(
                Arc::new(wackdepot_github::http::HttpTransport::default()),
                Arc::new(wackdepot_github::auth::EphemeralTokens::default()),
                None,
            )
        } else {
            Client::new()
        };
        let worker = client.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if qa {
                        let executable = if std::path::Path::new("/opt/homebrew/bin/gh").exists() {
                            "/opt/homebrew/bin/gh"
                        } else {
                            "gh"
                        };
                        let output = std::process::Command::new(executable)
                            .args(["auth", "token"])
                            .output()
                            .map_err(|_| {
                                "The explicit development QA session needs a signed-in GitHub CLI."
                                    .to_string()
                            })?;
                        if !output.status.success() {
                            return Err(
                                "Sign in to GitHub CLI before starting the development QA session."
                                    .into(),
                            );
                        }
                        let access_token = String::from_utf8(output.stdout)
                            .map_err(|_| "Couldn't read the development QA session.".to_string())?
                            .trim()
                            .to_string();
                        return worker
                            .install_tokens(wackdepot_github::auth::Tokens {
                                access_token,
                                refresh_token: None,
                                expires_at: None,
                                refresh_expires_at: None,
                            })
                            .map(Some);
                    }
                    if !restore {
                        return Ok(None);
                    }
                    if worker.connected()? {
                        worker.viewer().map(Some)
                    } else {
                        Ok(None)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(user) => this.user = user,
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        GitHubSession {
            client,
            user: None,
            connecting: false,
            error: None,
            generation: 0,
            cancellation: None,
        }
    });
    cx.set_global(SharedSession(entity));
}
pub fn session(cx: &App) -> Entity<GitHubSession> {
    cx.global::<SharedSession>().0.clone()
}

impl GitHubSession {
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        if self.connecting {
            return;
        }
        let prepared = OAuthConfig::configured().and_then(|config| {
            Authorization::start(&config).map(|authorization| (config, authorization))
        });
        let (config, authorization) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.generation += 1;
        let generation = self.generation;
        self.connecting = true;
        self.error = None;
        self.cancellation = Some(authorization.cancelled.clone());
        cx.open_url(&authorization.url);
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let cancelled = authorization.cancelled.clone();
                    let tokens = authorization.finish(&config, client.transport())?;
                    if cancelled.load(Ordering::SeqCst) {
                        return Err("GitHub sign-in was cancelled.".into());
                    }
                    client.install_tokens_cancellable(tokens, &cancelled)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.connecting = false;
                this.cancellation = None;
                match result {
                    Ok(user) => {
                        this.user = Some(user);
                        crate::app::prefs::update(cx, |s| s.github_disconnected = false);
                        this.error = this.client.persistence_error();
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.cancellation.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.generation += 1;
        self.connecting = false;
        self.error = None;
        cx.notify();
    }
    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.cancel(cx);
        crate::app::prefs::update(cx, |s| s.github_disconnected = true);
        self.user = None;
        self.generation += 1;
        let client = self.client.clone();
        self.connecting = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.disconnect() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connecting = false;
                match result {
                    Ok(()) => {
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
