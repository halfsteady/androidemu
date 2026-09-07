#!/usr/bin/env python3
"""Measure every concept's rasters against the things that make an icon ship.

Tone survives resampling because averaging preserves means. Structure does not,
because averaging destroys extremes. Checks 5 and 5b measure the half that
survives; 4, 7 and 8 measure the half that does not. That is why a concept can
pass every contrast check comfortably and still fall apart at 32px, and it is
the reason these are a set rather than a pile.

  python3 brand/tools/validate.py

Everything is measured on the real files with Pillow; nothing is inferred from
the SVG source, because the SVG is not what a launcher draws. The checks and
their numbering follow the sibling project's `brand/tools/validate-px.py`,
which found real defects with each of them:

  1  a silently blank or single-colour render (the whole pipeline's failure mode)
  2  the transparency contract: layers transparent, tiles opaque
  3  the Play Store icon spec
  4  the adaptive icon's 33dp always-visible circle
  5  greyscale survival, subject against ground
  6  detached specks in the themed layer, at every launcher density

Check 6 is calibrated to catch SLIVERS, not intended holes: an eye knocked out
of a fuselage is hundreds of pixels and passes, a rasteriser resolving a
near-miss between two outlines as a 3px hole does not. Under a flat system tint
that hole is a dead pixel.
"""
import math
import os
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SAFE_DP = 33.0
SPECK_MAX = 6
FLOOR_DP = 1.0       # below this a "channel" is a rasteriser seam, not a feature
SUSTAIN_DP = 1.5     # and it must persist this far along its length to be one
fails = []


def fail(check, msg):
    fails.append("[%d] %s" % (check, msg))
    print("  FAIL %s" % msg)


def srgb_lin(v):
    v = v / 255.0
    return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4


def rel_lum(rgb):
    return 0.2126 * srgb_lin(rgb[0]) + 0.7152 * srgb_lin(rgb[1]) + 0.0722 * srgb_lin(rgb[2])


def ratio(a, b):
    hi, lo = max(a, b), min(a, b)
    return (hi + 0.05) / (lo + 0.05)


