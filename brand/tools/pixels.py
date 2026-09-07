#!/usr/bin/env python3
"""Author Emulia's icon concepts from pixel maps.

  python3 brand/tools/pixels.py          # rewrite every concept's three layers

The concepts are pixel art, so the honest source is a pixel map -- eight rows of
eight characters that you can read and edit -- not 260 hand-written <rect>
nodes. This script is where a sprite is *designed*; `concepts/<slug>/adaptive-
*.svg` is its output, and `build.py` treats those SVGs as the source of truth
exactly as it does for a hand-drawn mark. Edit the maps here, re-run, re-build.

Two things are computed rather than eyeballed, because both were wrong by
inspection the first time:

`fit` solves for the largest pixel size whose *painted* corners still sit inside
the 33dp always-visible circle that validate.py check 4 measures. A sprite's
bounding box is not its ink -- the alien's corners are empty, so it fits a
circle far better than an 8x8 box does -- and doing this by hand means either
failing the check or leaving a third of the canvas unused.

`runs` merges horizontal pixel runs into one rect each. It is not an
optimisation: an Android vector drawable redraws every path node, and a 16x16
sprite as 256 rects is a measurable cost on a surface that draws at boot.
"""

import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CANVAS = 108.0
CENTRE = CANVAS / 2
SAFE_R = 33.0
# validate.py counts ANY alpha, and a rasterised edge carries an antialias
# fringe outside its own geometry. Solving `fit` to exactly SAFE_R put all
# five concepts 0.3-0.5dp over; the fringe is what that margin pays for.
FRINGE = 0.8

# --- palette -----------------------------------------------------------------
# Emulia's own, lifted from android/app/src/main/java/dev/androidemu/Ui.kt. The
# app's background (#111813) is NOT the icon's: at mean luma ~22 it is a hole on
# Play's light chrome and dissolves into Play's dark chrome (#1F1F1F). The two
# grounds below are the same hue pushed to luma 59 and 70.
GROUND = "#22513e"        # mid-dark saturated forest, luma ~70
GROUND_DEEP = "#173b2d"   # luma ~59, for the cross and the deeper vignette
SCREEN = "#0d2a20"        # inside a CRT bezel, where near-black is correct
LEAF = "#b9e38c"          # the app's accent, and the one colour Emulia owns
LEAF_HI = "#d6f6ac"
AMBER = "#f0d49a"
AMBER_DEEP = "#e0a94e"
INK = "#12291f"           # sprites on a bright field
# Only c-one-in-four uses these: it is the concept about palette swapping, and
# two of the four cells stay Emulia's so the swap reads as a variation on the
# brand rather than as a different app.
TEAL = "#7fd2c4"
ROSE = "#e8a39a"

WHITE = "#ffffff"         # the themed layer is one flat tint; the system retints

# --- sprites -----------------------------------------------------------------
# '.' empty, 'X' subject, 'o' accent, 'h' highlight.
# Deliberately no third-party silhouettes: an invader-shaped bug, a heart, a
# ship and a pad are the archetypes, not anybody's sprite.

ALIEN = """\
..X..X..
...XX...
..XXXX..
.XXXXXX.
XX.XX.XX
XXXXXXXX
..X..X..
.X....X.
"""

# The winner, refined after the pick. ALIEN above is left exactly as the five
# were reviewed -- the contact sheet on the shelf is the record of that
# comparison, and quietly improving the losers afterwards would make it a
# record of something that never happened. This is the same creature with a
# fuller lower body: at 32px the original's `..X..X..` legs broke into four
# specks and the mark lost its mass, which is the one thing it had.
ALIEN_MK2 = """\
..X..X..
...XX...
..XXXX..
.XXXXXX.
XX.XX.XX
XXXXXXXX
X.XXXX.X
.X....X.
"""

HEART = """\
........
.XX..XX.
XXXXXXXX
XXXXXXXX
.XXXXXX.
..XXXX..
...XX...
........
"""

SHIP = """\
...XX...
...XX...
..XXXX..
..XXXX..
.XXXXXX.
XXXXXXXX
X.XXXX.X
..o..o..
"""

# The category's generic glyph. It is in the set on purpose -- so that it can be
# looked at and rejected on the evidence rather than in the abstract.
PAD = """\
........
.XXXXXX.
XX.XXXXX
X...X.XX
XX.XXX.X
XXXXXXXX
.XXXXXX.
........
"""

