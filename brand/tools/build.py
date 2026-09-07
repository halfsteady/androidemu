#!/usr/bin/env python3
"""Build every shippable form of a Controlplaine mark from three SVG layers.

  python3 brand/tools/build.py                # every concept
  python3 brand/tools/build.py c-the-tower    # just one

The source of truth for a concept is `concepts/<slug>/adaptive-{background,
foreground,monochrome}.svg`, each authored on the adaptive icon's own 108x108
canvas. Everything below is derived from those three files, never re-derived
from geometry kept somewhere else: an Android drawable maintained beside its SVG
drifts silently, and nobody notices the launcher icon is two revisions behind
the favicon until it is on a phone. (The sibling project's
`brand/tools/build-android.mjs` reached the same rule and says it better: making
the drawable a parse of the emitted file makes it exactly the asset that was
signed off.)

Two constraints shape the SVG subset a concept may use -- see `to_avd`. Android
vector drawables have no dash arrays, no <use>, and no way to reproduce a
gradient's coordinates through a group transform; so concepts are authored flat,
absolute and transform-free. That costs nothing, because a launcher icon is
48px of flat shapes anyway.

Rasterizing goes through headless Chrome. There is no rsvg, inkscape or
cairosvg on this machine, and no pip to add one -- and Chrome renders the same
SVG the web client will serve, which is the comparison that matters.
"""

import json
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import quote

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
SVG = "{http://www.w3.org/2000/svg}"

CANVAS = 108           # the adaptive icon canvas, in dp
VISIBLE = 72           # what a launcher mask can show of it
SAFE_R = 33            # the always-visible circle's radius; validate.py measures it
MASTER = 1080          # 10x the canvas: headroom for the 512px Play icon in one render
# Chrome will not reliably paint a window much under this; see `rasterize`.
MIN_WINDOW = 512

# Legacy launcher rasters. minSdk is 29 so every device here has adaptive icons
# and reads the anydpi-v26 alias -- these exist for the odd launcher and widget
# surface that still asks for a bitmap, and cost five small files each.
DENSITIES = [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)]
# Review + web + store rasters of the full-bleed tile.
TILE_SIZES = [16, 32, 48, 64, 96, 128, 180, 192, 256, 512, 1024]
# The themed-icon layer is checked at every launcher density, because a sliver
# that resolves as a hole at one density is solid ink at another.
MONO_SIZES = [108, 162, 216, 324, 432]

# Whether the adaptive icon OFFERS its themed layer to the launcher.
#
# It is generated and measured either way -- check 6 reads its rasters, and it
# stays in the tree ready to be switched back on. What this controls is the one
# line of the anydpi alias that hands it to the system.
#
# Off, because of how "Themed icons" actually behaves rather than how it is
# documented: One UI recolours only the apps that SHIP a monochrome layer and
# leaves every other icon alone. Providing one therefore did not make Emulia
# match the home screen, it made Emulia the single dark tile on a screen full of
# colour -- the opposite of the intent. Emulia's mark is four colours; being
# flattened to one tint costs it the entire idea.
OFFER_MONOCHROME = False

CHROME = shutil.which("google-chrome") or shutil.which("google-chrome-stable")
PROFILE = Path("/tmp/claude-1000/emulia-brand-chrome")


# ---------------------------------------------------------------- rasterizing

