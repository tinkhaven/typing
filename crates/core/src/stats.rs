//! Session scoring: accuracy, speed and fluidness.
//!
//! The formulas are Klavaro's, from `src/tutor.c:1011`–`1047`, kept intact so
//! that scores produced here are comparable with the desktop program's.
//!
//! # Touch timing
//!
//! Klavaro keeps an array of intervals between *consecutive correct* touches. A
//! wrong touch restarts the clock but contributes no interval, so a stumble is
//! excluded from the rhythm measurement instead of poisoning it. Fluidness then
//! ignores the first two intervals, the first being measured from the moment the
//! session started and therefore mostly reaction time.

use serde::{Deserialize, Serialize};

/// Number of characters that make up one "word" for speed purposes.
pub const CHARS_PER_WORD: f64 = 5.0;

/// `60 s / 5 chars` — the constant Klavaro writes literally as `12`.
pub const WPM_FACTOR: f64 = 60.0 / CHARS_PER_WORD;

/// Intervals skipped before fluidness starts measuring rhythm.
const FLUIDNESS_WARMUP: usize = 2;

/// Lower bound Klavaro clamps fluidness to, so a chart never shows zero.
const FLUIDNESS_FLOOR: f64 = 2.0;

/// Accumulates keystrokes over one practice session.
///
/// The client owns this: evaluating a keystroke must not involve the network,
/// because fluidness *is* the variance of inter-keystroke intervals and would
/// simply measure connection jitter instead.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Every keystroke offered, including wrong ones.
    pub touches: u32,
    /// Keystrokes that did not match the expected character.
    pub errors: u32,
    /// Microseconds between consecutive correct touches, in order.
    pub intervals_us: Vec<u32>,
    /// Wall-clock duration of the session, in microseconds.
    pub elapsed_us: u64,
    /// Microseconds at which the last correct touch landed, relative to start.
    ///
    /// Bookkeeping for measuring the next gap, not part of the result. Skipped
    /// when serialising so it stays out of the wire format: a session that
    /// crosses the network is only ever read, never continued.
    #[serde(skip)]
    last_correct_us: u64,
    /// Whether any keystroke has been seen yet.
    #[serde(skip)]
    started: bool,
}

impl Session {
    /// Creates an empty session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a keystroke that matched the expected character.
    ///
    /// `at_us` is measured from the start of the session.
    pub fn correct(&mut self, at_us: u64) {
        self.touches += 1;
        if self.started {
            self.intervals_us
                .push(at_us.saturating_sub(self.last_correct_us) as u32);
        }
        self.last_correct_us = at_us;
        self.started = true;
        self.elapsed_us = at_us;
    }

    /// Records a keystroke that did not match.
    ///
    /// The clock is restarted, so the interval spanning the mistake is dropped
    /// rather than counted as a very slow keystroke.
    pub fn wrong(&mut self, at_us: u64) {
        self.touches += 1;
        self.errors += 1;
        self.last_correct_us = at_us;
        self.started = true;
        self.elapsed_us = at_us;
    }

    /// Records a keystroke that matched, but only after a correction.
    ///
    /// Fluidness practice lets you back up and fix a mistake. The retyped
    /// character is a real keystroke with a real interval, so it counts towards
    /// speed and rhythm — but it also counts as an error, because the position
    /// did not come out right the first time. That is what makes accuracy in
    /// this mode mean "typed right first time".
    pub fn retouched(&mut self, at_us: u64) {
        self.touches += 1;
        self.errors += 1;
        if self.started {
            self.intervals_us
                .push(at_us.saturating_sub(self.last_correct_us) as u32);
        }
        self.last_correct_us = at_us;
        self.started = true;
        self.elapsed_us = at_us;
    }

    /// Records a keystroke that neither counts nor times.
    ///
    /// Used in correction mode for the initial mistake and for the backspaces
    /// that undo it: the position will be scored when it is finally retyped, so
    /// counting the fumble as well would penalise it twice. The clock still
    /// restarts, so the time spent fumbling never becomes a "slow keystroke".
    pub fn stumbled(&mut self, at_us: u64) {
        self.last_correct_us = at_us;
        self.started = true;
        self.elapsed_us = at_us;
    }

    /// Marks the session as finished at `at_us` from its start.
    pub fn finish(&mut self, at_us: u64) {
        self.elapsed_us = at_us;
    }

