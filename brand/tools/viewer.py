#!/usr/bin/env python3
"""Build brand/index.html: every concept, live, under every launcher mask.

  python3 brand/tools/viewer.py

One self-contained page, no network beyond the webfonts and no build step, so it
opens from the filesystem and publishes as an artifact unchanged. Every mark on
it is the real SVG rather than a screenshot, which is the point: the tile you are
looking at is the file that ships, at whatever size and behind whatever mask you
pick.

The numbers in the measurement strips are measured here, off the same rasters
validate.py checks -- a review page that quotes hand-typed figures is a review
page that quotes last week's figures.
"""
import json
import math
import re
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent

# Real Android density buckets: what a launcher asks for, at the px it asks in.
DENSITIES = [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)]


def inner(svg_text, mono=False):
    """One concept layer's markup, ready to drop inside a <g>."""
    s = re.sub(r"<\?xml.*?\?>", "", svg_text, flags=re.S)
    s = re.sub(r"<!--.*?-->", "", s, flags=re.S)
    s = re.sub(r"<svg[^>]*>", "", s, count=1)
    s = s.replace("</svg>", "").strip()
    if mono:
        # Android keeps this layer's alpha and applies its own tint. currentColor
        # is how a page does the same thing, and it is why the themed previews
        # can be recoloured without a second copy of the art.
        s = s.replace('"#fff"', '"currentColor"').replace('"#ffffff"', '"currentColor"')
    return re.sub(r"\n\s+", "\n      ", s)


def srgb_lin(v):
    v = v / 255.0
    return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4


def rel_lum(rgb):
    return 0.2126 * srgb_lin(rgb[0]) + 0.7152 * srgb_lin(rgb[1]) + 0.0722 * srgb_lin(rgb[2])


def measure(slug):
    """Safe-circle reach and greyscale separation, off the shipped rasters."""
    im = Image.open(ROOT / "png" / slug / "adaptive-foreground-432.png").convert("RGBA")
    w, h = im.size
    a = im.load()
    c = (w - 1) / 2.0
    worst = 0.0
    for y in range(h):
        for x in range(w):
            if a[x, y][3]:
                d = math.hypot(x - c, y - c)
                if d > worst:
                    worst = d
    safe = worst * 108.0 / w
    im.close()

    im = Image.open(ROOT / "png" / slug / "tile-512.png").convert("RGB")
    px = im.load()
    w, h = im.size
    ground = rel_lum(px[4, 4])
    lums = sorted(rel_lum(px[x, y]) for y in range(0, h, 3) for x in range(0, w, 3))
    lo, hi = lums[int(0.01 * len(lums))], lums[int(0.99 * len(lums))]
    r = lambda p: (max(p, ground) + 0.05) / (min(p, ground) + 0.05)
    contrast = max(r(hi), r(lo))
    im.close()
    return safe, contrast