def rasterize(svg_path: Path, size: int, out: Path) -> Image.Image:
    """Render an SVG to a transparent PNG of exactly size x size.

    Two Chrome behaviours are worked around here, both of which produce a file
    that looks like a design mistake rather than a tooling one.

    The SVG is wrapped in a zero-margin HTML page, because Chrome gives a bare
    .svg file the default 8px body margin and crops the artwork.

    And it is rendered inside a window of at least MIN_WINDOW and then CROPPED,
    because a small `--window-size` silently paints only part of the page:
    measured on this machine at a 108px dart, a 108 and a 120 window came back
    with zero painted pixels, 128 with 26, 150 with 1996 and 162 with 2325. The
    themed-icon layer at 108dp -- the smallest launcher density, and the one the
    themed icon is most likely to be drawn at -- came out blank for every
    concept. The SVG is still sized to `size`, so each density is a true render
    at that scale and not a downsample of a bigger one; that is the whole point
    of cutting the themed layer at five densities.
    """
    if CHROME is None:
        sys.exit("no google-chrome on PATH; it is the only rasterizer here")
    out.parent.mkdir(parents=True, exist_ok=True)
    html = out.with_suffix(".build.html")
    svg = svg_path.read_text()
    svg = re.sub(r'width="108"', 'width="%d"' % size, svg)
    svg = re.sub(r'height="108"', 'height="%d"' % size, svg)
    win = max(size, MIN_WINDOW)
    html.write_text(
        "<!doctype html><meta charset=utf-8><style>html,body{margin:0;padding:0;"
        "background:transparent;width:%dpx;height:%dpx;overflow:hidden}"
        "svg{display:block}</style>%s" % (win, win, svg))
    subprocess.run([
        CHROME, "--headless", "--disable-gpu", "--no-sandbox", "--hide-scrollbars",
        "--user-data-dir=%s" % PROFILE, "--default-background-color=00000000",
        "--force-device-scale-factor=1", "--window-size=%d,%d" % (win, win),
        "--screenshot=%s" % out, str(html),
    ], check=True, capture_output=True)
    html.unlink()
    im = Image.open(out)
    if im.size != (win, win):
        sys.exit("chrome rendered %s at %s, wanted %dx%d"
                 % (svg_path.name, im.size, win, win))
    im = im.convert("RGBA")
    if win != size:
        im = im.crop((0, 0, size, size))
        im.save(out)
    if not any(im.getchannel("A").histogram()[1:]):
        sys.exit("chrome rendered %s at %d as ENTIRELY TRANSPARENT"
                 % (svg_path.name, size))
    return im


# ------------------------------------------------------------- SVG -> Android

def _stop_colour(stop) -> str:
    """A gradient stop as #RRGGBBAA, folding in stop-opacity.

    An Android gradient item has no separate alpha attribute -- the only place
    it can carry one is the colour. Dropping stop-opacity here silently turned
    the launcher background's 9%-white-to-26%-black vignette into an OPAQUE
    white-to-black radial painted over all four colour quadrants, which is what
    shipped in v0.2.3-rc2 through rc4: on the tablet the icon read as a grey
    disc with a sprite on it and none of the colour underneath.

    Nothing caught it because every check measures rasters rendered from the
    SVG, and the SVG was right. See check 9, which exists because of this.
    """
    col = stop.get("stop-color") or "#000000"
    a = stop.get("stop-opacity")
    if a in (None, "") or _num(a, 1.0) >= 1.0 or len(col) == 9:
        return col
    return col + "%02x" % max(0, min(255, round(_num(a, 1.0) * 255)))


def _color(v: str) -> str:
    """SVG writes alpha last (#RRGGBBAA); Android writes it first (#AARRGGBB)."""
    if re.fullmatch(r"#[0-9a-fA-F]{8}", v or ""):
        return "#" + v[7:9] + v[1:7]
    return v


def _num(v, default=0.0):
    return float(v) if v not in (None, "") else default


def _shape_to_path(el) -> str:
    """One SVG element's geometry as a path `d`: absolute, transform-free."""
    tag = el.tag.replace(SVG, "")
    if tag == "path":
        return " ".join(el.get("d").split())
    if tag == "rect":
        x, y = _num(el.get("x")), _num(el.get("y"))
        w, h = _num(el.get("width")), _num(el.get("height"))
        rx = _num(el.get("rx"), _num(el.get("ry")))
        ry = _num(el.get("ry"), rx)
        if not rx and not ry:
            return "M%g,%g h%g v%g h%g Z" % (x, y, w, h, -w)
        return ("M%g,%g h%g a%g,%g 0 0 1 %g,%g v%g a%g,%g 0 0 1 %g,%g "
                "h%g a%g,%g 0 0 1 %g,%g v%g a%g,%g 0 0 1 %g,%g Z") % (
            x + rx, y, w - 2 * rx, rx, ry, rx, ry, h - 2 * ry, rx, ry, -rx, ry,
            -(w - 2 * rx), rx, ry, -rx, -ry, -(h - 2 * ry), rx, ry, rx, -ry)
    if tag in ("circle", "ellipse"):
        cx, cy = _num(el.get("cx")), _num(el.get("cy"))
        rx = _num(el.get("r")) or _num(el.get("rx"))
        ry = _num(el.get("r")) or _num(el.get("ry"))
        # Two half-turn arcs: a single full-circle arc is degenerate and draws nothing.
        return "M%g,%g a%g,%g 0 1 0 %g,0 a%g,%g 0 1 0 %g,0 Z" % (
            cx - rx, cy, rx, ry, 2 * rx, rx, ry, -2 * rx)
    if tag in ("polygon", "polyline"):
        pts = [p for p in re.split(r"[,\s]+", el.get("points").strip()) if p]
        d = "M%s,%s" % (pts[0], pts[1]) + "".join(
            " L%s,%s" % (pts[i], pts[i + 1]) for i in range(2, len(pts) - 1, 2))
        return d + (" Z" if tag == "polygon" else "")
    if tag == "line":
        return "M%g,%g L%g,%g" % (_num(el.get("x1")), _num(el.get("y1")),
                                  _num(el.get("x2")), _num(el.get("y2")))
    return ""