    /// Scores the session.
    pub fn score(&self) -> Score {
        Score::of(self)
    }
}

/// The result of one practice session.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Percentage of keystrokes that were correct, `0.0..=100.0`.
    pub accuracy: f64,
    /// Words per minute, counting only correct keystrokes.
    pub speed: f64,
    /// Rhythm evenness as a percentage, or `None` when too few keystrokes were
    /// recorded for the figure to mean anything.
    pub fluidness: Option<f64>,
    /// Keystrokes offered.
    pub touches: u32,
    /// Keystrokes that were wrong.
    pub errors: u32,
    /// Session duration in seconds.
    pub seconds: f64,
}

impl Score {
    /// Scores a session.
    pub fn of(session: &Session) -> Score {
        let seconds = session.elapsed_us as f64 / 1_000_000.0;
        let touches = session.touches;
        let errors = session.errors;

        let accuracy = if touches == 0 {
            0.0
        } else {
            100.0 * (1.0 - errors as f64 / touches as f64)
        };

        let speed = if seconds <= 0.0 {
            0.0
        } else {
            WPM_FACTOR * (touches.saturating_sub(errors)) as f64 / seconds
        };

        Score {
            accuracy,
            speed,
            fluidness: fluidness(&session.intervals_us),
            touches,
            errors,
            seconds,
        }
    }
}

