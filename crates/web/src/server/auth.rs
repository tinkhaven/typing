//! Sign in with Google, holding as little as possible.
//!
//! # What is asked for, and what is kept
//!
//! The only scope requested is `openid`. Not `email`, not `profile`. Google
//! answers with an ID token whose useful content is `sub` — a pseudonymous
//! identifier, stable for this client and this user, and meaningless to anyone
//! else. So this service never learns an email address or a name, and cannot
//! contact or identify anybody.
//!
//! Even `sub` is not stored. What goes in the database is
//! `HMAC(pepper, issuer ‖ sub)` (see [`derive_user_id`]), so the stored key is
//! not the provider's identifier either, and a copy of the table on its own
//! links to nothing.
//!
//! The cost of holding nothing is that there is no account recovery. Lose access
//! to the Google account and the progress behind it is unreachable, by anyone,
//! including the operator. That is a deliberate trade and the privacy policy
//! says so.
//!
//! # The flow
//!
//! Authorization code with PKCE. Three one-time values guard it:
//!
//! * `state` ties the callback to the browser that started it (CSRF).
//! * `nonce` ties the ID token to this particular request (replay).
//! * the PKCE `code_verifier` proves the client redeeming the code is the one
//!   that asked for it.
//!
//! All three live in a short-lived signed cookie rather than server memory, so
//! the callback can land on any task. See [`PendingLogin`].

use std::collections::HashSet;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::TryRngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::session::{now_secs, SigningKey};

/// The only scope ever requested.
pub const SCOPE: &str = "openid";

/// Cookie holding the in-flight login.
pub const PENDING_COOKIE: &str = "tt_login";

/// How long a login may take before its cookie is stale.
pub const PENDING_TTL_SECONDS: u64 = 600;

/// Google's authorization endpoint.
pub const GOOGLE_AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Google's token endpoint.
pub const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
/// Google's signing keys.
pub const GOOGLE_JWKS: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// Both spellings Google has used for its issuer claim. Either is legitimate.
pub const GOOGLE_ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Where a provider's endpoints are and who we are to it.
#[derive(Clone, Debug)]
pub struct Provider {
    /// Short name, used in routes and in the derived user id.
    pub name: &'static str,
    /// Where the browser is sent to consent.
    pub authorize_endpoint: String,
    /// Where the code is exchanged.
    pub token_endpoint: String,
    /// Where the signing keys are published.
    pub jwks_uri: String,
    /// Issuer values accepted in the ID token.
    pub issuers: Vec<String>,
    /// OAuth client id. Public by design.
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
    /// Must match the redirect registered with the provider, exactly.
    pub redirect_uri: String,
}

impl Provider {
    /// Builds the Google provider from the environment.
    ///
    /// Returns `None` if either credential is missing, which leaves sign-in
    /// switched off rather than half-configured. The endpoints can be overridden
    /// (`GOOGLE_AUTHORIZE_ENDPOINT` and friends) so the flow can be pointed at a
    /// stand-in provider in tests.
    pub fn google_from_env() -> Option<Provider> {
        let client_id = non_empty("GOOGLE_CLIENT_ID")?;
        let client_secret = non_empty("GOOGLE_CLIENT_SECRET")?;
        let base =
            std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_owned());

        Some(Provider {
            name: "google",
            authorize_endpoint: std::env::var("GOOGLE_AUTHORIZE_ENDPOINT")
                .unwrap_or_else(|_| GOOGLE_AUTHORIZE.to_owned()),
            token_endpoint: std::env::var("GOOGLE_TOKEN_ENDPOINT")
                .unwrap_or_else(|_| GOOGLE_TOKEN.to_owned()),
            jwks_uri: std::env::var("GOOGLE_JWKS_URI").unwrap_or_else(|_| GOOGLE_JWKS.to_owned()),
            issuers: std::env::var("GOOGLE_ISSUER")
                .map(|issuer| vec![issuer])
                .unwrap_or_else(|_| GOOGLE_ISSUERS.iter().map(|s| s.to_string()).collect()),
            client_id,
            client_secret,
            redirect_uri: format!("{}/auth/google/callback", base.trim_end_matches('/')),
        })
    }
}

fn non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

