# Branding

Everything visual for **Tinkhaven Typing**, and the reasoning behind it.

Nothing here needs a design tool to change: `build.py` generates every file from
one description of the mark, so the variants cannot drift apart as one gets
edited. Run it after any change.

```bash
python3 branding/build.py     # needs cairosvg: pip install cairosvg
```

---

## The mark

Two LEGO bricks, one stacked on the other.

The **stone brick underneath is Klavaro**, holding the thing up. The blue one on
top is this port. Somebody else laid the foundation; this is what got built on
it — which is the entire story of the project in one shape, and the reason the
mark has two bricks rather than one. Only the upper brick shows studs and a top
face, because that is what a stacked brick actually looks like.

The **four studs carry Klavaro's finger colours** — A S D F, left little finger
through index, the same colours the on-screen keyboard uses. Anyone who has used
the tutor reads them straight away; anyone who has not just sees a brick.

It is drawn in three-quarter view, and that is not a style preference. Drawn
flat-on, a brick is a coloured slab and the studs come out as pills — the first
attempt looked like a pencil case. A brick only reads as a brick when you can
see the top face with round studs standing on it, so each brick is three faces
with the light coming from above.

### Proportions

Measured, not guessed. A LEGO module is 8.0 mm, a brick is 9.6 mm tall, a stud
is 4.8 mm across and stands 1.8 mm proud. As ratios of the module:

| | Ratio to module | Why it matters |
|---|---|---|
| Brick height | 1.20 | At 1.58 it read as a container, not a brick |
| Stud diameter | 0.60 | At 0.76 the studs nearly touch and look like buttons |
| Stud height | 0.225 | Any taller and they look like pegs |
| Stud centre inset | 0.50 from each edge | Studs sit centred in their module |

`build.py` derives everything from `MODULE`, so changing the size cannot break
the ratios. The projection is **2:1 dimetric** — a module of depth moves the
back edge a full module right but only half a module up. A 45° skew puts the eye
almost directly overhead and makes the top face nearly as deep as the front face
is tall, which is not what a brick on a desk looks like.

Two bricks that meet get a **seam shadow**. Without it the stack reads as one
object that changes colour halfway down.

## Palette

The key colours are not chosen, they are inherited: they are Klavaro's finger
colours from `src/keyboard.h`, so the logo, the on-screen keyboard and the
desktop program all agree.

| Role | Hex | Also used for |
|---|---|---|
| Stud 1 — left little finger | `#bbbbff` | `A` on the virtual keyboard |
| Stud 2 — left ring finger | `#eeaaaa` | `S` |
| Stud 3 — left middle finger | `#aaeebb` | `D` |
| Stud 4 — left index finger | `#eeee88` | `F` |
| Brick face — this port | `#2f6fd0` | — |
| Brick side | `#1d3f73` | — |
| Base face — Klavaro | `#b9b0a2` | — |
| Base side | `#8a8175` | — |
| Ink — wordmark | `#24211e` | body text |
| Ink on dark | `#f2ece4` | body text, dark mode |

## Which file to use

| Need | File |
|---|---|
| Favicon, small icon | `svg/mark.svg` |
| App icon, launcher, anywhere needing an opaque tile | `svg/mark-badge.svg` |
| Header, letterhead, README | `svg/logo-horizontal.svg` |
| Centred, above a heading | `svg/logo-stacked.svg` |
| Name without the mark | `svg/wordmark.svg` |
| Embroidery, stamps, one-colour print | `svg/mark-mono-dark.svg` |
| Favicon at 16px | `svg/mark.svg` — nothing smaller works |
| Social cards, GitHub preview | `png/social-preview-1200.png` |

The monochrome variants are **line art**, not silhouettes. In one colour the
three faces cannot be separated by shade, and a flat fill collapses the whole
thing into a blob with the studs and the seam between the bricks both swallowed.

Every file has a `-light` or `-dark` counterpart where the background matters.
Prefer the **SVG** wherever the medium allows it: it is a few hundred bytes,
stays sharp at any size, and the PNGs are only rasterised from it.

The PNGs are **transparent** (RGBA) except `mark-badge-*` and
`social-preview-*`, which are deliberately opaque because the places that want
them composite badly against nothing.

## Using it well

- **Give it room.** Clear space on all sides of at least the height of one stud.
- **Scale it whole.** Do not stretch one axis; the studs stop reading as studs.
- **Do not recolour the studs.** They mean something — they are finger
  assignments, not decoration.
- **Do not flatten it to a front view.** See above; it stops being a brick.
- **Do not drop the base brick** to simplify the mark. Without it the logo says
  "keyboard"; with it, it says "built on Klavaro", which is the point.
- **Below about 16px**, use `mark.svg` and nothing smaller. At that size the
  studs are one pixel each and the mark becomes a smudge.

## Attribution and trademarks

The finger colours are Klavaro's, and Klavaro is GPL-3.0-or-later — see
[CREDITS.md](../CREDITS.md).

The bricks are a nod to LEGO, which is a trademark of the LEGO Group. This
project is not associated with or endorsed by them. Two deliberate lines were
drawn while designing it:

- **The word LEGO appears nowhere in the mark**, and the studs carry no
  embossed wordmark. Real studs do; that part is theirs, and copying it would be
  the actual trademark problem. Do not add it.
- The mark is a **stylised brick**, not a reproduction of a specific product.
  The generic interlocking-brick shape was held not to be a valid trade mark in
  the EU (*Lego Juris v OHIM*, C-48/09 P), which is why a brick-shaped logo is
  defensible where a brick-shaped logo carrying their wordmark would not be.

None of that is legal advice. If this ever fronts something commercial, it is
worth twenty minutes of a lawyer's time.