CSS = """
:root {
  /* Dark first, because the product is: these are web/app.css's own tokens, so
     the review page and the thing being reviewed share one palette. */
  --bg: #0d1117;  --bg-2: #131923;  --panel: #161d27;
  --line: #232d3b; --line-soft: #1a222e;
  --ink: #e5ecf3; --muted: #93a1b3; --faint: #66748a;
  --accent: #3fbfa6; --accent-ink: #06231f; --amber: #f0a640; --rose: #f0706e;
  --sign: #f0a640; --sign-ink: #16202b;
  --shadow: 0 10px 30px rgba(0,0,0,.45);
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) {
    --bg: #f4f6f9; --bg-2: #ffffff; --panel: #ffffff;
    --line: #d9e0e9; --line-soft: #e6ebf1;
    --ink: #16202b; --muted: #566274; --faint: #7a8798;
    --accent: #12836f; --accent-ink: #ffffff; --amber: #a2670a; --rose: #c23b3b;
    --sign: #f0a640; --sign-ink: #16202b;
    --shadow: 0 10px 30px rgba(20,32,45,.12);
  }
}
:root[data-theme="light"] {
  --bg: #f4f6f9; --bg-2: #ffffff; --panel: #ffffff;
  --line: #d9e0e9; --line-soft: #e6ebf1;
  --ink: #16202b; --muted: #566274; --faint: #7a8798;
  --accent: #12836f; --accent-ink: #ffffff; --amber: #a2670a; --rose: #c23b3b;
  --sign: #f0a640; --sign-ink: #16202b;
  --shadow: 0 10px 30px rgba(20,32,45,.12);
}

* { box-sizing: border-box; }
body {
  margin: 0; background: var(--bg); color: var(--ink);
  font-family: Archivo, "Helvetica Neue", Arial, sans-serif;
  font-size: 15px; line-height: 1.55;
  -webkit-font-smoothing: antialiased;
}
.wrap { max-width: 1180px; margin: 0 auto; padding: 0 22px 80px; }

/* Nothing on this page is rounded except an icon, because a rounded corner here
   means one thing: a launcher mask. */
header.top { border-bottom: 1px solid var(--line); background: var(--bg); }
.top-in {
  max-width: 1180px; margin: 0 auto; padding: 18px 22px 0;
  display: flex; flex-wrap: wrap; gap: 18px 32px; align-items: flex-end;
}
.brandline { flex: 1 1 260px; min-width: 0; }
h1 {
  font-size: clamp(26px, 4.2vw, 40px); font-weight: 900; letter-spacing: -.022em;
  margin: 0; text-wrap: balance;
}
h1 em { font-style: normal; color: var(--accent); }
.sub { color: var(--muted); margin: 6px 0 0; font-size: 14.5px; max-width: 66ch; text-wrap: pretty; }

.controls { display: flex; flex-wrap: wrap; gap: 14px 22px; padding: 16px 0 18px; }
.cgroup { display: flex; flex-direction: column; gap: 7px; }
.clabel {
  font: 600 10.5px/1 "JetBrains Mono", ui-monospace, monospace;
  letter-spacing: .13em; text-transform: uppercase; color: var(--faint);
}
.seg { display: flex; border: 1px solid var(--line); background: var(--bg-2); }
.seg button {
  appearance: none; border: 0; background: transparent; color: var(--muted);
  font: 500 12.5px/1 Archivo, sans-serif; padding: 8px 11px; cursor: pointer;
  border-right: 1px solid var(--line-soft);
}
.seg button:last-child { border-right: 0; }
.seg button[aria-pressed="true"] { background: var(--accent); color: var(--accent-ink); font-weight: 600; }
.seg button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.sizerow { display: flex; align-items: center; gap: 10px; }
input[type=range] { width: 168px; accent-color: var(--accent); }
.sizeval {
  font: 600 12.5px/1 "JetBrains Mono", monospace; color: var(--ink);
  min-width: 52px; font-variant-numeric: tabular-nums;
}

/* --- the stage a mark sits on ------------------------------------------- */
.tile { display: block; overflow: hidden; flex: 0 0 auto; }
.tile svg { display: block; width: 100%; height: 100%; }
.mask-squircle { clip-path: url(#cp-squircle); }
.mask-circle   { clip-path: circle(50%); }
.mask-rounded  { border-radius: 22%; }
.mask-teardrop { border-radius: 50% 50% 50% 12%; }
.mask-none     { border-radius: 0; }

.bd-studio   { background: var(--panel); }
.bd-playlite { background: #ffffff; }
.bd-playdark { background: #1f1f1f; }
/* These two simulate Android tinting the themed layer itself, so their ink is
   fixed to their own ground rather than inherited from the page theme. */
.themed .bd-playdark { color: #e6eef8; }
.themed .bd-playlite { color: #20293a; }
.bd-home     { background: linear-gradient(148deg, #3a2f5c 0%, #2b4f6b 46%, #1c6a63 100%); }

/* --- one concept -------------------------------------------------------- */
.concept {
  display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 360px);
  gap: 30px 44px; padding: 40px 0; border-bottom: 1px solid var(--line);
  align-items: start;
}
.stage { display: flex; flex-direction: column; gap: 22px; min-width: 0; }
.hero { display: flex; align-items: flex-end; gap: 26px; flex-wrap: wrap; }
.hero .heroshadow { max-width: 100%; }
.heroshadow { box-shadow: var(--shadow); }

.ladder { display: flex; align-items: flex-end; gap: 18px; flex-wrap: wrap; }
.rung { display: flex; flex-direction: column; align-items: center; gap: 7px; }
.rung .cap {
  font: 400 9.5px/1.35 "JetBrains Mono", monospace; color: var(--faint);
  text-align: center; letter-spacing: .04em;
}
.rung .cap b { display: block; color: var(--muted); font-weight: 600; }

/* --- the words ---------------------------------------------------------- */
.head { display: flex; align-items: center; gap: 12px; margin: 0 0 12px; }
.chosen-tag {
  font: 600 10px/1 "JetBrains Mono", monospace; letter-spacing: .14em;
  text-transform: uppercase; color: var(--accent-ink); background: var(--accent);
  padding: 5px 8px 6px;
}
.concept.is-chosen .sign { box-shadow: 0 0 0 3px var(--accent); }
/* Taxiways are signed with a letter, and that is what these five are: five
   routes to the same apron. Black on amber is the real sign colourway. */
.sign {
  background: var(--sign); color: var(--sign-ink);
  font: 900 19px/1 Archivo, sans-serif; letter-spacing: .02em;
  padding: 7px 11px 8px; border: 2px solid var(--sign-ink);
}
h2 { font-size: 23px; font-weight: 800; letter-spacing: -.015em; margin: 0; }
.reads { color: var(--muted); font-size: 14px; margin: 0 0 14px; }
.pun { font-size: 14.5px; margin: 0 0 16px; }
.pun::before {
  content: ""; display: block; width: 34px; height: 2px;
  background: var(--accent); margin-bottom: 12px;
}
.risk {
  font-size: 13px; color: var(--muted); margin: 0 0 18px;
  padding-left: 12px; border-left: 2px solid var(--line);
}
.risk b { color: var(--amber); font-weight: 600; text-transform: uppercase;
  font-size: 10.5px; letter-spacing: .12em; font-family: "JetBrains Mono", monospace; }

.meas { display: grid; grid-template-columns: 1fr 1fr; gap: 1px; background: var(--line); }
.meas div { background: var(--bg-2); padding: 9px 11px; }
.meas dt {
  font: 400 9.5px/1.3 "JetBrains Mono", monospace; color: var(--faint);
  letter-spacing: .07em; text-transform: uppercase;
}
.meas dd {
  margin: 3px 0 0; font: 600 14px/1.2 "JetBrains Mono", monospace;
  font-variant-numeric: tabular-nums; color: var(--ink);
}
.meas dd small { font-weight: 400; color: var(--muted); font-size: 11px; }

.themed { display: flex; gap: 14px; align-items: flex-end; }
.themedwrap { display: flex; flex-direction: column; gap: 7px; }

/* --- shipping ----------------------------------------------------------- */
.ship { padding: 44px 0 0; }
h3 { font-size: 17px; font-weight: 800; margin: 0 0 10px; letter-spacing: -.01em; }
.ship p { color: var(--muted); max-width: 66ch; margin: 0 0 18px; font-size: 14.5px; }
pre {
  margin: 0 0 20px; padding: 15px 17px; background: var(--bg-2);
  border: 1px solid var(--line); overflow-x: auto;
  font: 400 12.5px/1.7 "JetBrains Mono", ui-monospace, monospace; color: var(--ink);
}
pre .c { color: var(--faint); }
.files { width: 100%; border-collapse: collapse; font-size: 13.5px; }
.files th, .files td { text-align: left; padding: 8px 12px 8px 0; border-bottom: 1px solid var(--line-soft); vertical-align: top; }
.files th {
  font: 600 10px/1.3 "JetBrains Mono", monospace; letter-spacing: .12em;
  text-transform: uppercase; color: var(--faint);
}
.files td:first-child { font-family: "JetBrains Mono", monospace; font-size: 12px; white-space: nowrap; }
.files td:last-child { color: var(--muted); }
.tablescroll { overflow-x: auto; }

@media (max-width: 860px) {
  .concept { grid-template-columns: minmax(0, 1fr); gap: 26px; }
  .words { order: -1; }
}
@media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
"""

