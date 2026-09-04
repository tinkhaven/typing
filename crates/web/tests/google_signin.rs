//! The Google sign-in flow, end to end, against a stand-in provider.
//!
//! The unit tests in `server::auth` cover every decision that can be made
//! without a network: PKCE, state, nonce, cookie hardening, user-id derivation.
//! What they cannot cover is whether the pieces fit together — whether the token
//! request has the shape a provider expects, whether a real RS256 signature is
//! actually checked against a fetched key set, and whether a bad token is
//! refused rather than shrugged off.
//!
//! So this stands up a minimal OpenID Connect provider on a local port, with a
//! freshly generated RSA key, and runs the real client against it. The key is
//! generated per test run rather than committed: a private key in a public
//! repository would be wrong even as a fixture.
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
use rsa::{pkcs1::EncodeRsaPrivateKey, traits::PublicKeyParts, RsaPrivateKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use typing_web::server::{
    auth::{authorize_url, AuthError, OidcClient, PendingLogin, Provider},
    session::now_secs,
};

const CLIENT_ID: &str = "test-client.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "test-client-secret";
const ISSUER: &str = "https://accounts.google.test";
const KEY_ID: &str = "test-key-1";

/// How the stand-in provider should behave, so one server covers every case.
#[derive(Clone, Default)]
struct Behaviour {
    /// Issue a token whose nonce is this instead of the requested one.
    override_nonce: Option<String>,
    /// Issue a token for this audience instead of our client id.
    override_audience: Option<String>,
    /// Issue a token that expired this long ago.
    expired_by: Option<u64>,
    /// Sign with a `kid` the published key set does not contain.
    unknown_kid: bool,
    /// Refuse the exchange with an OAuth error.
    refuse: Option<String>,
}

struct Fake {
    private_pem: String,
    modulus: String,
    exponent: String,
    behaviour: Mutex<Behaviour>,
    /// The form fields of the last token request, for assertions.
    last_request: Mutex<HashMap<String, String>>,
}

/// One RSA key for the whole test binary: generating 2048 bits is slow.
fn key() -> &'static (String, String, String) {
    static KEY: std::sync::OnceLock<(String, String, String)> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        // rsa 0.9 is built against rand_core 0.6 while the crate itself uses
        // rand 0.9, so the RNG has to come from rsa's own re-export or the trait
        // bounds refer to two different rand_cores.
        let mut rng = rsa::rand_core::OsRng;
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("generate an RSA key");
        let pem = private
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .expect("PEM")
            .to_string();
        let modulus = URL_SAFE_NO_PAD.encode(private.n().to_bytes_be());
        let exponent = URL_SAFE_NO_PAD.encode(private.e().to_bytes_be());
        (pem, modulus, exponent)
    })
}

