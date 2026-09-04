//! What the visitor has chosen, and remembering it between visits.
//!
//! Progress and preferences live in the browser's `localStorage`, not on the
//! server: there is no account to attach them to, and a typing tutor does not
//! need one. That keeps the hosted version free of cookies, sign-ups and any
//! personal data beyond a nickname the visitor types when they choose to publish
//! a score.

use std::collections::BTreeMap;

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use typing_core::{goals::Module, stats::Score, DEFAULT_LAYOUT};

use crate::i18n::Locale;

/// Key that preferences are stored under.
const STORAGE_KEY: &str = "tinkhaven-typing";

/// Key that progress is stored under.
///
/// Separate from preferences so that a change to either cannot corrupt the
/// other, and so clearing one is possible without losing the other.
const PROGRESS_KEY: &str = "tinkhaven-typing-progress";

/// Highest Basic lesson.
pub const LAST_LESSON: u32 = 43;

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
        let Some(raw) = read_storage(STORAGE_KEY) else {
            return;
        };
        let Ok(saved) = serde_json::from_str::<Saved>(&raw) else {
            return;
        };

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
        if let Some(lesson) = saved.lesson.filter(|n| (1..=LAST_LESSON).contains(n)) {
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
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(key)
        .ok()?
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

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// The best a visitor has managed in one module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Best {
    /// Words per minute.
    pub speed: f64,
    /// Accuracy percentage.
    pub accuracy: f64,
    /// Fluidness percentage, where the module measures it.
    pub fluidness: Option<f64>,
}

/// What the visitor has achieved, kept in their own browser.
///
/// There is no account to hang this on, and for a typing tutor there does not
/// need to be: the point of tracking progress is to see your own numbers move.
/// Keeping it in `localStorage` means no server-side personal data at all. The
/// cost is that it does not follow you between devices.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// Best run per module, keyed by [`Module::slug`].
    #[serde(default)]
    pub best: BTreeMap<String, Best>,
    /// Highest Basic lesson whose goals have been met.
    #[serde(default)]
    pub lesson_reached: u32,
    /// How many exercises have been finished.
    #[serde(default)]
    pub sessions: u32,
}

impl Progress {
    /// The best run recorded for a module.
    pub fn best_for(&self, module: Module) -> Option<Best> {
        self.best.get(module.slug()).copied()
    }

    /// Folds a finished run in, returning whether it set a personal best.
    ///
    /// "Best" is decided on speed, but only among runs that met the module's
    /// goals — otherwise the record would go to whoever typed fastest while
    /// ignoring accuracy, which is the opposite of the point.
    pub fn record(&mut self, module: Module, score: &Score, lesson: u32) -> bool {
        self.sessions = self.sessions.saturating_add(1);

        let goals_met = module.goals().met_by(score);
        if goals_met && module == Module::Basic {
            self.lesson_reached = self.lesson_reached.max(lesson);
        }
        if !goals_met {
            return false;
        }

        let entry = self.best.entry(module.slug().to_owned()).or_default();
        if score.speed > entry.speed {
            *entry = Best {
                speed: score.speed,
                accuracy: score.accuracy,
                fluidness: score.fluidness,
            };
            return true;
        }
        false
    }
}

impl Progress {
    /// Combines two records of the same person's progress, keeping the better.
    ///
    /// Used when a signed-in visitor's browser and the server disagree — after
    /// practising offline, or on a device that has not synced. Taking the
    /// field-wise maximum means a sync can never lose a personal best, which is
    /// the only outcome that would actually upset someone. Session counts are
    /// maxed rather than added, because both sides may have counted the same
    /// exercise and inflating the total is worse than under-counting it.
    pub fn merge(&self, other: &Progress) -> Progress {
        let mut best = self.best.clone();
        for (module, theirs) in &other.best {
            best.entry(module.clone())
                .and_modify(|ours| {
                    if theirs.speed > ours.speed {
                        *ours = *theirs;
                    }
                })
                .or_insert(*theirs);
        }
        Progress {
            best,
            lesson_reached: self.lesson_reached.max(other.lesson_reached),
            sessions: self.sessions.max(other.sessions),
        }
    }
}

/// Reactive access to [`Progress`], persisted on every change.
#[derive(Clone, Copy)]
pub struct ProgressStore {
    /// The current progress.
    pub data: RwSignal<Progress>,
}

impl Default for ProgressStore {
    fn default() -> Self {
        ProgressStore {
            data: RwSignal::new(Progress::default()),
        }
    }
}

impl ProgressStore {
    /// Loads saved progress, ignoring anything unreadable.
    pub fn restore(&self) {
        let Some(raw) = read_storage(PROGRESS_KEY) else {
            return;
        };
        if let Ok(saved) = serde_json::from_str::<Progress>(&raw) {
            self.data.set(saved);
        }
    }

    /// Records a finished run and saves. Returns whether it was a personal best.
    pub fn record(&self, module: Module, score: &Score, lesson: u32) -> bool {
        let improved = self
            .data
            .try_update(|progress| progress.record(module, score, lesson))
            .unwrap_or(false);
        self.persist();
        improved
    }

    /// Writes progress to storage. Failures are ignored on purpose.
    pub fn persist(&self) {
        if let Ok(json) = serde_json::to_string(&self.data.get_untracked()) {
            write_storage(PROGRESS_KEY, &json);
        }
    }

