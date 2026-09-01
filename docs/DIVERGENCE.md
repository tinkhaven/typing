# Where this port differs from Klavaro

The scoring formulas, keyboard layouts, lessons and practice text are Klavaro's,
so a score here means what it means in the desktop program. These are the places
the port deliberately or unavoidably differs. Anything not listed should behave
the same; if it does not, that is a bug in this port.

## Deliberate changes

**Line breaks are one character, not two.** Klavaro writes a literal pilcrow into
the exercise text *and* a newline after it, then carries a workaround for what its
own comment calls "the line breaking bug" (`src/tutor.c:803`). Here the text
contains a plain `\n`, the typist presses Return, and the pilcrow is drawn by the
stylesheet. Same keystroke count, no special case.

**Fluidness is not reported below two samples.** Upstream's formula divides by a
value that is effectively zero when fewer than three keystrokes were recorded,
which reports 100% for a session that produced no rhythm at all. This port
returns no figure instead. It cannot affect comparability: a fluidness result is
only ever published above 500 characters.

**Goals are fixed.** Klavaro lets the user edit the thresholds in
`preferences.ini`. Here they are constants, because a shared leaderboard is only
meaningful if everyone is measured against the same bar.

**Nonsense words never end on uninitialised memory.** In `adapt_create_word`,
upstream writes the final character only for languages that use a comma; for
other languages `word[n]` is left uninitialised and then typed. This port appends
nothing in that case.

**No early exit for two-character lessons.** Upstream ends the Basic drill early
if `len == 2` partway through (`src/basic.c:361`), where `len` is the *remaining*
pool count rather than the lesson size — a condition that does not reliably fire
for the lessons it was presumably meant for. Every drill here is eight lines.

**Extra figures are shown.** Velocity runs display a fluidness reading even
though the module's goals do not include one. It is free information, and
harmless.

## Not carried over

**Tibetan and Urdu handling.** Upstream has special cases throughout for Tibetan
word delimiters and stop marks, Urdu commas, and rules preventing two diacritics
in a row. The layouts and corpora for those languages are all present and the
modules work, but those refinements are not implemented. `VOWELS` in
`crates/core/src/kbd.rs` also omits the Tibetan vowels upstream lists.

**Non-Arabic digits.** `keyb_get_altnums` lets Adaptability generate numbers in a
layout's own digit glyphs. This port always uses `0`–`9`.

**Composed characters count as one keystroke.** Upstream counts a keystroke per
decomposed component (`g_unichar_fully_decompose`). Browser key events give whole
characters, so this port counts one. Only affects scripts using combining marks.

**Unicode punctuation is approximated.** `keyb_get_symbols` uses glib's
category-P test. Rust has no equivalent in the standard library, so
`kbd::is_symbol` accepts any printable non-alphanumeric character — slightly
broader, admitting `$` and `+`. They are keys a typist has to reach for anyway.

**Interface translations cover four languages.** English, Dutch, French and
German, with the strings that survived into this port taken from Klavaro's own
`po/*.po` catalogues. Practice corpora ship for all 38 languages regardless.

**Not implemented yet.** Progress charts over time (`src/plot.c`), custom lessons
beyond number 43, user-supplied practice text, the accuracy module's "too many
errors" targeted drill (`src/accuracy.c`), and speech output.

## Added here

**A seeded generator.** Exercises are generated from an explicit seed
(`crates/core/src/rng.rs`) rather than `rand()`, so the browser and the server
produce identical text and the server can check a reported run against what was
actually on screen.

**Server-side scoring.** The browser streams keystroke outcomes; the server keeps
its own tally and scores from that. See `crates/web/src/server/verify.rs`,
including a plain statement of what it does and does not prevent.