def _gradients(root):
    out = {}
    for kind in ("linearGradient", "radialGradient"):
        for g in root.iter(SVG + kind):
            if g.get("gradientUnits") != "userSpaceOnUse":
                sys.exit("gradient #%s must be gradientUnits=userSpaceOnUse: an "
                         "Android gradient has no object bounding box to map onto"
                         % g.get("id"))
            out[g.get("id")] = (kind, g.attrib,
                                [(_num(s.get("offset")), _stop_colour(s))
                                 for s in g.iter(SVG + "stop")])
    return out


def _clippaths(root):
    """Every <clipPath> in the document, as AVD path data.

    to_avd used to ignore a clip-path on a <g> the same silent way SVG ignores
    an unknown attribute -- which turned mark-small's rounded tile into a
    square one, with nothing saying so. Collected like gradients: by id, from
    the shapes inside the def.
    """
    out = {}
    for cp in root.iter(SVG + "clipPath"):
        ds = [_shape_to_path(el) for el in cp]
        out[cp.get("id")] = " ".join(d for d in ds if d)
    return out


def _paint(key, value, grads):
    """One AVD paint attribute, or a nested <gradient> when the SVG names one."""
    ref = re.match(r"url\(#(.+?)\)", value or "")
    if not ref:
        return ' android:%s="%s"' % (key, _color(value)), ""
    kind, at, stops = grads[ref.group(1)]
    items = "".join('\n                <item android:offset="%g" android:color="%s" />'
                    % (off, _color(col)) for off, col in stops)
    if kind == "linearGradient":
        geom = ('android:type="linear" android:startX="%s" android:startY="%s" '
                'android:endX="%s" android:endY="%s"'
                % (at.get("x1", "0"), at.get("y1", "0"),
                   at.get("x2", "0"), at.get("y2", "0")))
    else:
        geom = ('android:type="radial" android:centerX="%s" android:centerY="%s" '
                'android:gradientRadius="%s"'
                % (at.get("cx", "54"), at.get("cy", "54"), at.get("r", "54")))
    return "", ('\n            <aapt:attr name="android:%s">\n'
                '                <gradient %s>%s\n'
                '                </gradient>\n            </aapt:attr>'
                % (key, geom, items))


