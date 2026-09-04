//! Signing in, and carrying progress between devices.
//!
//! Signing in is strictly additive. Progress is recorded in local storage
//! whether or not anybody is signed in, and the server is a place to *sync* it
//! to — so practising works offline, with no account, and a failed sync costs
//! nothing but the carry-over. That ordering is why every call in here is
//! allowed to fail quietly.

use leptos::prelude::*;

use crate::{
    i18n::Msg,
    settings::{ProgressStore, Settings},
};

/// Whether sign-in is offered, and whether this visitor has used it.
#[derive(Clone, Copy)]
pub struct AccountState {
    /// Whether the deployment has sign-in configured at all.
    pub available: RwSignal<bool>,
    /// Whether this browser holds a valid session.
    pub signed_in: RwSignal<bool>,
    /// Set while a sync is in flight, to keep the UI honest.
    pub syncing: RwSignal<bool>,
}

impl Default for AccountState {
    fn default() -> Self {
        AccountState {
            available: RwSignal::new(false),
            signed_in: RwSignal::new(false),
            syncing: RwSignal::new(false),
        }
    }
}

/// The account area in the header.
#[component]
pub fn AccountBar() -> impl IntoView {
    let settings = expect_context::<Settings>();
    let account = expect_context::<AccountState>();
    let progress = expect_context::<ProgressStore>();
    let locale = move || settings.locale.get();

    // Ask the server what it offers, then sync if we are already signed in.
    Effect::new(move |_| {
        load_state(account, progress);
    });

    view! {
        {move || {
            if !account.available.get() {
                // Nothing to show on a deployment without sign-in configured.
                return None;
            }
            Some(
                if account.signed_in.get() {
                    view! {
                        <div class="account">
                            <span class="account-state" title="Signed in with Google">
                                {move || locale().text(Msg::SignedIn)}
                            </span>
                            <form method="post" action="/auth/signout">
                                <button type="submit" class="link-button">
                                    {move || locale().text(Msg::SignOut)}
                                </button>
                            </form>
                            <button
                                class="link-button link-button-danger"
                                on:click=move |_| forget_everything(account, progress)
                            >
                                {move || locale().text(Msg::DeleteMyData)}
                            </button>
                        </div>
                    }
                        .into_any()
                } else {
                    view! {
                        <div class="account">
                            <a class="button" href="/auth/google/start" rel="nofollow">
                                {move || locale().text(Msg::SignInWithGoogle)}
                            </a>
                        </div>
                    }
                        .into_any()
                },
            )
        }}
    }
}

/// Pushes local progress to the server and adopts whatever comes back merged.
///
/// Called after a finished exercise. Silent when nobody is signed in, and
/// silent on failure — a sync problem must never interrupt practising.
pub fn sync_now(account: AccountState, progress: ProgressStore) {
    if !account.signed_in.get_untracked() || account.syncing.get_untracked() {
        return;
    }
    push_progress(account, progress);
}

#[cfg(feature = "hydrate")]
fn load_state(account: AccountState, progress: ProgressStore) {
    use gloo_net::http::Request;
    leptos::task::spawn_local(async move {
        let Ok(response) = Request::get("/api/me").send().await else {
            return;
        };
        let Ok(me) = response.json::<crate::account::Me>().await else {
            return;
        };
        account.available.set(me.available);
        account.signed_in.set(me.signed_in);
        if me.signed_in {
            // Just signed in, or returning: reconcile the two records at once
            // rather than waiting for the next finished exercise.
            push_progress(account, progress);
        }
    });
}

/// Mirror of the server's `/api/me` response.
#[derive(serde::Deserialize)]
pub struct Me {
    /// Whether the deployment offers sign-in.
    pub available: bool,
    /// Whether this request carried a valid session.
    pub signed_in: bool,
}

#[cfg(feature = "hydrate")]
fn push_progress(account: AccountState, progress: ProgressStore) {
    use gloo_net::http::Request;

    use crate::settings::Progress;

    account.syncing.set(true);
    let local = progress.data.get_untracked();
    leptos::task::spawn_local(async move {
        let merged = async {
            let response = Request::put("/api/profile")
                .json(&local)
                .ok()?
                .send()
                .await
                .ok()?;
            if response.status() == 401 {
                account.signed_in.set(false);
                return None;
            }
            response.json::<Progress>().await.ok()
        }
        .await;

        if let Some(merged) = merged {
            // Adopt the merged record so both sides agree, and keep it locally
            // so the next visit starts from it even offline.
            progress.data.set(merged);
            progress.persist();
        }
        account.syncing.set(false);
    });
}

/// Deletes the stored profile, signs out, and clears this browser.
#[cfg(feature = "hydrate")]
fn forget_everything(account: AccountState, progress: ProgressStore) {
    use gloo_net::http::Request;
    leptos::task::spawn_local(async move {
        let _ = Request::delete("/api/profile").send().await;
        account.signed_in.set(false);
        progress.clear();
    });
}

#[cfg(not(feature = "hydrate"))]
fn load_state(_account: AccountState, _progress: ProgressStore) {}

#[cfg(not(feature = "hydrate"))]
fn push_progress(_account: AccountState, _progress: ProgressStore) {}

#[cfg(not(feature = "hydrate"))]
fn forget_everything(_account: AccountState, _progress: ProgressStore) {}
