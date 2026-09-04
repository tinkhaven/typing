"""Generates every brand asset from one description of the mark.

The variants share geometry rather than being drawn separately, so the studs sit
in the same places in the favicon and the social preview and cannot drift apart
as one gets edited.
"""
import os

# Relative to this file, so the script works from any clone and no local path
# is baked into a public repository.
OUT = os.path.dirname(os.path.abspath(__file__))

# Klavaro's finger colours (src/keyboard.h): left little, ring, middle, index.
# Each stud gets a darker side so it reads as a cylinder rather than a rectangle.
STUDS = [("#bbbbff", "#8f8fd6"), ("#eeaaaa", "#c98686"),
         ("#aaeebb", "#7fc596"), ("#eeee88", "#c9c95e")]

BRICK_FACE, BRICK_SIDE = "#2f6fd0", "#1d3f73"   # this port
BASE_FACE,  BASE_SIDE  = "#b9b0a2", "#8a8175"   # Klavaro, underneath
INK = "#24211e"

FONTS = ("system-ui,-apple-system,'Segoe UI',Helvetica,Arial,"
         "'DejaVu Sans',sans-serif")

# Three-quarter view. A brick only reads as a brick if you can see the top face
# with round studs on it — drawn flat-on it is just a coloured slab, and the
# studs come out as pills. So each brick is three faces (top, front, right) with
# the light coming from above, and the studs are little cylinders sitting on the
# top face.
#
# Geometry is in a 64x64 box:
#   the top face is a parallelogram skewed 10 right and 10 up,
#   the front face hangs below it,
#   the right face joins the two.
SKEW = 10.0          # horizontal offset of the back edge
BRICK_W = 38.0       # width of the front face
UPPER_H = 15.0       # height of the upper brick's front face
LOWER_H = 13.0       # height of the lower brick's front face
STUD_RX, STUD_RY = 3.6, 2.0
STUD_H = 2.3         # how far a stud stands proud

# Each brick needs three tones: lit top, mid front, shaded side.
BLUE  = ("#4a86e8", "#2f6fd0", "#1f4f9c")     # this port
STONE = ("#cfc6b8", "#b9b0a2", "#98907f")     # Klavaro, underneath

# Studs take their own colour on the top face, with a darker cylinder wall.
STUDS = [("#c8c8ff", "#9a9ae0"), ("#f2b6b6", "#d09090"),
         ("#b6f2c6", "#8ad2a2"), ("#f2f296", "#d0d068")]

GRADIENT = ""


def _poly(points, fill, stroke=None, width=2.0):
    pts = " ".join(f"{x:.2f},{y:.2f}" for x, y in points)
    if stroke:
        return (f'<polygon points="{pts}" fill="none" stroke="{stroke}" '
                f'stroke-width="{width}" stroke-linejoin="round"/>')
    return f'<polygon points="{pts}" fill="{fill}"/>'


def _stud(cx, cy, top, wall):
    """A cylinder: wall behind, then the lit top face."""
    return (
        f'<ellipse cx="{cx:.2f}" cy="{cy + STUD_H:.2f}" rx="{STUD_RX}" ry="{STUD_RY}" fill="{wall}"/>'
        f'<rect x="{cx - STUD_RX:.2f}" y="{cy:.2f}" width="{STUD_RX * 2}" height="{STUD_H}" fill="{wall}"/>'
        f'<ellipse cx="{cx:.2f}" cy="{cy:.2f}" rx="{STUD_RX}" ry="{STUD_RY}" fill="{top}"/>'
    )


def _brick(x, y, height, tones, studs, mono=None):
    """One brick. `y` is the front face's top-left; the top face sits above it.

    With `studs=False` the top face is omitted too, which is what a brick with
    another one stacked on it actually looks like.

    In one colour the faces cannot be separated by shade, so the mono variant is
    line art: outlines only. Filling it flat produced a black blob with the
    studs and the seam between the two bricks both swallowed.
    """
    top, front, side = tones
    out = []
    edge = mono if mono else None

    if studs:
        out.append(_poly([(x, y), (x + SKEW, y - SKEW),
                          (x + SKEW + BRICK_W, y - SKEW), (x + BRICK_W, y)],
                         top, stroke=edge))
        for i, (stud_top, stud_wall) in enumerate(STUDS):
            cx = x + SKEW / 2 + BRICK_W * (i + 0.5) / 4
            cy = y - SKEW / 2
            if mono:
                out.append(
                    f'<ellipse cx="{cx:.2f}" cy="{cy:.2f}" rx="{STUD_RX}" ry="{STUD_RY}" '
                    f'fill="none" stroke="{mono}" stroke-width="1.6"/>')
            else:
                out.append(_stud(cx, cy, stud_top, stud_wall))

    out.append(_poly([(x + BRICK_W, y), (x + SKEW + BRICK_W, y - SKEW),
                      (x + SKEW + BRICK_W, y - SKEW + height), (x + BRICK_W, y + height)],
                     side, stroke=edge))
    out.append(_poly([(x, y), (x + BRICK_W, y),
                      (x + BRICK_W, y + height), (x, y + height)],
                     front, stroke=edge))
    return out