# 16x16, because a bird needs a neck and a neck needs rows. The first cut gave
# it four rows of neck over a wide body and it read, unmistakably, as a duck --
# an emu IS its proportions, so the neck is now seven rows, the body is four,
# and the legs are another four. Head top-left, beak out to the left.
EMU = """\
.....XXX........
...ooXXXX.......
...ooXX.X.......
......XXX.......
......XXX.......
......XXX.......
......XXX.......
.....XXXXX......
....XXXXXXX.....
...XXXXXXXXXX...
...XXXXXXXXXX...
....XXXXXXX.....
.....XX.XX......
.....oo.oo......
.....oo.oo......
....ooo.ooo.....
"""


def grid(sprite):
    rows = [r for r in sprite.strip("\n").split("\n")]
    w = max(len(r) for r in rows)
    return [r.ljust(w, ".") for r in rows], w, len(rows)


def runs(sprite, chars):
    """Horizontal runs of `chars` as (col, row, length) triples."""
    rows, w, h = grid(sprite)
    out = []
    for r, line in enumerate(rows):
        c = 0
        while c < w:
            if line[c] in chars:
                s = c
                while c < w and line[c] in chars:
                    c += 1
                out.append((s, r, c - s))
            else:
                c += 1
    return out


def fit(sprites, chars="Xoh", max_px=12.0, pad_frac=0.0):
    """Largest pixel size at which every painted corner is inside SAFE_R.

    `sprites` is a list of (sprite, centre_x_in_px, centre_y_in_px) where the
    centres are offsets from the icon centre measured in *pixels*, so that the
    whole arrangement scales together.

    `pad_frac` is any outward growth the concept applies after placement, as a
    fraction of the pixel size. e-half-and-half grows its rounded runs to close
    their notches, and a solver that does not know that solves for the wrong
    shape -- which is exactly how it failed check 4 by 0.68dp.
    """
    lo, hi = 0.05, max_px
    for _ in range(60):
        p = (lo + hi) / 2
        worst = 0.0
        for sprite, ox, oy in sprites:
            _, w, h = grid(sprite)
            x0 = CENTRE + (ox - w / 2.0) * p
            y0 = CENTRE + (oy - h / 2.0) * p
            g = pad_frac * p
            for c, r, n in runs(sprite, chars):
                for cx in (x0 + c * p - g, x0 + (c + n) * p + g):
                    for cy in (y0 + r * p - g, y0 + (r + 1) * p + g):
                        worst = max(worst, math.hypot(cx - CENTRE, cy - CENTRE))
        if worst <= SAFE_R - FRINGE:
            lo = p
        else:
            hi = p
    return lo


def place(sprite, p, ox=0.0, oy=0.0):
    """Top-left dp of a sprite whose centre sits `ox,oy` pixels off the icon centre."""
    _, w, h = grid(sprite)
    return CENTRE + (ox - w / 2.0) * p, CENTRE + (oy - h / 2.0) * p


def rects(sprite, p, x0, y0, chars, fill, rx=0.0, grow=0.0):
    out = []
    for c, r, n in runs(sprite, chars):
        x = x0 + c * p - grow
        y = y0 + r * p - grow
        w = n * p + 2 * grow
        h = p + 2 * grow
        a = ' rx="%g"' % rx if rx else ""
        out.append('  <rect x="%g" y="%g" width="%g" height="%g"%s fill="%s"/>'
                   % (round(x, 3), round(y, 3), round(w, 3), round(h, 3), a, fill))
    return out


def svg(body, comment=""):
    head = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 108 108" '
            'width="108" height="108">')
    parts = [head]
    if comment:
        # "--" cannot appear inside an XML comment, and an em dash reads the same.
        text = comment.strip().replace("--", "\u2014").replace("\n", "\n       ")
        parts.append("  <!-- " + text + " -->")
    parts.extend(body)
    parts.append("</svg>\n")
    return "\n".join(parts)


def scanlines(top, height, colour="#000000", opacity=0.30, step=3.0, thick=1.2):
    out = []
    y = top
    while y < top + height:
        out.append('  <rect x="0" y="%g" width="108" height="%g" fill="%s" '
                   'fill-opacity="%g"/>' % (round(y, 3), thick, colour, opacity))
        y += step
    return out


# --- a-four-screens ----------------------------------------------------------
# The literal reading of the brief: the square cut in half twice, a different
# game in each quarter. The colour fields are FULL BLEED and live in the
# background layer -- a launcher mask then crops flat colour instead of cropping
# tile edges into a fifth shape, and the foreground only has to fit the four
# sprites inside the 33dp circle.

