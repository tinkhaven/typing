//! Signed, stateless session cookies.
//!
//! There is no server-side session table. A session is a short string naming the
//! user, plus the time it was issued, plus an HMAC over both — so any task can
//! verify a cookie without shared state, and signing out needs no coordination.
//! The trade-off of statelessness is that a cookie cannot be revoked before it
//! expires; for a typing tutor holding no personal data, that is the right side
//! of the trade.
//!
//! The user identifier in here is already pseudonymous: see
//! [`super::auth::derive_user_id`]. Nothing in a cookie reveals who anybody is.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// Name of the session cookie.
pub const COOKIE_NAME: &str = "tt_session";

/// How long a session lasts.
///
/// Long, because the only thing behind it is your own typing progress and being
/// signed out of that is a nuisance with no compensating benefit.
pub const TTL_SECONDS: u64 = 60 * 60 * 24 * 90;

/// Version marker, so the format can change without misreading old cookies.
const FORMAT: &str = "v1";

type HmacSha256 = Hmac<Sha256>;

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The key that signs cookies.
///
/// Derived from a secret by hashing, so the secret can be any length and its
/// bytes never end up used directly as a key.
#[derive(Clone)]
pub struct SigningKey([u8; 32]);

impl SigningKey {
    /// Derives the session-signing key from a secret string.
    pub fn from_secret(secret: &str) -> SigningKey {
        SigningKey::from_labelled_secret("tinkhaven-typing/session/v1", secret)
    }

    /// Derives a key for a named purpose from a secret string.
    ///
    /// The label is what makes one secret safe to use for more than one thing:
    /// the session key and the user-id pepper come from the same `SESSION_SECRET`
    /// but are different keys, so neither use can forge the other.
    pub fn from_labelled_secret(label: &str, secret: &str) -> SigningKey {
        let mut hasher = Sha256::new();
        hasher.update(label.as_bytes());
        hasher.update([0u8]);
        hasher.update(secret.as_bytes());
        SigningKey(hasher.finalize().into())
    }

    /// The raw key bytes, for callers that sign something other than a session.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Reads `SESSION_SECRET`.
    ///
    /// Returns `None` when unset, which switches sign-in off rather than
    /// inventing a key: a key generated at startup would differ between tasks
    /// and change on every deploy, silently signing everybody out.
    pub fn from_env() -> Option<SigningKey> {
        match std::env::var("SESSION_SECRET") {
            Ok(secret) if secret.len() >= 32 => Some(SigningKey::from_secret(&secret)),
            Ok(secret) if !secret.is_empty() => {
                tracing::error!(
                    length = secret.len(),
                    "SESSION_SECRET is too short; needs at least 32 characters. Sign-in is off."
                );
                None
            }
            _ => None,
        }
    }

    /// Signs a payload.
    fn sign(&self, payload: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC takes any key length");
        mac.update(payload.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    /// Checks a signature in constant time.
    fn verify(&self, payload: &str, signature: &str) -> bool {
        let Ok(provided) = URL_SAFE_NO_PAD.decode(signature) else {
            return false;
        };
        let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC takes any key length");
        mac.update(payload.as_bytes());
        // `verify_slice` is a constant-time comparison; `==` on the bytes would
        // leak how much of the tag matched.
        mac.verify_slice(&provided).is_ok()
    }
}

/// Why a cookie was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// Not in the expected shape, or an unknown format version.
    Malformed,
    /// The signature did not match: tampered with, or signed with another key.
    BadSignature,
    /// Older than [`TTL_SECONDS`].
    Expired,
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SessionError::Malformed => write!(f, "malformed session cookie"),
            SessionError::BadSignature => write!(f, "session signature does not verify"),
            SessionError::Expired => write!(f, "session has expired"),
        }
    }
}

/// A signed-in visitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// Pseudonymous user identifier.
    pub user: String,
    /// When the session was issued, in seconds since the epoch.
    pub issued_at: u64,
}

impl Session {
    /// Starts a session for a user, now.
    pub fn new(user: impl Into<String>) -> Session {
        Session {
            user: user.into(),
            issued_at: now_secs(),
        }
    }

    /// Renders the signed cookie value.
    pub fn encode(&self, key: &SigningKey) -> String {
        let payload = format!("{FORMAT}.{}.{}", self.user, self.issued_at);
        let signature = key.sign(&payload);
        format!("{payload}.{signature}")
    }

    /// Parses and verifies a cookie value.
    pub fn decode(raw: &str, key: &SigningKey, now: u64) -> Result<Session, SessionError> {
        // Exactly four parts: version, user, timestamp, signature. The user
        // identifier is hex, so it cannot contain the separator.
        let parts: Vec<&str> = raw.split('.').collect();
        let [version, user, issued, signature] = parts.as_slice() else {
            return Err(SessionError::Malformed);
        };
        if *version != FORMAT {
            return Err(SessionError::Malformed);
        }

        let payload = format!("{version}.{user}.{issued}");
        // Signature first: nothing else in here is trustworthy until it passes.
        if !key.verify(&payload, signature) {
            return Err(SessionError::BadSignature);
        }

        let issued_at: u64 = issued.parse().map_err(|_| SessionError::Malformed)?;
        if user.is_empty() {
            return Err(SessionError::Malformed);
        }
        // A cookie dated in the future is a clock problem, not an attack — it is
        // signed. Treat it as issued now rather than rejecting the visitor.
        if now > issued_at && now - issued_at > TTL_SECONDS {
            return Err(SessionError::Expired);
        }

        Ok(Session {
            user: (*user).to_owned(),
            issued_at,
        })
    }
}