JS = """
const state = { mask: 'squircle', bd: 'studio', size: 224 };
function apply() {
  document.querySelectorAll('.tile').forEach(t => {
    if (!t.dataset.fixed) { t.style.width = state.size + 'px'; t.style.height = state.size + 'px'; }
    t.className = t.className.replace(/mask-\\S+/g, '').replace(/bd-\\S+/g, '').trim()
      + ' mask-' + state.mask + (t.dataset.bd ? ' bd-' + t.dataset.bd : ' bd-' + state.bd);
  });
  document.getElementById('sizeval').textContent = state.size + 'px';
  document.querySelectorAll('[data-set]').forEach(b => {
    const [k, v] = b.dataset.set.split(':');
    b.setAttribute('aria-pressed', String(state[k] === v));
  });
}
document.querySelectorAll('[data-set]').forEach(b => b.addEventListener('click', () => {
  const [k, v] = b.dataset.set.split(':');
  state[k] = v; apply();
}));
const slider = document.getElementById('size');
slider.addEventListener('input', () => { state.size = +slider.value; apply(); });
apply();
"""


def tile(slug, size, mask_fixed=False, bd=None, mono=False, shadow=False):
    cls = "tile" + (" heroshadow" if shadow else "")
    attrs = ' data-fixed="1" style="width:%dpx;height:%dpx"' % (size, size) if mask_fixed else ""
    if bd:
        attrs += ' data-bd="%s"' % bd
    body = ('<svg viewBox="18 18 72 72" aria-hidden="true"><use href="#%s-%s"/></svg>'
            % ("mono" if mono else "mark", slug))
    return '<span class="%s"%s>%s</span>' % (cls, attrs, body)


