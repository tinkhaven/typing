//! The shared top-10 board.
//!
//! Klavaro has an online "contest" (`src/top10.c`) that posts results to a
//! central server. This is the same idea with the same shape: a board per module
//! per language, ranked on speed, and open only to results that already cleared
//! the module's accuracy goal — so it is a race between accurate typists rather
//! than a prize for the sloppiest.
//!
//! # What is stored
//!
//! A nickname the visitor typed, three numbers, and a date. No account, no email,
//! no IP address, and every row carries a one-year TTL so the table prunes itself.
//! A nickname is pseudonymous and freely chosen, which keeps the personal-data
//! footprint about as small as a leaderboard can have.
//!
//! # Storage
//!
//! DynamoDB on-demand: a leaderboard is a handful of reads and writes a day, and
//! this avoids running a database for it. One table, partitioned by
//! `module#language`, with a sort key built so that a plain forward query returns
//! the fastest first — see [`sort_key`].

use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use typing_core::goals::Module;

use crate::protocol::BoardEntry;

/// Rows returned for a board.
pub const BOARD_SIZE: i32 = 10;

/// How long a row lives before DynamoDB removes it.
pub const ROW_TTL_SECONDS: u64 = 365 * 24 * 60 * 60;

/// A score is scaled by this before being folded into the sort key.
const SPEED_SCALE: f64 = 100.0;

/// Speeds above this cannot be ordered; [`super::verify`] rejects them first.
const SPEED_CEILING: f64 = 10_000.0;

/// One result to record.
#[derive(Clone, Debug, PartialEq)]
pub struct Submission {
    /// Which module.
    pub module: Module,
    /// Which corpus language.
    pub language: String,
    /// The chosen display name, already cleaned.
    pub nickname: String,
    /// Words per minute.
    pub speed: f64,
    /// Accuracy percentage.
    pub accuracy: f64,
    /// Fluidness percentage, where measured.
    pub fluidness: Option<f64>,
}

/// Something went wrong talking to the store.
#[derive(Debug)]
pub enum BoardError {
    /// The underlying store failed.
    Store(String),
}

impl core::fmt::Display for BoardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BoardError::Store(msg) => write!(f, "leaderboard store: {msg}"),
        }
    }
}

impl std::error::Error for BoardError {}

/// The partition a board lives in.
pub fn partition_key(module: Module, language: &str) -> String {
    format!("{}#{}", module.slug(), language)
}

/// A sort key that puts the fastest result first under a forward query.
///
/// DynamoDB orders sort keys ascending, so the key is the *complement* of the
/// speed: faster becomes smaller. Zero-padding to a fixed width keeps the
/// ordering lexicographic, which is the only ordering a string sort key has.
/// The timestamp suffix breaks ties in favour of whoever got there first and
/// keeps two identical scores from overwriting each other.
pub fn sort_key(speed: f64, achieved_at_secs: u64) -> String {
    let scaled = (speed.clamp(0.0, SPEED_CEILING) * SPEED_SCALE).round() as u64;
    let complement = (SPEED_CEILING * SPEED_SCALE) as u64 - scaled;
    format!("{complement:08}#{achieved_at_secs:011}")
}

/// Seconds since the Unix epoch, or 0 if the clock is before it.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Formats a Unix timestamp as `YYYY-MM-DD`.
///
/// Uses Hinnant's civil-from-days algorithm rather than pulling in a date crate
/// for one function. Valid for any date after 1970.
pub fn civil_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Where boards are kept.
pub enum Leaderboard {
    /// DynamoDB, for the hosted deployment.
    Dynamo {
        /// The SDK client.
        client: Box<aws_sdk_dynamodb::Client>,
        /// Table name.
        table: String,
    },
    /// In this process only — used for local development and tests.
    ///
    /// Results vanish on restart and are not shared between tasks, which is why
    /// the hosted deployment must configure a table.
    Memory(Mutex<BTreeMap<String, BTreeMap<String, Submission>>>),
}