def a_four_screens():
    bg = ['  <defs>',
          '    <radialGradient id="a-g" gradientUnits="userSpaceOnUse" cx="54" cy="54" r="66">',
          '      <stop offset="0" stop-color="#ffffff" stop-opacity="0.10"/>',
          '      <stop offset="1" stop-color="#000000" stop-opacity="0.22"/>',
          '    </radialGradient>',
          '  </defs>',
          '  <rect width="108" height="108" fill="%s"/>' % GROUND_DEEP,
          # Two bright cells and two dark ones would put a dark tile next to the
          # dark cross and lose the grid. Alternating Emulia's two accents keeps
          # every cell bright, so the checker is what survives to 32px.
          '  <rect x="0" y="0" width="52" height="52" fill="%s"/>' % LEAF,
          '  <rect x="56" y="0" width="52" height="52" fill="%s"/>' % AMBER,
          '  <rect x="0" y="56" width="52" height="52" fill="%s"/>' % AMBER,
          '  <rect x="56" y="56" width="52" height="52" fill="%s"/>' % LEAF,
          '  <rect width="108" height="108" fill="url(#a-g)"/>']

    slots = [(ALIEN, -5, -5), (PAD, 5, -5), (HEART, -5, 5), (SHIP, 5, 5)]
    p = fit([(s, ox * 1.0, oy * 1.0) for s, ox, oy in slots])
    fg = []
    for sprite, ox, oy in slots:
        x0, y0 = place(sprite, p, ox, oy)
        fg += rects(sprite, p, x0, y0, "Xh", INK)
        fg += rects(sprite, p, x0, y0, "o", AMBER_DEEP)

    # Themed: four cells is the mark, so the themed layer is four cells -- with
    # one sprite knocked out across the whole grid, at a size that cannot speck.
    mp = fit([(ALIEN, 0, 0)])
    mx, my = place(ALIEN, mp)
    mono = ['  <path fill="%s" fill-rule="evenodd" d="%s"/>' % (
        WHITE,
        "M26,26 h22 v22 h-22 Z M60,26 h22 v22 h-22 Z "
        "M26,60 h22 v22 h-22 Z M60,60 h22 v22 h-22 Z")]
    return bg, fg, mono, p


# --- b-the-cabinet -----------------------------------------------------------
# The same four games, but stated as a screen rather than as a chart: bezel
# vignette, phosphor glow, scanlines. The screen is background because a bezel
# has to reach the canvas edge, and anything reaching the edge cannot be
# foreground and still pass the 33dp check.

def b_the_cabinet():
    bg = ['  <defs>',
          '    <radialGradient id="b-g" gradientUnits="userSpaceOnUse" cx="54" cy="50" r="60">',
          '      <stop offset="0" stop-color="#2f7a5e"/>',
          '      <stop offset="0.55" stop-color="%s"/>' % SCREEN,
          '      <stop offset="1" stop-color="#061510"/>',
          '    </radialGradient>',
          '    <radialGradient id="b-v" gradientUnits="userSpaceOnUse" cx="54" cy="54" r="58">',
          '      <stop offset="0.42" stop-color="#000000" stop-opacity="0"/>',
          '      <stop offset="1" stop-color="#000000" stop-opacity="0.62"/>',
          '    </radialGradient>',
          '  </defs>',
          '  <rect width="108" height="108" fill="url(#b-g)"/>']
    bg += scanlines(0, 108, opacity=0.26, step=3.0, thick=1.15)
    # The bezel reads as glass rather than as a frame: one inset highlight, one
    # inset shadow, both clipped to nothing figurative.
    # No bezel rectangle: at 8..100dp it lies outside the 72dp any launcher
    # mask shows, so it was a frame that only ever existed in the viewer. The
    # curved-glass read comes from the vignette instead, which cannot be cropped.
    bg += ['  <rect width="108" height="108" fill="url(#b-v)"/>']

    slots = [(ALIEN, -5, -5), (PAD, 5, -5), (HEART, -5, 5), (SHIP, 5, 5)]
    p = fit([(s, ox * 1.0, oy * 1.0) for s, ox, oy in slots])
    tint = {0: LEAF_HI, 1: LEAF, 2: AMBER, 3: LEAF}
    fg = []
    for i, (sprite, ox, oy) in enumerate(slots):
        x0, y0 = place(sprite, p, ox, oy)
        fg += rects(sprite, p, x0, y0, "Xh", tint[i])
        fg += rects(sprite, p, x0, y0, "o", AMBER_DEEP)

    mono = ['  <path fill="%s" fill-rule="evenodd" d="%s"/>' % (
        WHITE,
        # A screen with four holes: the frame survives a flat tint, four
        # separate sprites would not.
        "M20,22 h68 a10,10 0 0 1 10,10 v44 a10,10 0 0 1 -10,10 h-68 "
        "a10,10 0 0 1 -10,-10 v-44 a10,10 0 0 1 10,-10 Z "
        "M28,32 h20 v18 h-20 Z M60,32 h20 v18 h-20 Z "
        "M28,58 h20 v18 h-20 Z M60,58 h20 v18 h-20 Z")]
    return bg, fg, mono, p


