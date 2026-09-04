//! The Google sign-in flow, end to end, against a stand-in provider.
//!
//! The unit tests in `server::auth` and `server::jwt` cover every decision that
//! can be made without a network. What they cannot cover is whether the pieces
//! fit together: whether the token request has the shape a provider expects,
//! whether a real RS256 signature is checked against a fetched key set, and
//! whether a bad token is refused rather than shrugged off.
//!
//! So this stands up a minimal OpenID Connect provider on a local port and runs
//! the real client against it, serving genuinely signed tokens from
//! `tests/vectors`. Those were signed once with OpenSSL and the private key
//! discarded — see the README there for why a key is not generated at test time.
//!
//! Only reachable with the `ssr` feature, which is where the server code lives.
#![cfg(feature = "ssr")]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    extract::State,
    routing::{get, post},
    Form, Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::json;
use sha2::{Digest, Sha256};
use typing_web::server::{
    auth::{authorize_url, AuthError, OidcClient, PendingLogin, Provider},
    session::now_secs,
};

/// Matches the vectors in `tests/vectors`.
const CLIENT_ID: &str = "test-client.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "the-stand-in-provider-shared-value";
const ISSUER: &str = "https://accounts.google.test";
const KEY_ID: &str = "vector-key-1";
const NONCE: &str = "the-expected-nonce";

const MODULUS: &str = include_str!("vectors/modulus.b64");
const VALID: &str = include_str!("vectors/valid.jwt");
const EXPIRED: &str = include_str!("vectors/expired.jwt");
const WRONG_AUDIENCE: &str = include_str!("vectors/wrong_audience.jwt");
const WRONG_ISSUER: &str = include_str!("vectors/wrong_issuer.jwt");
const WRONG_NONCE: &str = include_str!("vectors/wrong_nonce.jwt");
const NO_NONCE: &str = include_str!("vectors/no_nonce.jwt");
const UNKNOWN_KID: &str = include_str!("vectors/unknown_kid.jwt");

struct Fake {
    /// The token to hand back, or an OAuth error to refuse with.
    answer: Mutex<Result<&'static str, String>>,
    /// How many times the key set has been fetched, to prove the refresh path.
    jwks_fetches: Mutex<u32>,
    /// The form fields of the last token request, for assertions.
    last_request: Mutex<HashMap<String, String>>,
}

