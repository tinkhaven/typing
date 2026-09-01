//! Keyboard layouts and finger assignment.
//!
//! A Klavaro layout file (`*.kbd`) is eight lines of fourteen columns: four rows
//! unshifted, then the same four rows shifted. A space means "no key here", which
//! is why the bottom row of `qwerty_us.kbd` starts with one — column zero is
//! where Shift sits.
//!
//! Ten of the 77 upstream files have had trailing spaces stripped, so rows can
//! be shorter than fourteen columns. The C reader indexes a fixed-size buffer and
//! sees NUL past the end, which is simply "no key"; [`Layout::parse`] pads to the
//! same effect rather than rejecting the file.

use serde::{Deserialize, Serialize};

/// Rows of keys in a layout, excluding the space bar.
pub const ROWS: usize = 4;

/// Key positions per row.
pub const COLS: usize = 14;

/// Characters Klavaro treats as vowels when inventing nonsense words.
///
/// From `src/keyboard.c:73`: Latin, Greek and Cyrillic. Upstream also lists
/// Tibetan vowels, which are omitted here along with the rest of the Tibetan and
/// Urdu special-casing — see `docs/DIVERGENCE.md`.
pub const VOWELS: [char; 15] = [
    'a', 'e', 'i', 'o', 'u', // Latin
    'α', 'ε', 'ι', 'ο', 'υ', // Greek
    'а', 'е', 'и', 'о', 'у', // Cyrillic
];

/// The finger expected to press a key.
///
/// Numbered as Klavaro numbers its key colours, left little finger to right
/// little finger, with both thumbs sharing 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Finger {
    /// Left little finger.
    LeftLittle = 1,
    /// Left ring finger.
    LeftRing = 2,
    /// Left middle finger.
    LeftMiddle = 3,
    /// Left index finger.
    LeftIndex = 4,
    /// Either thumb; the space bar.
    Thumb = 5,
    /// Right index finger.
    RightIndex = 6,
    /// Right middle finger.
    RightMiddle = 7,
    /// Right ring finger.
    RightRing = 8,
    /// Right little finger.
    RightLittle = 9,
}

impl Finger {
    /// Maps the digits used in `fingers_position.txt`.
    pub fn from_digit(d: char) -> Option<Finger> {
        Some(match d {
            '1' => Finger::LeftLittle,
            '2' => Finger::LeftRing,
            '3' => Finger::LeftMiddle,
            '4' => Finger::LeftIndex,
            '5' => Finger::Thumb,
            '6' => Finger::RightIndex,
            '7' => Finger::RightMiddle,
            '8' => Finger::RightRing,
            '9' => Finger::RightLittle,
            _ => return None,
        })
    }

    /// The colour slot for this finger, matching Klavaro's `key_1`…`key_9`.
    pub fn slot(self) -> u8 {
        self as u8
    }

    /// Which hand the finger belongs to; thumbs belong to neither in particular.
    pub fn hand(self) -> Hand {
        match self {
            Finger::Thumb => Hand::Either,
            f if f.slot() < 5 => Hand::Left,
            _ => Hand::Right,
        }
    }
}

/// Which hand a finger belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    /// Left hand.
    Left,
    /// Right hand.
    Right,
    /// Either hand — the thumbs.
    Either,
}

/// Klavaro's finger map: which finger owns each of the 4×14 key positions.
///
/// This is `data/fingers_position.txt`, one map shared by every layout, because
/// finger assignment follows the physical key position rather than the legend
/// printed on it.
pub const FINGERS_POSITION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/klavaro-data/fingers_position.txt"
));

/// Where a character sits on a layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPos {
    /// Row, `0` being the digit row.
    pub row: usize,
    /// Column within the row.
    pub col: usize,
    /// Whether Shift is needed.
    pub shifted: bool,
}