# --- c-one-in-four -----------------------------------------------------------
# Four quarters still, but the variety is the ground and the subject is one
# sprite laid across all of it. This is the concept that answers "what does the
# app do": the same game, every palette. It is also the only quadrant concept
# whose sprite pixels are still pixels at 48px.

def c_one_in_four():
    # Four palettes, and the SUBJECT swaps with the ground, not just the ground.
    # A palette swap on real hardware changes every colour in the frame, so an
    # icon about palette swapping that keeps one flat ink was only half telling
    # the truth. Each quadrant's ink is that quadrant's own hue taken to near
    # black: the four are close enough in luma that the silhouette stays one
    # shape at any size, and far enough apart in hue to be visible at 256px.
    # The eyes are knockouts, so each alien's eyes are its own quadrant's colour
    # for free.
    quads = [
        # The inks are near-black with a hint of their quadrant's hue, and they
        # are matched by EYE rather than by relative luminance. Equalising them
        # numerically at L=0.0135 still left the amber cell's brown reading
        # noticeably lighter than the other three and the top half of the
        # creature looking washed -- luminance does not model how much hue
        # weighs. These are a stop darker, where the silhouette holds as one
        # shape and the hue is a richness you find at 256px rather than a
        # difference you fight at 48.
        (0, 0, LEAF, "#0b1a10"),        # Emulia's own
        (54, 0, AMBER, "#1c1509"),      # Emulia's own
        (0, 54, TEAL, "#081918"),
        (54, 54, ROSE, "#251012"),
    ]
    bg = ['  <defs>',
          '    <radialGradient id="c-g" gradientUnits="userSpaceOnUse" cx="54" cy="54" r="66">',
          '      <stop offset="0" stop-color="#ffffff" stop-opacity="0.09"/>',
          '      <stop offset="1" stop-color="#000000" stop-opacity="0.26"/>',
          '    </radialGradient>',
          '  </defs>']
    for x, y, ground, _ in quads:
        bg.append('  <rect x="%d" y="%d" width="54" height="54" fill="%s"/>'
                  % (x, y, ground))
    bg.append('  <rect width="108" height="108" fill="url(#c-g)"/>')

    p = fit([(ALIEN_MK2, 0, 0)])
    x0, y0 = place(ALIEN_MK2, p)
    # One sprite, clipped four ways. Each pass paints the whole creature and the
    # clip decides which quarter of it survives, so the silhouette is identical
    # in all four and no seam can open between them.
    fg = ['  <defs>']
    for i, (x, y, _, _) in enumerate(quads):
        fg.append('    <clipPath id="c-q%d"><rect x="%d" y="%d" width="54" height="54"/></clipPath>' % (i, x, y))
    fg.append('  </defs>')
    for i, (_, _, _, ink) in enumerate(quads):
        fg.append('  <g clip-path="url(#c-q%d)">' % i)
        fg += ["  " + r for r in rects(ALIEN_MK2, p, x0, y0, "Xh", ink)]
        fg.append('  </g>')

    mono = rects(ALIEN_MK2, p, x0, y0, "Xh", WHITE)
    return bg, fg, mono, p


# --- d-the-emu ---------------------------------------------------------------
# Emu-lia. The name has a bird in it, and a name that hands you a pun is the
# one case where the obvious move is the right one.

def d_the_emu():
    bg = ['  <defs>',
          '    <radialGradient id="d-g" gradientUnits="userSpaceOnUse" cx="54" cy="46" r="64">',
          '      <stop offset="0" stop-color="#2c6a51"/>',
          '      <stop offset="0.6" stop-color="%s"/>' % GROUND,
          '      <stop offset="1" stop-color="#123024"/>',
          '    </radialGradient>',
          '  </defs>',
          '  <rect width="108" height="108" fill="url(#d-g)"/>']

    p = fit([(EMU, 0, 0)])
    x0, y0 = place(EMU, p)
    fg = rects(EMU, p, x0, y0, "Xh", LEAF)
    fg += rects(EMU, p, x0, y0, "o", AMBER_DEEP)

    mono = rects(EMU, p, x0, y0, "Xho", WHITE)
    return bg, fg, mono, p


