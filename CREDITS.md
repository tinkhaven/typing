# Credits and provenance

Tinkhaven Typing is a web port of **Klavaro Touch Typing Tutor**.

## Upstream

| | |
|---|---|
| Project | Klavaro — a flexible touch typing tutor |
| Author | Felipe Emmanuel Ferreira de Castro (`fefcas at gmail dot com`) |
| Copyright | © 2005–2021 Felipe Emmanuel Ferreira de Castro |
| Homepage | https://klavaro.sourceforge.io/ |
| Release ported | 3.14 (2022-12-13) |
| Source archive | `klavaro-3.14.tar.bz2` |
| SHA-256 | `87187e49d301c510e6964098cdb612126bf030d2a875fd799eadcad3eae56dab` |
| License | GNU GPL version 3 or later |

Tinkhaven Typing is **not** affiliated with or endorsed by the Klavaro project.
Please report bugs in this port here, not upstream.

## What is derived from Klavaro

**Data, used verbatim** (`assets/klavaro-data/`):

- `layouts/*.kbd` — 77 keyboard layouts
- `fingers_position.txt` — the 4×14 finger assignment map
- `basic_lessons.txt` — 43 progressive lessons as key bitmasks
- `corpora/*.words`, `corpora/*.paragraphs` — practice text for 38 languages

**Algorithms, reimplemented in Rust** (`crates/core/`), preserving upstream
behaviour so scores remain comparable:

| Concept | Upstream reference |
|---|---|
| accuracy / speed / fluidness formulas | `src/tutor.c:1011`–`1047` |
| skill goals and level thresholds | `src/tutor.c:182` |
| basic lesson character sets | `src/basic.c` `basic_init_char_set()` |
| basic drill generation | `src/basic.c` `basic_draw_lesson()` |
| adaptability word generation | `src/adaptability.c` |
| velocity word drawing | `src/velocity.c` `velo_draw_random_words()` |
| fluidness paragraph selection | `src/fluidness.c` |

**UI strings** are taken from Klavaro's own `po/*.po` catalogues where a string
survived into this port, so existing translations are credited to their original
translators (listed in each `po` file upstream).

## License consequence

This port is a derivative work and is therefore distributed under the
**GNU GPL version 3 or later** — see [LICENSE](LICENSE). The browser client is
compiled to WebAssembly and served to visitors, which is a distribution of
object code; the corresponding source is this repository.