/// Starts the stand-in provider and returns a client pointed at it.
async fn provider_with(behaviour: Behaviour) -> (OidcClient, Arc<Fake>) {
    let (pem, modulus, exponent) = key().clone();
    let fake = Arc::new(Fake {
        private_pem: pem,
        modulus,
        exponent,
        behaviour: Mutex::new(behaviour),
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
    Json(json!({
        "keys": [{
            "kty": "RSA", "use": "sig", "alg": "RS256",
            "kid": KEY_ID, "n": fake.modulus, "e": fake.exponent,
        }]
    }))
}

async fn serve_token(
    State(fake): State<Arc<Fake>>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let behaviour = fake.behaviour.lock().expect("lock").clone();
    *fake.last_request.lock().expect("lock") = form.clone();

    if let Some(error) = behaviour.refuse {
        return Json(json!({"error": "invalid_grant", "error_description": error}));
    }

    let now = now_secs();
    let claims = json!({
        "iss": ISSUER,
        "sub": "subject-108121",
        "aud": behaviour.override_audience.unwrap_or_else(|| CLIENT_ID.to_owned()),
        "exp": match behaviour.expired_by { Some(ago) => now - ago, None => now + 300 },
        "iat": now,
        "nonce": behaviour
            .override_nonce
            .unwrap_or_else(|| form.get("nonce").cloned().unwrap_or_default()),
    });

    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(if behaviour.unknown_kid {
        "rotated-away".into()
    } else {
        KEY_ID.into()
    });
    let token = jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_rsa_pem(fake.private_pem.as_bytes()).expect("key"),
    )
    .expect("sign");

    Json(json!({"id_token": token, "token_type": "Bearer"}))
}

/// The provider echoes the nonce back through the token request, the way a real
/// one carries it from the authorize step.
fn login_with_nonce(pending: &PendingLogin) -> PendingLogin {
    pending.clone()
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_real_sign_in_succeeds_and_yields_only_a_subject() {
    let pending = PendingLogin::new();
    let (client, fake) = provider_with(Behaviour::default()).await;

    // The stand-in echoes back the nonce it is told to, as a real provider
    // carries it from the authorize request. Set in its own scope so the lock is
    // demonstrably released before anything is awaited.
    {
        fake.behaviour.lock().expect("lock").override_nonce = Some(pending.nonce.clone());
    }
    let claims = client
        .complete_login(&login_with_nonce(&pending), &pending.state, "the-code")
        .await
        .expect("sign-in succeeds");

    assert_eq!(claims.sub, "subject-108121");
    assert_eq!(claims.iss, ISSUER);
    assert_eq!(claims.aud, CLIENT_ID);

    // The token request must carry the PKCE verifier and the secret, and the
    // verifier must be the one the challenge was derived from.
    let request = fake.last_request.lock().expect("lock").clone();
    assert_eq!(
        request.get("grant_type").map(String::as_str),
        Some("authorization_code")
    );
    assert_eq!(request.get("code").map(String::as_str), Some("the-code"));
    assert_eq!(request.get("code_verifier"), Some(&pending.verifier));
    assert_eq!(
        request.get("client_secret").map(String::as_str),
        Some(CLIENT_SECRET)
    );

    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(
        request.get("code_verifier").expect("verifier").as_bytes(),
    ));
    assert_eq!(
        challenge,
        pending.challenge(),
        "the verifier must match the challenge sent"
    );
}

#[tokio::test]
async fn a_replayed_token_for_another_request_is_refused() {
    let pending = PendingLogin::new();
    let (client, fake) = provider_with(Behaviour {
        // A token that is perfectly valid, but minted for a different request.
        override_nonce: Some(PendingLogin::new().nonce),
        ..Behaviour::default()
    })
    .await;
    let _ = &fake;

    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert_eq!(verdict.unwrap_err(), AuthError::NonceMismatch);
}

#[tokio::test]
async fn a_token_minted_for_another_client_is_refused() {
    let pending = PendingLogin::new();
    let (client, fake) = provider_with(Behaviour {
        override_nonce: Some(pending.nonce.clone()),
        override_audience: Some("someone-elses-client.apps.googleusercontent.com".into()),
        ..Behaviour::default()
    })
    .await;
    let _ = &fake;

    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert!(
        matches!(verdict, Err(AuthError::InvalidIdToken(_))),
        "expected the audience to be rejected, got {verdict:?}"
    );
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let pending = PendingLogin::new();
    let (client, _) = provider_with(Behaviour {
        override_nonce: Some(pending.nonce.clone()),
        // Well past the 60s leeway.
        expired_by: Some(3_600),
        ..Behaviour::default()
    })
    .await;

    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert!(
        matches!(verdict, Err(AuthError::InvalidIdToken(_))),
        "expected expiry to be rejected, got {verdict:?}"
    );
}

#[tokio::test]
async fn a_token_signed_by_an_unpublished_key_is_refused() {
    let pending = PendingLogin::new();
    let (client, _) = provider_with(Behaviour {
        override_nonce: Some(pending.nonce.clone()),
        unknown_kid: true,
        ..Behaviour::default()
    })
    .await;

    let verdict = client
        .complete_login(&pending, &pending.state, "the-code")
        .await;
    assert_eq!(verdict.unwrap_err(), AuthError::UnknownSigningKey);
}

#[tokio::test]
async fn a_mismatched_state_never_reaches_the_token_endpoint() {
    let pending = PendingLogin::new();
    let (client, fake) = provider_with(Behaviour::default()).await;

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
    let pending = PendingLogin::new();
    let (client, _) = provider_with(Behaviour {
        refuse: Some("code already redeemed".into()),
        ..Behaviour::default()
    })
    .await;

    let verdict = client
        .complete_login(&pending, &pending.state, "stale-code")
        .await;
    assert_eq!(
        verdict.unwrap_err(),
        AuthError::ProviderRefused("code already redeemed".into())
    );
}

#[tokio::test]
async fn the_authorize_url_requests_only_openid() {
    let (client, _) = provider_with(Behaviour::default()).await;
    let url = authorize_url(client.provider(), &PendingLogin::new());
    assert!(url.contains("scope=openid"));
    assert!(
        !url.contains("email"),
        "an email scope would collect more than needed"
    );
    assert!(!url.contains("profile"));
}
