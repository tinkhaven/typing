//! The 43 progressive lessons of the Basic module.
//!
//! `basic_lessons.txt` describes each lesson as a bitmask over key *positions*
//! rather than characters, so the same lesson file teaches the right keys on all
//! 77 layouts: lesson 1 lights up the two index-finger home keys, whatever those
//! keys happen to produce.
//!
//! Each block is a `Lesson NN` header, four mask rows for the unshifted grid, a
//! blank line, four mask rows for the shifted grid, and a blank line.

use serde::{Deserialize, Serialize};

use crate::kbd::{Layout, COLS, ROWS};

/// The lesson definitions shipped with Klavaro.
pub const BASIC_LESSONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/klavaro-data/basic_lessons.txt"
));

/// Upstream's cap on characters in one lesson (`src/basic.c:35`).
pub const MAX_CHAR_SET: usize = ROWS * 2 * COLS;

/// Which key positions one lesson practises.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lesson {
    /// The lesson's number, starting at 1.
    pub number: u32,
    lower: [[bool; COLS]; ROWS],
    upper: [[bool; COLS]; ROWS],
}

impl Lesson {
    /// The characters this lesson practises on a given layout.
    ///
    /// Shifted keys contribute their *lowercased* character, as upstream does:
    /// the Basic module never asks for Shift, so the shifted number row teaches
    /// `!` and `?` rather than capitals. A character enabled in both grids is
    /// therefore listed twice, which is deliberate — the drill generator samples
    /// from this list, so a repeat makes that key come up more often.
    pub fn char_set(&self, layout: &Layout) -> Vec<char> {
        let mut out = Vec::new();
        for (mask, shifted) in [(&self.lower, false), (&self.upper, true)] {
            for (row, mask_row) in mask.iter().enumerate() {
                for (col, &enabled) in mask_row.iter().enumerate() {
                    if !enabled {
                        continue;
                    }
                    let key = if shifted {
                        layout.upper(row, col)
                    } else {
                        layout.lower(row, col)
                    };
                    if let Some(ch) = key {
                        let lowered = ch.to_lowercase().next().unwrap_or(ch);
                        out.push(lowered);
                    }
                }
            }
            if out.len() >= MAX_CHAR_SET {
                out.truncate(MAX_CHAR_SET);
                break;
            }
        }
        out
    }

    /// Whether a key position is part of this lesson, in either grid.
    pub fn includes(&self, row: usize, col: usize) -> bool {
        self.lower
            .get(row)
            .and_then(|r| r.get(col))
            .copied()
            .unwrap_or(false)
            || self
                .upper
                .get(row)
                .and_then(|r| r.get(col))
                .copied()
                .unwrap_or(false)
    }
}

/// Why the lesson file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LessonError {
    /// A lesson block did not contain eight mask rows.
    WrongMaskCount {
        /// The lesson being read.
        number: u32,
        /// How many mask rows were found.
        found: usize,
    },
    /// A header line was not of the form `Lesson NN`.
    BadHeader {
        /// The offending line.
        line: String,
    },
    /// The file contained no lessons at all.
    Empty,
}

impl core::fmt::Display for LessonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LessonError::WrongMaskCount { number, found } => {
                write!(f, "lesson {number}: expected 8 mask rows, found {found}")
            }
            LessonError::BadHeader { line } => {
                write!(f, "expected a line like \"Lesson 01\", found {line:?}")
            }
            LessonError::Empty => write!(f, "no lessons found"),
        }
    }
}

impl std::error::Error for LessonError {}

/// Reads `basic_lessons.txt`.
pub fn parse_lessons(src: &str) -> Result<Vec<Lesson>, LessonError> {
    let mut lessons = Vec::new();
    let mut number: Option<u32> = None;
    let mut masks: Vec<[bool; COLS]> = Vec::new();

    let finish = |number: u32, masks: &[[bool; COLS]]| -> Result<Lesson, LessonError> {
        if masks.len() != ROWS * 2 {
            return Err(LessonError::WrongMaskCount {
                number,
                found: masks.len(),
            });
        }
        let mut lower = [[false; COLS]; ROWS];
        let mut upper = [[false; COLS]; ROWS];
        lower.copy_from_slice(&masks[..ROWS]);
        upper.copy_from_slice(&masks[ROWS..]);
        Ok(Lesson {
            number,
            lower,
            upper,
        })
    };

    for line in src.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Lesson") {
            if let Some(previous) = number.take() {
                lessons.push(finish(previous, &masks)?);
            }
            masks.clear();
            number = Some(rest.trim().parse().map_err(|_| LessonError::BadHeader {
                line: trimmed.to_owned(),
            })?);
            continue;
        }
        if number.is_none() {
            return Err(LessonError::BadHeader {
                line: trimmed.to_owned(),
            });
        }
        let mut mask = [false; COLS];
        for (col, ch) in trimmed.chars().take(COLS).enumerate() {
            mask[col] = ch == '1';
        }
        masks.push(mask);
    }

    if let Some(last) = number {
        lessons.push(finish(last, &masks)?);
    }
    if lessons.is_empty() {
        return Err(LessonError::Empty);
    }
    Ok(lessons)
}

