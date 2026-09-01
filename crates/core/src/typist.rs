//! The typing loop: what the typist is asked for, what they pressed, and how the
//! text should be coloured.
//!
//! This lives in the core crate rather than the UI on purpose. It is a state
//! machine over a character array with no rendering in it, so it can be tested
//! exhaustively without a browser, and the server can replay a reported session
//! through the same code to check it.
//!
//! # Two modes
//!
//! Klavaro evaluates keystrokes two different ways.
//!
//! [`Correction::Forbidden`] (`tutor_eval_forward`) is used by Basic,
//! Adaptability and Velocity: a wrong key is marked wrong, the cursor moves on
//! regardless, and there is no going back. Every keystroke is one touch, and
//! accuracy is the share that matched.
//!
//! [`Correction::Required`] (`tutor_eval_forward_backward`) is used by Fluidness:
//! a wrong key blocks the cursor until it is backspaced away and retyped. The
//! fumble is not counted at all; the retype is counted as a touch *and* an error.
//! Each position therefore contributes exactly one touch, and accuracy means
//! "typed right the first time".

use serde::{Deserialize, Serialize};

use crate::goals::Module;
use crate::stats::{Score, Session};

/// Whether the typist may back up to fix a mistake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Correction {
    /// Mistakes stand; the cursor always moves forward.
    Forbidden,
    /// A mistake must be backspaced away and retyped before moving on.
    Required,
}

impl Correction {
    /// The mode Klavaro uses for a module.
    pub fn for_module(module: Module) -> Correction {
        match module {
            Module::Fluidness => Correction::Required,
            _ => Correction::Forbidden,
        }
    }
}

/// How one character of the exercise currently stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CharState {
    /// Not reached yet.
    Untouched,
    /// Typed correctly first time.
    Correct,
    /// Typed wrongly.
    Wrong,
    /// Typed correctly, but only after a correction.
    Retouched,
}

/// A key the typist pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A character key. Return arrives as `Char('\n')`.
    Char(char),
    /// Backspace.
    Backspace,
}

/// What a keystroke did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Matched; the cursor moved on.
    Correct,
    /// Matched after a correction; counts against accuracy.
    Retouched,
    /// Did not match.
    Wrong,
    /// Moved back one position.
    Backspaced,
    /// Had no effect — nothing to correct, or the exercise is over.
    Ignored,
}

impl Press {
    /// Whether the UI should signal a mistake.
    pub fn is_mistake(self) -> bool {
        matches!(self, Press::Wrong | Press::Ignored)
    }

    /// Which [`Session`] transition this press produced.
    ///
    /// The mode matters, and this is the trap: a wrong key going forward is a
    /// counted keystroke *and* an error, while a wrong key in correction mode is
    /// counted as nothing at all, because the position will be scored when it is
    /// finally retyped. Anything that needs to reproduce a session from a stream
    /// of presses — the wire protocol, the server's tally — must ask here rather
    /// than map `Press` on its own, or the two sides will disagree.
    pub fn counted(self, mode: Correction) -> Option<Counted> {
        Some(match (self, mode) {
            (Press::Correct, _) => Counted::Correct,
            (Press::Retouched, _) => Counted::Retouched,
            (Press::Wrong, Correction::Forbidden) => Counted::Wrong,
            (Press::Wrong, Correction::Required) => Counted::Stumble,
            (Press::Backspaced, _) => Counted::Stumble,
            (Press::Ignored, _) => return None,
        })
    }
}

/// A [`Session`] transition, named so a keystroke can be replayed elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counted {
    /// [`Session::correct`].
    Correct,
    /// [`Session::wrong`].
    Wrong,
    /// [`Session::retouched`].
    Retouched,
    /// [`Session::stumbled`].
    Stumble,
}

impl Counted {
    /// Applies this transition to a session.
    pub fn apply(self, session: &mut Session, at_us: u64) {
        match self {
            Counted::Correct => session.correct(at_us),
            Counted::Wrong => session.wrong(at_us),
            Counted::Retouched => session.retouched(at_us),
            Counted::Stumble => session.stumbled(at_us),
        }
    }
}

/// One run through an exercise.
#[derive(Clone, Debug)]
pub struct Typist {
    expected: Vec<char>,
    states: Vec<CharState>,
    cursor: usize,
    mode: Correction,
    /// Wrong characters typed that have not yet been backspaced away.
    pending_errors: u32,
    /// Positions backed over, whose retype should count as a correction.
    correcting: u32,
    session: Session,
    finished: bool,
}

impl Typist {
    /// Starts a run over `text`.
    pub fn new(text: &str, mode: Correction) -> Typist {
        let expected: Vec<char> = text.chars().collect();
        let states = vec![CharState::Untouched; expected.len()];
        Typist {
            expected,
            states,
            cursor: 0,
            mode,
            pending_errors: 0,
            correcting: 0,
            session: Session::new(),
            finished: false,
        }
    }