def to_avd(svg_path: Path, tint: str | None = None,
           viewport: tuple[float, float, float] | None = None,
           note: str | None = None) -> str:
    """An Android vector drawable equivalent to one concept layer.

    `tint` forces every shape to one colour, which is what the themed-icon
    layer needs: Android keeps that layer's alpha and supplies its own tint, so
    the colour written into it is never seen.

    `viewport` is (minX, minY, size): crop the drawable to that square. An AVD
    viewport has no origin, so the crop is a translate <group> around the whole
    body rather than rewritten coordinates -- Android applies a group's matrix
    to gradient shaders along with their paths, so userSpaceOnUse coordinates
    stay attached to the art. It exists for UI marks: the adaptive canvas is
    108 with the art inside 18-90, and a drawable that keeps the margin draws a
    72dp Icon's art at 48dp.

    `note` is an extra line for the GENERATED header, for a drawable whose
    form needs a sentence of why.
    """
    root = ET.parse(svg_path).getroot()
    grads = _gradients(root)
    clips = _clippaths(root)
    body, needs_aapt = [], False

    def walk(node):
        nonlocal needs_aapt
        for el in node:
            tag = el.tag.replace(SVG, "")
            if tag in ("defs", "title", "desc", "metadata"):
                continue
            if tag == "g":
                # Grouping is fine; inherited paint is not. An AVD path with no
                # fill draws BLACK rather than nothing, so a fill that lived on
                # the <g> becomes a black shape on the phone and nowhere else.
                if {"fill", "stroke", "transform", "opacity"} & set(el.attrib):
                    sys.exit("%s: a <g> carries paint or transform; put them on the "
                             "shapes so this conversion stays mechanical" % svg_path.name)
                clip = el.get("clip-path")
                if clip:
                    ref = re.match(r"url\(#(.+?)\)", clip)
                    d = clips.get(ref.group(1)) if ref else None
                    if not d:
                        sys.exit("%s: a <g> names a clip-path this file does not "
                                 "define" % svg_path.name)
                    body.append('        <group>\n'
                                '            <clip-path android:pathData="%s" />' % d)
                    walk(el)
                    body.append('        </group>')
                else:
                    walk(el)
                continue
            if "transform" in el.attrib:
                sys.exit("%s: <%s> carries a transform; author absolute coordinates "
                         "(a VectorDrawable group cannot carry gradient coords "
                         "through a scale)" % (svg_path.name, tag))
            d = _shape_to_path(el)
            if not d:
                continue
            fill, stroke = el.get("fill"), el.get("stroke")
            if not fill and not stroke:
                sys.exit("%s: a <%s> has no explicit fill or stroke" % (svg_path.name, tag))
            if (fill in (None, "none")) and (stroke in (None, "none")):
                continue
            attrs, nested = "", ""
            if fill and fill != "none":
                a, n = _paint("fillColor", tint or fill, grads)
                attrs += a
                nested += n
            if stroke and stroke != "none":
                a, n = _paint("strokeColor", tint or stroke, grads)
                attrs += a
                nested += n
                attrs += ' android:strokeWidth="%s"' % el.get("stroke-width", "1")
                for s, a_ in (("stroke-linecap", "strokeLineCap"),
                              ("stroke-linejoin", "strokeLineJoin")):
                    if el.get(s):
                        attrs += ' android:%s="%s"' % (a_, el.get(s))
            if el.get("fill-rule") == "evenodd":
                attrs += ' android:fillType="evenOdd"'
            for s, a_ in (("fill-opacity", "fillAlpha"), ("stroke-opacity", "strokeAlpha")):
                if el.get(s):
                    attrs += ' android:%s="%s"' % (a_, el.get(s))
            needs_aapt = needs_aapt or bool(nested)
            attrs = attrs.replace(' android:', '\n            android:')
            if nested:
                body.append('        <path\n            android:pathData="%s"%s>%s\n'
                            '        </path>' % (d, attrs, nested))
            else:
                body.append('        <path\n            android:pathData="%s"%s />' % (d, attrs))

    walk(root)
    size = CANVAS
    if viewport:
        minx, miny, size = viewport
        body = (['        <group\n'
                 '            android:translateX="%g"\n'
                 '            android:translateY="%g">' % (-minx, -miny)]
                + body + ['        </group>'])
    ns = 'xmlns:android="http://schemas.android.com/apk/res/android"'
    if needs_aapt:
        ns += '\n    xmlns:aapt="http://schemas.android.com/aapt"'
    extra = ('     %s\n' % note) if note else ''
    return ('<?xml version="1.0" encoding="utf-8"?>\n'
            '<!-- GENERATED by brand/tools/build.py from\n'
            '     brand/concepts/%s/%s - do not hand-edit.\n'
            '%s'
            '     Regenerate: python3 brand/tools/build.py %s -->\n'
            '<vector %s\n'
            '    android:width="%gdp"\n    android:height="%gdp"\n'
            '    android:viewportWidth="%g"\n    android:viewportHeight="%g">\n'
            '%s\n</vector>\n'
            % (svg_path.parent.name, svg_path.name, extra, svg_path.parent.name,
               ns, size, size, size, size, "\n".join(body)))


# --------------------------------------------------------------- compositing