# --- e-half-and-half ---------------------------------------------------------
# One sprite, drawn twice: hard pixels on the left, the same silhouette rounded
# and merged on the right. It is the only mark in the set that says what an
# emulator argues about -- Emulia ships ten picture modes and a palette
# importer, and this is that argument as a shape.
#
# The rounding is real geometry, not a filter: SVG filters do not survive the
# conversion to an Android vector drawable, so a blur here would look right in
# the viewer and ship as hard squares. Overlapping rounded rects merge into the
# same blob and convert exactly.

def e_half_and_half():
    bg = ['  <defs>',
          '    <linearGradient id="e-g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="108" y2="108">',
          '      <stop offset="0" stop-color="%s"/>' % GROUND_DEEP,
          '      <stop offset="0.5" stop-color="#1d4735"/>',
          '      <stop offset="1" stop-color="%s"/>' % GROUND,
          '    </linearGradient>',
          '  </defs>',
          '  <rect width="108" height="108" fill="url(#e-g)"/>',
          # The seam was a 1.2dp bright line, which is 0.53px at mdpi. Measured
          # on the rasters its contrast against the ground fell 203 -> 97 -> 67
          # from 256 to 32px: a sub-pixel highlight does not survive, it smears,
          # and a smear down the middle of a sprite reads as a crack rather than
          # as a join. The split is carried on area instead, which downscales
          # exactly -- the left half simply sits a step darker.
          '  <rect x="0" y="0" width="54" height="108" fill="#000000" '
          'fill-opacity="0.17"/>']

    p = fit([(ALIEN, 0, 0)], pad_frac=0.22)
    x0, y0 = place(ALIEN, p)

    rows, w, h = grid(ALIEN)
    lmap = "\n".join(r[:4] + "." * (w - 4) for r in rows) + "\n"
    rmap = "\n".join("." * 4 + r[4:] for r in rows) + "\n"

    # grow closes the hairline between adjacent rounded runs so they read as one
    # smoothed form rather than as a column of lozenges. It has to exceed the
    # notch two convex corners leave where runs of different length stack --
    # rx*(1 - 1/sqrt2), or 0.117p here. At grow 0.10p it did not, and the themed
    # layer came back with eleven 1px holes in it.
    RX, GROW = p * 0.40, p * 0.22
    fg = rects(lmap, p, x0, y0, "Xh", LEAF)
    fg += rects(rmap, p, x0, y0, "Xh", LEAF_HI, rx=RX, grow=GROW)

    mono = rects(lmap, p, x0, y0, "Xh", WHITE)
    mono += rects(rmap, p, x0, y0, "Xh", WHITE, rx=RX, grow=GROW)
    return bg, fg, mono, p


CONCEPTS = {
    "a-four-screens": a_four_screens,
    "b-the-cabinet": b_the_cabinet,
    "c-one-in-four": c_one_in_four,
    "d-the-emu": d_the_emu,
    "e-half-and-half": e_half_and_half,
}

NOTES = {
    "a-four-screens": "Four games, four cells, full-bleed checker ground.",
    "b-the-cabinet": "The same four, stated as a CRT: bezel, phosphor, scanlines.",
    "c-one-in-four": "Four palettes under one sprite -- the quadrants are the ground.",
    "d-the-emu": "Emu-lia. The pun the name was already making.",
    "e-half-and-half": "One sprite rendered twice: hard pixels, then smoothed.",
}


def main():
    for slug, fn in CONCEPTS.items():
        d = ROOT / "concepts" / slug
        d.mkdir(parents=True, exist_ok=True)
        bg, fg, mono, p = fn()
        (d / "adaptive-background.svg").write_text(svg(bg, NOTES[slug]))
        (d / "adaptive-foreground.svg").write_text(svg(fg))
        (d / "adaptive-monochrome.svg").write_text(svg(mono))
        print("%-18s pixel %.2fdp  = %.2fpx at mdpi(48)  %.2fpx at xhdpi(96)"
              % (slug, p, p * 48 / 108.0, p * 96 / 108.0))


if __name__ == "__main__":
    main()
