//! The four tutor modules, their skill goals, and the names given to speeds.
//!
//! Values are Klavaro's defaults from `src/tutor.c:182`. Upstream lets the user
//! override them in `preferences.ini`; here they are constants, because a shared
//! leaderboard is only meaningful if everyone is measured against the same bar.

use serde::{Deserialize, Serialize};

use crate::stats::Score;

/// One of Klavaro's four practice modules, in the order they should be learned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Module {
    /// Introduces the keys a few at a time, over 43 lessons.
    Basic,
    /// Nonsense words drawn from the whole layout, to break lookup habits.
    Adaptability,
    /// Real words from a language corpus, for raw speed.
    Velocity,
    /// Real paragraphs with punctuation, for even rhythm.
    Fluidness,
}

impl Module {
    /// Every module, in learning order.
    pub const ALL: [Module; 4] = [
        Module::Basic,
        Module::Adaptability,
        Module::Velocity,
        Module::Fluidness,
    ];

    /// Stable identifier used in URLs, storage keys and the wire protocol.
    pub fn slug(self) -> &'static str {
        match self {
            Module::Basic => "basic",
            Module::Adaptability => "adaptability",
            Module::Velocity => "velocity",
            Module::Fluidness => "fluidness",
        }
    }

    /// Parses a [`Module::slug`].
    pub fn from_slug(slug: &str) -> Option<Module> {
        Module::ALL.into_iter().find(|m| m.slug() == slug)
    }

    /// The bar to clear before moving on.
    pub fn goals(self) -> Goals {
        match self {
            Module::Basic => Goals { accuracy: 95.0, speed: 10.0, fluidness: None },
            Module::Adaptability => Goals { accuracy: 98.0, speed: 10.0, fluidness: None },
            Module::Velocity => Goals { accuracy: 95.0, speed: 50.0, fluidness: None },
            Module::Fluidness => {
                Goals { accuracy: 97.0, speed: 50.0, fluidness: Some(70.0) }
            }
        }
    }

    /// Whether a session in this module counts towards the shared leaderboard.
    ///
    /// Basic and Adaptability are about finding the keys rather than racing, and
    /// their goals are set accordingly; ranking them would reward the wrong thing.
    pub fn is_ranked(self) -> bool {
        matches!(self, Module::Velocity | Module::Fluidness)
    }

    /// Keystrokes required before a run may be published.
    ///
    /// Upstream applies its 500-character floor to fluidness alone
    /// (`src/tutor.c:1050`, guarded by `tutor.type == TT_FLUID`), because an
    /// evenness figure needs a long sample to mean anything. Applying the same
    /// floor to velocity would make that board unreachable: a velocity exercise
    /// is four paragraphs of twenty words, about 470 characters, so no run could
    /// ever clear 500.
    pub fn min_chars_to_rank(self) -> u32 {
        match self {
            Module::Fluidness => FLUIDNESS_MIN_CHARS,
            _ => 0,
        }
    }
}

/// Keystrokes a fluidness run needs before it may be published.
///
/// Klavaro's `MIN_CHARS_TO_LOG` (`src/top10.h:23`).
pub const FLUIDNESS_MIN_CHARS: u32 = 500;

/// The thresholds a session must meet to pass a module.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Goals {
    /// Required accuracy percentage.
    pub accuracy: f64,
    /// Required speed in words per minute.
    pub speed: f64,
    /// Required fluidness percentage, where the module measures it.
    pub fluidness: Option<f64>,
}

