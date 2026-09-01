//! Plausibility checks on a reported session.
//!
//! # What this is and is not
//!
//! The server keeps its own tally from the keystroke stream, so a client cannot
//! simply post a number and have it believed. What it *can* still do is
//! fabricate a convincing stream — a script that emits "correct, 180 ms" four
//! hundred times produces a session indistinguishable from a real one.
//!
//! So this is not anti-cheat, and the leaderboard should not be described as if
//! it were. It rules out results that could not have come from the exercise that
//! was issued: wrong number of keystrokes, superhuman speed, timings that do not
//! add up. That is enough to keep the board free of accidents and idle
//! tampering. Anything stronger would need either an account to hold
//! accountable or behavioural analysis of the keystroke distribution, and
//! neither is worth it for a typing tutor.

use typing_core::{goals::Module, stats::Session, typist::Correction};

/// Fastest sustained speed treated as real, in words per minute.
///
/// Documented human records for sustained typing sit a little over 200 WPM, so
/// this leaves generous headroom while still catching a script.
pub const MAX_PLAUSIBLE_WPM: f64 = 300.0;

/// What the server issued, and therefore what a report has to match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expectation {
    /// The module the exercise was for.
    pub module: Module,
    /// How many characters the exercise text contained.
    pub chars: u32,
    /// Whether the module lets mistakes be corrected.
    pub correction: Correction,
}

/// Why a reported session was not believed.
#[derive(Clone, Debug, PartialEq)]
pub enum Rejection {
    /// More keystrokes than the exercise had characters.
    TooManyTouches {
        /// Keystrokes counted.
        touches: u32,
        /// Characters available to type.
        chars: u32,
    },
    /// The exercise was not finished, so there is nothing to score.
    Incomplete {
        /// Keystrokes counted.
        touches: u32,
        /// Characters that needed typing.
        chars: u32,
    },
    /// Keystrokes were reported but no time passed.
    NoTimeElapsed {
        /// Keystrokes counted.
        touches: u32,
    },
    /// Faster than a person types.
    ImpossibleSpeed {
        /// The speed computed.
        speed: f64,
        /// The ceiling it exceeded.
        limit: f64,
    },
    /// The gaps between keystrokes add up to more than the session lasted.
    TimingMismatch {
        /// Sum of the reported gaps, in microseconds.
        intervals_us: u64,
        /// Reported session length, in microseconds.
        elapsed_us: u64,
    },
    /// More gaps than there were keystrokes to have gaps between.
    TooManyIntervals {
        /// Gaps reported.
        intervals: usize,
        /// Keystrokes counted.
        touches: u32,
    },
}

impl core::fmt::Display for Rejection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Rejection::TooManyTouches { touches, chars } => write!(
                f,
                "{touches} keystrokes reported for an exercise of {chars} characters"
            ),
            Rejection::Incomplete { touches, chars } => {
                write!(f, "exercise not finished: {touches} of {chars} characters")
            }
            Rejection::NoTimeElapsed { touches } => {
                write!(f, "{touches} keystrokes in no time at all")
            }
            Rejection::ImpossibleSpeed { speed, limit } => {
                write!(f, "{speed:.0} wpm exceeds the {limit:.0} wpm ceiling")
            }
            Rejection::TimingMismatch { intervals_us, elapsed_us } => write!(
                f,
                "gaps total {intervals_us} µs but the session lasted {elapsed_us} µs"
            ),
            Rejection::TooManyIntervals { intervals, touches } => {
                write!(f, "{intervals} gaps between {touches} keystrokes")
            }
        }
    }
}

impl std::error::Error for Rejection {}

