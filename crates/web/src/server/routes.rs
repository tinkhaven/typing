//! The sign-in and profile endpoints.
//!
//! Progress lives in the visitor's browser and is *synced* here, rather than
//! being owned here. That ordering is deliberate: practising works with no
//! account and no connection, and signing in adds cross-device carry-over
//! without becoming a dependency. See [`crate::settings::Progress::merge`] for
//! how the two sides are reconciled.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    server::{
        auth::{authorize_url, AuthError, PendingLogin},
        session::{self, Session, SigningKey},
        AppState,
    },
    settings::Progress,
};

/// What the callback receives from the provider.
#[derive(Debug, Deserialize)]
pub struct Callback {
    /// The authorization code, on success.
    pub code: Option<String>,
    /// Echoed back from the authorize request.
    pub state: Option<String>,
    /// Set instead of `code` when the visitor declined or something failed.
    pub error: Option<String>,
}

/// Whether anybody is signed in, for the client to render itself with.
#[derive(Debug, Serialize)]
pub struct Me {
    /// Whether this deployment offers sign-in at all.
    pub available: bool,
    /// Whether this request carries a valid session.
    pub signed_in: bool,
}

/// Reads and verifies the session on a request, if there is one.
fn current_session(state: &AppState, headers: &HeaderMap) -> Option<Session> {
    let key: &SigningKey = &state.accounts.as_ref()?.session_key;
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    let raw = session::read_cookie(cookies, session::COOKIE_NAME)?;
    match Session::decode(&raw, key, session::now_secs()) {
        Ok(session) => Some(session),
        Err(why) => {
            // Expected: cookies expire and keys get rotated. Not worth an error
            // level, but worth being able to see.
            tracing::debug!(%why, "ignoring a session cookie");
            None
        }
    }
}

/// `GET /api/me`
pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Json<Me> {
    Json(Me {
        available: state.accounts.is_some(),
        signed_in: current_session(&state, &headers).is_some(),
    })
}

/// `GET /auth/google/start` — begin a sign-in.
pub async fn google_start(State(state): State<AppState>) -> Response {
    let Some(accounts) = state.accounts.as_ref() else {
        return problem(StatusCode::NOT_IMPLEMENTED, AuthError::NotConfigured);
    };

    let pending = PendingLogin::new();
    let destination = authorize_url(accounts.google.provider(), &pending);

    // The one-time values ride along in a signed cookie, so the callback can be
    // served by any task without shared state.
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, destination),
            (
                header::SET_COOKIE,
                pending.set_cookie(&accounts.session_key),
            ),
            // Never let a redirect carrying login state be cached.
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
    )
        .into_response()
}

/// `GET /auth/google/callback` — finish a sign-in.
pub async fn google_callback(
    State(state): State<AppState>,
    Query(callback): Query<Callback>,
    headers: HeaderMap,
) -> Response {
    let Some(accounts) = state.accounts.as_ref() else {
        return problem(StatusCode::NOT_IMPLEMENTED, AuthError::NotConfigured);
    };

    if let Some(error) = callback.error {
        // Most often the visitor pressed "cancel". Not an error to shout about.
        tracing::info!(%error, "sign-in was not completed");
        return back_to_start("/?signin=declined");
    }

    let outcome = complete(&state, accounts, &callback, &headers).await;
    match outcome {
        Ok(user) => {
            let session = Session::new(user);
            (
                StatusCode::SEE_OTHER,
                [
                    (header::LOCATION, "/?signin=ok".to_owned()),
                    (
                        header::SET_COOKIE,
                        session::set_cookie(&session, &accounts.session_key),
                    ),
                    (header::CACHE_CONTROL, "no-store".to_owned()),
                ],
                // The login cookie has served its purpose; drop it immediately so
                // a code cannot be redeemed twice.
                [(header::SET_COOKIE, PendingLogin::clear_cookie())],
            )
                .into_response()
        }
        Err(why) => {
            tracing::warn!(%why, "sign-in failed");
            back_to_start("/?signin=failed")
        }
    }
}

/// The verification steps, separated so the handler stays about HTTP.
async fn complete(
    _state: &AppState,
    accounts: &crate::server::accounts::Accounts,
    callback: &Callback,
    headers: &HeaderMap,
) -> Result<String, AuthError> {
    let cookies = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::PendingMissing)?;
    let raw = session::read_cookie(cookies, crate::server::auth::PENDING_COOKIE)
        .ok_or(AuthError::PendingMissing)?;
    let pending = PendingLogin::decode(&raw, &accounts.session_key, session::now_secs())?;

    let code = callback.code.as_deref().ok_or(AuthError::NoIdToken)?;
    let returned_state = callback.state.as_deref().unwrap_or_default();

    let claims = accounts
        .google
        .complete_login(&pending, returned_state, code)
        .await?;
    Ok(crate::server::auth::derive_user_id(
        &accounts.pepper,
        &claims.iss,
        &claims.sub,
    ))
}

/// `POST /auth/signout`
pub async fn signout() -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_owned()),
            (header::SET_COOKIE, session::clear_cookie()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
    )
        .into_response()
}

/// `GET /api/profile` — the signed-in visitor's stored progress.
pub async fn load_profile(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = current_session(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "not signed in").into_response();
    };
    match state.profiles.load(&session.user).await {
        // Nothing stored yet is not an error: it is a new profile.
        Ok(progress) => Json(progress.unwrap_or_default()).into_response(),
        Err(why) => {
            tracing::error!(%why, "could not read a profile");
            (StatusCode::BAD_GATEWAY, "the profile store is unavailable").into_response()
        }
    }
}

/// `PUT /api/profile` — merge the browser's progress into the stored profile.
///
/// A merge rather than a replace, so a device that has been offline cannot
/// overwrite a better result recorded elsewhere. The merged record is returned,
/// which is what the browser then adopts.
pub async fn save_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(incoming): Json<Progress>,
) -> Response {
    let Some(session) = current_session(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "not signed in").into_response();
    };

    let stored = match state.profiles.load(&session.user).await {
        Ok(stored) => stored.unwrap_or_default(),
        Err(why) => {
            tracing::error!(%why, "could not read a profile before merging");
            return (StatusCode::BAD_GATEWAY, "the profile store is unavailable").into_response();
        }
    };

    let merged = stored.merge(&incoming);
    if let Err(why) = state.profiles.save(&session.user, &merged).await {
        tracing::error!(%why, "could not write a profile");
        return (StatusCode::BAD_GATEWAY, "the profile store is unavailable").into_response();
    }
    Json(merged).into_response()
}

/// `DELETE /api/profile` — erase everything held about the signed-in visitor.
///
/// Erasure as a button rather than an email. Also signs them out, since leaving
/// a session pointing at a deleted profile only invites confusion.
pub async fn delete_profile(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = current_session(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "not signed in").into_response();
    };
    if let Err(why) = state.profiles.delete(&session.user).await {
        tracing::error!(%why, "could not delete a profile");
        return (StatusCode::BAD_GATEWAY, "the profile store is unavailable").into_response();
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, session::clear_cookie())],
    )
        .into_response()
}

/// Sends the browser back to the app with a marker it can render a message from.
fn back_to_start(location: &str) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location.to_owned()),
            (header::SET_COOKIE, PendingLogin::clear_cookie()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
    )
        .into_response()
}

fn problem(status: StatusCode, why: AuthError) -> Response {
    (status, why.to_string()).into_response()
}
