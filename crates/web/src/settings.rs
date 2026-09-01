//! What the visitor has chosen, and remembering it between visits.
//!
//! Progress and preferences live in the browser's `localStorage`, not on the
//! server: there is no account to attach them to, and a typing tutor does not
//! need one. That keeps the hosted version free of cookies, sign-ups and any
//! personal data beyond a nickname the visitor types when they choose to publish
//! a score.

use leptos::prelude::*;
use typing_core::{goals::Module, DEFAULT_LAYOUT};

use crate::i18n::Locale;

/// Key that preferences are stored under.
const STORAGE_KEY: &str = "tinkhaven-typing";

/// The choices that drive an exercise.
#[derive(Clone, Copy)]
pub struct Settings {
    /// Interface language.
    pub locale: RwSignal<Locale>,
    /// Which module is being practised.
    pub module: RwSignal<Module>,
    /// Keyboard layout name.
    pub layout_name: RwSignal<String>,
    /// Corpus language for practice text.
    pub corpus_language: RwSignal<String>,
    /// Current Basic lesson, 1-based.
    pub lesson: RwSignal<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            locale: RwSignal::new(Locale::En),
            module: RwSignal::new(Module::Basic),
            layout_name: RwSignal::new(DEFAULT_LAYOUT.to_owned()),
            corpus_language: RwSignal::new("en_GB".to_owned()),
            lesson: RwSignal::new(1),
        }
    }
}

impl Settings {
    /// Applies a locale, moving the layout and practice text with it.
    ///
    /// Someone switching the interface to Dutch almost certainly wants Dutch
    /// practice text on a Belgian keyboard, so the three move together — and each
    /// can still be overridden afterwards.
    pub fn choose_locale(&self, locale: Locale) {
        self.locale.set(locale);
        self.layout_name.set(locale.likely_layout().to_owned());
        self.corpus_language.set(locale.default_corpus().to_owned());
    }

    /// Reads saved preferences, falling back to defaults for anything missing.
    ///
    /// Storage can be unavailable (private browsing, blocked site data) or hold
    /// something written by an older version, so every field is validated rather
    /// than trusted. A layout name that no longer exists would otherwise leave
    /// the keyboard blank.
    pub fn restore(&self) {
        let Some(raw) = read_storage(STORAGE_KEY) else { return };
        let Ok(saved) = serde_json::from_str::<Saved>(&raw) else { return };

        if let Some(locale) = saved.locale.as_deref().and_then(Locale::from_code) {
            self.locale.set(locale);
        }
        if let Some(module) = saved.module.as_deref().and_then(Module::from_slug) {
            self.module.set(module);
        }
        if let Some(layout) = saved
            .layout
            .filter(|name| typing_core::load_layout(name).is_some())
        {
            self.layout_name.set(layout);
        }
        if let Some(language) = saved.language.filter(|l| !l.is_empty()) {
            self.corpus_language.set(language);
        }
        if let Some(lesson) = saved.lesson.filter(|n| (1..=43).contains(n)) {
            self.lesson.set(lesson);
        }
    }

    /// Writes the current choices to storage. Failures are ignored on purpose.
    pub fn persist(&self) {
        let saved = Saved {
            locale: Some(self.locale.get().code().to_owned()),
            module: Some(self.module.get().slug().to_owned()),
            layout: Some(self.layout_name.get()),
            language: Some(self.corpus_language.get()),
            lesson: Some(self.lesson.get()),
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            write_storage(STORAGE_KEY, &json);
        }
    }
}

/// The stored shape. Every field optional so an older save still loads.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Saved {
    locale: Option<String>,
    module: Option<String>,
    layout: Option<String>,
    language: Option<String>,
    lesson: Option<u32>,
}

#[cfg(feature = "hydrate")]
fn read_storage(key: &str) -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()?
}

#[cfg(feature = "hydrate")]
fn write_storage(key: &str, value: &str) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(key, value);
    }
}

#[cfg(not(feature = "hydrate"))]
fn read_storage(_key: &str) -> Option<String> {
    None
}

#[cfg(not(feature = "hydrate"))]
fn write_storage(_key: &str, _value: &str) {}
