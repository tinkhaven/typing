//! Exercise generation for the four modules.
//!
//! Text is generated from a seed rather than sent over the wire. The server
//! issues a seed, the client generates the exercise from it and can keep going
//! if the connection drops; when the session ends, the server regenerates the
//! same text from the same seed and checks the reported keystroke count against
//! it. See [`crate::rng`] for why the generator is written out longhand.
//!
//! Behaviour follows Klavaro's generators — `src/basic.c`, `src/adaptability.c`,
//! `src/velocity.c`, `src/fluidness.c` — including the odd-looking probabilities,
//! which are what give the nonsense words of the Adaptability module their
//! pronounceable-but-unfamiliar texture.

use crate::corpus::Corpus;
use crate::goals::Module;
use crate::kbd::Layout;
use crate::lesson::Lesson;
use crate::rng::Rng;

/// The glyph to show where a newline must be typed (`UPSYM`, `src/keyboard.h:23`).
///
/// This is a rendering concern only: generated text separates lines with a plain
/// `\n`, which the typist produces by pressing Return. Upstream instead puts a
/// literal pilcrow in the text *and* a newline after it, then carries a hack for
/// what its own comment calls "the line breaking bug" (`src/tutor.c:803`). One
/// character per line break avoids the whole problem.
pub const LINE_END_MARK: char = '¶';

/// Lines in one Basic drill (`N_LINES`).
const BASIC_LINES: usize = 8;
/// Words per Basic drill line.
const BASIC_WORDS_PER_LINE: usize = 9;
/// Letters per Basic drill word.
const BASIC_LETTERS_PER_WORD: usize = 5;

/// Paragraphs in one Adaptability exercise (`LINES`).
const ADAPT_PARAGRAPHS: usize = 4;
/// Words per Adaptability paragraph (`WORDS`).
const ADAPT_WORDS: usize = 22;
/// Longest nonsense word (`MAX_WORD_LEN`).
const ADAPT_MAX_WORD_LEN: usize = 9;

/// Paragraphs in one Velocity exercise.
const VELO_PARAGRAPHS: usize = 4;
/// Words per Velocity paragraph.
const VELO_WORDS: usize = 20;

/// Paragraphs in one Fluidness exercise (`fluid_paragraphs` default).
const FLUID_PARAGRAPHS: usize = 3;

/// What a generator needs in order to produce text.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// Which module to generate for.
    pub module: Module,
    /// The keyboard being practised; decides which characters exist.
    pub layout: &'a Layout,
    /// The current Basic lesson. Required for [`Module::Basic`].
    pub lesson: Option<&'a Lesson>,
    /// Practice text. Required for [`Module::Velocity`] and [`Module::Fluidness`].
    pub corpus: Option<&'a Corpus>,
    /// Whether the language ends sentences with `.` and separates with `,`.
    ///
    /// Upstream calls this `trans_lang_has_stopmark()`; scripts such as Tibetan
    /// use their own marks instead.
    pub stop_marks: bool,
}

/// A generated exercise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exercise {
    /// The module it was generated for.
    pub module: Module,
    /// The seed it came from; replaying this reproduces `text` exactly.
    pub seed: u64,
    /// The text to type. Lines are separated by `\n`.
    pub text: String,
}

impl Exercise {
    /// Characters the typist is asked to produce, newlines included.
    pub fn len_chars(&self) -> usize {
        self.text.chars().count()
    }
}

/// Why an exercise could not be generated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerateError {
    /// [`Module::Basic`] was asked for without a lesson.
    MissingLesson,
    /// A corpus-driven module was asked for without a corpus.
    MissingCorpus(Module),
    /// The lesson produces fewer than two characters on this layout.
    ///
    /// A drill needs at least two distinct keys to alternate between.
    LessonTooSmall {
        /// The lesson number.
        lesson: u32,
        /// The layout it was applied to.
        layout: String,
    },
    /// The layout has no letters, so nonsense words cannot be built.
    LayoutHasNoLetters {
        /// The layout's name.
        layout: String,
    },
    /// The corpus is missing the text this module needs.
    CorpusEmpty {
        /// The language code.
        language: String,
        /// Which kind of text was missing.
        needs: &'static str,
    },
}

