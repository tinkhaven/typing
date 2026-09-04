//! Whether sign-in is switched on, and everything it needs if so.
//!
//! Grouped into one optional value so that "is sign-in available?" is a single
//! check rather than three, and so a half-configured deployment is impossible:
//! either all of the client id, the client secret and the session secret are
//! present, or the feature is off and the routes say so.

use super::{
    auth::{pepper, OidcClient, Provider},
    session::SigningKey,
};

/// The configuration behind sign-in.
pub struct Accounts {
    /// The Google OpenID Connect client.
    pub google: OidcClient,
    /// Signs session cookies and the in-flight login cookie.
    pub session_key: SigningKey,
    /// Turns a provider subject into the identifier we store.
    pub pepper: SigningKey,
}

impl Accounts {
    /// Reads configuration from the environment.
    ///
    /// Returns `None` when anything is missing, and logs which piece, because a
    /// deployment where sign-in silently does nothing is worse than one where
    /// the reason is in the log.
    pub fn from_env() -> Option<Accounts> {
        let secret = std::env::var("SESSION_SECRET")
            .ok()
            .filter(|s| s.len() >= 32);
        let session_key = SigningKey::from_env();
        let provider = Provider::google_from_env();

        match (session_key, provider, secret) {
            (Some(session_key), Some(provider), Some(secret)) => {
                tracing::info!(
                    redirect_uri = %provider.redirect_uri,
                    "sign-in: Google, requesting only the openid scope"
                );
                Some(Accounts {
                    google: OidcClient::new(provider),
                    session_key,
                    pepper: pepper(&secret),
                })
            }
            (session_key, provider, secret) => {
                let mut missing = Vec::new();
                if session_key.is_none() || secret.is_none() {
                    missing.push("SESSION_SECRET (32+ characters)");
                }
                if provider.is_none() {
                    missing.push("GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET");
                }
                tracing::warn!(missing = ?missing, "sign-in is off");
                None
            }
        }
    }
}