def main():
    slugs = sorted(p.name for p in (ROOT / "concepts").iterdir() if p.is_dir())
    sprites, rows = [], []

    for slug in slugs:
        d = ROOT / "concepts" / slug
        sprites.append('    <g id="mark-%s">\n      %s\n      %s\n    </g>' % (
            slug, inner((d / "adaptive-background.svg").read_text()),
            inner((d / "adaptive-foreground.svg").read_text())))
        sprites.append('    <g id="mono-%s">\n      %s\n    </g>' % (
            slug, inner((d / "adaptive-monochrome.svg").read_text(), mono=True)))

    for slug in slugs:
        m = json.loads((ROOT / "concepts" / slug / "meta.json").read_text())
        safe, contrast = measure(slug)
        ladder = "\n".join(
            '        <span class="rung">%s<span class="cap"><b>%s</b>%dpx</span></span>'
            % (tile(slug, px, mask_fixed=True), name, px) for name, px in DENSITIES)
        rows.append("""
    <section class="concept{chosen_cls}" id="{slug}">
      <div class="stage">
        <div class="hero">
          {hero}
          <div class="themed">
            <div class="themedwrap">
              {mono_d}
              <span class="cap" style="font:400 9.5px/1.35 'JetBrains Mono',monospace;color:var(--faint)">themed · dark</span>
            </div>
            <div class="themedwrap">
              {mono_l}
              <span class="cap" style="font:400 9.5px/1.35 'JetBrains Mono',monospace;color:var(--faint)">themed · light</span>
            </div>
          </div>
        </div>
        <div class="ladder">
{ladder}
        </div>
      </div>
      <div class="words">
        <div class="head"><span class="sign">{letter}</span><h2>{title}</h2>{tag}</div>
        <p class="reads">{reads}</p>
        <p class="pun">{pun}</p>
        <p class="risk"><b>Trade-off</b><br>{risk}</p>
        <dl class="meas">
          <div><dt>Safe circle</dt><dd>{safe:.1f}<small> / 33 dp</small></dd></div>
          <div><dt>Greyscale</dt><dd>{contrast:.1f}<small> : 1</small></dd></div>
          <div><dt>Mood</dt><dd style="font:500 12.5px/1.3 Archivo,sans-serif">{mood}</dd></div>
          <div><dt>Themed layer</dt><dd style="font:500 12.5px/1.3 Archivo,sans-serif">clean, 5 densities</dd></div>
        </dl>
      </div>
    </section>""".format(
            slug=slug, letter=m["letter"].upper(), title=m["title"],
            reads=m["reads_as"], pun=m["pun"], risk=m["risk"], mood=m["mood"],
            safe=safe, contrast=contrast, ladder=ladder,
            chosen_cls=" is-chosen" if m.get("chosen") else "",
            tag=('<span class="chosen-tag">chosen %s</span>' % m["chosen"]
                 if m.get("chosen") else ""),
            hero=tile(slug, 0, shadow=True),
            mono_d=tile(slug, 88, mask_fixed=True, bd="playdark", mono=True),
            mono_l=tile(slug, 88, mask_fixed=True, bd="playlite", mono=True)))

    html = """<title>Five Ways to Draw Emulia</title>
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Archivo:wght@400;500;600;800;900&family=JetBrains+Mono:wght@400;600&display=swap">
<style>%s</style>

<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>
    <clipPath id="cp-squircle" clipPathUnits="objectBoundingBox">
      <path d="M.5 0C.78 0 .89.04.94.10 .99.16 1 .27 1 .5 1 .73 .99.84.94.90 .89.96.78 1 .5 1 .22 1 .11.96.06.90 .01.84 0 .73 0 .5 0 .27.01.16.06.10 .11.04.22 0 .5 0Z"/>
    </clipPath>
%s
</defs></svg>

<header class="top">
  <div class="top-in">
    <div class="brandline">
      <h1>Five ways to draw <em>Emulia</em></h1>
      <p class="sub"><strong>B, Captain Plaine, is the mark</strong> — chosen 2 September 2026 and
      now live on the launcher, the notification thread and the web client. The other four are
      kept because the reasoning is worth more than the files. Every tile is the live SVG,
      on the adaptive icon's own 108dp canvas.</p>
    </div>
  </div>
  <div class="top-in" style="padding-top:0">
    <div class="controls">
      <div class="cgroup">
        <span class="clabel">Launcher mask</span>
        <span class="seg">
          <button data-set="mask:squircle">Squircle</button><button data-set="mask:circle">Circle</button><button data-set="mask:rounded">Rounded</button><button data-set="mask:teardrop">Teardrop</button><button data-set="mask:none">Square</button>
        </span>
      </div>
      <div class="cgroup">
        <span class="clabel">Ground</span>
        <span class="seg">
          <button data-set="bd:studio">Studio</button><button data-set="bd:home">Home screen</button><button data-set="bd:playlite">Play light</button><button data-set="bd:playdark">Play dark</button>
        </span>
      </div>
      <div class="cgroup">
        <span class="clabel">Size</span>
        <span class="sizerow">
          <input type="range" id="size" min="48" max="288" step="8" value="224" aria-label="Icon size">
          <span class="sizeval" id="sizeval">224px</span>
        </span>
      </div>
    </div>
  </div>
</header>

<div class="wrap">
%s

  <section class="ship">
    <h3>How one of these ships</h3>
    <p>All five are built and measured against the same six checks — nothing here is a
    mockup, and every raster on this page is the file that would ship. Nothing is chosen
    yet, so <code>apply.py</code> is deliberately not ported from the sibling project:
    the promotion step gets written once there is something to promote, against the one
    concept that won rather than against five that might.</p>
    <pre><span class="c"># edit the pixel maps, then regenerate the three SVG layers</span>
python3 brand/tools/pixels.py

<span class="c"># rebuild every concept from its three SVG layers, then re-measure</span>
python3 brand/tools/build.py
python3 brand/tools/validate.py
python3 brand/tools/contact_sheet.py</pre>
    <div class="tablescroll">
    <table class="files">
      <tr><th>What lands where</th><th>Why that file</th></tr>
      <tr><td>res/drawable/ic_launcher_foreground.xml</td><td>the colour art, cut from the reviewed SVG rather than re-drawn</td></tr>
      <tr><td>res/drawable/ic_launcher_background.xml</td><td>the ground, gradients and all — minSdk 29, so vector gradients are fine</td></tr>
      <tr><td>res/drawable/ic_launcher_monochrome.xml</td><td>its own composition, not a desaturation: the system flattens this one to a single tint</td></tr>
      <tr><td>res/mipmap-anydpi-v26/ic_launcher{,_round}.xml</td><td>the adaptive icon every device here actually draws</td></tr>
      <tr><td>res/mipmap-{m,h,xh,xxh,xxxh}dpi/…png</td><td>square and round bitmaps, for the surfaces that still ask</td></tr>
      <tr><td>brand/png/&lt;slug&gt;/play-512.png</td><td>the Play listing icon, uploaded by hand at submission</td></tr>
      <tr><td>web/index.html favicon</td><td>swapped inline as a data URI — the web client has no build step and stays that way</td></tr>
    </table>
    </div>
  </section>
</div>
<script>%s</script>
""" % (CSS, "\n".join(sprites), "\n".join(rows), JS)

    (ROOT / "index.html").write_text(html)
    print("wrote %s  (%.1f KB)" % (ROOT / "index.html", len(html) / 1024.0))


if __name__ == "__main__":
    main()