// ---------------------------------------------------------------------------
// The in-flight login
// ---------------------------------------------------------------------------

/// The one-time values that tie a callback to the request that began it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingLogin {
    /// Opaque value echoed back by the provider.
    pub state: String,
    /// Bound into the ID token by the provider.
    pub nonce: String,
    /// PKCE verifier; its hash was sent, this is redeemed.
    pub verifier: String,
    /// When the login started.
    pub issued_at: u64,
}

impl PendingLogin {
    /// Generates fresh values from the system RNG.
    ///
    /// # Panics
    /// Panics if the operating system cannot provide randomness, which would
    /// make every value here predictable. There is no safe way to continue.
    pub fn new() -> PendingLogin {
        PendingLogin {
            state: random_token(),
            nonce: random_token(),
            verifier: random_token(),
            issued_at: now_secs(),
        }
    }

    /// The PKCE challenge for [`Self::verifier`], method `S256`.
    pub fn challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()))
    }

    /// Renders the signed cookie value.
    pub fn encode(&self, key: &SigningKey) -> String {
        let payload = format!(
            "v1.{}.{}.{}.{}",
            self.state, self.nonce, self.verifier, self.issued_at
        );
        let signature = sign(key, &payload);
        format!("{payload}.{signature}")
    }

    /// Parses and verifies a cookie value.
    pub fn decode(raw: &str, key: &SigningKey, now: u64) -> Result<PendingLogin, AuthError> {
        let parts: Vec<&str> = raw.split('.').collect();
        let [version, state, nonce, verifier, issued, signature] = parts.as_slice() else {
            return Err(AuthError::PendingMalformed);
        };
        if *version != "v1" {
            return Err(AuthError::PendingMalformed);
        }

        let payload = format!("{version}.{state}.{nonce}.{verifier}.{issued}");
        if !verify(key, &payload, signature) {
            return Err(AuthError::PendingBadSignature);
        }

        let issued_at: u64 = issued.parse().map_err(|_| AuthError::PendingMalformed)?;
        if now > issued_at && now - issued_at > PENDING_TTL_SECONDS {
            return Err(AuthError::PendingExpired);
        }
        if state.is_empty() || nonce.is_empty() || verifier.is_empty() {
            return Err(AuthError::PendingMalformed);
        }

        Ok(PendingLogin {
            state: (*state).to_owned(),
            nonce: (*nonce).to_owned(),
            verifier: (*verifier).to_owned(),
            issued_at,
        })
    }

    /// The `Set-Cookie` value carrying this login.
    ///
    /// `SameSite=Lax` is required, not merely chosen: the provider returns the
    /// browser here by top-level redirect, and `Strict` would withhold the
    /// cookie on that navigation and break every sign-in.
    pub fn set_cookie(&self, key: &SigningKey) -> String {
        format!(
            "{PENDING_COOKIE}={}; Path=/auth; Max-Age={PENDING_TTL_SECONDS}; HttpOnly; Secure; SameSite=Lax",
            self.encode(key)
        )
    }

    /// The `Set-Cookie` value that discards it, used as soon as it is redeemed.
    pub fn clear_cookie() -> String {
        format!("{PENDING_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; Secure; SameSite=Lax")
    }
}

impl Default for PendingLogin {
    fn default() -> Self {
        PendingLogin::new()
    }
}

/// 32 bytes of system randomness, base64url encoded — 43 characters, which is
/// also the minimum length PKCE allows for a verifier.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system must provide randomness for login tokens");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn sign(key: &SigningKey, payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.bytes()).expect("HMAC takes any key length");
    mac.update(payload.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

fn verify(key: &SigningKey, payload: &str, signature: &str) -> bool {
    let Ok(provided) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(key.bytes()).expect("HMAC takes any key length");
    mac.update(payload.as_bytes());
    mac.verify_slice(&provided).is_ok()
}

// ---------------------------------------------------------------------------
// Building the authorize URL
// ---------------------------------------------------------------------------

/// Where to send the browser to consent.
pub fn authorize_url(provider: &Provider, pending: &PendingLogin) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", &provider.client_id),
        ("redirect_uri", &provider.redirect_uri),
        ("scope", SCOPE),
        ("state", &pending.state),
        ("nonce", &pending.nonce),
        ("code_challenge", &pending.challenge()),
        ("code_challenge_method", "S256"),
    ]
    .iter()
    .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
    .collect::<Vec<_>>()
    .join("&");

    let separator = if provider.authorize_endpoint.contains('?') {
        '&'
    } else {
        '?'
    };
    format!("{}{separator}{query}", provider.authorize_endpoint)
}