def _mask(size: int, kind: str) -> Image.Image:
    """A launcher mask, drawn at 4x and downsampled: PIL's draw has no AA."""
    s = size * 4
    m = Image.new("L", (s, s), 0)
    d = ImageDraw.Draw(m)
    if kind == "round":
        d.ellipse((0, 0, s - 1, s - 1), fill=255)
    else:
        # Launchers of this era round a square icon fairly hard, and each OEM
        # differently; 22% is inside every mask we cannot predict.
        d.rounded_rectangle((0, 0, s - 1, s - 1), radius=int(s * 0.22), fill=255)
    return m.resize((size, size), Image.LANCZOS)


def visible_area(master: Image.Image) -> Image.Image:
    """The 72dp a launcher actually shows, out of the 108dp canvas."""
    inset = round(MASTER * (CANVAS - VISIBLE) / (2 * CANVAS))
    return master.crop((inset, inset, MASTER - inset, MASTER - inset))


def _inner(svg_text: str) -> str:
    s = re.sub(r"<\?xml.*?\?>", "", svg_text, flags=re.S)
    s = re.sub(r"<!--.*?-->", "", s, flags=re.S)
    s = re.sub(r"<svg[^>]*>", "", s, count=1)
    return s.replace("</svg>", "").strip()


def logo_svg(bg: str, fg: str, size: int, radius_pct: float) -> str:
    """The layers as one self-contained SVG, clipped the way a launcher clips.

    The visible 72dp is scaled up to fill the frame: a logo should not be the
    launcher's parallax slack with the art shrunk in the middle of it.
    """
    r = CANVAS * radius_pct / 100.0
    scale = CANVAS / float(VISIBLE)
    off = (CANVAS - VISIBLE) / 2.0
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" '
            'width="%d" height="%d">\n'
            '<defs><clipPath id="cp"><rect width="%d" height="%d" rx="%g"/></clipPath></defs>\n'
            '<g clip-path="url(#cp)"><g transform="scale(%g) translate(%g,%g)">\n'
            '%s\n%s\n</g></g>\n</svg>\n'
            % (CANVAS, CANVAS, size, size, CANVAS, CANVAS, r, scale, -off, -off,
               _inner(bg), _inner(fg)))


# --------------------------------------------------------------------- build

def datauri(svg_text: str) -> str:
    """An SVG as one data: URI line, for an href the web client pastes inline."""
    return ("data:image/svg+xml,"
            + quote(re.sub(r"\s+", " ", svg_text).strip(), safe="/:=<>#(){}[];,.-+*'")
            + "\n")


def inline_svg(svg_text: str) -> str:
    """The same SVG as markup to embed in a page, comments and xml decl stripped."""
    s = re.sub(r"<\?xml.*?\?>", "", svg_text, flags=re.S)
    s = re.sub(r"<!--.*?-->", "", s, flags=re.S)
    # A page sizes the mark with CSS; a hard width/height on the element fights it.
    s = re.sub(r'\s(?:width|height)="108"', "", s, count=2)
    return re.sub(r"\n\s*\n", "\n", s).strip() + "\n"