impl core::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GenerateError::MissingLesson => write!(f, "the basic module needs a lesson"),
            GenerateError::MissingCorpus(m) => {
                write!(f, "the {} module needs a corpus", m.slug())
            }
            GenerateError::LessonTooSmall { lesson, layout } => write!(
                f,
                "lesson {lesson} yields fewer than two characters on layout {layout}"
            ),
            GenerateError::LayoutHasNoLetters { layout } => {
                write!(f, "layout {layout} has no letters to build words from")
            }
            GenerateError::CorpusEmpty { language, needs } => {
                write!(f, "corpus {language} has no {needs}")
            }
        }
    }
}

impl std::error::Error for GenerateError {}

/// Generates an exercise, reproducibly from `seed`.
pub fn generate(request: Request<'_>, seed: u64) -> Result<Exercise, GenerateError> {
    let mut rng = Rng::from_seed(seed);
    let text = match request.module {
        Module::Basic => {
            let lesson = request.lesson.ok_or(GenerateError::MissingLesson)?;
            let char_set = lesson.char_set(request.layout);
            if char_set.len() < 2 {
                return Err(GenerateError::LessonTooSmall {
                    lesson: lesson.number,
                    layout: request.layout.name.clone(),
                });
            }
            basic_drill(&mut rng, &char_set)
        }
        Module::Adaptability => adaptability(&mut rng, request.layout, request.stop_marks)?,
        Module::Velocity => {
            let corpus = request
                .corpus
                .ok_or(GenerateError::MissingCorpus(Module::Velocity))?;
            velocity(&mut rng, corpus, request.stop_marks)?
        }
        Module::Fluidness => {
            let corpus = request
                .corpus
                .ok_or(GenerateError::MissingCorpus(Module::Fluidness))?;
            fluidness(&mut rng, corpus)?
        }
    };

    Ok(Exercise {
        module: request.module,
        seed,
        text,
    })
}

/// Basic: eight lines of nine five-letter groups drawn from the lesson's keys.
///
/// Keys are drawn *without* replacement from a pool that refills when exhausted,
/// so a short lesson cycles evenly through its keys instead of clustering by
/// luck. Small character sets get a doubled pool, which lets a key repeat within
/// a group — `ffjfj` is a useful thing to practise, `fjfjf` forever is not.
fn basic_drill(rng: &mut Rng, char_set: &[char]) -> String {
    let refill = |pool: &mut Vec<char>| {
        pool.clear();
        pool.extend_from_slice(char_set);
        if char_set.len() > 4 && char_set.len() < 14 {
            pool.extend_from_slice(char_set);
        }
    };

    let mut pool = Vec::with_capacity(char_set.len() * 2);
    refill(&mut pool);

    let mut out = String::new();
    for _ in 0..BASIC_LINES {
        for word in 0..BASIC_WORDS_PER_LINE {
            for _ in 0..BASIC_LETTERS_PER_WORD {
                let pick = rng.below(pool.len() as u32) as usize;
                out.push(pool[pick]);
                // Swap-remove: the last live key fills the hole.
                let last = pool.len() - 1;
                pool[pick] = pool[last];
                pool.truncate(last);
                if pool.is_empty() {
                    refill(&mut pool);
                }
            }
            if word < BASIC_WORDS_PER_LINE - 1 {
                out.push(' ');
            }
        }
        out.push('\n');
    }
    out
}

/// Adaptability: four paragraphs of 22 invented words and numbers.
fn adaptability(rng: &mut Rng, layout: &Layout, stop_marks: bool) -> Result<String, GenerateError> {
    let vowels = layout.vowels();
    let consonants = layout.consonants();
    let symbols = layout.symbols();
    if vowels.is_empty() || consonants.is_empty() {
        return Err(GenerateError::LayoutHasNoLetters {
            layout: layout.name.clone(),
        });
    }

    let mut out = String::new();
    for _ in 0..ADAPT_PARAGRAPHS {
        let mut words: Vec<String> = Vec::with_capacity(ADAPT_WORDS);
        for index in 0..ADAPT_WORDS {
            // Roughly one word in fifteen is a four-digit number instead.
            let mut word = if rng.usually(15) {
                nonsense_word(rng, &vowels, &consonants, &symbols, stop_marks)
            } else {
                four_digits(rng)
            };
            if index == 0 {
                word = capitalise(&word);
            }
            words.push(word);
        }
        out.push_str(&words.join(" "));
        if stop_marks {
            out.push('.');
        }
        out.push('\n');
    }
    Ok(out)
}