    /// Starts a run over an exercise, using the mode its module calls for.
    pub fn for_module(text: &str, module: Module) -> Typist {
        Typist::new(text, Correction::for_module(module))
    }

    /// The character the typist should press next, if any remain.
    pub fn expected(&self) -> Option<char> {
        self.expected.get(self.cursor).copied()
    }

    /// The full text being typed.
    pub fn text(&self) -> &[char] {
        &self.expected
    }

    /// The state of every character, for colouring the text.
    pub fn states(&self) -> &[CharState] {
        &self.states
    }

    /// Where the cursor is.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Whether the exercise is complete.
    ///
    /// In [`Correction::Required`] mode, reaching the end is not enough: any
    /// uncorrected mistake must be fixed first.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The keystroke record, for scoring.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The score so far.
    pub fn score(&self) -> Score {
        self.session.score()
    }

    /// How many mistakes are waiting to be backspaced away.
    pub fn pending_errors(&self) -> u32 {
        self.pending_errors
    }

    /// Handles a keystroke, `at_us` microseconds after the session started.
    pub fn press(&mut self, key: Key, at_us: u64) -> Press {
        if self.finished {
            return Press::Ignored;
        }
        match self.mode {
            Correction::Forbidden => self.press_forward(key, at_us),
            Correction::Required => self.press_forward_backward(key, at_us),
        }
    }

    /// Basic, Adaptability, Velocity: no going back.
    fn press_forward(&mut self, key: Key, at_us: u64) -> Press {
        let Key::Char(typed) = key else {
            // Upstream beeps at backspace here and does nothing else.
            return Press::Ignored;
        };
        let Some(&wanted) = self.expected.get(self.cursor) else {
            return Press::Ignored;
        };

        let outcome = if typed == wanted {
            self.states[self.cursor] = CharState::Correct;
            self.session.correct(at_us);
            Press::Correct
        } else {
            self.states[self.cursor] = CharState::Wrong;
            self.session.wrong(at_us);
            Press::Wrong
        };

        self.cursor += 1;
        if self.cursor >= self.expected.len() {
            self.finish(at_us);
        }
        outcome
    }

    /// Fluidness: mistakes must be corrected before the cursor moves on.
    fn press_forward_backward(&mut self, key: Key, at_us: u64) -> Press {
        if key == Key::Backspace {
            if self.pending_errors == 0 || self.cursor == 0 {
                // Nothing to undo. Backspace is not a general edit key here.
                return Press::Ignored;
            }
            self.cursor -= 1;
            self.states[self.cursor] = CharState::Untouched;
            self.pending_errors -= 1;
            self.correcting += 1;
            self.session.stumbled(at_us);
            return Press::Backspaced;
        }

        let Key::Char(typed) = key else { unreachable!("backspace handled above") };
        let Some(&wanted) = self.expected.get(self.cursor) else {
            return Press::Ignored;
        };

        let outcome = if typed == wanted && self.pending_errors == 0 {
            if self.correcting > 0 {
                self.states[self.cursor] = CharState::Retouched;
                self.session.retouched(at_us);
                Press::Retouched
            } else {
                self.states[self.cursor] = CharState::Correct;
                self.session.correct(at_us);
                Press::Correct
            }
        } else {
            self.states[self.cursor] = CharState::Wrong;
            self.pending_errors += 1;
            self.session.stumbled(at_us);
            Press::Wrong
        };

        self.correcting = self.correcting.saturating_sub(1);

        // A wrong key still moves the cursor, so the typist sees what they typed;
        // it just cannot be left there.
        if self.cursor < self.expected.len() {
            self.cursor += 1;
        }
        if self.cursor >= self.expected.len() && self.pending_errors == 0 {
            self.finish(at_us);
        }
        outcome
    }

