//! OAuth authorization code + PKCE, loopback callbacks and Keychain storage.
//! Credentials deliberately do not implement Debug or appear in settings.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::http::{Request, Transport};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}
impl OAuthConfig {
    pub fn configured() -> Result<Self, String> {
        let id = std::env::var("WACKDEPOT_GITHUB_CLIENT_ID")
            .ok()
            .or_else(|| option_env!("WACKDEPOT_GITHUB_CLIENT_ID").map(str::to_string));
        let secret = std::env::var("WACKDEPOT_GITHUB_CLIENT_SECRET")
            .ok()
            .or_else(|| option_env!("WACKDEPOT_GITHUB_CLIENT_SECRET").map(str::to_string));
        match (id,secret) {
            (Some(client_id),Some(client_secret)) if !client_id.is_empty() && !client_secret.is_empty() => Ok(Self { client_id,client_secret }),
            _ => Err("GitHub sign-in needs this build's OAuth client configuration. See docs/GITHUB_SETUP.md to configure a development build.".into()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub refresh_expires_at: Option<u64>,
}

pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<Tokens>, String>;
    fn save(&self, tokens: &Tokens) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

pub struct Keychain;
impl Keychain {
    fn service() -> &'static str {
        if cfg!(debug_assertions) {
            "com.wackdepot.app.dev.github"
        } else {
            "com.wackdepot.app.github"
        }
    }
}
impl TokenStore for Keychain {
    fn load(&self) -> Result<Option<Tokens>, String> {
        match security_framework::passwords::get_generic_password(Self::service(),"github.com") {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_|"Couldn't read the saved GitHub session. Reconnect GitHub in Settings.".into()),
            Err(e) if e.code()==-25300 => Ok(None),
            Err(_) => Err("Couldn't access the GitHub session in macOS Keychain. Unlock your login Keychain and try again.".into()),
        }
    }
    fn save(&self, tokens: &Tokens) -> Result<(), String> {
        let bytes = serde_json::to_vec(tokens)
            .map_err(|_| "Couldn't prepare the GitHub session for Keychain.".to_string())?;
        security_framework::passwords::set_generic_password(Self::service(), "github.com", &bytes)
            .map_err(|_| "Couldn't save the GitHub session in macOS Keychain.".into())
    }
    fn clear(&self) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(Self::service(), "github.com")
        {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()),
            Err(_) => Err("Couldn't remove the GitHub session from macOS Keychain.".into()),
        }
    }
}