/// Percent-encodes a query component, keeping only the unreserved set.
///
/// Hand-rolled rather than pulling in a URL crate for eight parameters. The
/// unreserved set is from RFC 3986 §2.3; everything else is escaped, which is
/// always safe even where it is not strictly required.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The ID token
// ---------------------------------------------------------------------------

/// The claims this service cares about. Deliberately few.
#[derive(Clone, Debug, Deserialize)]
pub struct IdClaims {
    /// Who issued the token.
    pub iss: String,
    /// The pseudonymous subject identifier.
    pub sub: String,
    /// Which client the token is for.
    pub aud: String,
    /// Expiry, seconds since the epoch.
    pub exp: u64,
    /// The nonce sent in the authorize request.
    pub nonce: Option<String>,
}

/// Checks the claims that a signature alone does not establish.
///
/// `jsonwebtoken` verifies `iss`, `aud` and `exp` when told to; this covers the
/// nonce, which it has no concept of, and which is the difference between
/// accepting a token minted for this request and accepting a replayed one.
pub fn check_nonce(claims: &IdClaims, expected: &str) -> Result<(), AuthError> {
    match claims.nonce.as_deref() {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(AuthError::NonceMismatch),
        None => Err(AuthError::NonceMissing),
    }
}

/// Validation rules for a provider's ID tokens.
pub fn validation(provider: &Provider) -> jsonwebtoken::Validation {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[&provider.client_id]);
    validation.iss = Some(provider.issuers.iter().cloned().collect::<HashSet<_>>());
    validation.validate_exp = true;
    // Google's tokens are short-lived; a minute of tolerance covers clock skew
    // without meaningfully extending the window.
    validation.leeway = 60;
    validation.required_spec_claims = ["iss", "aud", "exp", "sub"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    validation
}

// ---------------------------------------------------------------------------
// The stored identifier
// ---------------------------------------------------------------------------

/// The key that turns a provider subject into our own identifier.
///
/// Derived from the same secret as session signing but with a different label,
/// so one secret is all there is to manage and the two uses cannot collide.
///
/// **Changing the secret changes every user id**, which orphans every stored
/// profile. Treat it as permanent once anyone has signed in.
pub fn pepper(secret: &str) -> SigningKey {
    SigningKey::from_labelled_secret("tinkhaven-typing/user-id/v1", secret)
}