/// The lessons shipped with Klavaro, in order.
pub fn klavaro_lessons() -> Vec<Lesson> {
    parse_lessons(BASIC_LESSONS).expect("bundled lessons must parse")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kbd::Layout;

    const QWERTY_US: &str = concat!(
        "`1234567890-= \n",
        "qwertyuiop[]\\ \n",
        "asdfghjkl;'   \n",
        " zxcvbnm,./   \n",
        "~!@#$%^&*()_+ \n",
        "QWERTYUIOP{}| \n",
        "ASDFGHJKL:\"   \n",
        " ZXCVBNM<>?   \n",
    );

    fn qwerty() -> Layout {
        Layout::parse("qwerty_us", QWERTY_US).unwrap()
    }

    #[test]
    fn bundled_file_holds_forty_three_lessons() {
        let lessons = klavaro_lessons();
        assert_eq!(lessons.len(), 43);
        assert_eq!(lessons[0].number, 1);
        assert_eq!(lessons[42].number, 43);
    }

    #[test]
    fn lesson_one_teaches_the_index_finger_home_keys() {
        let lessons = klavaro_lessons();
        assert_eq!(lessons[0].char_set(&qwerty()), vec!['f', 'j']);
    }

    #[test]
    fn lesson_two_moves_to_the_middle_fingers() {
        let lessons = klavaro_lessons();
        assert_eq!(lessons[1].char_set(&qwerty()), vec!['d', 'k']);
    }

    #[test]
    fn lessons_walk_the_keyboard_row_by_row() {
        // The curriculum is grouped by region rather than cumulative: the home
        // row first, then the row above, then below, then digits, then symbols.
        let lessons = klavaro_lessons();
        let layout = qwerty();
        let set = |n: usize| lessons[n - 1].char_set(&layout).iter().collect::<String>();

        assert_eq!(set(10), "asdfghjkl;", "lesson 10 completes the home row");
        assert_eq!(set(20), "qwertyuiop", "lesson 20 completes the row above");
        assert_eq!(set(30), "zxcvbnm,./", "lesson 30 completes the row below");
        assert_eq!(set(35), "1234567890", "lesson 35 completes the digits");
        assert!(
            set(41).chars().all(|c| !c.is_alphanumeric()),
            "the last lessons are symbols only"
        );
    }

    #[test]
    fn a_key_enabled_in_both_grids_is_listed_twice() {
        // Lesson 42 switches on the same positions unshifted and shifted. Both
        // contribute, so those keys come up twice as often in the drill — which
        // is upstream's behaviour and worth pinning down.
        let lessons = klavaro_lessons();
        let set = lessons[41].char_set(&qwerty());
        assert_eq!(set.iter().filter(|&&c| c == 'q').count(), 2, "{set:?}");
    }

    #[test]
    fn every_lesson_offers_something_to_type() {
        let layout = qwerty();
        for lesson in klavaro_lessons() {
            let set = lesson.char_set(&layout);
            assert!(!set.is_empty(), "lesson {} is empty", lesson.number);
            assert!(
                set.len() <= MAX_CHAR_SET,
                "lesson {} overflows",
                lesson.number
            );
        }
    }

    #[test]
    fn shifted_masks_contribute_lowercased_characters() {
        // Enable only the shifted '1' key, which produces '!' on qwerty_us.
        let src = concat!(
            "Lesson 01\n",
            "00000000000000\n00000000000000\n00000000000000\n00000000000000\n",
            "\n",
            "01000000000000\n00000000000000\n00000000000000\n00000000000000\n",
            "\n",
        );
        let lessons = parse_lessons(src).unwrap();
        assert_eq!(lessons[0].char_set(&qwerty()), vec!['!']);
    }

    #[test]
    fn rejects_a_truncated_block() {
        let src = "Lesson 01\n00000000000000\n";
        assert!(matches!(
            parse_lessons(src),
            Err(LessonError::WrongMaskCount {
                number: 1,
                found: 1
            })
        ));
    }

    #[test]
    fn rejects_masks_without_a_header() {
        assert!(matches!(
            parse_lessons("00000000000000\n"),
            Err(LessonError::BadHeader { .. })
        ));
    }

    #[test]
    fn rejects_an_empty_file() {
        assert_eq!(parse_lessons(""), Err(LessonError::Empty));
    }
}