impl Goals {
    /// Whether a score clears every goal.
    pub fn met_by(&self, score: &Score) -> bool {
        if score.accuracy < self.accuracy || score.speed < self.speed {
            return false;
        }
        match (self.fluidness, score.fluidness) {
            (Some(required), Some(actual)) => actual >= required,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

/// Klavaro's speed bands, as `(upper bound exclusive, name)`.
///
/// Upstream encodes these as `LEVEL_GSET (velo, speed_*)` thresholds and picks a
/// message with a ladder of `velocity < tutor_goal_level(n)` tests
/// (`src/velocity.c:436`). The thresholds are therefore upper bounds: you are
/// "walking" until you reach 30 WPM. The 50 WPM rung is the module goal itself,
/// which upstream leaves unnamed in prose; the short labels here are ours.
pub const SPEED_BANDS: [(f64, &str); 10] = [
    (10.0, "beginning"),
    (20.0, "stepping"),
    (30.0, "walking"),
    (40.0, "jogging"),
    (50.0, "almost there"),
    (60.0, "running"),
    (70.0, "professional"),
    (80.0, "racer"),
    (90.0, "flying"),
    (f64::INFINITY, "master"),
];

/// Names a speed in words per minute.
pub fn speed_band(wpm: f64) -> &'static str {
    SPEED_BANDS
        .iter()
        .find(|(upper, _)| wpm < *upper)
        .map(|(_, name)| *name)
        .unwrap_or("master")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(accuracy: f64, speed: f64, fluidness: Option<f64>) -> Score {
        Score { accuracy, speed, fluidness, touches: 600, errors: 0, seconds: 60.0 }
    }

    #[test]
    fn slugs_round_trip() {
        for m in Module::ALL {
            assert_eq!(Module::from_slug(m.slug()), Some(m));
        }
        assert_eq!(Module::from_slug("nope"), None);
    }

    #[test]
    fn velocity_goal_needs_both_accuracy_and_speed() {
        let goals = Module::Velocity.goals();
        assert!(goals.met_by(&score(96.0, 51.0, None)));
        assert!(!goals.met_by(&score(94.0, 51.0, None)), "accuracy short");
        assert!(!goals.met_by(&score(96.0, 49.0, None)), "speed short");
    }

    #[test]
    fn fluidness_goal_requires_a_fluidness_reading() {
        let goals = Module::Fluidness.goals();
        assert!(goals.met_by(&score(98.0, 55.0, Some(75.0))));
        assert!(!goals.met_by(&score(98.0, 55.0, Some(65.0))), "fluidness short");
        assert!(!goals.met_by(&score(98.0, 55.0, None)), "no reading at all");
    }

    #[test]
    fn only_the_speed_modules_are_ranked() {
        assert!(!Module::Basic.is_ranked());
        assert!(!Module::Adaptability.is_ranked());
        assert!(Module::Velocity.is_ranked());
        assert!(Module::Fluidness.is_ranked());
    }

    #[test]
    fn the_character_floor_applies_to_fluidness_only() {
        // A 500-character floor on velocity would make that board impossible to
        // reach, since a velocity exercise is only around 470 characters.
        assert_eq!(Module::Velocity.min_chars_to_rank(), 0);
        assert_eq!(Module::Fluidness.min_chars_to_rank(), FLUIDNESS_MIN_CHARS);
    }

    #[test]
    fn a_real_velocity_exercise_can_reach_its_board() {
        let layout = crate::load_layout("qwerty_us").expect("bundled");
        let corpus = crate::corpus::Corpus::new(
            "en",
            "the\nquick\nbrown\nfox\njumps\nover\nlazy\ndog\n",
            "A paragraph.\n\nAnother one.\n",
        );
        let generated = crate::exercise::generate(
            crate::exercise::Request {
                module: Module::Velocity,
                layout: &layout,
                lesson: None,
                corpus: Some(&corpus),
                stop_marks: true,
            },
            99,
        )
        .expect("generates");
        assert!(
            generated.len_chars() as u32 >= Module::Velocity.min_chars_to_rank(),
            "a velocity run must be able to reach the board"
        );
    }

    #[test]
    fn speed_bands_are_upper_bounds() {
        assert_eq!(speed_band(0.0), "beginning");
        assert_eq!(speed_band(9.9), "beginning");
        assert_eq!(speed_band(10.0), "stepping", "the threshold ends the band");
        assert_eq!(speed_band(29.9), "walking");
        assert_eq!(speed_band(50.0), "running");
        assert_eq!(speed_band(89.9), "flying");
        assert_eq!(speed_band(90.0), "master");
        assert_eq!(speed_band(1000.0), "master");
    }
}