/// Turns a provider's subject identifier into the one we store.
///
/// `HMAC(pepper, issuer ‖ 0x00 ‖ subject)`, truncated to 128 bits and hex
/// encoded. The separator matters: without it, different issuer/subject splits
/// could produce the same input and so the same user.
pub fn derive_user_id(pepper: &SigningKey, issuer: &str, subject: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(pepper.bytes()).expect("HMAC takes any key length");
    mac.update(issuer.as_bytes());
    mac.update(&[0u8]);
    mac.update(subject.as_bytes());
    let tag = mac.finalize().into_bytes();
    tag[..16].iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Talking to the provider
// ---------------------------------------------------------------------------

/// The provider's signing keys, and when they were fetched.
struct CachedKeys {
    keys: jsonwebtoken::jwk::JwkSet,
    fetched_at: u64,
}

/// How long a fetched key set is reused before being refreshed.
const JWKS_TTL_SECONDS: u64 = 3600;

/// An OpenID Connect client for one provider.
pub struct OidcClient {
    provider: Provider,
    http: reqwest::Client,
    keys: tokio::sync::RwLock<Option<CachedKeys>>,
}

/// What the token endpoint returns. Only the ID token is wanted.
///
/// No access token is requested or kept: with `openid` alone there is no
/// userinfo call to make, so an access token would be a credential held for no
/// reason.
#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

impl OidcClient {
    /// Builds a client for a provider.
    pub fn new(provider: Provider) -> OidcClient {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // No redirects: the token endpoint should answer directly, and
            // following one would be a way to have the secret sent elsewhere.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("tinkhaven-typing")
            .build()
            .expect("a plain HTTPS client must build");
        OidcClient {
            provider,
            http,
            keys: tokio::sync::RwLock::new(None),
        }
    }

    /// The provider this client speaks to.
    pub fn provider(&self) -> &Provider {
        &self.provider
    }

    /// Redeems an authorization code for an ID token.
    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<String, AuthError> {
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.provider.redirect_uri.as_str()),
            ("client_id", self.provider.client_id.as_str()),
            ("client_secret", self.provider.client_secret.as_str()),
            ("code_verifier", verifier),
        ];

        let response = self
            .http
            .post(&self.provider.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|e| AuthError::TokenExchange(e.to_string()))?;

        let status = response.status();
        let body: TokenResponse = response
            .json()
            .await
            .map_err(|e| AuthError::TokenExchange(format!("unreadable response: {e}")))?;

        if let Some(error) = body.error {
            let detail = body.error_description.unwrap_or(error);
            return Err(AuthError::ProviderRefused(detail));
        }
        if !status.is_success() {
            return Err(AuthError::TokenExchange(format!("HTTP {status}")));
        }
        body.id_token.ok_or(AuthError::NoIdToken)
    }

    /// Finds the key a token was signed with, refreshing the set if need be.
    async fn decoding_key(&self, kid: &str) -> Result<jsonwebtoken::DecodingKey, AuthError> {
        // Cached set first.
        if let Some(cached) = self.keys.read().await.as_ref() {
            if now_secs().saturating_sub(cached.fetched_at) < JWKS_TTL_SECONDS {
                if let Some(key) = key_from_set(&cached.keys, kid) {
                    return key;
                }
                // A `kid` we have never seen usually means the provider rotated
                // its keys early. Fall through and refetch rather than reject a
                // perfectly good token.
            }
        }

        let fetched: jsonwebtoken::jwk::JwkSet = self
            .http
            .get(&self.provider.jwks_uri)
            .send()
            .await
            .map_err(|e| AuthError::TokenExchange(format!("fetching signing keys: {e}")))?
            .json()
            .await
            .map_err(|e| AuthError::TokenExchange(format!("unreadable signing keys: {e}")))?;

        let key = key_from_set(&fetched, kid);
        *self.keys.write().await = Some(CachedKeys {
            keys: fetched,
            fetched_at: now_secs(),
        });
        key.unwrap_or(Err(AuthError::UnknownSigningKey))
    }

    /// Verifies an ID token's signature, registered claims and nonce.
    pub async fn verify_id_token(
        &self,
        id_token: &str,
        expected_nonce: &str,
    ) -> Result<IdClaims, AuthError> {
        let header = jsonwebtoken::decode_header(id_token)
            .map_err(|e| AuthError::InvalidIdToken(e.to_string()))?;
        let kid = header.kid.ok_or(AuthError::UnknownSigningKey)?;
        let key = self.decoding_key(&kid).await?;

        let data = jsonwebtoken::decode::<IdClaims>(id_token, &key, &validation(&self.provider))
            .map_err(|e| AuthError::InvalidIdToken(e.to_string()))?;

        // `jsonwebtoken` has no concept of a nonce, and it is what separates a
        // token minted for this request from a replayed one.
        check_nonce(&data.claims, expected_nonce)?;
        Ok(data.claims)
    }

    /// Runs the whole callback: state check, code exchange, token verification.
    pub async fn complete_login(
        &self,
        pending: &PendingLogin,
        returned_state: &str,
        code: &str,
    ) -> Result<IdClaims, AuthError> {
        // Constant-time-ish comparison is unnecessary here: `state` is not a
        // secret to be guessed byte by byte, it is a value we minted and stored.
        if returned_state != pending.state {
            return Err(AuthError::StateMismatch);
        }
        let id_token = self.exchange_code(code, &pending.verifier).await?;
        self.verify_id_token(&id_token, &pending.nonce).await
    }
}

