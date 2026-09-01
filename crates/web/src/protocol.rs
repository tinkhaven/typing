//! The WebSocket protocol between the browser and the server.
//!
//! # Why a socket at all
//!
//! Keystrokes are evaluated in the browser, never over the network — fluidness is
//! the variance of the gaps between keystrokes, so measuring it across a
//! connection would measure the connection. The socket exists for the things that
//! genuinely need a server and genuinely benefit from being pushed rather than
//! polled:
//!
//! 1. **A seed, not a text.** The client picks a seed and generates the exercise
//!    from it locally, so typing starts with no round trip and carries on if the
//!    connection drops. It tells the server the seed, and the server regenerates
//!    the same text to learn exactly what was on screen.
//! 2. **A keystroke stream.** The client forwards batched keystroke outcomes as it
//!    goes, so the server keeps its *own* tally. Leaderboard entries are scored
//!    from the server's tally, not from a number the client reported.
//! 3. **A live board.** Rankings are pushed when they change.
//!
//! Everything here is `serde`-JSON. The volumes are small — a batch of twenty
//! keystrokes is a few hundred bytes — and being able to read the traffic in
//! browser dev tools is worth more than the bytes a binary format would save.

use serde::{Deserialize, Serialize};
use typing_core::{
    goals::Module,
    stats::{Score, Session},
    typist::Counted,
};

/// How often the client flushes keystrokes to the server.
pub const TOUCH_BATCH_SIZE: usize = 20;

/// How often the client sends a keep-alive when idle.
///
/// An ALB closes an idle connection after its own timeout; a ping well inside
/// that keeps the socket up without the typist noticing a reconnect.
pub const PING_INTERVAL_MS: u32 = 30_000;

/// Longest nickname accepted for the leaderboard.
pub const MAX_NICKNAME_LEN: usize = 24;

/// What one keystroke did, as reported by the client.
///
/// This is the wire form of [`Counted`] — the session transition the client's
/// state machine actually performed. Deriving it from `Counted` rather than from
/// `Press` is deliberate: the same `Press` means different things in the two
/// correction modes, so mapping it here would let the two sides drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TouchKind {
    /// Matched first time.
    Correct,
    /// Did not match.
    Wrong,
    /// Matched after a correction.
    Retouched,
    /// A fumble or backspace in correction mode: counts for nothing.
    Stumble,
}

impl From<Counted> for TouchKind {
    fn from(counted: Counted) -> TouchKind {
        match counted {
            Counted::Correct => TouchKind::Correct,
            Counted::Wrong => TouchKind::Wrong,
            Counted::Retouched => TouchKind::Retouched,
            Counted::Stumble => TouchKind::Stumble,
        }
    }
}

impl From<TouchKind> for Counted {
    fn from(kind: TouchKind) -> Counted {
        match kind {
            TouchKind::Correct => Counted::Correct,
            TouchKind::Wrong => Counted::Wrong,
            TouchKind::Retouched => Counted::Retouched,
            TouchKind::Stumble => Counted::Stumble,
        }
    }
}

/// One keystroke: what it did, and how long after the previous one it landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Touch {
    /// The outcome.
    pub kind: TouchKind,
    /// Microseconds since the previous keystroke, or since the session started.
    pub dt_us: u32,
}

/// Messages the browser sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientMessage {
    /// Announce the exercise about to be typed.
    ///
    /// The client has already generated it; the seed lets the server reproduce
    /// the same text and so know how many characters a finished run must have.
    Start {
        /// Which module is being practised.
        module: Module,
        /// Keyboard layout name, e.g. `azerty_be`.
        layout: String,
        /// Corpus language code, e.g. `nl`.
        language: String,
        /// Basic lesson number, 1-based. Ignored by other modules.
        lesson: Option<u32>,
        /// The seed the client generated the exercise from.
        seed: u64,
    },
    /// Forward a batch of keystrokes.
    Touches {
        /// The keystrokes, oldest first.
        touches: Vec<Touch>,
    },
    /// The exercise is over; flush and score.
    Finish {
        /// The client's own tally, kept only to compare against the server's.
        ///
        /// The leaderboard never uses this. It is here so a mismatch can be
        /// logged, which is how a broken client or a lost batch gets noticed.
        client_session: Session,
    },
    /// Follow a leaderboard.
    WatchBoard {
        /// Which module's board.
        module: Module,
        /// Which language's board.
        language: String,
    },
    /// Publish the last result under a nickname.
    Publish {
        /// The name to show. Trimmed and truncated by the server.
        nickname: String,
    },
    /// Keep the connection alive.
    Ping,
}

/// Messages the server sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerMessage {
    /// The announced exercise was reproduced and the server is following along.
    Following {
        /// Characters the server's copy of the exercise contains.
        ///
        /// The client compares this with its own text. A mismatch means the two
        /// sides disagree about the exercise — a version skew between the WASM
        /// bundle and the server — and the client stops reporting rather than
        /// sending keystrokes the server will reject.
        expected_chars: u32,
    },
    /// The session, scored by the server.
    Scored {
        /// The score, from the server's own tally.
        score: Score,
        /// Whether the module's goals were met.
        goals_met: bool,
        /// Whether this result may be published to a board.
        publishable: bool,
        /// Where it would place, if published.
        would_rank: Option<u32>,
    },
    /// A leaderboard, in full.
    Board {
        /// Which module.
        module: Module,
        /// Which language.
        language: String,
        /// Entries, best first.
        entries: Vec<BoardEntry>,
    },
    /// The request could not be served.
    Problem {
        /// A short machine-readable reason.
        code: ProblemCode,
        /// Something to show the visitor.
        detail: String,
    },
    /// Reply to [`ClientMessage::Ping`].
    Pong,
}