/// Why a layout or finger map could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// A layout needs eight lines: four unshifted rows then four shifted.
    WrongRowCount {
        /// The layout or file being read.
        name: String,
        /// How many non-empty lines were found.
        found: usize,
    },
    /// A row had more than [`COLS`] columns.
    RowTooWide {
        /// The layout or file being read.
        name: String,
        /// Which row.
        row: usize,
        /// How many columns it had.
        found: usize,
    },
    /// The finger map contained something other than the digits `1`–`9`.
    BadFingerDigit {
        /// Which row.
        row: usize,
        /// Which column.
        col: usize,
        /// The offending character.
        found: char,
    },
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::WrongRowCount { name, found } => write!(
                f,
                "layout {name}: expected {} rows (4 unshifted + 4 shifted), found {found}",
                ROWS * 2
            ),
            ParseError::RowTooWide { name, row, found } => {
                write!(
                    f,
                    "layout {name}: row {row} has {found} columns, maximum is {COLS}"
                )
            }
            ParseError::BadFingerDigit { row, col, found } => write!(
                f,
                "finger map: expected a digit 1-9 at row {row} column {col}, found {found:?}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// A keyboard layout: which character each key position produces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// The layout's file stem, e.g. `azerty_be`.
    pub name: String,
    lower: [[Option<char>; COLS]; ROWS],
    upper: [[Option<char>; COLS]; ROWS],
}

impl Layout {
    /// Reads a `*.kbd` file body.
    pub fn parse(name: &str, src: &str) -> Result<Layout, ParseError> {
        let rows: Vec<&str> = src.lines().filter(|l| !l.trim().is_empty()).collect();
        if rows.len() != ROWS * 2 {
            return Err(ParseError::WrongRowCount {
                name: name.to_owned(),
                found: rows.len(),
            });
        }

        let mut grids = [[[None; COLS]; ROWS], [[None; COLS]; ROWS]];
        for (i, line) in rows.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            if chars.len() > COLS {
                return Err(ParseError::RowTooWide {
                    name: name.to_owned(),
                    row: i,
                    found: chars.len(),
                });
            }
            // Short rows simply have no keys in the missing columns.
            for (col, &ch) in chars.iter().enumerate() {
                if !ch.is_whitespace() {
                    grids[i / ROWS][i % ROWS][col] = Some(ch);
                }
            }
        }

        Ok(Layout {
            name: name.to_owned(),
            lower: grids[0],
            upper: grids[1],
        })
    }

    /// The character produced without Shift, if any key is there.
    pub fn lower(&self, row: usize, col: usize) -> Option<char> {
        *self.lower.get(row)?.get(col)?
    }

    /// The character produced with Shift, if any key is there.
    pub fn upper(&self, row: usize, col: usize) -> Option<char> {
        *self.upper.get(row)?.get(col)?
    }

    /// Finds where a character is typed, preferring the unshifted position.
    pub fn find(&self, ch: char) -> Option<KeyPos> {
        for row in 0..ROWS {
            for col in 0..COLS {
                if self.lower(row, col) == Some(ch) {
                    return Some(KeyPos {
                        row,
                        col,
                        shifted: false,
                    });
                }
            }
        }
        for row in 0..ROWS {
            for col in 0..COLS {
                if self.upper(row, col) == Some(ch) {
                    return Some(KeyPos {
                        row,
                        col,
                        shifted: true,
                    });
                }
            }
        }
        None
    }

    /// Every distinct character the layout can produce, unshifted then shifted.
    pub fn characters(&self) -> Vec<char> {
        let mut out = Vec::new();
        for grid in [&self.lower, &self.upper] {
            for row in grid {
                out.extend(row.iter().flatten().copied());
            }
        }
        out
    }

    /// Vowels available on the layout (`keyb_get_vowels`).
    pub fn vowels(&self) -> Vec<char> {
        let mut out: Vec<char> = self
            .lower
            .iter()
            .flatten()
            .flatten()
            .copied()
            .filter(|c| is_vowel(*c))
            .collect();
        out.truncate(20); // upstream's fixed buffer
        out
    }

    /// Consonants available on the layout (`keyb_get_consonants`).
    ///
    /// A shifted key contributes its lowercase form only when it differs from the
    /// unshifted key at the same position, which is how layouts with distinct
    /// shifted letters (rather than plain capitals) get picked up.
    pub fn consonants(&self) -> Vec<char> {
        let mut out = Vec::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                if let Some(c) = self.lower(row, col) {
                    if c.is_alphabetic() && !is_vowel(c) {
                        out.push(c);
                    }
                }
                if let Some(u) = self.upper(row, col) {
                    let lowered = lower_first(u);
                    if lowered.is_alphabetic()
                        && !is_vowel(lowered)
                        && Some(lowered) != self.lower(row, col)
                    {
                        out.push(lowered);
                    }
                }
            }
        }
        out
    }

    /// Punctuation and symbols available on the layout (`keyb_get_symbols`).
    ///
    /// Rust has no Unicode `ispunct` in the standard library, so this accepts any
    /// printable non-alphanumeric character. That is slightly broader than glib's
    /// category-P test, admitting symbols such as `$` and `+`; those are keys a
    /// typist has to reach for anyway, so the practice value is the same.
    pub fn symbols(&self) -> Vec<char> {
        let mut out = Vec::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                for ch in [self.lower(row, col), self.upper(row, col)]
                    .into_iter()
                    .flatten()
                {
                    if is_symbol(ch) {
                        out.push(ch);
                    }
                }
            }
        }
        out
    }
}