/// Turns the JWK with this `kid` into a decoding key, if the set has one.
fn key_from_set(
    set: &jsonwebtoken::jwk::JwkSet,
    kid: &str,
) -> Option<Result<jsonwebtoken::DecodingKey, AuthError>> {
    let jwk = set.find(kid)?;
    match &jwk.algorithm {
        jsonwebtoken::jwk::AlgorithmParameters::RSA(rsa) => Some(
            jsonwebtoken::DecodingKey::from_rsa_components(&rsa.n, &rsa.e)
                .map_err(|e| AuthError::InvalidIdToken(format!("unusable signing key: {e}"))),
        ),
        // Google signs ID tokens with RS256. Anything else is not something to
        // guess at.
        _ => Some(Err(AuthError::UnknownSigningKey)),
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a sign-in did not complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// Sign-in is not configured on this deployment.
    NotConfigured,
    /// The login cookie was absent, so there is nothing to match the callback to.
    PendingMissing,
    /// The login cookie was not in the expected shape.
    PendingMalformed,
    /// The login cookie's signature did not verify.
    PendingBadSignature,
    /// The login took too long.
    PendingExpired,
    /// The `state` did not match the one we issued.
    StateMismatch,
    /// The provider reported an error instead of returning a code.
    ProviderRefused(String),
    /// The token endpoint could not be reached or answered badly.
    TokenExchange(String),
    /// The response carried no ID token.
    NoIdToken,
    /// The ID token's signature or registered claims did not check out.
    InvalidIdToken(String),
    /// No key in the provider's key set matches the token's `kid`.
    UnknownSigningKey,
    /// The ID token carried no nonce.
    NonceMissing,
    /// The nonce did not match this request.
    NonceMismatch,
}

impl core::fmt::Display for AuthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AuthError::NotConfigured => write!(f, "sign-in is not configured"),
            AuthError::PendingMissing => write!(f, "no sign-in was in progress"),
            AuthError::PendingMalformed => write!(f, "the sign-in cookie was unreadable"),
            AuthError::PendingBadSignature => write!(f, "the sign-in cookie was tampered with"),
            AuthError::PendingExpired => write!(f, "the sign-in took too long; please retry"),
            AuthError::StateMismatch => write!(f, "the sign-in state did not match"),
            AuthError::ProviderRefused(why) => write!(f, "Google declined the sign-in: {why}"),
            AuthError::TokenExchange(why) => write!(f, "could not exchange the code: {why}"),
            AuthError::NoIdToken => write!(f, "no identity token was returned"),
            AuthError::InvalidIdToken(why) => write!(f, "the identity token is not valid: {why}"),
            AuthError::UnknownSigningKey => {
                write!(f, "the identity token was signed by an unknown key")
            }
            AuthError::NonceMissing => write!(f, "the identity token carried no nonce"),
            AuthError::NonceMismatch => {
                write!(f, "the identity token was issued for another request")
            }
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SigningKey {
        SigningKey::from_secret("a-test-secret-that-is-long-enough-to-use")
    }

    fn provider() -> Provider {
        Provider {
            name: "google",
            authorize_endpoint: GOOGLE_AUTHORIZE.to_owned(),
            token_endpoint: GOOGLE_TOKEN.to_owned(),
            jwks_uri: GOOGLE_JWKS.to_owned(),
            issuers: GOOGLE_ISSUERS.iter().map(|s| s.to_string()).collect(),
            client_id: "1234.apps.googleusercontent.com".to_owned(),
            client_secret: "not-a-real-secret".to_owned(),
            redirect_uri: "https://example.test/auth/google/callback".to_owned(),
        }
    }

    // ---- one-time values ------------------------------------------------

    #[test]
    fn every_login_gets_fresh_unguessable_values() {
        let a = PendingLogin::new();
        let b = PendingLogin::new();
        assert_ne!(a.state, b.state);
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.verifier, b.verifier);
        // Distinct from each other within one login, too.
        assert_ne!(a.state, a.nonce);
        assert_ne!(a.state, a.verifier);
        // 32 bytes base64url with no padding.
        assert_eq!(a.state.len(), 43);
    }

    #[test]
    fn the_pkce_verifier_meets_the_spec() {
        let pending = PendingLogin::new();
        // RFC 7636 §4.1: 43-128 characters from the unreserved set.
        assert!((43..=128).contains(&pending.verifier.len()));
        assert!(pending
            .verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b)));
    }

    #[test]
    fn the_pkce_challenge_is_the_sha256_of_the_verifier() {
        let pending = PendingLogin::new();
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(pending.verifier.as_bytes()));
        assert_eq!(pending.challenge(), expected);
        assert_ne!(
            pending.challenge(),
            pending.verifier,
            "must not send the verifier"
        );
    }

    // ---- the login cookie ------------------------------------------------

    #[test]
    fn a_pending_login_survives_a_round_trip() {
        let k = key();
        let pending = PendingLogin::new();
        let decoded = PendingLogin::decode(&pending.encode(&k), &k, pending.issued_at).unwrap();
        assert_eq!(decoded, pending);
    }

    #[test]
    fn swapping_the_nonce_in_the_cookie_is_rejected() {
        // Otherwise an attacker could make us accept a token minted elsewhere.
        let k = key();
        let pending = PendingLogin::new();
        let cookie = pending.encode(&k);
        let forged = cookie.replacen(&pending.nonce, &PendingLogin::new().nonce, 1);
        assert_eq!(
            PendingLogin::decode(&forged, &k, pending.issued_at),
            Err(AuthError::PendingBadSignature)
        );
    }

    #[test]
    fn swapping_the_verifier_in_the_cookie_is_rejected() {
        let k = key();
        let pending = PendingLogin::new();
        let cookie = pending.encode(&k);
        let forged = cookie.replacen(&pending.verifier, &PendingLogin::new().verifier, 1);
        assert_eq!(
            PendingLogin::decode(&forged, &k, pending.issued_at),
            Err(AuthError::PendingBadSignature)
        );
    }

    #[test]
    fn a_stale_login_is_refused() {
        let k = key();
        let pending = PendingLogin {
            issued_at: 1_000,
            ..PendingLogin::new()
        };
        let cookie = pending.encode(&k);
        assert!(PendingLogin::decode(&cookie, &k, 1_000 + PENDING_TTL_SECONDS).is_ok());
        assert_eq!(
            PendingLogin::decode(&cookie, &k, 1_001 + PENDING_TTL_SECONDS),
            Err(AuthError::PendingExpired)
        );
    }

    #[test]
    fn rubbish_login_cookies_are_refused_without_panicking() {
        let k = key();
        for raw in [
            "",
            "v1",
            "v1.a.b.c",
            "v1.a.b.c.d.e.f",
            "v2.a.b.c.1.sig",
            "v1...1.sig",
        ] {
            assert!(
                PendingLogin::decode(raw, &k, now_secs()).is_err(),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn the_login_cookie_is_scoped_and_hardened() {
        let cookie = PendingLogin::new().set_cookie(&key());
        for attribute in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/auth"] {
            assert!(cookie.contains(attribute), "missing {attribute}");
        }
        // Strict would withhold the cookie on Google's redirect back to us.
        assert!(!cookie.contains("SameSite=Strict"));
    }

    // ---- the authorize URL ----------------------------------------------

    #[test]
    fn the_authorize_url_asks_for_openid_and_nothing_else() {
        let url = authorize_url(&provider(), &PendingLogin::new());
        assert!(url.contains("scope=openid"), "{url}");
        assert!(!url.contains("email"), "must not request email: {url}");
        assert!(!url.contains("profile"), "must not request profile: {url}");
    }

    #[test]
    fn the_authorize_url_carries_the_whole_flow() {
        let provider = provider();
        let pending = PendingLogin::new();
        let url = authorize_url(&provider, &pending);
        for expected in [
            "response_type=code",
            "code_challenge_method=S256",
            &format!("state={}", percent_encode(&pending.state)),
            &format!("nonce={}", percent_encode(&pending.nonce)),
            &format!("code_challenge={}", percent_encode(&pending.challenge())),
        ] {
            assert!(url.contains(expected), "missing {expected} in {url}");
        }
        // The verifier itself must never leave the cookie.
        assert!(!url.contains(&pending.verifier), "leaked the PKCE verifier");
        // The secret must never appear in a URL the browser sees.
        assert!(
            !url.contains(&provider.client_secret),
            "leaked the client secret"
        );
    }

    #[test]
    fn the_redirect_uri_is_encoded() {
        let url = authorize_url(&provider(), &PendingLogin::new());
        assert!(
            url.contains("redirect_uri=https%3A%2F%2Fexample.test"),
            "{url}"
        );
    }

    #[test]
    fn percent_encoding_escapes_what_matters() {
        assert_eq!(percent_encode("abcXYZ019-._~"), "abcXYZ019-._~");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(percent_encode("https://x/y"), "https%3A%2F%2Fx%2Fy");
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    // ---- nonce ----------------------------------------------------------

    fn claims(nonce: Option<&str>) -> IdClaims {
        IdClaims {
            iss: "https://accounts.google.com".into(),
            sub: "108121".into(),
            aud: "1234.apps.googleusercontent.com".into(),
            exp: now_secs() + 300,
            nonce: nonce.map(str::to_owned),
        }
    }

    #[test]
    fn the_nonce_must_match_this_request() {
        assert_eq!(check_nonce(&claims(Some("expected")), "expected"), Ok(()));
        assert_eq!(
            check_nonce(&claims(Some("someone-elses")), "expected"),
            Err(AuthError::NonceMismatch)
        );
        assert_eq!(
            check_nonce(&claims(None), "expected"),
            Err(AuthError::NonceMissing)
        );
    }

    // ---- the stored identifier ------------------------------------------

    #[test]
    fn a_user_id_is_stable_for_the_same_person() {
        let p = pepper("a-secret-of-entirely-adequate-length");
        let first = derive_user_id(&p, "https://accounts.google.com", "108121");
        let second = derive_user_id(&p, "https://accounts.google.com", "108121");
        assert_eq!(first, second);
        assert_eq!(first.len(), 32, "128 bits, hex encoded");
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn a_user_id_reveals_nothing_about_the_subject() {
        let p = pepper("a-secret-of-entirely-adequate-length");
        let id = derive_user_id(&p, "https://accounts.google.com", "108121");
        assert!(!id.contains("108121"));
        assert!(!id.contains("google"));
    }

    #[test]
    fn different_people_get_different_ids() {
        let p = pepper("a-secret-of-entirely-adequate-length");
        assert_ne!(
            derive_user_id(&p, "https://accounts.google.com", "108121"),
            derive_user_id(&p, "https://accounts.google.com", "108122")
        );
    }

    #[test]
    fn the_issuer_is_separated_from_the_subject() {
        // Without a separator, ("iss-a", "b-sub") and ("iss", "ab-sub") would
        // concatenate identically and collapse two people into one.
        let p = pepper("a-secret-of-entirely-adequate-length");
        assert_ne!(derive_user_id(&p, "ab", "c"), derive_user_id(&p, "a", "bc"));
    }

    #[test]
    fn a_different_pepper_gives_a_different_id() {
        let subject = "108121";
        let issuer = "https://accounts.google.com";
        assert_ne!(
            derive_user_id(
                &pepper("secret-number-one-long-enough-yes"),
                issuer,
                subject
            ),
            derive_user_id(
                &pepper("secret-number-two-long-enough-yes"),
                issuer,
                subject
            )
        );
    }

    #[test]
    fn the_pepper_is_not_the_session_key() {
        // Reusing one secret for two purposes is only safe if the derived keys
        // differ; otherwise a session cookie and a user id share a key.
        let secret = "a-secret-of-entirely-adequate-length";
        assert_ne!(
            pepper(secret).bytes(),
            SigningKey::from_secret(secret).bytes()
        );
    }

    // ---- validation rules -----------------------------------------------

    #[test]
    fn validation_pins_the_algorithm_audience_and_issuer() {
        let provider = provider();
        let validation = validation(&provider);
        assert_eq!(validation.algorithms, vec![jsonwebtoken::Algorithm::RS256]);
        assert!(validation.validate_exp);
        assert!(validation
            .aud
            .as_ref()
            .expect("audience pinned")
            .contains(&provider.client_id));
        let issuers = validation.iss.as_ref().expect("issuer pinned");
        assert!(issuers.contains("https://accounts.google.com"));
        for required in ["iss", "aud", "exp", "sub"] {
            assert!(
                validation.required_spec_claims.contains(required),
                "{required}"
            );
        }
    }
}