def brick(x=0.0, y=0.0, mono=None):
    """The stacked mark: a studded brick sitting on a plain one.

    Only the upper brick shows studs and a top face. The lower one is the
    foundation — Klavaro — and you see just its front and side, exactly as you
    would if something were stacked on it.
    """
    upper_y = y + SKEW + STUD_RY + STUD_H
    lower_y = upper_y + UPPER_H

    out = []
    out += _brick(x, lower_y, LOWER_H, STONE, studs=False, mono=mono)
    out += _brick(x, upper_y, UPPER_H, BLUE, studs=True, mono=mono)
    return "\n    ".join(out)


def svg(view, body, defs=True, title="Tinkhaven Typing"):
    d = GRADIENT if defs else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}" '
            f'role="img" aria-label="{title}">\n'
            f'  <title>{title}</title>\n{d}\n    {body}\n</svg>\n')


def wordmark(x, y, size, ink=INK, sub=True, anchor="start"):
    """Name, and optionally the tagline under it."""
    out = [f'<text x="{x}" y="{y}" font-family="{FONTS}" font-size="{size}" '
           f'font-weight="650" letter-spacing="{-size*0.012:.2f}" fill="{ink}" '
           f'text-anchor="{anchor}">Tinkhaven Typing</text>']
    if sub:
        out.append(f'<text x="{x}" y="{y + size*0.92:.1f}" font-family="{FONTS}" '
                   f'font-size="{size*0.42:.2f}" fill="{ink}" opacity=".62" '
                   f'text-anchor="{anchor}">Learn to touch type</text>')
    return "\n    ".join(out)


files = {}

# Square mark, transparent. 8px padding inside a 64 box.
files["mark.svg"] = svg("0 0 64 64", brick(8, 7))

# Single colour, for one-colour printing or stamps.
files["mark-mono-dark.svg"]  = svg("0 0 64 64", brick(8, 7, mono=INK), defs=False)
files["mark-mono-light.svg"] = svg("0 0 64 64", brick(8, 7, mono="#f2ece4"), defs=False)

# A tile for places that need an opaque icon (app icons, some launchers).
files["mark-badge.svg"] = svg(
    "0 0 64 64",
    f'<rect width="64" height="64" rx="14" fill="#faf8f6"/>\n    {brick(8, 7)}')
files["mark-badge-dark.svg"] = svg(
    "0 0 64 64",
    f'<rect width="64" height="64" rx="14" fill="#17161a"/>\n    {brick(8, 7)}')

# Horizontal lockup: mark left, name right.
files["logo-horizontal.svg"] = svg(
    "0 0 300 64", brick(2, 7) + "\n    " + wordmark(60, 34, 21))
files["logo-horizontal-light.svg"] = svg(
    "0 0 300 64", brick(2, 7) + "\n    " + wordmark(60, 34, 21, ink="#f2ece4"))

# Stacked lockup: mark above centred name.
files["logo-stacked.svg"] = svg(
    "0 0 240 116", brick(96, 4) + "\n    " + wordmark(120, 80, 22, anchor="middle"))
files["logo-stacked-light.svg"] = svg(
    "0 0 240 116",
    brick(96, 4) + "\n    " + wordmark(120, 80, 22, ink="#f2ece4", anchor="middle"))

# Text only.
files["wordmark.svg"] = svg("0 0 260 44", wordmark(0, 22, 22), defs=False)
files["wordmark-light.svg"] = svg("0 0 260 44", wordmark(0, 22, 22, ink="#f2ece4"), defs=False)

# Social preview, the size GitHub and most link unfurlers want.
files["social-preview.svg"] = svg(
    "0 0 1200 630",
    '<rect width="1200" height="630" fill="#faf8f6"/>\n    '
    '<g transform="translate(476 150) scale(5)">' + brick(0, 0) + '</g>\n    '
    + wordmark(600, 500, 54, anchor="middle"))

os.makedirs(f"{OUT}/svg", exist_ok=True)
for name, content in files.items():
    open(f"{OUT}/svg/{name}", "w", encoding="utf-8").write(content)
print(f"{len(files)} SVGs written")

# --- PNGs, transparent where the source is transparent -------------------
import cairosvg
png = [
    ("mark.svg",              [16, 32, 48, 64, 128, 180, 256, 512, 1024]),
    ("mark-badge.svg",        [180, 512, 1024]),
    ("mark-badge-dark.svg",   [512]),
    ("mark-mono-dark.svg",    [256]),
    ("mark-mono-light.svg",   [256]),
    ("logo-horizontal.svg",   [400, 800, 1600]),
    ("logo-horizontal-light.svg", [800]),
    ("logo-stacked.svg",      [400, 800]),
    ("logo-stacked-light.svg",[800]),
    ("wordmark.svg",          [600, 1200]),
    ("social-preview.svg",    [1200]),
]
os.makedirs(f"{OUT}/png", exist_ok=True)
count = 0
for name, widths in png:
    stem = name[:-4]
    for w in widths:
        cairosvg.svg2png(url=f"{OUT}/svg/{name}",
                         write_to=f"{OUT}/png/{stem}-{w}.png",
                         output_width=w)
        count += 1
print(f"{count} PNGs written")