pub struct Authorization {
    listener: TcpListener,
    pub url: String,
    pub redirect_uri: String,
    verifier: String,
    state: String,
    pub cancelled: Arc<AtomicBool>,
}
impl Authorization {
    pub fn start(config: &OAuthConfig) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| {
            "Couldn't open the local GitHub sign-in callback. Try connecting again.".to_string()
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Couldn't prepare the GitHub sign-in callback.".to_string())?;
        let port = listener
            .local_addr()
            .map_err(|_| "Couldn't find the GitHub callback port.".to_string())?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}/github/callback");
        let mut random = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let verifier = URL_SAFE_NO_PAD.encode(random);
        rand::rngs::OsRng.fill_bytes(&mut random);
        let state = URL_SAFE_NO_PAD.encode(random);
        let challenge = pkce_challenge(&verifier);
        let mut url =
            url::Url::parse("https://github.com/login/oauth/authorize").expect("constant URL");
        url.query_pairs_mut().extend_pairs([
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", &redirect_uri),
            ("scope", "repo read:org workflow offline_access"),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]);
        Ok(Self {
            listener,
            url: url.into(),
            redirect_uri,
            verifier,
            state,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn finish(self, config: &OAuthConfig, transport: &dyn Transport) -> Result<Tokens, String> {
        let deadline = Instant::now() + Duration::from_secs(600);
        while Instant::now() < deadline {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err("GitHub sign-in was cancelled.".into());
            }
            match self.listener.accept() {
                Ok((mut stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let result = read_callback(&mut stream, &self.state);
                    let body = if result.is_ok() {
                        "GitHub authorization received. You can return to WackDepot."
                    } else {
                        "GitHub authorization could not be completed. Return to WackDepot and reconnect."
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    match result {
                        Ok(code) => {
                            return exchange(
                                transport,
                                serde_json::json!({"client_id":config.client_id,"client_secret":config.client_secret,"code":code,"redirect_uri":self.redirect_uri,"code_verifier":self.verifier}),
                            );
                        }
                        Err(e)
                            if e == "That request isn't the GitHub callback."
                                || e == "The GitHub callback didn't match this sign-in attempt." =>
                        {
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => {
                    return Err(
                        "The GitHub sign-in callback stopped unexpectedly. Try connecting again."
                            .into(),
                    );
                }
            }
        }
        Err("GitHub sign-in timed out. Click Connect GitHub to try again.".into())
    }
}

fn read_callback(stream: &mut TcpStream, state: &str) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "Couldn't read the GitHub callback.".to_string())?;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 1024];
    while bytes.len() < 8192 {
        let n = stream
            .read(&mut buf)
            .map_err(|_| "Couldn't read the GitHub callback.".to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    let line = text.lines().next().unwrap_or_default();
    let mut parts = line.split_whitespace();
    if parts.next() != Some("GET") {
        return Err("That request isn't the GitHub callback.".into());
    }
    parse_callback(parts.next().unwrap_or_default(), state)
}

pub fn parse_callback(target: &str, state: &str) -> Result<String, String> {
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "That request isn't the GitHub callback.".to_string())?;
    if url.path() != "/github/callback" {
        return Err("That request isn't the GitHub callback.".into());
    }
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    if params.get("state").map(String::as_str) != Some(state) {
        return Err("The GitHub callback didn't match this sign-in attempt.".into());
    }
    if params.contains_key("error") {
        return Err("GitHub sign-in wasn't authorized. Click Connect GitHub when you're ready to try again.".into());
    }
    params
        .get("code")
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or_else(|| "GitHub didn't return an authorization code. Try connecting again.".into())
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn refresh(
    config: &OAuthConfig,
    tokens: &Tokens,
    transport: &dyn Transport,
) -> Result<Tokens, String> {
    if tokens
        .refresh_expires_at
        .is_some_and(|expiry| expiry <= now())
    {
        return Err("Your GitHub session has expired. Reconnect GitHub in Settings.".into());
    }
    let refresh = tokens
        .refresh_token
        .as_deref()
        .ok_or("Your GitHub session has expired. Reconnect GitHub in Settings.")?;
    exchange(
        transport,
        serde_json::json!({"client_id":config.client_id,"client_secret":config.client_secret,"grant_type":"refresh_token","refresh_token":refresh}),
    )
}

pub fn exchange(transport: &dyn Transport, body: serde_json::Value) -> Result<Tokens, String> {
    let response = transport.send(Request {
        method: "POST",
        url: "https://github.com/login/oauth/access_token".into(),
        headers: vec![
            ("Accept".into(), "application/json".into()),
            ("Content-Type".into(), "application/json".into()),
            ("User-Agent".into(), "WackDepot".into()),
        ],
        body: Some(body.to_string().into_bytes()),
    })?;
    if response.status != 200 {
        return Err("GitHub couldn't complete sign-in. Check this build's OAuth registration and reconnect.".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|_| "GitHub returned an unreadable sign-in response.".to_string())?;
    let access_token = value["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("GitHub couldn't authorize this session. Reconnect GitHub in Settings.")?
        .to_string();
    Ok(Tokens {
        access_token,
        refresh_token: value["refresh_token"].as_str().map(str::to_string),
        expires_at: value["expires_in"].as_u64().map(|n| now() + n),
        refresh_expires_at: value["refresh_token_expires_in"]
            .as_u64()
            .map(|n| now() + n),
    })
}

/// Entry point for Git's command-scoped credential helper. Git sends the
/// protocol and host over stdin; credentials go only to Git's stdout pipe.
pub fn credential_helper(operation: &str) -> Result<(), String> {
    if operation != "get" {
        return Ok(());
    }
    let mut input = String::new();
    std::io::stdin()
        .take(8192)
        .read_to_string(&mut input)
        .map_err(|_| "Couldn't read Git's credential request.".to_string())?;
    let fields: std::collections::HashMap<_, _> =
        input.lines().filter_map(|l| l.split_once('=')).collect();
    if fields.get("protocol") != Some(&"https") || fields.get("host") != Some(&"github.com") {
        return Ok(());
    }
    let store = Keychain;
    let Some(mut tokens) = store.load()? else {
        return Ok(());
    };
    if tokens.expires_at.is_some_and(|expiry| expiry <= now() + 60) {
        tokens = refresh(
            &OAuthConfig::configured()?,
            &tokens,
            &crate::http::HttpTransport::default(),
        )?;
        store.save(&tokens)?;
    }
    writeln!(
        std::io::stdout(),
        "username=x-access-token\npassword={}\n",
        tokens.access_token
    )
    .map_err(|_| "Couldn't deliver the GitHub credential to Git.".into())
}