    /// Forgets everything. Offered because it is the visitor's own data.
    pub fn clear(&self) {
        self.data.set(Progress::default());
        write_storage(PROGRESS_KEY, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(accuracy: f64, speed: f64, fluidness: Option<f64>) -> Score {
        Score {
            accuracy,
            speed,
            fluidness,
            touches: 600,
            errors: 0,
            seconds: 60.0,
        }
    }

    #[test]
    fn a_qualifying_run_sets_a_best() {
        let mut p = Progress::default();
        // Velocity needs 95% accuracy and 50 wpm.
        assert!(p.record(Module::Velocity, &score(96.0, 55.0, None), 1));
        assert_eq!(p.best_for(Module::Velocity).unwrap().speed, 55.0);
        assert_eq!(p.sessions, 1);
    }

    #[test]
    fn a_faster_run_that_misses_the_goals_does_not_take_the_record() {
        let mut p = Progress::default();
        p.record(Module::Velocity, &score(96.0, 55.0, None), 1);
        // Much faster, but sloppy: the goal is accuracy first.
        assert!(!p.record(Module::Velocity, &score(80.0, 90.0, None), 1));
        assert_eq!(p.best_for(Module::Velocity).unwrap().speed, 55.0);
        assert_eq!(p.sessions, 2, "it still counts as a session");
    }

    #[test]
    fn a_slower_qualifying_run_does_not_lower_the_best() {
        let mut p = Progress::default();
        p.record(Module::Velocity, &score(96.0, 70.0, None), 1);
        assert!(!p.record(Module::Velocity, &score(99.0, 60.0, None), 1));
        assert_eq!(p.best_for(Module::Velocity).unwrap().speed, 70.0);
    }

    #[test]
    fn modules_keep_separate_records() {
        let mut p = Progress::default();
        p.record(Module::Velocity, &score(96.0, 55.0, None), 1);
        p.record(Module::Fluidness, &score(98.0, 55.0, Some(80.0)), 1);
        assert!(p.best_for(Module::Velocity).is_some());
        assert!(p.best_for(Module::Fluidness).is_some());
        assert!(p.best_for(Module::Adaptability).is_none());
    }

    #[test]
    fn clearing_a_basic_lesson_advances_the_high_water_mark() {
        let mut p = Progress::default();
        // Basic needs 95% accuracy and 10 wpm.
        p.record(Module::Basic, &score(97.0, 20.0, None), 7);
        assert_eq!(p.lesson_reached, 7);
        // Going back to an earlier lesson must not lower it.
        p.record(Module::Basic, &score(97.0, 20.0, None), 3);
        assert_eq!(p.lesson_reached, 7);
        // Failing a later one does not advance it.
        p.record(Module::Basic, &score(50.0, 20.0, None), 9);
        assert_eq!(p.lesson_reached, 7);
    }

    #[test]
    fn merging_keeps_the_better_of_each_record() {
        let mut local = Progress::default();
        local.record(Module::Velocity, &score(96.0, 55.0, None), 1);
        local.record(Module::Basic, &score(97.0, 20.0, None), 4);

        let mut remote = Progress::default();
        remote.record(Module::Velocity, &score(96.0, 70.0, None), 1);
        remote.record(Module::Fluidness, &score(98.0, 60.0, Some(80.0)), 1);
        remote.record(Module::Basic, &score(97.0, 20.0, None), 9);

        let merged = local.merge(&remote);
        assert_eq!(
            merged.best_for(Module::Velocity).unwrap().speed,
            70.0,
            "remote was faster"
        );
        assert_eq!(merged.best_for(Module::Basic).unwrap().speed, 20.0);
        assert!(
            merged.best_for(Module::Fluidness).is_some(),
            "remote-only record kept"
        );
        assert_eq!(merged.lesson_reached, 9, "furthest lesson wins");
    }

    #[test]
    fn merging_never_loses_a_local_best() {
        // The outcome that would actually upset somebody.
        let mut local = Progress::default();
        local.record(Module::Velocity, &score(99.0, 90.0, None), 1);
        let merged = local.merge(&Progress::default());
        assert_eq!(merged.best_for(Module::Velocity).unwrap().speed, 90.0);
        // And the other way round.
        assert_eq!(
            Progress::default()
                .merge(&local)
                .best_for(Module::Velocity)
                .unwrap()
                .speed,
            90.0
        );
    }

    #[test]
    fn merging_is_order_independent() {
        let mut a = Progress::default();
        a.record(Module::Velocity, &score(96.0, 55.0, None), 3);
        let mut b = Progress::default();
        b.record(Module::Velocity, &score(96.0, 70.0, None), 7);
        assert_eq!(a.merge(&b), b.merge(&a));
    }

    #[test]
    fn merging_does_not_inflate_the_session_count() {
        // Both sides may have counted the same exercises; adding would
        // double-count every one of them.
        let mut a = Progress::default();
        let mut b = Progress::default();
        for _ in 0..5 {
            a.record(Module::Basic, &score(97.0, 20.0, None), 1);
            b.record(Module::Basic, &score(97.0, 20.0, None), 1);
        }
        assert_eq!(a.merge(&b).sessions, 5);
    }

    #[test]
    fn merging_with_nothing_changes_nothing() {
        let mut local = Progress::default();
        local.record(Module::Velocity, &score(96.0, 55.0, None), 2);
        assert_eq!(local.merge(&Progress::default()), local);
    }

    #[test]
    fn older_saved_progress_still_loads() {
        // Fields are all #[serde(default)], so a save from a version that did
        // not have them yet must not be thrown away.
        let progress: Progress = serde_json::from_str("{}").expect("loads");
        assert_eq!(progress, Progress::default());
        let partial: Progress = serde_json::from_str(r#"{"lesson_reached":5}"#).expect("loads");
        assert_eq!(partial.lesson_reached, 5);
        assert_eq!(partial.sessions, 0);
    }
}