/// Why a request was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemCode {
    /// No such keyboard layout.
    UnknownLayout,
    /// No corpus for that language.
    UnknownLanguage,
    /// Lesson number out of range.
    UnknownLesson,
    /// A message arrived out of order, e.g. keystrokes before a start.
    OutOfOrder,
    /// The reported session could not have come from the exercise issued.
    Implausible,
    /// The nickname was empty or unusable.
    BadNickname,
    /// The leaderboard store is unavailable.
    StoreUnavailable,
}

/// One row of a leaderboard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoardEntry {
    /// Position, starting at 1.
    pub rank: u32,
    /// The nickname the visitor chose.
    pub nickname: String,
    /// Words per minute.
    pub speed: f64,
    /// Accuracy percentage.
    pub accuracy: f64,
    /// Fluidness percentage, where the module measures it.
    pub fluidness: Option<f64>,
    /// When it was set, as an RFC 3339 date.
    pub achieved_on: String,
}

/// Cleans a nickname for display and storage.
///
/// Control characters are dropped and whitespace collapsed, so a nickname cannot
/// smuggle newlines or invisible padding into the board. Returns `None` if nothing
/// usable is left.
pub fn clean_nickname(raw: &str) -> Option<String> {
    // Control characters become spaces rather than vanishing: dropping the
    // newline from "two\nlines" would silently rename someone to "twolines".
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned: String = cleaned.chars().take(MAX_NICKNAME_LEN).collect();
    let cleaned = cleaned.trim().to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Rebuilds a [`Session`] from a stream of keystrokes.
///
/// This is the server's tally: the same transitions the browser applied, replayed
/// from the reported outcomes and gaps. Feeding both sides through
/// [`typing_core::stats::Session`] is what keeps the two scores comparable.
pub fn replay(touches: &[Touch]) -> Session {
    let mut session = Session::new();
    let mut at_us: u64 = 0;
    apply_all(&mut session, touches, &mut at_us);
    session
}

/// Folds keystrokes into an existing session, advancing `at_us` as it goes.
///
/// The server receives keystrokes in batches, so it needs to continue a session
/// rather than build one. Doing that by replaying each batch on its own would
/// silently lose a gap at every batch boundary — [`Session::correct`] records a
/// gap only once it has a previous keystroke to measure from — which is why the
/// live path and [`replay`] share this one loop.
pub fn apply_all(session: &mut Session, touches: &[Touch], at_us: &mut u64) {
    for touch in touches {
        *at_us += u64::from(touch.dt_us);
        Counted::from(touch.kind).apply(session, *at_us);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_as_json() {
        let start = ClientMessage::Start {
            module: Module::Velocity,
            layout: "azerty_be".into(),
            language: "nl".into(),
            lesson: None,
            seed: 0xC0FFEE,
        };
        let json = serde_json::to_string(&start).unwrap();
        assert!(json.contains("\"type\":\"start\""), "{json}");
        assert_eq!(serde_json::from_str::<ClientMessage>(&json).unwrap(), start);

        let problem = ServerMessage::Problem {
            code: ProblemCode::UnknownLayout,
            detail: "no such layout".into(),
        };
        let json = serde_json::to_string(&problem).unwrap();
        assert!(json.contains("unknown-layout"), "{json}");
        assert_eq!(serde_json::from_str::<ServerMessage>(&json).unwrap(), problem);
    }

    #[test]
    fn replay_rebuilds_the_same_tally_the_client_had() {
        let touches = vec![
            Touch { kind: TouchKind::Correct, dt_us: 150_000 },
            Touch { kind: TouchKind::Correct, dt_us: 150_000 },
            Touch { kind: TouchKind::Wrong, dt_us: 300_000 },
            Touch { kind: TouchKind::Correct, dt_us: 150_000 },
        ];
        let session = replay(&touches);
        assert_eq!(session.touches, 4);
        assert_eq!(session.errors, 1);
        assert_eq!(session.elapsed_us, 750_000);
        assert!((session.score().accuracy - 75.0).abs() < 1e-9);
    }

    #[test]
    fn replay_honours_stumbles_and_retouches() {
        let touches = vec![
            Touch { kind: TouchKind::Correct, dt_us: 100_000 },
            Touch { kind: TouchKind::Correct, dt_us: 100_000 },
            Touch { kind: TouchKind::Stumble, dt_us: 2_000_000 },
            Touch { kind: TouchKind::Stumble, dt_us: 100_000 },
            Touch { kind: TouchKind::Retouched, dt_us: 100_000 },
        ];
        let session = replay(&touches);
        assert_eq!(session.touches, 3, "stumbles are not touches");
        assert_eq!(session.errors, 1);
        assert_eq!(session.intervals_us, vec![100_000, 100_000]);
    }

    #[test]
    fn nicknames_are_cleaned_not_trusted() {
        assert_eq!(clean_nickname("  Dieter  "), Some("Dieter".into()));
        assert_eq!(clean_nickname("two\nlines"), Some("two lines".into()));
        assert_eq!(clean_nickname("a\u{0}b"), Some("a b".into()));
        assert_eq!(clean_nickname("   "), None);
        assert_eq!(clean_nickname(""), None);
        let long = "x".repeat(100);
        assert_eq!(clean_nickname(&long).unwrap().chars().count(), MAX_NICKNAME_LEN);
    }
}