impl Leaderboard {
    /// Builds a board from the environment.
    ///
    /// With `LEADERBOARD_TABLE` set, talks to DynamoDB. Without it, keeps results
    /// in memory and says so, so a misconfigured deployment is obvious in the log
    /// rather than silently losing every result.
    pub async fn from_env() -> Leaderboard {
        match std::env::var("LEADERBOARD_TABLE") {
            Ok(table) if !table.is_empty() => {
                let config = aws_config::load_from_env().await;
                tracing::info!(table, "leaderboard: DynamoDB");
                Leaderboard::Dynamo {
                    client: Box::new(aws_sdk_dynamodb::Client::new(&config)),
                    table,
                }
            }
            _ => {
                tracing::warn!(
                    "leaderboard: in-memory (set LEADERBOARD_TABLE to persist results)"
                );
                Leaderboard::Memory(Mutex::new(BTreeMap::new()))
            }
        }
    }

    /// An in-memory board, for tests.
    pub fn in_memory() -> Leaderboard {
        Leaderboard::Memory(Mutex::new(BTreeMap::new()))
    }

    /// The top [`BOARD_SIZE`] results for a module and language, fastest first.
    pub async fn top(
        &self,
        module: Module,
        language: &str,
    ) -> Result<Vec<BoardEntry>, BoardError> {
        match self {
            Leaderboard::Memory(store) => {
                let store = store.lock().expect("leaderboard mutex");
                let rows = store
                    .get(&partition_key(module, language))
                    .map(|by_key| {
                        by_key
                            .values()
                            .take(BOARD_SIZE as usize)
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Ok(rows
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| BoardEntry {
                        rank: i as u32 + 1,
                        nickname: s.nickname,
                        speed: s.speed,
                        accuracy: s.accuracy,
                        fluidness: s.fluidness,
                        achieved_on: civil_date(now_secs()),
                    })
                    .collect())
            }
            Leaderboard::Dynamo { client, table } => {
                let response = client
                    .query()
                    .table_name(table)
                    .key_condition_expression("board = :board")
                    .expression_attribute_values(
                        ":board",
                        aws_sdk_dynamodb::types::AttributeValue::S(partition_key(
                            module, language,
                        )),
                    )
                    .scan_index_forward(true) // sort key is a complement: ascending is fastest-first
                    .limit(BOARD_SIZE)
                    .send()
                    .await
                    .map_err(|e| BoardError::Store(e.to_string()))?;

                Ok(response
                    .items()
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let s = |key: &str| {
                            item.get(key).and_then(|v| v.as_s().ok()).cloned().unwrap_or_default()
                        };
                        let n = |key: &str| {
                            item.get(key)
                                .and_then(|v| v.as_n().ok())
                                .and_then(|v| v.parse::<f64>().ok())
                        };
                        BoardEntry {
                            rank: i as u32 + 1,
                            nickname: s("nickname"),
                            speed: n("speed").unwrap_or(0.0),
                            accuracy: n("accuracy").unwrap_or(0.0),
                            fluidness: n("fluidness"),
                            achieved_on: s("achieved_on"),
                        }
                    })
                    .collect())
            }
        }
    }

    /// Records a result and returns where it placed, if it made the board.
    pub async fn submit(&self, submission: Submission) -> Result<Option<u32>, BoardError> {
        let at = now_secs();
        let key = sort_key(submission.speed, at);
        let partition = partition_key(submission.module, &submission.language);

        match self {
            Leaderboard::Memory(store) => {
                let mut store = store.lock().expect("leaderboard mutex");
                store.entry(partition).or_default().insert(key, submission.clone());
            }
            Leaderboard::Dynamo { client, table } => {
                use aws_sdk_dynamodb::types::AttributeValue as Av;
                let mut request = client
                    .put_item()
                    .table_name(table)
                    .item("board", Av::S(partition))
                    .item("result", Av::S(key))
                    .item("nickname", Av::S(submission.nickname.clone()))
                    .item("speed", Av::N(format!("{:.2}", submission.speed)))
                    .item("accuracy", Av::N(format!("{:.2}", submission.accuracy)))
                    .item("achieved_on", Av::S(civil_date(at)))
                    .item("expires_at", Av::N((at + ROW_TTL_SECONDS).to_string()));
                if let Some(fluidness) = submission.fluidness {
                    request = request.item("fluidness", Av::N(format!("{fluidness:.2}")));
                }
                request.send().await.map_err(|e| BoardError::Store(e.to_string()))?;
            }
        }

        // Read the board back so the rank reported is the real one.
        let board = self.top(submission.module, &submission.language).await?;
        Ok(board
            .iter()
            .find(|entry| {
                entry.nickname == submission.nickname
                    && (entry.speed - submission.speed).abs() < 0.01
            })
            .map(|entry| entry.rank))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submission(nickname: &str, speed: f64) -> Submission {
        Submission {
            module: Module::Velocity,
            language: "nl".into(),
            nickname: nickname.into(),
            speed,
            accuracy: 98.0,
            fluidness: None,
        }
    }

    #[test]
    fn faster_results_sort_first() {
        let slow = sort_key(40.0, 1_000);
        let fast = sort_key(80.0, 1_000);
        assert!(fast < slow, "{fast} should sort before {slow}");
    }

    #[test]
    fn sort_keys_are_fixed_width_so_the_string_order_is_numeric() {
        // Without padding, "9" would sort after "80" and the board would be wrong.
        let keys: Vec<String> = [5.0, 50.0, 500.0].iter().map(|&s| sort_key(s, 1)).collect();
        let widths: Vec<usize> = keys.iter().map(|k| k.len()).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{keys:?}");
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(sorted, vec![keys[2].clone(), keys[1].clone(), keys[0].clone()]);
    }

    #[test]
    fn equal_speeds_favour_the_earlier_attempt() {
        let first = sort_key(60.0, 1_000);
        let second = sort_key(60.0, 2_000);
        assert!(first < second);
    }

    #[test]
    fn partitions_keep_modules_and_languages_apart() {
        assert_eq!(partition_key(Module::Velocity, "nl"), "velocity#nl");
        assert_ne!(
            partition_key(Module::Velocity, "nl"),
            partition_key(Module::Fluidness, "nl")
        );
        assert_ne!(
            partition_key(Module::Velocity, "nl"),
            partition_key(Module::Velocity, "fr")
        );
    }

    #[test]
    fn civil_date_converts_known_timestamps() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(1_000_000_000), "2001-09-09");
        assert_eq!(civil_date(1_767_225_600), "2026-01-01");
    }

    #[tokio::test]
    async fn an_empty_board_is_empty_not_an_error() {
        let board = Leaderboard::in_memory();
        assert_eq!(board.top(Module::Velocity, "nl").await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn submissions_come_back_ranked_by_speed() {
        let board = Leaderboard::in_memory();
        board.submit(submission("slow", 40.0)).await.unwrap();
        board.submit(submission("fast", 90.0)).await.unwrap();
        board.submit(submission("middling", 65.0)).await.unwrap();

        let top = board.top(Module::Velocity, "nl").await.unwrap();
        let names: Vec<&str> = top.iter().map(|e| e.nickname.as_str()).collect();
        assert_eq!(names, vec!["fast", "middling", "slow"]);
        assert_eq!(top[0].rank, 1);
        assert_eq!(top[2].rank, 3);
    }

    #[tokio::test]
    async fn submit_reports_the_rank_achieved() {
        let board = Leaderboard::in_memory();
        board.submit(submission("fast", 90.0)).await.unwrap();
        let rank = board.submit(submission("slower", 50.0)).await.unwrap();
        assert_eq!(rank, Some(2));
    }

    #[tokio::test]
    async fn boards_do_not_leak_between_languages() {
        let board = Leaderboard::in_memory();
        board.submit(submission("dutch", 70.0)).await.unwrap();
        let mut french = submission("french", 80.0);
        french.language = "fr".into();
        board.submit(french).await.unwrap();

        let nl = board.top(Module::Velocity, "nl").await.unwrap();
        assert_eq!(nl.len(), 1);
        assert_eq!(nl[0].nickname, "dutch");
    }

    #[tokio::test]
    async fn a_board_shows_at_most_ten_rows() {
        let board = Leaderboard::in_memory();
        for i in 0..25 {
            board.submit(submission(&format!("t{i}"), 30.0 + i as f64)).await.unwrap();
        }
        let top = board.top(Module::Velocity, "nl").await.unwrap();
        assert_eq!(top.len(), BOARD_SIZE as usize);
        assert_eq!(top[0].nickname, "t24", "fastest first");
    }
}
