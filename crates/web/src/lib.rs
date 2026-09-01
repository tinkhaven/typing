//! Tinkhaven Typing — a web port of the Klavaro touch typing tutor.
//!
//! The crate compiles twice. With `hydrate` it becomes the WebAssembly client,
//! which owns the typing loop. With `ssr` it becomes the Axum server, which
//! renders the shell, serves practice text, and scores sessions for the
//! leaderboard. [`crate::protocol`] is the seam between the two, and
//! [`typing_core`] holds everything both halves agree on.

// Leptos view types nest deeply enough that computing the layout of the top-level
// view exceeds rustc's default query depth. `cargo leptos` mitigates this with
// `--cfg erase_components`, but the limit still has to be raised for the release
// profile and for a plain `cargo build`.
#![recursion_limit = "512"]
#![forbid(unsafe_code)]

pub mod about;
pub mod board;
pub mod i18n;
pub mod keyboard;
pub mod practice;
pub mod protocol;
pub mod settings;
pub mod socket;

#[cfg(feature = "ssr")]
pub mod server;

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes, A},
    StaticSegment,
};

use crate::{
    about::About,
    i18n::{Locale, Msg},
    practice::Practice,
    settings::Settings,
};

/// The HTML document that wraps the application.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta
                    name="description"
                    content="Learn to touch type in your browser. A web port of the Klavaro typing tutor."
                />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The application: a header of pickers, and one of two pages.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let settings = Settings::default();
    provide_context(settings);

    view! {
        <Stylesheet id="leptos" href="/pkg/typing.css" />
        <Title text="Tinkhaven Typing" />
        <Router>
            <Header />
            <main>
                <Routes fallback=|| view! { <NotFound /> }>
                    <Route path=StaticSegment("") view=Practice />
                    <Route path=StaticSegment("about") view=About />
                </Routes>
            </main>
            <Footer />
        </Router>
    }
}

/// Name, and the pickers that decide what an exercise looks like.
#[component]
fn Header() -> impl IntoView {
    let settings = expect_context::<Settings>();
    let locale = move || settings.locale.get();

    view! {
        <header class="site-header">
            <A href="/" attr:class="brand">
                <span class="brand-name">{move || locale().text(Msg::AppName)}</span>
                <span class="brand-tagline">{move || locale().text(Msg::Tagline)}</span>
            </A>

            <div class="pickers">
                <label>
                    <span>{move || locale().text(Msg::Keyboard)}</span>
                    <select on:change=move |ev| settings.layout_name.set(event_target_value(&ev))>
                        {typing_core::layout_names()
                            .map(|name| {
                                view! {
                                    <option
                                        value=name
                                        selected=move || settings.layout_name.get() == name
                                    >
                                        {pretty_layout(name)}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>

                <label>
                    <span>{move || locale().text(Msg::InterfaceLanguage)}</span>
                    <select on:change=move |ev| {
                        if let Some(chosen) = Locale::from_code(&event_target_value(&ev)) {
                            settings.choose_locale(chosen);
                        }
                    }>
                        {Locale::ALL
                            .into_iter()
                            .map(|option| {
                                view! {
                                    <option
                                        value=option.code()
                                        selected=move || settings.locale.get() == option
                                    >
                                        {option.endonym()}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
            </div>
        </header>
    }
}

/// Attribution, always visible — it is a GPL obligation, not a nicety.
#[component]
fn Footer() -> impl IntoView {
    let settings = expect_context::<Settings>();
    view! {
        <footer class="site-footer">
            <p>
                "A web port of "
                <a href="https://klavaro.sourceforge.io/" rel="noreferrer">"Klavaro"</a>
                " by Felipe Emmanuel Ferreira de Castro. GPL-3.0-or-later. "
                <a href="https://github.com/tinkhaven/typing" rel="noreferrer">"Source"</a>
                " · "
                <A href="/about">
                    {move || settings.locale.get().text(Msg::About)}
                </A>
            </p>
        </footer>
    }
}

/// Shown for an unknown path.
#[component]
fn NotFound() -> impl IntoView {
    // Tell the client that this really was a 404, so a crawler does not index it.
    #[cfg(feature = "ssr")]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(axum::http::StatusCode::NOT_FOUND);
    }

    view! {
        <section class="prose">
            <h1>"Not found"</h1>
            <p>
                "There is nothing here. "
                <A href="/">"Back to practising"</A>"."
            </p>
        </section>
    }
}

/// Turns a layout file name into something readable: `azerty_be` → `AZERTY be`.
fn pretty_layout(name: &str) -> String {
    let mut parts = name.split('_');
    let family = parts.next().unwrap_or(name);
    let rest: Vec<&str> = parts.collect();
    let family = match family {
        "qwerty" | "azerty" | "qwertz" | "dvorak" | "colemak" | "workman" | "norman" => {
            family.to_uppercase()
        }
        other => capitalise(other),
    };
    if rest.is_empty() {
        family
    } else {
        format!("{family} {}", rest.join(" "))
    }
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The WebAssembly entry point.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}

#[cfg(test)]
mod tests {
    use super::pretty_layout;

    #[test]
    fn layout_names_become_readable() {
        assert_eq!(pretty_layout("qwerty_us"), "QWERTY us");
        assert_eq!(pretty_layout("azerty_be"), "AZERTY be");
        assert_eq!(pretty_layout("dvorak_fr_bepo"), "DVORAK fr bepo");
        assert_eq!(pretty_layout("jtsuken_ru"), "Jtsuken ru");
        assert_eq!(pretty_layout("qwerty"), "QWERTY");
    }

    #[test]
    fn every_bundled_layout_gets_a_non_empty_label() {
        for name in typing_core::layout_names() {
            assert!(!pretty_layout(name).is_empty(), "{name}");
        }
    }
}
