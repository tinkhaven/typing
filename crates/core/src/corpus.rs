//! Practice text: word lists and paragraphs, per language.
//!
//! Klavaro ships two files per language. `*.words` is one word per line, used by
//! the Velocity module to build sentences of real words with no punctuation to
//! slow you down. `*.paragraphs` is prose separated by blank lines, used by the
//! Fluidness module because even rhythm only shows up over real sentences.

use serde::{Deserialize, Serialize};

/// Upstream's cap on paragraphs taken from one file (`src/fluidness.h:18`).
pub const MAX_PARAGRAPHS: usize = 100;

/// The practice text for one language.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpus {
    /// The language code the files were named for, e.g. `nl`.
    pub language: String,
    /// Individual words, in file order.
    pub words: Vec<String>,
    /// Paragraphs of prose, in file order.
    pub paragraphs: Vec<String>,
}

impl Corpus {
    /// Builds a corpus from the two file bodies.
    pub fn new(language: &str, words: &str, paragraphs: &str) -> Corpus {
        Corpus {
            language: language.to_owned(),
            words: parse_words(words),
            paragraphs: parse_paragraphs(paragraphs),
        }
    }

    /// Whether there is enough text to generate from.
    pub fn is_usable(&self) -> bool {
        !self.words.is_empty() && !self.paragraphs.is_empty()
    }
}

/// Reads a `*.words` file: one word per line, blanks skipped.
pub fn parse_words(src: &str) -> Vec<String> {
    src.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reads a `*.paragraphs` file: prose separated by blank lines.
///
/// Line breaks inside a paragraph are folded to spaces, because the typing view
/// wraps text itself and a hard break would ask the typist for a newline that is
/// an artefact of the source file rather than of the prose.
pub fn parse_paragraphs(src: &str) -> Vec<String> {
    let normalised = src.replace("\r\n", "\n").replace('\r', "\n");
    normalised
        .split("\n\n")
        .map(|block| block.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|paragraph| !paragraph.is_empty())
        .take(MAX_PARAGRAPHS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_one_word_per_line() {
        let words = parse_words("the\nquick\n\n  brown  \nfox\n");
        assert_eq!(words, vec!["the", "quick", "brown", "fox"]);
    }

    #[test]
    fn splits_paragraphs_on_blank_lines() {
        let src = "First one.\n\nSecond one,\nwrapped across lines.\n\n\nThird.\n";
        assert_eq!(
            parse_paragraphs(src),
            vec!["First one.", "Second one, wrapped across lines.", "Third.",]
        );
    }

    #[test]
    fn handles_windows_line_endings() {
        assert_eq!(parse_paragraphs("a\r\n\r\nb\r\n"), vec!["a", "b"]);
    }

    #[test]
    fn caps_paragraph_count() {
        let src = (0..250)
            .map(|i| format!("p{i}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        assert_eq!(parse_paragraphs(&src).len(), MAX_PARAGRAPHS);
    }

    #[test]
    fn empty_input_yields_nothing_usable() {
        let corpus = Corpus::new("xx", "", "");
        assert!(corpus.words.is_empty());
        assert!(corpus.paragraphs.is_empty());
        assert!(!corpus.is_usable());
    }
}