    fn finish(&mut self, at_us: u64) {
        self.session.finish(at_us);
        self.finished = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types a string one character every 100 ms, from 100 ms.
    fn type_all(typist: &mut Typist, keys: &str) {
        for (i, ch) in keys.chars().enumerate() {
            typist.press(Key::Char(ch), (i as u64 + 1) * 100_000);
        }
    }

    // ---- forward mode ----------------------------------------------------

    #[test]
    fn forward_run_typed_perfectly() {
        let mut t = Typist::new("fj jf", Correction::Forbidden);
        type_all(&mut t, "fj jf");
        assert!(t.is_finished());
        assert!(t.states().iter().all(|s| *s == CharState::Correct));
        let score = t.score();
        assert_eq!(score.touches, 5);
        assert_eq!(score.errors, 0);
        assert_eq!(score.accuracy, 100.0);
    }

    #[test]
    fn forward_mode_moves_past_mistakes() {
        let mut t = Typist::new("fjf", Correction::Forbidden);
        assert_eq!(t.press(Key::Char('f'), 100_000), Press::Correct);
        assert_eq!(t.press(Key::Char('x'), 200_000), Press::Wrong);
        assert_eq!(t.press(Key::Char('f'), 300_000), Press::Correct);
        assert!(t.is_finished());
        assert_eq!(
            t.states(),
            [CharState::Correct, CharState::Wrong, CharState::Correct]
        );
        let score = t.score();
        assert_eq!((score.touches, score.errors), (3, 1));
        assert!((score.accuracy - 200.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn forward_mode_ignores_backspace() {
        let mut t = Typist::new("fj", Correction::Forbidden);
        t.press(Key::Char('x'), 100_000);
        assert_eq!(t.press(Key::Backspace, 200_000), Press::Ignored);
        assert_eq!(t.cursor(), 1, "backspace must not move the cursor");
        assert_eq!(t.states()[0], CharState::Wrong, "the mistake stands");
    }

    #[test]
    fn newline_is_just_another_expected_character() {
        let mut t = Typist::new("fj\nfj", Correction::Forbidden);
        type_all(&mut t, "fj\nfj");
        assert!(t.is_finished());
        assert_eq!(t.score().errors, 0);
    }

    #[test]
    fn pressing_return_where_a_letter_is_wanted_is_a_mistake() {
        let mut t = Typist::new("fj", Correction::Forbidden);
        assert_eq!(t.press(Key::Char('\n'), 100_000), Press::Wrong);
    }

    #[test]
    fn keys_after_the_end_are_ignored() {
        let mut t = Typist::new("f", Correction::Forbidden);
        t.press(Key::Char('f'), 100_000);
        assert!(t.is_finished());
        assert_eq!(t.press(Key::Char('f'), 200_000), Press::Ignored);
        assert_eq!(t.score().touches, 1, "no extra touches after the end");
    }

    // ---- correction mode -------------------------------------------------

    #[test]
    fn correction_mode_blocks_until_the_mistake_is_fixed() {
        let mut t = Typist::new("fjf", Correction::Required);
        assert_eq!(t.press(Key::Char('f'), 100_000), Press::Correct);
        assert_eq!(t.press(Key::Char('x'), 200_000), Press::Wrong);
        assert_eq!(t.pending_errors(), 1);
        // Typing on without correcting keeps going wrong.
        assert_eq!(t.press(Key::Char('f'), 300_000), Press::Wrong);
        assert_eq!(t.pending_errors(), 2);
        // Back out both.
        assert_eq!(t.press(Key::Backspace, 400_000), Press::Backspaced);
        assert_eq!(t.press(Key::Backspace, 500_000), Press::Backspaced);
        assert_eq!(t.pending_errors(), 0);
        assert_eq!(t.cursor(), 1);
        assert_eq!(t.states()[1], CharState::Untouched, "cleared on backspace");
        // Now the retype lands, marked as a correction.
        assert_eq!(t.press(Key::Char('j'), 600_000), Press::Retouched);
        assert_eq!(t.states()[1], CharState::Retouched);
    }

    #[test]
    fn correction_mode_charges_a_fixed_position_exactly_once() {
        let mut t = Typist::new("fj", Correction::Required);
        t.press(Key::Char('f'), 100_000);
        t.press(Key::Char('x'), 200_000); // wrong
        t.press(Key::Backspace, 300_000);
        t.press(Key::Char('j'), 400_000); // retyped correctly
        assert!(t.is_finished());
        let score = t.score();
        assert_eq!(score.touches, 2, "one touch per position");
        assert_eq!(score.errors, 1, "charged once, not twice");
        assert_eq!(score.accuracy, 50.0);
    }

    #[test]
    fn correction_mode_is_not_finished_with_a_mistake_outstanding() {
        let mut t = Typist::new("fj", Correction::Required);
        t.press(Key::Char('f'), 100_000);
        t.press(Key::Char('x'), 200_000);
        assert!(!t.is_finished(), "must not finish on an uncorrected mistake");
        t.press(Key::Backspace, 300_000);
        t.press(Key::Char('j'), 400_000);
        assert!(t.is_finished());
    }

    #[test]
    fn correction_mode_ignores_backspace_with_nothing_to_undo() {
        let mut t = Typist::new("fj", Correction::Required);
        assert_eq!(t.press(Key::Backspace, 100_000), Press::Ignored);
        assert_eq!(t.cursor(), 0);
        t.press(Key::Char('f'), 200_000);
        assert_eq!(t.press(Key::Backspace, 300_000), Press::Ignored);
        assert_eq!(t.cursor(), 1, "correct characters are not editable");
    }

    #[test]
    fn a_clean_correction_run_scores_full_marks() {
        let mut t = Typist::new("the quick brown", Correction::Required);
        type_all(&mut t, "the quick brown");
        assert!(t.is_finished());
        let score = t.score();
        assert_eq!(score.errors, 0);
        assert_eq!(score.accuracy, 100.0);
        assert!(score.fluidness.expect("steady rhythm") > 99.0);
    }

    #[test]
    fn time_spent_fumbling_does_not_become_a_slow_keystroke() {
        // Type "fjfj" cleanly, then stumble for two seconds mid-word and recover.
        let mut clean = Typist::new("fjfjfjfjfj", Correction::Required);
        type_all(&mut clean, "fjfjfjfjfj");

        let mut stumbled = Typist::new("fjfjfjfjfj", Correction::Required);
        let mut at = 0u64;
        for (i, ch) in "fjfjfjfjfj".chars().enumerate() {
            if i == 5 {
                at += 100_000;
                stumbled.press(Key::Char('z'), at);
                at += 2_000_000; // long pause staring at the mistake
                stumbled.press(Key::Backspace, at);
            }
            at += 100_000;
            stumbled.press(Key::Char(ch), at);
        }
        assert!(stumbled.is_finished());
        // Accuracy is dented by the one bad position, rhythm barely at all.
        assert!(stumbled.score().accuracy < 100.0);
        let steady = clean.score().fluidness.unwrap();
        let dented = stumbled.score().fluidness.unwrap();
        assert!(
            dented > steady - 15.0,
            "a single stumble should not wreck fluidness: {dented} vs {steady}"
        );
    }

    // ---- module wiring ---------------------------------------------------

    #[test]
    fn a_wrong_key_counts_differently_in_each_mode() {
        // The bug this guards against: treating a correction-mode mistake as a
        // counted error would give more keystrokes than the text has characters.
        assert_eq!(
            Press::Wrong.counted(Correction::Forbidden),
            Some(Counted::Wrong)
        );
        assert_eq!(
            Press::Wrong.counted(Correction::Required),
            Some(Counted::Stumble)
        );
    }

    #[test]
    fn every_press_maps_to_the_transition_it_performed() {
        for mode in [Correction::Forbidden, Correction::Required] {
            assert_eq!(Press::Correct.counted(mode), Some(Counted::Correct));
            assert_eq!(Press::Retouched.counted(mode), Some(Counted::Retouched));
            assert_eq!(Press::Backspaced.counted(mode), Some(Counted::Stumble));
            assert_eq!(Press::Ignored.counted(mode), None);
        }
    }

    #[test]
    fn replaying_the_counted_transitions_reproduces_the_session() {
        // This is exactly what the server does with the reported stream, so the
        // two must land on the same numbers.
        for (text, module) in [("fjfjfjfj", Module::Velocity), ("fjfjfjfj", Module::Fluidness)] {
            let mode = Correction::for_module(module);
            let mut typist = Typist::new(text, mode);
            let mut replayed = crate::stats::Session::new();
            let mut at = 0u64;
            for (i, ch) in text.chars().enumerate() {
                at += 100_000;
                // Get one wrong, then fix it if the mode requires it.
                let key = if i == 2 { Key::Char('q') } else { Key::Char(ch) };
                if let Some(counted) = typist.press(key, at).counted(mode) {
                    counted.apply(&mut replayed, at);
                }
                if i == 2 && mode == Correction::Required {
                    at += 100_000;
                    if let Some(counted) = typist.press(Key::Backspace, at).counted(mode) {
                        counted.apply(&mut replayed, at);
                    }
                    at += 100_000;
                    if let Some(counted) = typist.press(Key::Char(ch), at).counted(mode) {
                        counted.apply(&mut replayed, at);
                    }
                }
            }
            assert_eq!(
                replayed.touches,
                typist.session().touches,
                "{module:?}: keystroke counts differ"
            );
            assert_eq!(replayed.errors, typist.session().errors, "{module:?}");
            assert_eq!(replayed.score(), typist.score(), "{module:?}");
        }
    }

    #[test]
    fn only_fluidness_allows_correction() {
        assert_eq!(Correction::for_module(Module::Basic), Correction::Forbidden);
        assert_eq!(Correction::for_module(Module::Adaptability), Correction::Forbidden);
        assert_eq!(Correction::for_module(Module::Velocity), Correction::Forbidden);
        assert_eq!(Correction::for_module(Module::Fluidness), Correction::Required);
    }

    #[test]
    fn empty_text_needs_no_keystrokes() {
        let t = Typist::new("", Correction::Forbidden);
        assert_eq!(t.expected(), None);
        assert!(!t.is_finished(), "nothing was typed, so nothing finished");
    }
}