/// Checks that a session could have come from the exercise that was issued.
///
/// `complete` says whether the report claims a finished exercise; only finished
/// ones are eligible for a board, and only those are held to the exact keystroke
/// count.
pub fn verify(
    session: &Session,
    expected: &Expectation,
    complete: bool,
) -> Result<(), Rejection> {
    let touches = session.touches;

    // Every position of the text yields exactly one counted keystroke, in both
    // modes: forward mode advances on any key, and correction mode counts only
    // the keystroke that finally lands. So more touches than characters is
    // impossible however the typist behaved.
    if touches > expected.chars {
        return Err(Rejection::TooManyTouches { touches, chars: expected.chars });
    }
    if complete && touches < expected.chars {
        return Err(Rejection::Incomplete { touches, chars: expected.chars });
    }

    if touches > 0 && session.elapsed_us == 0 {
        return Err(Rejection::NoTimeElapsed { touches });
    }

    if session.intervals_us.len() > touches as usize {
        return Err(Rejection::TooManyIntervals {
            intervals: session.intervals_us.len(),
            touches,
        });
    }

    let intervals_us: u64 = session.intervals_us.iter().map(|&us| u64::from(us)).sum();
    if intervals_us > session.elapsed_us {
        return Err(Rejection::TimingMismatch {
            intervals_us,
            elapsed_us: session.elapsed_us,
        });
    }

    let speed = session.score().speed;
    if speed > MAX_PLAUSIBLE_WPM {
        return Err(Rejection::ImpossibleSpeed { speed, limit: MAX_PLAUSIBLE_WPM });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use typing_core::typist::{Correction, Key, Typist};

    fn expectation(text: &str, module: Module) -> Expectation {
        Expectation {
            module,
            chars: text.chars().count() as u32,
            correction: Correction::for_module(module),
        }
    }

    /// A real run: type `text` correctly, one keystroke every `step_us`.
    fn honest_run(text: &str, module: Module, step_us: u64) -> Session {
        let mut typist = Typist::for_module(text, module);
        let mut at = 0;
        for ch in text.chars() {
            at += step_us;
            typist.press(Key::Char(ch), at);
        }
        typist.session().clone()
    }

    #[test]
    fn an_honest_run_is_accepted() {
        let text = "the quick brown fox jumps over the lazy dog";
        let session = honest_run(text, Module::Velocity, 200_000);
        assert_eq!(verify(&session, &expectation(text, Module::Velocity), true), Ok(()));
    }

    #[test]
    fn an_honest_run_with_mistakes_is_accepted() {
        let text = "fjfjfjfjfj";
        let mut typist = Typist::for_module(text, Module::Velocity);
        let mut at = 0;
        for (i, ch) in text.chars().enumerate() {
            at += 200_000;
            let key = if i == 3 { 'z' } else { ch };
            typist.press(Key::Char(key), at);
        }
        assert_eq!(
            verify(typist.session(), &expectation(text, Module::Velocity), true),
            Ok(())
        );
    }

    #[test]
    fn an_honest_correction_run_is_accepted() {
        let text = "fjfj";
        let mut typist = Typist::for_module(text, Module::Fluidness);
        typist.press(Key::Char('f'), 100_000);
        typist.press(Key::Char('z'), 200_000); // wrong
        typist.press(Key::Backspace, 300_000);
        typist.press(Key::Char('j'), 400_000); // retyped
        typist.press(Key::Char('f'), 500_000);
        typist.press(Key::Char('j'), 600_000);
        assert!(typist.is_finished());
        assert_eq!(
            verify(typist.session(), &expectation(text, Module::Fluidness), true),
            Ok(())
        );
    }

    #[test]
    fn padding_the_keystroke_count_is_rejected() {
        let text = "fjfj";
        let mut session = honest_run(text, Module::Velocity, 200_000);
        session.correct(1_000_000); // one keystroke too many
        assert!(matches!(
            verify(&session, &expectation(text, Module::Velocity), true),
            Err(Rejection::TooManyTouches { touches: 5, chars: 4 })
        ));
    }

    #[test]
    fn an_unfinished_exercise_cannot_be_published() {
        let session = honest_run("fjfj", Module::Velocity, 200_000);
        let bigger = expectation("fjfjfjfjfj", Module::Velocity);
        assert!(matches!(
            verify(&session, &bigger, true),
            Err(Rejection::Incomplete { touches: 4, chars: 10 })
        ));
        // The same partial run is fine when not claiming completion.
        assert_eq!(verify(&session, &bigger, false), Ok(()));
    }

    #[test]
    fn superhuman_speed_is_rejected() {
        // 500 keystrokes in one second.
        let text = "f".repeat(500);
        let mut session = Session::new();
        for i in 1..=500u64 {
            session.correct(i * 2_000);
        }
        let verdict = verify(&session, &expectation(&text, Module::Velocity), true);
        assert!(matches!(verdict, Err(Rejection::ImpossibleSpeed { .. })), "{verdict:?}");
    }

    #[test]
    fn a_speed_just_under_the_ceiling_is_accepted() {
        // 290 wpm: 290*5 = 1450 correct keystrokes in 60 s.
        let count = 1450u64;
        let text = "f".repeat(count as usize);
        let mut session = Session::new();
        let step = 60_000_000 / count;
        for i in 1..=count {
            session.correct(i * step);
        }
        let verdict = verify(&session, &expectation(&text, Module::Velocity), true);
        assert_eq!(verdict, Ok(()), "speed was {}", session.score().speed);
    }

    #[test]
    fn keystrokes_in_zero_time_are_rejected() {
        let mut session = Session::new();
        session.correct(0);
        assert!(matches!(
            verify(&session, &expectation("f", Module::Velocity), true),
            Err(Rejection::NoTimeElapsed { touches: 1 })
        ));
    }

    #[test]
    fn gaps_that_outlast_the_session_are_rejected() {
        let mut session = honest_run("fjfj", Module::Velocity, 200_000);
        session.intervals_us.push(u32::MAX);
        let verdict = verify(&session, &expectation("fjfj", Module::Velocity), true);
        assert!(matches!(verdict, Err(Rejection::TimingMismatch { .. })), "{verdict:?}");
    }

    #[test]
    fn more_gaps_than_keystrokes_is_rejected() {
        let mut session = Session::new();
        session.correct(100_000);
        session.intervals_us = vec![1_000, 1_000, 1_000];
        let verdict = verify(&session, &expectation("f", Module::Velocity), true);
        assert!(matches!(verdict, Err(Rejection::TooManyIntervals { .. })), "{verdict:?}");
    }

    #[test]
    fn an_empty_report_is_harmless() {
        let session = Session::new();
        assert_eq!(verify(&session, &expectation("fjfj", Module::Velocity), false), Ok(()));
    }
}
