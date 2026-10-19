//! GitHub.com integration, independent of gpui and the working tree.
//! All errors are sentences suitable for presentation in the application.

pub mod api;
pub mod auth;
pub mod drafts;
pub mod http;
pub mod types;

pub use api::Client;
pub use auth::{Keychain, OAuthConfig, TokenStore};
pub use types::*;

#[cfg(test)]
mod tests;