/// Klavaro's "magic" fluidness: `100 · (1 − σ/μ)` over samples `sᵢ = √(1/Δtᵢ)`.
///
/// Working in `√(1/Δt)` rather than `Δt` compresses the long tail of pauses, so
/// one hesitation does not dominate the deviation. `σ` uses the sample (Bessel)
/// divisor `n − 1`, as upstream does.
///
/// Returns `None` when fewer than two intervals survive the warm-up. Upstream
/// returns 100% in that case through a division-by-almost-zero; refusing to
/// report a figure is more honest and cannot affect comparability, since
/// fluidness sessions are only ever recorded above 500 characters.
pub fn fluidness(intervals_us: &[u32]) -> Option<f64> {
    let samples: Vec<f64> = intervals_us
        .iter()
        .skip(FLUIDNESS_WARMUP)
        .map(|&us| {
            // Upstream floors the interval at 1e-8 s to avoid dividing by zero.
            let seconds = (us as f64 / 1_000_000.0).max(1.0e-8);
            (1.0 / seconds).sqrt()
        })
        .collect();

    if samples.len() < 2 {
        return None;
    }

    let n = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / n;
    if mean <= 0.0 {
        return None;
    }

    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let deviation = variance.sqrt();

    Some((100.0 * (1.0 - deviation / mean)).max(FLUIDNESS_FLOOR))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `n` correct keystrokes exactly `step_us` apart.
    fn metronome(n: u32, step_us: u64) -> Session {
        let mut s = Session::new();
        for i in 1..=n {
            s.correct(i as u64 * step_us);
        }
        s
    }

    #[test]
    fn accuracy_is_share_of_correct_touches() {
        let mut s = Session::new();
        for i in 1..=10 {
            if i % 5 == 0 {
                s.wrong(i * 100_000);
            } else {
                s.correct(i * 100_000);
            }
        }
        // 10 touches, 2 wrong.
        assert_eq!(s.touches, 10);
        assert_eq!(s.errors, 2);
        assert!((s.score().accuracy - 80.0).abs() < 1e-9);
    }

    #[test]
    fn speed_matches_the_five_character_word() {
        // 300 correct keystrokes in 60 s = 300/5 = 60 words in a minute.
        let s = metronome(300, 200_000);
        let score = s.score();
        assert!((score.seconds - 60.0).abs() < 1e-6, "{}", score.seconds);
        assert!((score.speed - 60.0).abs() < 1e-6, "{}", score.speed);
    }

    #[test]
    fn speed_counts_only_correct_touches() {
        let mut s = Session::new();
        for i in 1..=100u64 {
            s.correct(i * 100_000);
        }
        let clean = s.score().speed;
        for i in 101..=200u64 {
            s.wrong(i * 100_000);
        }
        // Twice the time, no additional correct touches: half the speed.
        assert!((s.score().speed - clean / 2.0).abs() < 1e-6);
    }

    #[test]
    fn perfect_rhythm_is_perfectly_fluid() {
        let s = metronome(50, 150_000);
        let f = s.score().fluidness.expect("enough samples");
        assert!((f - 100.0).abs() < 1e-6, "{f}");
    }

    #[test]
    fn erratic_rhythm_scores_below_steady_rhythm() {
        let steady = fluidness(&[150_000; 40]).unwrap();
        let mut erratic: Vec<u32> = Vec::new();
        for i in 0..40 {
            erratic.push(if i % 2 == 0 { 40_000 } else { 600_000 });
        }
        let erratic = fluidness(&erratic).unwrap();
        assert!(erratic < steady, "erratic {erratic} should be < steady {steady}");
    }

    #[test]
    fn fluidness_never_reports_below_its_floor() {
        // Working in sqrt(1/dt) means a few slow keystrokes among fast ones barely
        // move the deviation. It takes the opposite shape - a long crawl with one
        // freak-fast keystroke - to drive 100*(1 - sd/mean) negative.
        let mut lopsided: Vec<u32> = vec![5_000_000; 30];
        lopsided.push(1);
        let f = fluidness(&lopsided).expect("enough samples");
        assert_eq!(f, FLUIDNESS_FLOOR, "should clamp, got {f}");
    }

    #[test]
    fn a_few_slow_keystrokes_barely_dent_fluidness() {
        // The counterpart of the test above, and the reason the metric is useful:
        // it rewards steady rhythm rather than punishing the occasional pause.
        let mut mostly_fast: Vec<u32> = vec![150_000; 40];
        mostly_fast.extend([2_000_000, 2_000_000]);
        let f = fluidness(&mostly_fast).expect("enough samples");
        assert!(f > 50.0, "two pauses in 42 keystrokes should not ruin it: {f}");
    }

    #[test]
    fn fluidness_needs_samples_beyond_the_warmup() {
        assert_eq!(fluidness(&[]), None);
        assert_eq!(fluidness(&[100_000, 100_000]), None, "both are warm-up");
        assert_eq!(fluidness(&[100_000, 100_000, 100_000]), None, "one sample");
        assert!(fluidness(&[100_000; 4]).is_some(), "two samples");
    }

    #[test]
    fn wrong_touch_drops_the_interval_around_it() {
        // A long pause spent on a wrong key must not appear as a slow interval.
        let mut s = Session::new();
        s.correct(100_000);
        s.correct(200_000);
        s.wrong(5_000_000); // long stumble
        s.correct(5_100_000);
        assert_eq!(s.intervals_us, vec![100_000, 100_000]);
    }

    #[test]
    fn a_retouch_counts_as_a_touch_an_error_and_an_interval() {
        let mut s = Session::new();
        s.correct(100_000);
        s.correct(200_000);
        s.retouched(300_000);
        assert_eq!(s.touches, 3);
        assert_eq!(s.errors, 1);
        assert_eq!(s.intervals_us, vec![100_000, 100_000]);
        // Two of three positions came out right first time.
        assert!((s.score().accuracy - 200.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_stumble_counts_for_nothing_but_restarts_the_clock() {
        let mut s = Session::new();
        s.correct(100_000);
        s.correct(200_000);
        s.stumbled(900_000); // wrong key, then backspace
        s.stumbled(1_000_000);
        s.retouched(1_100_000);
        assert_eq!(s.touches, 3, "the fumble itself is not a touch");
        assert_eq!(s.errors, 1, "the position is charged once, not twice");
        // The 700 ms spent fumbling never becomes an interval.
        assert_eq!(s.intervals_us, vec![100_000, 100_000]);
    }

    #[test]
    fn the_wire_form_carries_results_not_bookkeeping() {
        let mut s = Session::new();
        s.correct(100_000);
        s.correct(200_000);
        s.wrong(300_000);
        let json = serde_json::to_string(&s).expect("serialises");
        assert!(!json.contains("last_correct_us"), "{json}");
        assert!(!json.contains("started"), "{json}");

        // Everything that makes up a score survives the round trip.
        let back: Session = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back.touches, s.touches);
        assert_eq!(back.errors, s.errors);
        assert_eq!(back.intervals_us, s.intervals_us);
        assert_eq!(back.elapsed_us, s.elapsed_us);
        assert_eq!(back.score(), s.score());
    }

    #[test]
    fn empty_session_scores_zero_without_panicking() {
        let score = Session::new().score();
        assert_eq!(score.accuracy, 0.0);
        assert_eq!(score.speed, 0.0);
        assert_eq!(score.fluidness, None);
    }
}