/// Builds one pronounceable-but-meaningless word.
///
/// Alternating positions prefer vowels and consonants respectively, with a small
/// chance of swapping, which is what stops the output reading as strict CVCVCV.
fn nonsense_word(
    rng: &mut Rng,
    vowels: &[char],
    consonants: &[char],
    symbols: &[char],
    stop_marks: bool,
) -> String {
    let length = rng.below(ADAPT_MAX_WORD_LEN as u32 - 1) as usize + 1;
    let mut word = String::new();

    for position in 0..length {
        // Roughly one character in 25 is punctuation rather than a letter.
        if rng.usually(25) {
            let ch = if position % 2 == 1 {
                if rng.usually(30) {
                    pick(rng, vowels)
                } else {
                    pick(rng, consonants)
                }
            } else if rng.usually(50) {
                pick(rng, consonants)
            } else {
                pick(rng, vowels)
            };
            if position == 0 && !rng.usually(7) {
                word.extend(ch.to_uppercase());
            } else {
                word.push(ch);
            }
        } else {
            let mut ch = pick(rng, symbols);
            if symbols.is_empty() {
                ch = pick(rng, consonants);
            }
            // A backslash or acute mid-word is awkward to reach; substitute.
            if position > 0 && ch == '\\' {
                ch = '-';
            }
            if position > 0 && ch == '´' {
                ch = '`';
            }
            word.push(ch);
            // A symbol usually ends the word.
            if rng.usually(5) || ch == '-' || ch == '\\' {
                return word;
            }
        }
    }

    // Words usually close on a vowel, occasionally on a comma.
    if rng.usually(20) {
        word.push(pick(rng, vowels));
    } else if stop_marks {
        word.push(',');
    }
    word
}

/// A four-digit number, as upstream's `adapt_create_number`.
fn four_digits(rng: &mut Rng) -> String {
    (0..4)
        .map(|_| char::from(b'0' + rng.below(10) as u8))
        .collect()
}

/// Velocity: four paragraphs of 20 real words, chosen independently.
fn velocity(rng: &mut Rng, corpus: &Corpus, stop_marks: bool) -> Result<String, GenerateError> {
    if corpus.words.is_empty() {
        return Err(GenerateError::CorpusEmpty {
            language: corpus.language.clone(),
            needs: "words",
        });
    }

    let mut out = String::new();
    for _ in 0..VELO_PARAGRAPHS {
        let mut words: Vec<String> = Vec::with_capacity(VELO_WORDS);
        for index in 0..VELO_WORDS {
            let word = pick_ref(rng, &corpus.words);
            words.push(if index == 0 {
                capitalise(word)
            } else {
                word.clone()
            });
        }
        out.push_str(&words.join(" "));
        if stop_marks {
            out.push('.');
        }
        out.push('\n');
    }
    Ok(out)
}

/// Fluidness: whole paragraphs of real prose, no two the same.
fn fluidness(rng: &mut Rng, corpus: &Corpus) -> Result<String, GenerateError> {
    if corpus.paragraphs.is_empty() {
        return Err(GenerateError::CorpusEmpty {
            language: corpus.language.clone(),
            needs: "paragraphs",
        });
    }

    let wanted = FLUID_PARAGRAPHS.min(corpus.paragraphs.len());
    let mut chosen: Vec<usize> = Vec::with_capacity(wanted);
    while chosen.len() < wanted {
        // Upstream draws and retries until the index is new; with 100 paragraphs
        // and 3 draws the retry is rare, and it keeps the seed behaviour simple.
        let candidate = rng.below(corpus.paragraphs.len() as u32) as usize;
        if !chosen.contains(&candidate) {
            chosen.push(candidate);
        }
    }

    let mut out = String::new();
    for index in chosen {
        out.push_str(&corpus.paragraphs[index]);
        out.push('\n');
    }
    Ok(out)
}

/// Picks a character, falling back to a space if the set is somehow empty.
fn pick(rng: &mut Rng, from: &[char]) -> char {
    rng.choose(from).copied().unwrap_or(' ')
}

/// Picks a string reference from a non-empty slice.
fn pick_ref<'a>(rng: &mut Rng, from: &'a [String]) -> &'a String {
    rng.choose(from)
        .expect("caller checked the slice is non-empty")
}