def build(slug: str) -> dict:
    src = ROOT / "concepts" / slug
    meta = json.loads((src / "meta.json").read_text())
    png = ROOT / "png" / slug
    res = ROOT / "android" / slug
    svgd = ROOT / "svg" / slug
    for p in (png, svgd, res / "drawable", res / "mipmap-anydpi-v26"):
        p.mkdir(parents=True, exist_ok=True)

    # 1. one master render per layer
    bg = rasterize(src / "adaptive-background.svg", MASTER, png / "_master-bg.png")
    fg = rasterize(src / "adaptive-foreground.svg", MASTER, png / "_master-fg.png")
    (png / "_master-bg.png").unlink()
    (png / "_master-fg.png").unlink()
    full = Image.alpha_composite(bg, fg)
    fg.resize((432, 432), Image.LANCZOS).save(png / "adaptive-foreground-432.png")
    full.resize((432, 432), Image.LANCZOS).save(png / "adaptive-full-432.png")
    vis = visible_area(full)

    # 2. the themed-icon layer at every launcher density
    for s in MONO_SIZES:
        rasterize(src / "adaptive-monochrome.svg", s, png / ("adaptive-monochrome-%d.png" % s))

    # 3. the full-bleed tile, for review, the web and the store
    for s in TILE_SIZES:
        vis.resize((s, s), Image.LANCZOS).save(png / ("tile-%d.png" % s))
    # Play wants 32-bit PNG and does its own rounding, so: opaque, unmasked.
    vis.resize((512, 512), Image.LANCZOS).convert("RGB").convert("RGBA").save(
        png / "play-512.png")

    # 4. legacy launcher rasters, square and round
    for dens, sz in DENSITIES:
        (res / ("mipmap-" + dens)).mkdir(parents=True, exist_ok=True)
        small = vis.resize((sz, sz), Image.LANCZOS)
        for kind, name in (("square", "ic_launcher.png"), ("round", "ic_launcher_round.png")):
            im = small.copy()
            im.putalpha(Image.composite(im.getchannel("A"),
                                        Image.new("L", (sz, sz), 0), _mask(sz, kind)))
            im.save(res / ("mipmap-" + dens) / name)

    # 5. the adaptive layers and the two mipmap aliases
    (res / "drawable" / "ic_launcher_background.xml").write_text(
        to_avd(src / "adaptive-background.svg"))
    (res / "drawable" / "ic_launcher_foreground.xml").write_text(
        to_avd(src / "adaptive-foreground.svg"))
    # Flat white here is never seen -- the system tints this layer -- but it
    # makes the drawable obvious in a preview pane.
    (res / "drawable" / "ic_launcher_monochrome.xml").write_text(
        to_avd(src / "adaptive-monochrome.svg", tint="#FFFFFF"))
    # Deliberately NOT the foreground where it is offered: the system flattens
    # that layer to a single tint, and a multi-colour foreground becomes a blob.
    mono = ('    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />\n'
            if OFFER_MONOCHROME else
            '    <!-- No <monochrome>: see OFFER_MONOCHROME in brand/tools/build.py.\n'
            '         Without it a themed launcher falls back to the colour art, which\n'
            '         is the whole mark rather than one quarter of its idea. -->\n')
    alias = ('<?xml version="1.0" encoding="utf-8"?>\n'
             '<!-- GENERATED by brand/tools/build.py -->\n'
             '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
             '    <background android:drawable="@drawable/ic_launcher_background" />\n'
             '    <foreground android:drawable="@drawable/ic_launcher_foreground" />\n'
             + mono +
             '</adaptive-icon>\n')
    for f in ("ic_launcher.xml", "ic_launcher_round.xml"):
        (res / "mipmap-anydpi-v26" / f).write_text(alias)

    # 6. the logo as one file, and the favicon the web client pastes inline
    bt, ft = (src / "adaptive-background.svg").read_text(), (src / "adaptive-foreground.svg").read_text()
    (svgd / "logo.svg").write_text(logo_svg(bt, ft, 512, 22.0))
    fav = logo_svg(bt, ft, 32, 25.0)
    (svgd / "favicon.svg").write_text(fav)
    (svgd / "favicon-datauri.txt").write_text(datauri(fav))

    # 7. the optional small master: a concept's own drawing for 32px and under,
    #    where the icon's finest details stop being details. Optional because
    #    only a concept that HAS such details needs one -- a-paper-plane is two
    #    flat folds and reduces to itself.
    small = src / "mark-small.svg"
    if small.exists():
        text = small.read_text()
        (svgd / "mark-small.svg").write_text(text)
        (svgd / "mark-small-datauri.txt").write_text(datauri(text))
        # Inline form for the web client, which has no build step: markup only,
        # so it can be dropped straight between the topbar's brand markers.
        (svgd / "mark-small-inline.svg").write_text(inline_svg(text))
        # The same drawing as an app drawable, for the Compose UI's empty
        # states and the Settings about line. Full colour WITH the rounded
        # tile: the mark carries its own sky, so it reads on a light panel and
        # a dark one without asking the theme -- the web's #empty solves the
        # same problem the other way (a themed two-tone composition), which a
        # drawable cannot do because android:tint paints every path. Viewport
        # cropped to the tile so a 72dp slot draws a 72dp mark.
        (res / "drawable" / "ic_plaine_mark.xml").write_text(
            to_avd(small, viewport=(18, 18, 72),
                   note="Full-colour small master, cropped to the tile: "
                        "draw with Image, not a tinted Icon."))
    return meta


if __name__ == "__main__":
    slugs = sys.argv[1:] or sorted(p.name for p in (ROOT / "concepts").iterdir()
                                   if p.is_dir())
    for s in slugs:
        m = build(s)
        print("built %-18s %s" % (s, m.get("title", "")))