def contrast(path):
    """Subject-vs-ground contrast in greyscale, as (ground, subject, ratio).

    Extracted so checks 5 and 5b cannot drift apart: they are the same metric,
    and the only thing 5b varies is the size of the raster it is handed.
    """
    im = Image.open(path).convert("RGB")
    px = im.load()
    w, h = im.size
    ground = rel_lum(px[max(1, w // 128), max(1, h // 128)])
    step = 3 if w >= 128 else 1
    lums = sorted(rel_lum(px[x, y]) for y in range(0, h, step) for x in range(0, w, step))
    lo, hi = lums[int(0.01 * len(lums))], lums[int(0.99 * len(lums))]
    subject = hi if ratio(hi, ground) >= ratio(lo, ground) else lo
    im.close()
    return ground, subject, ratio(subject, ground)


def enclosed_regions(painted, w, h, want_painted):
    """Connected runs of `want_painted` pixels that do not reach the border.

    Shared deliberately by checks 7 and 8: one geometry asked two questions,
    is a counter too NARROW (it closes) or is a channel too narrow (its two
    sides merge). Keeping them on one flood fill is not tidiness -- the two
    checks are coupled, and in opposite directions. Growing runs to kill a
    check-6 speck is what narrowed E's eyes from 6.07dp to 3.25dp and made
    check 7 worse; the sibling project's check-6 pressure made its knockouts
    generous and its check 7 pass. Same pair, opposite sign, and only visible
    if both read the same pixels.
    """
    seen = [[False] * w for _ in range(h)]
    out = []
    for sy in range(h):
        for sx in range(w):
            if painted[sy][sx] != want_painted or seen[sy][sx]:
                continue
            stack, cells, edge = [(sx, sy)], [], False
            seen[sy][sx] = True
            while stack:
                x, y = stack.pop()
                cells.append((x, y))
                if x in (0, w - 1) or y in (0, h - 1):
                    edge = True
                for nx, ny in ((x+1, y), (x-1, y), (x, y+1), (x, y-1)):
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny][nx] \
                            and painted[ny][nx] == want_painted:
                        seen[ny][nx] = True
                        stack.append((nx, ny))
            if not edge:
                out.append(cells)
    return out


def widest_point(cells):
    """The pixel whose run-across and run-down are jointly largest, and that run.

    NOT the centroid and NOT the bounding box. A ring's centre of mass sits on
    the ink in the middle of it, so sampling there measures the wrong pixel
    entirely; and a bounding box's narrow side cannot be reconciled with the
    alpha you read afterwards, so when the two disagree there is no way to tell
    which one lied. Measuring clearance AT a point cannot disagree with itself,
    and taking the widest point makes the later alpha read the best case -- a
    gap that still closes has nowhere to hide.
    """
    inside = set(cells)
    best_clear, ties = -1, []
    for (x, y) in cells:
        across = 1
        i = x - 1
        while (i, y) in inside:
            across += 1
            i -= 1
        i = x + 1
        while (i, y) in inside:
            across += 1
            i += 1
        down = 1
        j = y - 1
        while (x, j) in inside:
            down += 1
            j -= 1
        j = y + 1
        while (x, j) in inside:
            down += 1
            j += 1
        clear = min(across, down)
        if clear > best_clear:
            best_clear, ties = clear, [(x, y)]
        elif clear == best_clear:
            ties.append((x, y))
    # Every interior pixel of a square gap ties on clearance, and keeping the
    # first in scan order picks a pixel on the region's EDGE -- where the alpha
    # read afterwards is contaminated by the ink next to it. C read 87@48px that
    # way and 0 once the tie is broken toward the middle. Among equally wide
    # points, take the most central.
    mx = sum(t[0] for t in ties) / len(ties)
    my = sum(t[1] for t in ties) / len(ties)
    at = min(ties, key=lambda t: (t[0] - mx) ** 2 + (t[1] - my) ** 2)
    return best_clear, at


def _alpha_mask(path):
    im = Image.open(path).convert("RGBA")
    a = im.getchannel("A")
    w, h = a.size
    px = a.load()
    return im, a, w, h, [[px[x, y] > 128 for x in range(w)] for y in range(h)]


def counters(fg_432, sizes=(48, 32)):
    """Check 7: enclosed gaps, and whether they survive downscaling.

    A counter closing is how a letterform dies, and no contrast metric notices
    because the ink either side of the gap is exactly as dark as it was. Holes
    are found on the rendered alpha rather than read from any pixel map, so
    this runs on a hand-drawn mark unmodified.
    """
    im, a, w, h, painted = _alpha_mask(fg_432)
    smalls = {n: im.resize((n, n), Image.LANCZOS).getchannel("A").load()
              for n in sizes}
    out = []
    for cells in enclosed_regions(painted, w, h, want_painted=False):
        clear_px, (px_, py_) = widest_point(cells)
        alphas = {}
        for n, sp in smalls.items():
            ix = min(n - 1, max(0, int((px_ + 0.5) * n / w)))
            iy = min(n - 1, max(0, int((py_ + 0.5) * n / h)))
            alphas[n] = sp[ix, iy]
        out.append((clear_px * 108.0 / w, alphas))
    im.close()
    return sorted(out, key=lambda t: t[0])


def channels(fg_432, sizes=(48, 32)):
    """Check 8: the narrowest unpainted channel, and whether it fills in.

    The inverse of check 7, and deliberately WIDER than "two components merge".
    That formulation misses the case that actually bit here: the emu's legs are
    joined to its body, so the whole bird is one shape, and the slot between the
    legs is open at the bottom -- so it is neither an enclosed counter (check 7
    never sees it) nor a separation between components (a component-pair metric
    never sees it either). It is still a 3.6dp channel that closes, and losing
    it costs the bird its legs.

    So a channel is any unpainted run bounded by paint on two opposite sides,
    open-ended or not. Enclosed counters are excluded because check 7 already
    reports those; what is left is exactly the slots.
    """
    im, a_, w, h, painted = _alpha_mask(fg_432)
    in_counter = set()
    for cells in enclosed_regions(painted, w, h, want_painted=False):
        in_counter.update(cells)

    # Bounded run lengths, by row and by column. A run touching the canvas edge
    # is the background, not a channel.
    hlen = [[0] * w for _ in range(h)]
    for y in range(h):
        x = 0
        while x < w:
            if painted[y][x]:
                x += 1
                continue
            s0 = x
            while x < w and not painted[y][x]:
                x += 1
            if s0 > 0 and x < w:
                for i in range(s0, x):
                    hlen[y][i] = x - s0
    vlen = [[0] * w for _ in range(h)]
    for x in range(w):
        y = 0
        while y < h:
            if painted[y][x]:
                y += 1
                continue
            s0 = y
            while y < h and not painted[y][x]:
                y += 1
            if s0 > 0 and y < h:
                for j in range(s0, y):
                    vlen[j][x] = y - s0
            y += 0
    width = {}
    for y in range(h):
        for x in range(w):
            if painted[y][x] or (x, y) in in_counter:
                continue
            # A channel below the floor is the seam where two shapes meet at a
            # corner, not a feature anybody drew. Stated in dp, NOT in raster
            # pixels: this render is 4px per dp, the sibling project's is also
            # 4, and a rule written in pixels silently changes meaning the day
            # either master size does. Nothing in this set is drawn under
            # 2.67dp (one sprite pixel at the tightest concept), so a sub-1dp
            # channel is arithmetic. Left in, it reported 0.25dp MERGED for two
            # concepts and buried the real tightest channel in each.
            cand = [v for v in (hlen[y][x], vlen[y][x]) if v * 108.0 / w >= FLOOR_DP]
            if cand:
                width[(x, y)] = min(cand)
    if not width:
        im.close()
        return None

    # Group the channel pixels, then take each channel's WIDEST point, for the
    # same reason check 7 does: measuring at a point cannot disagree with
    # itself, and the widest point is the best case.
    seen, out = set(), []
    for start in width:
        if start in seen:
            continue
        stack, cells = [start], []
        seen.add(start)
        while stack:
            c = stack.pop()
            cells.append(c)
            x, y = c
            for nb in ((x+1, y), (x-1, y), (x, y+1), (x, y-1)):
                if nb in width and nb not in seen:
                    seen.add(nb)
                    stack.append(nb)
        # A channel must be SUSTAINED. One scanline through two nearly tangent
        # edges finds a pinch that narrows continuously to zero, so "narrowest
        # run anywhere" converges on the tangency limit rather than on anything
        # drawn. Axis-aligned pixel art mostly dodges this -- but E's rounded
        # runs are curves, and a long shallow diagonal staircase is exactly a
        # sequence of one-line pinches.
        xs = [c[0] for c in cells]
        ys = [c[1] for c in cells]
        extent = max(max(xs) - min(xs), max(ys) - min(ys)) + 1
        if extent * 108.0 / w < SUSTAIN_DP:
            continue
        best = max(cells, key=lambda c: width[c])
        out.append((width[best], best))
    if not out:
        im.close()
        return None
    out.sort(key=lambda t: t[0])
    gap_px, (mx, my) = out[0]
    alphas = {}
    for n in sizes:
        sp = im.resize((n, n), Image.LANCZOS).getchannel("A").load()
        ix = min(n - 1, max(0, int((mx + 0.5) * n / w)))
        iy = min(n - 1, max(0, int((my + 0.5) * n / h)))
        alphas[n] = sp[ix, iy]
    im.close()
    return len(out), gap_px * 108.0 / w, alphas


def main():
    slugs = sorted(p.name for p in (ROOT / "concepts").iterdir() if p.is_dir())

    print("## 1. Nothing rendered blank, and no tile is a flat colour")
    for slug in slugs:
        for p in sorted((ROOT / "png" / slug).glob("*.png")):
            im = Image.open(p).convert("RGBA")
            w, h = im.size
            a = im.getchannel("A")
            painted = sum(a.histogram()[1:]) / float(w * h)
            transparent_layer = p.name.startswith(("adaptive-foreground", "adaptive-monochrome"))
            if transparent_layer:
                if painted < 0.005:
                    fail(1, "%s/%s is %.3f%% painted (blank render?)"
                         % (slug, p.name, 100 * painted))
            else:
                cols = im.convert("RGB").getcolors(maxcolors=1 << 20)
                if cols is not None and len(cols) < 8:
                    fail(1, "%s/%s has only %d distinct colours (art missing?)"
                         % (slug, p.name, len(cols)))
            im.close()
    print("  scanned %d PNGs across %d concepts"
          % (sum(len(list((ROOT / "png" / s).glob("*.png"))) for s in slugs), len(slugs)))

    print("\n## 2. Transparency contract")
    for slug in slugs:
        for name in ("adaptive-foreground-432.png", "adaptive-monochrome-432.png"):
            im = Image.open(ROOT / "png" / slug / name).convert("RGBA")
            w, h = im.size
            a = im.getchannel("A")
            corners = [a.getpixel((0, 0)), a.getpixel((w - 1, 0)),
                       a.getpixel((0, h - 1)), a.getpixel((w - 1, h - 1))]
            if max(corners) != 0:
                fail(2, "%s/%s corner alphas %s, want all 0" % (slug, name, corners))
            im.close()
        # A launcher composites the background layer; a hole in it shows the
        # wallpaper through the icon.
        im = Image.open(ROOT / "png" / slug / "tile-512.png").convert("RGBA")
        amin = im.getchannel("A").getextrema()[0]
        if amin != 255:
            fail(2, "%s/tile-512.png has alpha %d somewhere: the background layer "
                    "is not opaque" % (slug, amin))
        im.close()
    print("  layers transparent at the corners, tiles opaque")

    print("\n## 3. Play Store icon: 512x512, 32-bit PNG, under 1MB")
    for slug in slugs:
        p = ROOT / "png" / slug / "play-512.png"
        im = Image.open(p)
        n = os.path.getsize(p)
        ok = im.size == (512, 512) and im.format == "PNG" and im.mode == "RGBA" and n < 1 << 20
        print("  %s %-18s %s %s %d bytes (%.0f KiB)"
              % ("pass" if ok else "FAIL", slug, im.size, im.mode, n, n / 1024.0))
        if not ok:
            fail(3, "%s play-512.png is %s %s %d bytes" % (slug, im.size, im.mode, n))
        im.close()

    print("\n## 4. Adaptive foreground inside the 33dp always-visible circle")
    print("     432px = 108dp, so 1dp = 4px. Any alpha at all counts: an")
    print("     antialias fringe outside the circle is still a cropped edge.")
    for slug in slugs:
        im = Image.open(ROOT / "png" / slug / "adaptive-foreground-432.png").convert("RGBA")
        w, h = im.size
        scale = 108.0 / w
        cx = cy = (w - 1) / 2.0
        a = im.load()
        worst, at = 0.0, None
        for y in range(h):
            for x in range(w):
                if a[x, y][3]:
                    d = math.hypot(x - cx, y - cy)
                    if d > worst:
                        worst, at = d, (x, y)
        dp = worst * scale
        print("  %s %-18s farthest painted pixel at r=%.2fdp  (px %s)"
              % ("pass" if dp <= SAFE_DP else "FAIL", slug, dp, at))
        if dp > SAFE_DP:
            fail(4, "%s foreground paints out to %.2fdp, outside the %.0fdp circle"
                 % (slug, dp, SAFE_DP))
        im.close()

    print("\n## 5. Greyscale survival: subject against its own ground, >= 3:1")
    for slug in slugs:
        p512 = ROOT / "png" / slug / "tile-512.png"
        ground, subject, r = contrast(p512)
        print("  %s %-18s ground L=%.4f  subject L=%.4f  %.2f:1"
              % ("pass" if r >= 3.0 else "FAIL", slug, ground, subject, r))
        if r < 3.0:
            fail(5, "%s subject-vs-ground is %.2f:1 in greyscale" % (slug, r))

    # 5b is advisory and adds no failure mode. Check 5 reads tile-512 and
    # nothing else, so a subject that dies small measures fine at full size --
    # which is the same 48px cliff the fringe margin and the sub-pixel rule sit
    # on, approached from a third direction. The number that matters here is not
    # the level but the DIRECTION: a subject carried as area resamples into
    # itself and holds or rises, a subject carried as small detail averages into
    # its ground and falls. Borrowed from the sibling project's check 5b.
    #
    # Two limits, stated because they bound what it can tell you. It measures
    # the BITMAP path, so it is exactly right for the legacy mipmaps and
    # pessimistic for the adaptive vector a launcher re-renders. And below 128px
    # it samples every pixel: a step of 3 over a 32px tile is 121 samples, and
    # the answer would then depend on where the grid landed.
    print("\n## 5b. The same metric small (advisory; contrast is not legibility)")
    print("     On THIS set 5b does not corroborate the costs column, and that is")
    print("     worth knowing rather than hiding: a-four-screens holds ~9:1 all the")
    print("     way down while its sprites visibly become texture. An extreme")
    print("     percentile finds the darkest surviving pixel, and a blurred sprite")
    print("     still has a dark core -- so contrast measures whether the subject is")
    print("     THERE, never whether its shape is READABLE. For a subject carried as")
    print("     detail the governing number is px per sprite pixel against the ~2px")
    print("     floor below which a feature cannot be resolved: 1.19px for (a) and")
    print("     (b), 2.86px for (c). See brand/README.md; pixels.py prints it.")
    for slug in slugs:
        cells = []
        for s_ in (128, 48, 32):
            _, _, r = contrast(ROOT / "png" / slug / ("tile-%d.png" % s_))
            cells.append("%dpx %5.2f" % (s_, r))
        big = contrast(ROOT / "png" / slug / "tile-512.png")[2]
        small = contrast(ROOT / "png" / slug / "tile-32.png")[2]
        drift = (small - big) / big * 100.0
        print("  %-18s %s   %+.0f%% from 512px" % (slug, "  ".join(cells), drift))

    print("\n## 6. Themed layer: no detached specks, at any launcher density")
    for slug in slugs:
        found = []
        for dens in (108, 162, 216, 324, 432):
            p = ROOT / "png" / slug / ("adaptive-monochrome-%d.png" % dens)
            if not p.exists():
                continue
            im = Image.open(p).convert("RGBA")
            w, h = im.size
            a = im.load()
            thr = 96
            # Flood the transparent OUTSIDE from the corners; whatever
            # transparent remains is enclosed by ink.
            seen = bytearray(w * h)
            stack = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
            while stack:
                x, y = stack.pop()
                if not (0 <= x < w and 0 <= y < h) or seen[y * w + x] or a[x, y][3] >= thr:
                    continue
                seen[y * w + x] = 1
                stack += [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
            done = bytearray(w * h)
            for Y in range(h):
                for X in range(w):
                    i = Y * w + X
                    if a[X, Y][3] < thr and not seen[i] and not done[i]:
                        st, n = [(X, Y)], 0
                        while st:
                            x, y = st.pop()
                            j = y * w + x
                            if (not (0 <= x < w and 0 <= y < h) or done[j]
                                    or seen[j] or a[x, y][3] >= thr):
                                continue
                            done[j] = 1
                            n += 1
                            st += [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
                        if n <= SPECK_MAX:
                            found.append((dens, n, (X, Y)))
            im.close()
        if found:
            fail(6, "%s themed layer has %d speck(s), e.g. %dpx at %s in the %ddp "
                    "render - a dead pixel under flat tint"
                 % (slug, len(found), found[0][1], found[0][2], found[0][0]))
        else:
            print("  pass %-18s clean at 108/162/216/324/432dp" % slug)

    print("\n## 7. Counters: do the knocked-out gaps survive the way down?")
    print("     The one failure mode that is about shape, not tone -- an eye or a")
    print("     d-pad cutout closing is invisible to every contrast check, because")
    print("     the ink either side of it is exactly as dark as it was. Alpha at the")
    print("     gap's centre after downscaling: 0 is fully open, 255 is filled in.")
    for slug in slugs:
        cs = counters(ROOT / "png" / slug / "adaptive-foreground-432.png")
        if not cs:
            print("  %-18s no enclosed counters" % slug)
            continue
        narrow, al = cs[0]
        worst = max(al[48], al[32])
        verdict = "open" if worst < 96 else ("part" if worst < 176 else "CLOSED")
        print("  %-18s %d counters, narrowest %.2fdp -> alpha %3d@48px %3d@32px  %s"
              % (slug, len(cs), narrow, al[48], al[32], verdict))

    print("\n## 8. Channels: do separate shapes stay separate? (check 7 inverted)")
    print("     A leg against a body, two adjacent sprites. Both sides of the")
    print("     channel are ink, so no contrast metric can see it close either.")
    print("     Open-ended slots count, not just gaps between whole components:")
    print("     the emu's legs join its body, so a component metric misses them.")
    print("     Alpha in the channel: 0 still separated, 255 fully merged.")
    print("     Labels, not verdicts. Whether a channel carries MEANING or only")
    print("     texture is the one thing the file cannot know: d-the-emu's 3.50dp")
    print("     is the gap between the legs and losing it costs the bird its legs;")
    print("     c-one-in-four's 6.25dp is a decorative notch under the body.")
    for slug in slugs:
        m = channels(ROOT / "png" / slug / "adaptive-foreground-432.png")
        if m is None:
            print("  %-18s no bounded channels" % slug)
            continue
        n, gap, al = m
        worst = max(al[48], al[32])
        verdict = "clear" if worst < 96 else ("part" if worst < 176 else "MERGED")
        print("  %-18s %2d channels, tightest %.2fdp -> alpha %3d@48px %3d@32px  %s"
              % (slug, n, gap, al[48], al[32], verdict))

    print("\n### %d failures" % len(fails))
    for f in fails:
        print("  " + f)
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
