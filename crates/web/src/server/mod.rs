//! The server half: shared state, the WebSocket endpoint, and the leaderboard.
//!
//! Only compiled with the `ssr` feature; the WASM client never sees any of it.

pub mod accounts;
pub mod auth;
pub mod corpus;
pub mod jwt;
pub mod leaderboard;
pub mod profiles;
pub mod routes;
pub mod session;
pub mod verify;
pub mod ws;

use std::sync::Arc;

use axum::extract::FromRef;
use leptos::config::LeptosOptions;
use tokio::sync::broadcast;
use typing_core::{goals::Module, lesson::Lesson};

use accounts::Accounts;
use corpus::Corpora;
use leaderboard::Leaderboard;
use profiles::Profiles;

/// How many board notifications are buffered per subscriber.
const BOARD_CHANNEL_CAPACITY: usize = 16;

/// A board changed and anyone following it should refresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardChanged {
    /// Which module's board.
    pub module: Module,
    /// Which language's board.
    pub language: String,
}

/// Everything a request handler needs, built once at startup.
///
/// A newtype over `Arc` rather than a bare `Arc<Shared>` passed around, because
/// `LeptosOptions` has to be extractable from the router state and the orphan
/// rule will not allow a foreign trait to be implemented for a foreign type via
/// a foreign wrapper: `Arc<Shared>` is not a local type, `AppState` is. `Deref`
/// keeps field access unchanged at every call site.
#[derive(Clone)]
pub struct AppState(Arc<Shared>);

impl std::ops::Deref for AppState {
    type Target = Shared;

    fn deref(&self) -> &Shared {
        &self.0
    }
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}

/// The state itself, behind the [`AppState`] handle.
pub struct Shared {
    /// Practice text, by language.
    pub corpora: Corpora,
    /// The 43 Basic lessons.
    pub lessons: Vec<Lesson>,
    /// Where results are recorded.
    pub leaderboard: Leaderboard,
    /// Broadcasts board changes to connected clients.
    pub board_changes: broadcast::Sender<BoardChanged>,
    /// Leptos build configuration, for the rendering handler.
    pub leptos_options: LeptosOptions,
    /// Sign-in configuration, or `None` when it is switched off.
    pub accounts: Option<Accounts>,
    /// Where signed-in visitors' progress is kept.
    pub profiles: Profiles,
}

impl AppState {
    /// Builds the shared state.
    pub async fn new(leptos_options: LeptosOptions) -> std::io::Result<AppState> {
        let corpora = Corpora::from_env()?;
        tracing::info!(languages = corpora.len(), "loaded practice text");
        let (board_changes, _) = broadcast::channel(BOARD_CHANNEL_CAPACITY);
        Ok(AppState(Arc::new(Shared {
            corpora,
            lessons: typing_core::lesson::klavaro_lessons(),
            leaderboard: Leaderboard::from_env().await,
            board_changes,
            leptos_options,
            accounts: Accounts::from_env(),
            profiles: Profiles::from_env().await,
        })))
    }
}
