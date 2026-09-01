//! Loading Klavaro's practice text at startup.
//!
//! The corpora are 1.7 MB across 38 languages, so they are neither embedded in
//! the WASM bundle nor read per request: the server parses them once into memory
//! and serves the one language a visitor needs as JSON, which the browser then
//! caches. The keyboard layouts go the other way — small enough to embed, and
//! needed to draw the virtual keyboard before any request completes.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use typing_core::corpus::Corpus;

/// Where the data files live, unless `KLAVARO_DATA_DIR` says otherwise.
pub const DEFAULT_DATA_DIR: &str = "assets/klavaro-data";

/// Every language whose practice text was found, keyed by language code.
#[derive(Debug, Default)]
pub struct Corpora {
    by_language: BTreeMap<String, Arc<Corpus>>,
}

impl Corpora {
    /// Reads every `*.words` / `*.paragraphs` pair under `dir/corpora`.
    ///
    /// A language needs both files to be usable; one without the other is skipped
    /// with a warning rather than failing startup, since a partial data directory
    /// should degrade to fewer languages, not to no service.
    pub fn load(dir: &Path) -> io::Result<Corpora> {
        let corpora_dir = dir.join("corpora");
        let mut words: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut paragraphs: BTreeMap<String, PathBuf> = BTreeMap::new();

        for entry in fs::read_dir(&corpora_dir)? {
            let path = entry?.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            match path.extension().and_then(|e| e.to_str()) {
                Some("words") => {
                    words.insert(stem.to_owned(), path);
                }
                Some("paragraphs") => {
                    paragraphs.insert(stem.to_owned(), path);
                }
                _ => {}
            }
        }

        let mut by_language = BTreeMap::new();
        for (language, words_path) in words {
            let Some(paragraphs_path) = paragraphs.get(&language) else {
                tracing::warn!(language, "skipping: no .paragraphs file");
                continue;
            };
            let corpus = Corpus::new(
                &language,
                &fs::read_to_string(&words_path)?,
                &fs::read_to_string(paragraphs_path)?,
            );
            if corpus.is_usable() {
                by_language.insert(language, Arc::new(corpus));
            } else {
                tracing::warn!(language, "skipping: no usable text");
            }
        }

        if by_language.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no usable corpora under {}", corpora_dir.display()),
            ));
        }
        Ok(Corpora { by_language })
    }

    /// Reads from `KLAVARO_DATA_DIR`, or [`DEFAULT_DATA_DIR`].
    pub fn from_env() -> io::Result<Corpora> {
        let dir = std::env::var("KLAVARO_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.into());
        Corpora::load(Path::new(&dir))
    }

    /// The practice text for a language, if it was loaded.
    pub fn get(&self, language: &str) -> Option<Arc<Corpus>> {
        self.by_language.get(language).cloned()
    }

    /// The language codes available, sorted.
    pub fn languages(&self) -> Vec<&str> {
        self.by_language.keys().map(String::as_str).collect()
    }

    /// How many languages were loaded.
    pub fn len(&self) -> usize {
        self.by_language.len()
    }

    /// Whether nothing was loaded.
    pub fn is_empty(&self) -> bool {
        self.by_language.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/klavaro-data")
    }

    #[test]
    fn loads_the_bundled_corpora() {
        let corpora = Corpora::load(&data_dir()).expect("bundled data loads");
        assert!(corpora.len() >= 30, "only {} languages", corpora.len());
        for language in ["en_GB", "nl", "fr", "de"] {
            let corpus = corpora.get(language).unwrap_or_else(|| {
                panic!("missing corpus {language}, have {:?}", corpora.languages())
            });
            assert!(corpus.words.len() > 100, "{language} has too few words");
            assert!(
                !corpus.paragraphs.is_empty(),
                "{language} has no paragraphs"
            );
        }
    }

    #[test]
    fn unknown_languages_are_none() {
        let corpora = Corpora::load(&data_dir()).unwrap();
        assert!(corpora.get("klingon").is_none());
    }

    #[test]
    fn a_missing_directory_is_an_error_not_a_panic() {
        assert!(Corpora::load(Path::new("/nonexistent/klavaro")).is_err());
    }
}