/// Starts the stand-in provider and returns a client pointed at it.
async fn provider_serving(answer: Result<&'static str, String>) -> (OidcClient, Arc<Fake>) {
    let fake = Arc::new(Fake {
        answer: Mutex::new(answer),
        jwks_fetches: Mutex::new(0),
        last_request: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/certs", get(serve_jwks))
        .route("/token", post(serve_token))
        .with_state(fake.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let provider = Provider {
        name: "google",
        authorize_endpoint: format!("http://127.0.0.1:{port}/auth"),
        token_endpoint: format!("http://127.0.0.1:{port}/token"),
        jwks_uri: format!("http://127.0.0.1:{port}/certs"),
        issuers: vec![ISSUER.to_owned()],
        client_id: CLIENT_ID.to_owned(),
        client_secret: CLIENT_SECRET.to_owned(),
        redirect_uri: "https://typing.test/auth/google/callback".to_owned(),
    };
    (OidcClient::new(provider), fake)
}

async fn serve_jwks(State(fake): State<Arc<Fake>>) -> Json<serde_json::Value> {
    *fake.jwks_fetches.lock().expect("lock") += 1;
    Json(json!({
        "keys": [{
            "kty": "RSA", "use": "sig", "alg": "RS256",
            "kid": KEY_ID, "n": MODULUS.trim(), "e": "AQAB",
        }]
    }))
}

async fn serve_token(
    State(fake): State<Arc<Fake>>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    *fake.last_request.lock().expect("lock") = form;
    match fake.answer.lock().expect("lock").clone() {
        Ok(token) => Json(json!({"id_token": token.trim(), "token_type": "Bearer"})),
        Err(why) => Json(json!({"error": "invalid_grant", "error_description": why})),
    }
}

/// A login whose nonce matches the one baked into the vectors.
fn login() -> PendingLogin {
    PendingLogin {
        nonce: NONCE.to_owned(),
        ..PendingLogin::new()
    }
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_real_sign_in_succeeds_and_yields_only_a_subject() {
    let pending = login();
    let (client, fake) = provider_serving(Ok(VALID)).await;

    let claims = client
        .complete_login(&pending, &pending.state, "the-code")
        .await
        .expect("sign-in succeeds");

    assert_eq!(claims.sub, "subject-108121");
    assert_eq!(claims.iss, ISSUER);
    assert_eq!(claims.aud, CLIENT_ID);

    // The token request must carry the secret and the PKCE verifier, and the
    // verifier must be the one the challenge was derived from.
    let request = fake.last_request.lock().expect("lock").clone();
    assert_eq!(
        request.get("grant_type").map(String::as_str),
        Some("authorization_code")
    );
    assert_eq!(request.get("code").map(String::as_str), Some("the-code"));
    assert_eq!(
        request.get("client_secret").map(String::as_str),
        Some(CLIENT_SECRET)
    );
    assert_eq!(request.get("code_verifier"), Some(&pending.verifier));

    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(request["code_verifier"].as_bytes()));
    assert_eq!(
        challenge,
        pending.challenge(),
        "verifier must match the challenge sent"
    );
}

#[tokio::test]
async fn a_replayed_token_for_another_request_is_refused() {
    let pending = login();
    let (client, _) = provider_serving(Ok(WRONG_NONCE)).await;
    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert_eq!(verdict.unwrap_err(), AuthError::NonceMismatch);
}

#[tokio::test]
async fn a_token_with_no_nonce_at_all_is_refused() {
    let pending = login();
    let (client, _) = provider_serving(Ok(NO_NONCE)).await;
    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert_eq!(verdict.unwrap_err(), AuthError::NonceMissing);
}

#[tokio::test]
async fn a_token_minted_for_another_client_is_refused() {
    let pending = login();
    let (client, _) = provider_serving(Ok(WRONG_AUDIENCE)).await;
    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert!(
        matches!(verdict, Err(AuthError::InvalidIdToken(_))),
        "expected the audience to be rejected, got {verdict:?}"
    );
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused() {
    let pending = login();
    let (client, _) = provider_serving(Ok(WRONG_ISSUER)).await;
    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert!(
        matches!(verdict, Err(AuthError::InvalidIdToken(_))),
        "expected the issuer to be rejected, got {verdict:?}"
    );
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let pending = login();
    let (client, _) = provider_serving(Ok(EXPIRED)).await;
    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert!(
        matches!(verdict, Err(AuthError::InvalidIdToken(_))),
        "expected expiry to be rejected, got {verdict:?}"
    );
}

#[tokio::test]
async fn a_token_signed_by_an_unpublished_key_is_refused_after_one_refresh() {
    let pending = login();
    let (client, fake) = provider_serving(Ok(UNKNOWN_KID)).await;

    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert_eq!(verdict.unwrap_err(), AuthError::UnknownSigningKey);

    // An unknown key id should prompt exactly one refetch, in case the provider
    // rotated its keys — and then give up rather than loop.
    assert_eq!(
        *fake.jwks_fetches.lock().expect("lock"),
        2,
        "expected the initial fetch plus one refresh"
    );
}

#[tokio::test]
async fn a_mismatched_state_never_reaches_the_token_endpoint() {
    let pending = login();
    let (client, fake) = provider_serving(Ok(VALID)).await;

    let verdict = client
        .complete_login(&pending, "not-the-state-we-issued", "the-code")
        .await;
    assert_eq!(verdict.unwrap_err(), AuthError::StateMismatch);
    assert!(
        fake.last_request.lock().expect("lock").is_empty(),
        "the code must not be redeemed when state does not match"
    );
}

#[tokio::test]
async fn a_provider_error_is_surfaced_not_swallowed() {
    let pending = login();
    let (client, _) = provider_serving(Err("code already redeemed".to_owned())).await;
    let verdict = client
        .complete_login(&pending, &pending.state, "stale-code")
        .await;
    assert_eq!(
        verdict.unwrap_err(),
        AuthError::ProviderRefused("code already redeemed".into())
    );
}

#[tokio::test]
async fn the_key_set_is_cached_rather_than_fetched_per_token() {
    let pending = login();
    let (client, fake) = provider_serving(Ok(VALID)).await;
    for _ in 0..3 {
        client
            .complete_login(&pending, &pending.state, "the-code")
            .await
            .expect("sign-in succeeds");
    }
    assert_eq!(
        *fake.jwks_fetches.lock().expect("lock"),
        1,
        "the key set should be fetched once and reused"
    );
}

#[tokio::test]
async fn the_authorize_url_requests_only_openid() {
    let (client, _) = provider_serving(Ok(VALID)).await;
    let url = authorize_url(client.provider(), &login());
    assert!(url.contains("scope=openid"));
    assert!(
        !url.contains("email"),
        "an email scope would collect more than needed"
    );
    assert!(!url.contains("profile"));
    let _ = now_secs();
}
