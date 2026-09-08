#!/usr/bin/env python3
"""Store assets Play asks for that are not the launcher icon.

  python3 brand/tools/play_assets.py [slug]

The app icon Play wants is already `png/<slug>/play-512.png` from build.py --
that one is the mark, and it is measured by validate.py check 3. This makes the
**feature graphic**, which is 1024x500 and has no equivalent anywhere else: it
is the banner at the top of a store listing, and it is a required field, so
without it the listing cannot be completed no matter how good the icon is.

Composition is deliberately the mark and the name and nothing else. A feature
graphic is shown at wildly different sizes and is often cropped, and Play's own
guidance is that it should not carry small text or device frames. The four
quadrant colours are the only decoration, taken from the mark itself so the
banner and the icon are visibly the same object.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
W, H = 1024, 500

INK = (17, 24, 19)            # the app's own background
LEAF = (185, 227, 140)
QUADS = [(185, 227, 140), (240, 212, 154), (127, 210, 196), (232, 163, 154)]


def font(sz, bold=True):
    p = "/usr/share/fonts/truetype/dejavu/DejaVuSans%s.ttf" % ("-Bold" if bold else "")
    return ImageFont.truetype(p, sz) if Path(p).exists() else ImageFont.load_default()


def main():
    slug = sys.argv[1] if len(sys.argv) > 1 else "c-one-in-four"
    src = ROOT / "png" / slug / "tile-512.png"
    if not src.exists():
        sys.exit("no built art for %s - run build.py first" % slug)

    im = Image.new("RGB", (W, H), INK)
    d = ImageDraw.Draw(im)

    # A thin band of the mark's own four colours along the bottom. It reads as
    # the icon's palette at any crop, and carries no information that being
    # cropped could lose.
    band = 10
    for i, c in enumerate(QUADS):
        d.rectangle((i * W // 4, H - band, (i + 1) * W // 4, H), fill=c)

    # The mark, with its rounded tile, at the size it is actually legible.
    size = 260
    mark = Image.open(src).convert("RGBA").resize((size, size), Image.LANCZOS)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1),
                                           radius=int(size * 24 / 108.0), fill=255)
    mx, my = 96, (H - band - size) // 2
    im.paste(mark, (mx, my), mask)

    x = mx + size + 56
    d.text((x, my + 62), "EMULIA", font=font(92), fill=LEAF)
    d.text((x, my + 172), "8-bit games, on your tablet.",
           font=font(34, bold=False), fill=(200, 214, 196))

    out = ROOT / "png" / slug / "play-feature-1024x500.png"
    im.save(out)
    print("wrote %s  (%dx%d)" % (out.relative_to(ROOT.parent), W, H))


if __name__ == "__main__":
    main()
