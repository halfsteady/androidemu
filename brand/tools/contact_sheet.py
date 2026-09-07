#!/usr/bin/env python3
"""One image showing every concept at every size that decides anything.

  python3 brand/tools/contact_sheet.py        # -> brand/review/contact-sheet.png

Three questions this is built to answer, in this order: does the mark survive
48px (the launcher), does it survive a flat system tint (the themed icon), and
does it hold together large. Everything is the real raster off disk -- nothing
here re-renders, so what is on the sheet is what would ship.
"""
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
BIG = 256
SMALL = [128, 96, 64, 48, 32]
PAD = 18
LABEL_W = 340   # the label column; "costs" is the widest thing on the sheet
DARK, LIGHT = (13, 17, 23), (244, 246, 249)
INK_D, INK_L = (229, 236, 243), (22, 32, 43)


def font(sz, bold=False):
    for p in ("/usr/share/fonts/truetype/dejavu/DejaVuSans%s.ttf"
              % ("-Bold" if bold else ""),):
        if Path(p).exists():
            return ImageFont.truetype(p, sz)
    return ImageFont.load_default()


def wrap(d, text, fnt, width):
    """Greedy wrap: the label column is fixed, and the first sheet ran its
    descriptions straight under the 256px tile."""
    words, lines, cur = text.split(), [], ""
    for w in words:
        trial = (cur + " " + w).strip()
        if d.textlength(trial, font=fnt) <= width or not cur:
            cur = trial
        else:
            lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return lines


def tinted(mono: Image.Image, colour) -> Image.Image:
    """The themed icon as Android draws it: this layer's alpha, one flat tint."""
    out = Image.new("RGBA", mono.size, colour + (0,))
    out.putalpha(mono.getchannel("A"))
    return out


def main():
    slugs = sorted(p.name for p in (ROOT / "concepts").iterdir() if p.is_dir())
    metas = {s: json.loads((ROOT / "concepts" / s / "meta.json").read_text()) for s in slugs}

    col_x, x = [], PAD + LABEL_W
    for s in [BIG] + SMALL:
        col_x.append(x)
        x += s + PAD
    mono_x = x + 10
    width = mono_x + (96 + PAD) * 2 + PAD
    row_h = BIG + PAD * 2 + 26
    height = 84 + row_h * len(slugs) + PAD

    sheet = Image.new("RGB", (width, height), DARK)
    d = ImageDraw.Draw(sheet)
    d.text((PAD, 22), "Emulia — launcher icon concepts", font=font(26, True), fill=INK_D)
    d.text((PAD, 54), "every raster is the shipped file; the last two columns are the "
                      "Android 13 themed icon under a flat tint",
           font=font(13), fill=(147, 161, 179))
    for i, s in enumerate([BIG] + SMALL):
        d.text((col_x[i], 62), "%dpx" % s, font=font(12, True), fill=(147, 161, 179))
    d.text((mono_x, 62), "themed (dark)", font=font(12, True), fill=(147, 161, 179))
    d.text((mono_x + 96 + PAD, 62), "themed (light)", font=font(12, True), fill=(147, 161, 179))

    for r, slug in enumerate(slugs):
        y = 84 + r * row_h
        if r % 2:
            d.rectangle((0, y - 6, width, y + row_h - 12), fill=(19, 25, 35))
        m = metas[slug]
        d.text((PAD, y + 8), "%s.  %s" % (m["letter"].upper(), m["title"]),
               font=font(19, True), fill=INK_D)
        ty = y + 34
        for line in wrap(d, m["reads_as"], font(12), LABEL_W - PAD):
            d.text((PAD, ty), line, font=font(12), fill=(147, 161, 179))
            ty += 16
        ty += 4
        for line in wrap(d, m["mood"], font(12), LABEL_W - PAD):
            d.text((PAD, ty), line, font=font(12), fill=(99, 116, 138))
            ty += 16
        ty += 6
        for line in wrap(d, "costs: " + m["risk"], font(11), LABEL_W - PAD):
            d.text((PAD, ty), line, font=font(11), fill=(126, 106, 90))
            ty += 15

        for i, s in enumerate([BIG] + SMALL):
            tile = Image.open(ROOT / "png" / slug / ("tile-%d.png" % s))
            sheet.paste(tile.convert("RGB"), (col_x[i], y + 8))
            # A second copy of the smallest sizes on a LIGHT ground: a dark tile
            # dissolving into white store chrome is a real failure mode.
            if s <= 64:
                chip = Image.new("RGB", (s + 10, s + 10), LIGHT)
                chip.paste(tile.convert("RGB"), (5, 5))
                sheet.paste(chip, (col_x[i] - 5, y + 8 + BIG - s - 5))

        mono = Image.open(ROOT / "png" / slug / "adaptive-monochrome-216.png").convert("RGBA")
        mono = mono.resize((96, 96), Image.LANCZOS)
        for j, (bgc, tint) in enumerate((((32, 38, 48), (232, 240, 248)),
                                         ((226, 232, 240), (32, 40, 52)))):
            chip = Image.new("RGB", (96, 96), bgc)
            chip.paste(tinted(mono, tint), (0, 0), tinted(mono, tint))
            sheet.paste(chip, (mono_x + j * (96 + PAD), y + 8))

    out = ROOT / "review"
    out.mkdir(exist_ok=True)
    sheet.save(out / "contact-sheet.png")
    print("wrote %s  (%dx%d)" % (out / "contact-sheet.png", width, height))


if __name__ == "__main__":
    main()