/// Lowercases a character, keeping only the first resulting character.
///
/// Unicode lowercasing can expand (`İ` → `i̇`); Klavaro works one code point at a
/// time, so the first is what corresponds.
fn lower_first(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

/// Whether a character counts as a vowel, ignoring case.
pub fn is_vowel(ch: char) -> bool {
    let lowered = lower_first(ch);
    VOWELS.contains(&lowered)
}

/// Whether a character is a printable non-alphanumeric — punctuation or a symbol.
pub fn is_symbol(ch: char) -> bool {
    !ch.is_alphanumeric() && !ch.is_whitespace() && !ch.is_control()
}

/// Which finger presses each key position, parsed from `fingers_position.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct FingerMap {
    fingers: [[Option<Finger>; COLS]; ROWS],
}

impl FingerMap {
    /// The map shipped with Klavaro.
    pub fn klavaro() -> FingerMap {
        FingerMap::parse(FINGERS_POSITION).expect("bundled finger map must parse")
    }

    /// Reads a finger map: [`ROWS`] lines of up to [`COLS`] digits.
    pub fn parse(src: &str) -> Result<FingerMap, ParseError> {
        let rows: Vec<&str> = src.lines().filter(|l| !l.trim().is_empty()).collect();
        if rows.len() != ROWS {
            return Err(ParseError::WrongRowCount {
                name: "fingers_position".to_owned(),
                found: rows.len(),
            });
        }

        let mut fingers = [[None; COLS]; ROWS];
        for (row, line) in rows.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            if chars.len() > COLS {
                return Err(ParseError::RowTooWide {
                    name: "fingers_position".to_owned(),
                    row,
                    found: chars.len(),
                });
            }
            for (col, &ch) in chars.iter().enumerate() {
                match Finger::from_digit(ch) {
                    Some(finger) => fingers[row][col] = Some(finger),
                    // Upstream's map uses '0' as filler past the last real key.
                    None if ch == '0' || ch.is_whitespace() => {}
                    None => {
                        return Err(ParseError::BadFingerDigit {
                            row,
                            col,
                            found: ch,
                        })
                    }
                }
            }
        }
        Ok(FingerMap { fingers })
    }

    /// The finger for a key position.
    pub fn at(&self, row: usize, col: usize) -> Option<Finger> {
        *self.fingers.get(row)?.get(col)?
    }

    /// The finger for a character on a given layout; the space bar is a thumb.
    pub fn for_char(&self, layout: &Layout, ch: char) -> Option<Finger> {
        if ch == ' ' {
            return Some(Finger::Thumb);
        }
        let pos = layout.find(ch)?;
        self.at(pos.row, pos.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        Layout::parse("qwerty_us", QWERTY_US).expect("parses")
    }

    #[test]
    fn reads_the_grid() {
        let l = qwerty();
        assert_eq!(l.lower(0, 0), Some('`'));
        assert_eq!(l.lower(2, 0), Some('a'));
        assert_eq!(l.lower(2, 3), Some('f'));
        assert_eq!(l.lower(2, 6), Some('j'));
        assert_eq!(l.upper(2, 3), Some('F'));
        // Column zero of the bottom row is where Shift sits: no key.
        assert_eq!(l.lower(3, 0), None);
        assert_eq!(l.lower(3, 1), Some('z'));
        // Padding past the end of a row.
        assert_eq!(l.lower(2, 13), None);
    }

    #[test]
    fn short_rows_are_padded_not_rejected() {
        // Trailing spaces stripped, as in 10 of the upstream files.
        let stripped = QWERTY_US.replace(" \n", "\n").replace("   \n", "\n");
        let l = Layout::parse("stripped", &stripped).expect("short rows are fine");
        assert_eq!(l.lower(2, 0), Some('a'));
        assert_eq!(l.lower(2, 13), None);
    }

    #[test]
    fn rejects_a_wrong_row_count() {
        let err = Layout::parse("truncated", "abc\ndef\n").unwrap_err();
        assert!(matches!(err, ParseError::WrongRowCount { found: 2, .. }));
    }

    #[test]
    fn rejects_an_overwide_row() {
        let wide = QWERTY_US.replace("asdfghjkl;'   ", "asdfghjkl;'aaaaaaa");
        let err = Layout::parse("wide", &wide).unwrap_err();
        assert!(
            matches!(err, ParseError::RowTooWide { row: 2, .. }),
            "{err}"
        );
    }

    #[test]
    fn finds_characters_preferring_unshifted() {
        let l = qwerty();
        assert_eq!(
            l.find('f'),
            Some(KeyPos {
                row: 2,
                col: 3,
                shifted: false
            })
        );
        assert_eq!(
            l.find('F'),
            Some(KeyPos {
                row: 2,
                col: 3,
                shifted: true
            })
        );
        assert_eq!(
            l.find('?'),
            Some(KeyPos {
                row: 3,
                col: 10,
                shifted: true
            })
        );
        assert_eq!(l.find('€'), None);
    }

    #[test]
    fn classifies_letters_and_symbols() {
        let l = qwerty();
        let vowels = l.vowels();
        for v in ['a', 'e', 'i', 'o', 'u'] {
            assert!(vowels.contains(&v), "missing vowel {v}");
        }
        let consonants = l.consonants();
        assert!(consonants.contains(&'f'));
        assert!(!consonants.contains(&'a'), "a is a vowel");
        // Plain capitals duplicate their unshifted key and must not be added again.
        assert_eq!(consonants.iter().filter(|&&c| c == 'f').count(), 1);
        let symbols = l.symbols();
        assert!(symbols.contains(&';'));
        assert!(symbols.contains(&'!'));
        assert!(!symbols.iter().any(|c| c.is_alphanumeric()));
    }

    #[test]
    fn vowels_ignore_case_and_script() {
        assert!(is_vowel('A'));
        assert!(is_vowel('α'));
        assert!(is_vowel('и'));
        assert!(!is_vowel('f'));
        assert!(!is_vowel('1'));
    }

    #[test]
    fn bundled_finger_map_matches_the_home_row() {
        let map = FingerMap::klavaro();
        let l = qwerty();
        // asdf are the four left fingers, jkl; the four right ones.
        assert_eq!(map.for_char(&l, 'a'), Some(Finger::LeftLittle));
        assert_eq!(map.for_char(&l, 's'), Some(Finger::LeftRing));
        assert_eq!(map.for_char(&l, 'd'), Some(Finger::LeftMiddle));
        assert_eq!(map.for_char(&l, 'f'), Some(Finger::LeftIndex));
        assert_eq!(map.for_char(&l, 'g'), Some(Finger::LeftIndex), "reach key");
        assert_eq!(map.for_char(&l, 'h'), Some(Finger::RightIndex), "reach key");
        assert_eq!(map.for_char(&l, 'j'), Some(Finger::RightIndex));
        assert_eq!(map.for_char(&l, 'k'), Some(Finger::RightMiddle));
        assert_eq!(map.for_char(&l, 'l'), Some(Finger::RightRing));
        assert_eq!(map.for_char(&l, ';'), Some(Finger::RightLittle));
        // Shift does not change which finger presses the key.
        assert_eq!(map.for_char(&l, 'F'), Some(Finger::LeftIndex));
        assert_eq!(map.for_char(&l, ' '), Some(Finger::Thumb));
    }

    #[test]
    fn finger_hands_split_at_the_thumbs() {
        assert_eq!(Finger::LeftLittle.hand(), Hand::Left);
        assert_eq!(Finger::LeftIndex.hand(), Hand::Left);
        assert_eq!(Finger::Thumb.hand(), Hand::Either);
        assert_eq!(Finger::RightIndex.hand(), Hand::Right);
        assert_eq!(Finger::RightLittle.hand(), Hand::Right);
    }

    #[test]
    fn finger_map_rejects_junk() {
        let err = FingerMap::parse("1234\n1234\n1234\n12x4\n").unwrap_err();
        assert!(
            matches!(err, ParseError::BadFingerDigit { found: 'x', .. }),
            "{err}"
        );
    }
}
