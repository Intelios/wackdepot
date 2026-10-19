//! Bounded, authenticated HTTP. The injectable transport never owns tokens.

use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

pub const API_VERSION: &str = "2026-03-10";
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Default)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub trait Transport: Send + Sync {
    fn send(&self, request: Request) -> Result<Response, String>;
}

pub struct HttpTransport {
    agent: ureq::Agent,
    lane: std::sync::Mutex<()>,
}

impl Default for HttpTransport {
    fn default() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            // Never forward credentials through a redirect to another host.
            .max_redirects(0)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent();
        Self {
            agent,
            lane: std::sync::Mutex::new(()),
        }
    }
}

impl Transport for HttpTransport {
    fn send(&self, request: Request) -> Result<Response, String> {
        let _lane = self
            .lane
            .lock()
            .map_err(|_| "Couldn't reserve the GitHub connection. Try again.".to_string())?;
        let url = url::Url::parse(&request.url)
            .map_err(|_| "That GitHub address isn't valid.".to_string())?;
        if url.scheme() != "https"
            || !matches!(url.host_str(), Some("api.github.com" | "github.com"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return Err("WackDepot only sends GitHub credentials to GitHub's secure API.".into());
        }
        let mut builder = ureq::http::Request::builder()
            .method(request.method)
            .uri(&request.url);
        for (key, value) in request.headers {
            builder = builder.header(key, value);
        }
        let req = builder
            .body(request.body.unwrap_or_default())
            .map_err(|_| "Couldn't prepare the GitHub request.".to_string())?;
        let mut response = self.agent.run(req).map_err(|_| {
            "Couldn't reach GitHub. Check your connection and try again.".to_string()
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_ascii_lowercase(), v.to_string()))
            })
            .collect();
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((MAX_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut body)
            .map_err(|_| {
                "GitHub's response was interrupted. Refresh before trying that action again."
                    .to_string()
            })?;
        if body.len() > MAX_RESPONSE_BYTES {
            return Err("This GitHub response is too large to display safely. Open this pull request on GitHub to see the complete content.".into());
        }
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

pub fn error(response: &Response) -> String {
    let message = serde_json::from_slice::<serde_json::Value>(&response.body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .unwrap_or_default();
    match response.status {
        401 => "Your GitHub session has expired or was revoked. Reconnect GitHub in Settings.".into(),
        403 if response.headers.contains_key("x-github-sso") => "Your organization requires single sign-on approval for WackDepot. Authorize this connection in GitHub, then reconnect in Settings.".into(),
        403 | 429 if response.headers.get("x-ratelimit-remaining").is_some_and(|v|v=="0") || response.headers.contains_key("retry-after") || response.status==429 => "GitHub has temporarily limited requests. WackDepot will wait before refreshing again.".into(),
        403 => format!("GitHub didn't allow this action. {}", sentence(&message)),
        404 => "That GitHub repository or pull request isn't available to this account. Check its address and your organization's access settings.".into(),
        409 => "The pull request changed before this action completed. Refresh and review its latest commits before trying again.".into(),
        422 => format!("GitHub couldn't accept this change. {}", sentence(&message)),
        500..=599 => "GitHub is having trouble completing this request. Refresh before trying that action again.".into(),
        _ => format!("GitHub couldn't complete this request. {}", sentence(&message)),
    }
}

fn sentence(text: &str) -> String {
    if text.is_empty() {
        return "Refresh its details and try again.".into();
    }
    let mut s = text.trim().to_string();
    if !s.ends_with(['.', '!', '?']) {
        s.push('.');
    }
    s
}