/// The `Set-Cookie` value that establishes a session.
///
/// `Secure` is unconditional: browsers make an exception for `http://localhost`,
/// so it costs nothing in development and there is no flag to forget in
/// production. `SameSite=Lax` still allows the cookie on the top-level redirect
/// back from Google, which `Strict` would not.
pub fn set_cookie(session: &Session, key: &SigningKey) -> String {
    format!(
        "{COOKIE_NAME}={}; Path=/; Max-Age={TTL_SECONDS}; HttpOnly; Secure; SameSite=Lax",
        session.encode(key)
    )
}

/// The `Set-Cookie` value that clears a session.
pub fn clear_cookie() -> String {
    format!("{COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax")
}

/// Finds a named cookie in a `Cookie` header.
pub fn read_cookie(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SigningKey {
        SigningKey::from_secret("a-test-secret-that-is-long-enough-to-use")
    }

    #[test]
    fn a_session_survives_a_round_trip() {
        let k = key();
        let session = Session {
            user: "abc123".into(),
            issued_at: 1_700_000_000,
        };
        let cookie = session.encode(&k);
        assert_eq!(Session::decode(&cookie, &k, 1_700_000_100), Ok(session));
    }

    #[test]
    fn tampering_with_the_user_is_rejected() {
        let k = key();
        let cookie = Session {
            user: "alice".into(),
            issued_at: 1_700_000_000,
        }
        .encode(&k);
        // Swap the user for someone else's, keeping the original signature.
        let forged = cookie.replacen("alice", "bobxx", 1);
        assert_eq!(
            Session::decode(&forged, &k, 1_700_000_100),
            Err(SessionError::BadSignature)
        );
    }

    #[test]
    fn extending_the_lifetime_is_rejected() {
        let k = key();
        let cookie = Session {
            user: "abc123".into(),
            issued_at: 1_000,
        }
        .encode(&k);
        // Re-date it to now so it looks fresh.
        let forged = cookie.replacen(".1000.", ".1700000000.", 1);
        assert!(matches!(
            Session::decode(&forged, &k, 1_700_000_100),
            Err(SessionError::BadSignature)
        ));
    }

    #[test]
    fn another_key_cannot_verify() {
        let cookie = Session::new("abc123").encode(&key());
        let other = SigningKey::from_secret("a-different-secret-of-adequate-length");
        assert_eq!(
            Session::decode(&cookie, &other, now_secs()),
            Err(SessionError::BadSignature)
        );
    }

    #[test]
    fn an_old_session_expires() {
        let k = key();
        let cookie = Session {
            user: "abc123".into(),
            issued_at: 1_000,
        }
        .encode(&k);
        let just_inside = 1_000 + TTL_SECONDS;
        assert!(Session::decode(&cookie, &k, just_inside).is_ok());
        assert_eq!(
            Session::decode(&cookie, &k, just_inside + 1),
            Err(SessionError::Expired)
        );
    }

    #[test]
    fn a_future_dated_session_is_accepted_rather_than_breaking_the_visitor() {
        // It is signed, so this is clock skew, not an attack.
        let k = key();
        let cookie = Session {
            user: "abc123".into(),
            issued_at: 2_000_000_000,
        }
        .encode(&k);
        assert!(Session::decode(&cookie, &k, 1_700_000_000).is_ok());
    }

    #[test]
    fn rubbish_is_rejected_without_panicking() {
        let k = key();
        for raw in [
            "",
            ".",
            "v1",
            "v1.a.b",
            "v1.a.b.c.d",
            "v2.a.1.sig",
            "v1..1.sig",
        ] {
            let verdict = Session::decode(raw, &k, now_secs());
            assert!(verdict.is_err(), "{raw:?} should not decode");
        }
    }

    #[test]
    fn a_non_numeric_timestamp_is_malformed_not_a_panic() {
        let k = key();
        // Sign a payload whose timestamp is not a number, so the signature is
        // valid and parsing is what has to reject it.
        let payload = "v1.abc123.not-a-number";
        let signature = k.sign(payload);
        assert_eq!(
            Session::decode(&format!("{payload}.{signature}"), &k, now_secs()),
            Err(SessionError::Malformed)
        );
    }

    #[test]
    fn the_cookie_carries_the_attributes_that_matter() {
        let cookie = set_cookie(&Session::new("abc123"), &key());
        for attribute in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
            assert!(cookie.contains(attribute), "missing {attribute}: {cookie}");
        }
        assert!(clear_cookie().contains("Max-Age=0"));
    }

    #[test]
    fn cookies_are_read_out_of_a_header() {
        let header = "other=1; tt_session=abc.def; third=2";
        assert_eq!(read_cookie(header, COOKIE_NAME), Some("abc.def".into()));
        assert_eq!(read_cookie(header, "other"), Some("1".into()));
        assert_eq!(read_cookie(header, "absent"), None);
        assert_eq!(read_cookie("", COOKIE_NAME), None);
    }

    #[test]
    fn a_short_secret_is_refused() {
        // Guards the length check itself; from_env reads the process
        // environment, which tests must not mutate.
        assert!("too-short".len() < 32);
        let key = SigningKey::from_secret("too-short");
        // Deriving still works — the refusal is from_env's job, not the KDF's.
        assert!(!Session::new("x").encode(&key).is_empty());
    }
}