/// Uppercases the first character, leaving the rest alone.
fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kbd::Layout;
    use crate::lesson::klavaro_lessons;

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

    fn corpus() -> Corpus {
        Corpus::new(
            "en",
            "the\nquick\nbrown\nfox\njumps\nover\nlazy\ndog\n",
            "First paragraph here.\n\nSecond paragraph here.\n\nThird one.\n\nFourth one.\n",
        )
    }

    fn request<'a>(
        module: Module,
        layout: &'a Layout,
        lesson: Option<&'a Lesson>,
        corpus: Option<&'a Corpus>,
    ) -> Request<'a> {
        Request {
            module,
            layout,
            lesson,
            corpus,
            stop_marks: true,
        }
    }

    // ---- reproducibility -------------------------------------------------

    #[test]
    fn same_seed_reproduces_every_module() {
        let layout = qwerty();
        let lessons = klavaro_lessons();
        let corpus = corpus();
        for module in Module::ALL {
            let req = request(module, &layout, Some(&lessons[20]), Some(&corpus));
            let a = generate(req, 12345).expect("generates");
            let b = generate(req, 12345).expect("generates");
            assert_eq!(a, b, "{} is not reproducible", module.slug());
        }
    }

    #[test]
    fn different_seeds_give_different_text() {
        let layout = qwerty();
        let lessons = klavaro_lessons();
        let corpus = corpus();
        for module in Module::ALL {
            let req = request(module, &layout, Some(&lessons[20]), Some(&corpus));
            let a = generate(req, 1).expect("generates");
            let b = generate(req, 2).expect("generates");
            assert_ne!(a.text, b.text, "{} ignores its seed", module.slug());
        }
    }

    // ---- basic -----------------------------------------------------------

    #[test]
    fn basic_drill_has_the_expected_shape() {
        let layout = qwerty();
        let lessons = klavaro_lessons();
        let req = request(Module::Basic, &layout, Some(&lessons[0]), None);
        let ex = generate(req, 7).unwrap();

        let lines: Vec<&str> = ex.text.lines().collect();
        assert_eq!(lines.len(), BASIC_LINES);
        for line in lines {
            let groups: Vec<&str> = line.split(' ').collect();
            assert_eq!(groups.len(), BASIC_WORDS_PER_LINE);
            for group in groups {
                assert_eq!(
                    group.chars().count(),
                    BASIC_LETTERS_PER_WORD,
                    "group {group:?} in {line:?}"
                );
            }
        }
        assert!(
            !ex.text.contains(LINE_END_MARK),
            "the pilcrow is drawn by the UI, not typed"
        );
    }

    #[test]
    fn basic_drill_uses_only_the_lessons_keys() {
        let layout = qwerty();
        let lessons = klavaro_lessons();
        for lesson in [&lessons[0], &lessons[10], &lessons[42]] {
            let allowed = lesson.char_set(&layout);
            let req = request(Module::Basic, &layout, Some(lesson), None);
            let ex = generate(req, 99).unwrap();
            for ch in ex.text.chars() {
                if ch == ' ' || ch == '\n' {
                    continue;
                }
                assert!(
                    allowed.contains(&ch),
                    "lesson {} produced {ch:?}, which it does not teach",
                    lesson.number
                );
            }
        }
    }

    #[test]
    fn basic_drill_spreads_keys_evenly() {
        // Lesson 1 teaches exactly f and j; drawing without replacement should
        // give a near-even split rather than whatever chance dictates.
        let layout = qwerty();
        let lessons = klavaro_lessons();
        let req = request(Module::Basic, &layout, Some(&lessons[0]), None);
        let ex = generate(req, 4).unwrap();
        let f = ex.text.chars().filter(|&c| c == 'f').count();
        let j = ex.text.chars().filter(|&c| c == 'j').count();
        assert!(f.abs_diff(j) <= 2, "uneven split: {f} f, {j} j");
    }

    #[test]
    fn basic_needs_a_lesson() {
        let layout = qwerty();
        let req = request(Module::Basic, &layout, None, None);
        assert_eq!(generate(req, 1), Err(GenerateError::MissingLesson));
    }

    // ---- adaptability ----------------------------------------------------

    #[test]
    fn adaptability_has_the_expected_shape() {
        let layout = qwerty();
        let req = request(Module::Adaptability, &layout, None, None);
        let ex = generate(req, 5).unwrap();
        let lines: Vec<&str> = ex.text.lines().collect();
        assert_eq!(lines.len(), ADAPT_PARAGRAPHS);
        for line in lines {
            assert!(line.ends_with('.'), "no stop mark: {line:?}");
            let words: Vec<&str> = line.trim_end_matches('.').split(' ').collect();
            assert_eq!(words.len(), ADAPT_WORDS, "in {line:?}");
            assert!(!words[0].is_empty());
            for word in words {
                assert!(
                    word.chars().count() <= ADAPT_MAX_WORD_LEN + 1,
                    "over-long word {word:?}"
                );
            }
        }
    }

    #[test]
    fn adaptability_starts_paragraphs_with_a_capital() {
        let layout = qwerty();
        let req = request(Module::Adaptability, &layout, None, None);
        let ex = generate(req, 6).unwrap();
        for line in ex.text.lines() {
            let first = line.chars().next().unwrap();
            assert!(
                first.is_uppercase() || !first.is_alphabetic(),
                "paragraph starts with {first:?}"
            );
        }
    }

    #[test]
    fn adaptability_produces_some_digits_over_many_seeds() {
        // Numbers are ~1 in 15 words, so they must show up somewhere.
        let layout = qwerty();
        let req = request(Module::Adaptability, &layout, None, None);
        let any_digits = (0..20).any(|seed| {
            generate(req, seed)
                .unwrap()
                .text
                .chars()
                .any(|c| c.is_ascii_digit())
        });
        assert!(any_digits, "no numbers generated in 20 exercises");
    }

    // ---- velocity --------------------------------------------------------

    #[test]
    fn velocity_uses_only_corpus_words() {
        let layout = qwerty();
        let corpus = corpus();
        let req = request(Module::Velocity, &layout, None, Some(&corpus));
        let ex = generate(req, 11).unwrap();
        let lines: Vec<&str> = ex.text.lines().collect();
        assert_eq!(lines.len(), VELO_PARAGRAPHS);
        for line in lines {
            let words: Vec<&str> = line.trim_end_matches('.').split(' ').collect();
            assert_eq!(words.len(), VELO_WORDS);
            for word in words {
                let lowered = word.to_lowercase();
                assert!(
                    corpus.words.contains(&lowered),
                    "{word:?} is not in the corpus"
                );
            }
        }
    }

    #[test]
    fn velocity_needs_a_corpus() {
        let layout = qwerty();
        let req = request(Module::Velocity, &layout, None, None);
        assert_eq!(
            generate(req, 1),
            Err(GenerateError::MissingCorpus(Module::Velocity))
        );
    }

    #[test]
    fn velocity_rejects_an_empty_word_list() {
        let layout = qwerty();
        let empty = Corpus::new("xx", "", "a\n");
        let req = request(Module::Velocity, &layout, None, Some(&empty));
        assert!(matches!(
            generate(req, 1),
            Err(GenerateError::CorpusEmpty { needs: "words", .. })
        ));
    }

    // ---- fluidness -------------------------------------------------------

    #[test]
    fn fluidness_takes_distinct_paragraphs_verbatim() {
        let layout = qwerty();
        let corpus = corpus();
        let req = request(Module::Fluidness, &layout, None, Some(&corpus));
        let ex = generate(req, 13).unwrap();
        let lines: Vec<&str> = ex.text.lines().collect();
        assert_eq!(lines.len(), FLUID_PARAGRAPHS);
        for line in &lines {
            assert!(corpus.paragraphs.contains(&line.to_string()), "{line:?}");
        }
        let unique: std::collections::HashSet<_> = lines.iter().collect();
        assert_eq!(unique.len(), lines.len(), "repeated paragraph");
    }

    #[test]
    fn fluidness_copes_with_a_short_corpus() {
        let layout = qwerty();
        let thin = Corpus::new("xx", "a\n", "Only one paragraph.\n");
        let req = request(Module::Fluidness, &layout, None, Some(&thin));
        let ex = generate(req, 1).unwrap();
        assert_eq!(ex.text.lines().count(), 1);
    }

    #[test]
    fn fluidness_rejects_an_empty_paragraph_list() {
        let layout = qwerty();
        let empty = Corpus::new("xx", "a\n", "");
        let req = request(Module::Fluidness, &layout, None, Some(&empty));
        assert!(matches!(
            generate(req, 1),
            Err(GenerateError::CorpusEmpty {
                needs: "paragraphs",
                ..
            })
        ));
    }
}
