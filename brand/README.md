# Emulia icon concepts

## The mark — chosen 2026-09-07

**One in Four** (`c-one-in-four`) is Emulia's icon: one 8-bit creature laid
across four blocks of colour. It is what the app does — the same game, every
palette, and a `.pal` importer to add more — and it was the only quadrant
concept whose pixels are still pixels at 48px.

It is no longer a candidate. It is the launcher icon, the round icon, the
cold-start splash, the mark on the shelf header and the mark on an empty shelf.
The other four stay in this directory because the reasoning is worth more than
the files.

**The themed (Android 13) layer is generated but deliberately not offered** —
see `OFFER_MONOCHROME` in `tools/build.py`. It is switched off because of how
"Themed icons" behaves rather than how it is documented: One UI recolours only
the apps that *ship* a monochrome layer and leaves every other icon alone. So
providing one did not make Emulia match the home screen, it made Emulia the
single dark tile on a screen full of colour — the opposite of the intent, and
observed on a real phone rather than argued from the spec. Without the element,
a themed launcher falls back to the colour art. The layer is still built and
still measured by check 6, one line from being switched back on.

The mark it replaced was a flat green D-pad written as a placeholder and never
revisited. It is in [`previous/`](previous/).

**Look at them:** open [`index.html`](index.html) directly. Every mark on it is
live SVG, and it carries the controls that actually decide this: the launcher
mask (squircle / circle / rounded / teardrop / square), the ground (studio /
home screen / Play light / Play dark) and a size slider from 48 to 288px.

[`review/contact-sheet.png`](review/contact-sheet.png) is the five as they were
compared, at 256 / 128 / 96 / 64 / 48 / 32px with the themed icon beside them.

### What changed after the pick

The four losers are exactly as they were reviewed — quietly improving them
afterwards would make the contact sheet a record of a comparison that never
happened. Only the winner was refined, and both changes were measured:

- **A fuller lower body.** The original's `..X..X..` legs broke into four specks
  at 32px and the creature lost its mass, which was the one thing it had. The
  channel between them read **MERGED** (alpha 183@48px, 203@32px); it now reads
  **clear** at 76/55, and the counters went from 0/9 to fully open at both
  sizes. Contrast rose 7.67:1 → 8.91:1.
- **The subject swaps with the ground, not just the ground.** A palette swap on
  real hardware changes every colour in the frame, so an icon about palette
  swapping that kept one flat ink was only half telling the truth. Each
  quadrant's ink is its own hue taken to near black — matched by eye rather than
  by luminance, because equalising them numerically at L=0.0135 still left the
  amber cell reading washed. Luminance does not model how much hue weighs.

Where it appears, and which kind of surface each one is — **generated** (this
tooling writes it), **reference** (it names generated art and follows for free),
**redrawn** (drawn per-surface; a generator will NOT update it):

| surface | file | kind |
|---|---|---|
| launcher icon, adaptive | `res/mipmap-anydpi-v26/ic_launcher{,_round}.xml` | generated |
| launcher, legacy bitmaps | `res/mipmap-*dpi/ic_launcher{,_round}.png` | generated |
| shelf header, 34dp | `MainActivity.ShelfHeading` | reference |
| the three layers | `res/drawable/ic_launcher_{background,foreground,monochrome}.xml` | generated |
| in-app mark, empty shelf | `res/drawable/ic_emulia_mark.xml` | generated |
| cold-start splash, API 31+ | `res/values-v31/styles.xml` | reference |
| manifest `icon` + `roundIcon` | `AndroidManifest.xml` | reference |

**Nothing is redrawn, and that is the state to keep.** The moment a surface is
drawn by hand for its own size — a 24dp notification icon is the usual first —
it stops following the mark, and it is stale from the next change onward unless
this table says so.

```sh
python3 brand/tools/apply.py c-one-in-four   # promote (build.py first)
```

## The five, as they were compared

| | reads as | says | costs |
|---|---|---|---|
| **A. Four Screens** | a 2×2 checker of bright cells, a small sprite in each | four games, one machine | **each sprite pixel is 1.19px at mdpi.** The four sprites stop being four games and become texture; what survives is a checker, and a checker does not say NES |
| **B. The Cabinet** | a small dark CRT with four glowing sprites on it | the app is about a *display* | same 1.19px sprites as A, and the scanlines are a 1.15dp feature that is gone below 96px. Bright specks on a dark ground is also the single most generic emulator-icon shape there is |
| **C. One in Four** | one big 8-bit bug on four blocks of colour | the same game, every palette | the only quadrant concept whose pixels are still pixels at 48px (2.86px each). Two of the four cells leave Emulia's greens, so the mark owns its shape more than its colour |
| **D. The Emu** | a chunky pixel emu, side on, in Emulia's green | Emu‑lia | nothing in it says NES, emulator, or games — it is a bird until somebody tells you the joke |
| **E. Half and Half** | an 8-bit bug whose right half has melted | how a pixel should be drawn | reads as a rendering *bug* rather than a rendering *choice* until explained, and the joke needs both halves, so it loses the most to a circle mask |

A, B and C are the same brief three ways: the square cut in half twice. A is the
literal reading, B states it as a screen, and C keeps the four quarters but moves
the variety into the ground so that one sprite can be big enough to survive.
D and E are the free ones.

**The gamepad is in the set on purpose.** It is the category's generic glyph —
every emulator on the Play Store is a controller on a dark disc — and it sits in
one cell of A and B so that it can be rejected on the evidence rather than in the
abstract.

## What the measurements settled

Two things were decided by the validators rather than by looking, and both
changed the artwork:

- **Four distinct sprites cannot survive a launcher.** `pixels.py` solves for
  the largest pixel size whose painted corners still sit inside the 33dp
  always-visible circle. Four sprites in four quadrants force that down to
  2.67dp, which is **1.19px per sprite pixel at mdpi**; one sprite alone gets
  6.44dp, or 2.86px. That ratio is the whole argument for C, and it is why A
  and B are honest about being texture at small sizes.
- **Emulia's own background is the wrong ground for an icon.** The app runs on
  `#111813`, mean luma ~22. At that value the tile is a hole in Play's light
  chrome and dissolves into Play's dark chrome (`#1F1F1F`, luma 31). The icons
  use the same hue pushed to luma 59 and 70.

Four defects the six checks and the contact sheet caught that inspection had not:

- Every concept's foreground painted 0.3–0.5dp outside the safe circle. The
  geometry was inside it; the **antialias fringe** was not. `FRINGE` in
  `pixels.py` is what that costs.
- E's themed layer came back with eleven 1px holes — the notch two convex
  rounded corners leave where runs of different length stack. `GROW` has to
  exceed `rx·(1 − 1/√2)`, and at first it did not.
- B had a bezel rectangle at 8–100dp, which is **entirely outside** the 72dp any
  launcher mask shows. It was a frame that existed only in the viewer. The
  curved-glass read is a vignette now, which nothing can crop.
- E's join was a 1.2dp bright seam, which is **0.53px at mdpi**. It did not
  vanish, which is worse: measured against its own ground its contrast fell
  203 → 97 → 67 from 256 to 32px, so it smeared into a soft line whose position
  and weight depend on the rasteriser — a crack through the sprite rather than a
  join. The split is a tonal step in the ground now, which measures 11–12 luma
  at *every* size because area downscales exactly and hairlines do not.

And one the contact sheet caught that no validator would: D's first cut had four
rows of neck over a wide body and read, unmistakably, as a duck. An emu is its
proportions.

## The measurement that did work

Where 5b failed, **check 7** succeeds, and it is about *shape* rather than tone:
an enclosed gap — a sprite's eye, the cutout that makes a d-pad a d-pad — closing
as the icon shrinks. No contrast metric can see it, because the ink either side
of the gap is exactly as dark as it was. Holes are found on the rendered alpha
rather than read from any pixel map, so it works on a hand-drawn mark too.

```
a-four-screens   5 counters, narrowest 2.50dp -> alpha  41@48px 157@32px  part
b-the-cabinet    5 counters, narrowest 2.50dp -> alpha  41@48px 157@32px  part
c-one-in-four    2 counters, narrowest 6.50dp -> alpha   0@48px   9@32px  open
d-the-emu        1 counter,  narrowest 3.50dp -> alpha   0@48px  89@32px  open
e-half-and-half  2 counters, narrowest 3.25dp -> alpha  29@48px 115@32px  part
```

This is the costs column measured. C's eyes are still eyes at 32px (alpha 9);
A's and B's have filled to 157 — the aliens are blind and the d-pad is a blank
slab. It also caught something nothing else had: **E's smoothing eats its own
counters.** Growing the rounded runs so they merge narrows its eye gaps from
6.07dp to 3.25dp, so E's counters close sooner than C's despite carrying the same
sprite at nearly the same size. The fix for one defect made another one worse,
and only a shape metric would say so.

### And its inverse, check 8

Two shapes *merging* is the same resample from the other side, equally invisible
to contrast because both sides of the channel are ink. It is worth having
separately because it caught something check 7 structurally could not:

```
a-four-screens   6 channels, tightest 2.75dp -> alpha  46@48px  89@32px  clear
b-the-cabinet    6 channels, tightest 2.75dp -> alpha  46@48px  89@32px  clear
c-one-in-four    6 channels, tightest 6.25dp -> alpha 183@48px 203@32px  MERGED
d-the-emu        4 channels, tightest 3.50dp -> alpha 161@48px  88@32px  part
e-half-and-half  6 channels, tightest 5.00dp -> alpha  33@48px 156@32px  part
```

**D's legs are the case that justifies the check.** They are joined to the body,
so the emu is one shape and a "do two components merge" metric never sees them;
and the slot between them is open at the bottom, so check 7 never sees it either.
It is still a 3.50dp channel that fills to 161/255 at 48px. That corrected the
costs line — it is the **legs** that go first, not the neck, which is three
pixels wide and safe.

**These are labels, not verdicts**, for the same reason check 7's are: whether a
channel carries meaning or texture is the one thing the file cannot know. C's
flagged 6.25dp is a decorative notch under the alien's body; D's 3.50dp is what
the bird stands on. The check finds them equally.

Three calibrations it needed. The first two produced confident wrong answers
here; the third is inert on this set and kept anyway, because it is the one that
stops the check lying on art nobody has drawn yet:

- **Sample at a channel's widest point, not its centroid**, and break ties toward
  the most interior pixel. A ring's centre of mass sits on the ink in the middle
  of it, and for a square gap every interior pixel ties on clearance — keeping
  the first in scan order picks an *edge* pixel, where the alpha read is
  contaminated by the ink beside it. C read 87@48px that way and 0 once fixed.
- **A channel below the floor is a rasteriser seam, not a feature.** Left in,
  two concepts reported 0.25dp/MERGED — the corner where two rects touch
  diagonally — which buried the real tightest channel in each. The floor is
  stated in **dp, not raster pixels**: this render is 4px per dp, and a rule
  written in pixels silently changes meaning the day the master size does.
- **A channel must be sustained along its length.** One scanline through two
  nearly-tangent edges finds a pinch that narrows continuously to zero, so
  "narrowest run anywhere" converges on the tangency limit rather than on
  anything drawn. This set is axis-aligned rectangles and never had the defect —
  the guard changes no number here — but E's rounded runs are curves, and a long
  shallow diagonal staircase is exactly a sequence of one-line pinches. It is
  kept for the concept that would hit it next.

## One measurement that did not work

`validate.py` check **5b** re-runs check 5's subject-vs-ground contrast at
128/48/32px instead of only at 512px, borrowed from the sibling project where it
turned a concept's asserted cost into a number (its busiest mark loses a quarter
of its contrast by 32px). On this set it found nothing:

```
a-four-screens   128px  8.80  48px  9.73  32px  9.93   +24% from 512px
b-the-cabinet    128px 17.51  48px 16.92  32px 16.99    +2%
c-one-in-four    128px  8.35  48px  8.79  32px  9.50   +24%
d-the-emu        128px  7.40  48px  7.30  32px  7.10    +0%
e-half-and-half  128px 11.68  48px 11.86  32px 12.24    +8%
```

A holds ~9:1 at 32px while its sprites are visibly mush on the contact sheet, so
the number is right and the inference from it would be wrong. **Contrast is not
legibility.** An extreme percentile finds the darkest surviving pixel, and a
blurred sprite still has a dark core — so 5b measures whether the subject is
*there*, never whether its shape is *readable*. It is kept, and its printed
header says so, because it is a real check on the bitmap path that the legacy
mipmaps and any favicon actually take.

The number that does govern detail is the one `pixels.py` prints: **px per
sprite pixel, against the ~2px floor below which a feature cannot be resolved.**
1.19px for A and B is under it; 2.86px for C is over. That is the whole
quadrant argument, and no contrast metric will restate it.

(A second attempt — measuring the fraction of pixels that are neither ground nor
subject, on the theory that blur creates intermediate values — failed for a
different reason worth recording: A and C have *multi-colour grounds*, so most of
their intermediate pixels are the other ground colours rather than smear.)

## Layout

```
concepts/<slug>/adaptive-{background,foreground,monochrome}.svg   the source of truth
concepts/<slug>/meta.json                                         title, reads-as, costs
tools/pixels.py     the pixel maps; regenerates the SVG layers above
tools/build.py      SVG layers -> Android drawables + every raster, via headless Chrome
tools/validate.py   the eight checks, measured on the rasters and never on the SVG
tools/apply.py      promotes one concept into android/, and says what it touched
tools/contact_sheet.py, tools/viewer.py    the two ways to compare them
previous/           whatever apply.py overwrote, kept once, on its first run
png/, android/      generated and gitignored; rebuild with build.py
review/, index.html generated but tracked - they are the record, not a build output
```

The pixel maps in `tools/pixels.py` are where a sprite is *designed* — eight rows
of eight characters you can read and edit. The SVGs are its output, and
`build.py` treats them as the source of truth exactly as it would a hand-drawn
mark.

```sh
python3 brand/tools/pixels.py          # maps -> the three SVG layers
python3 brand/tools/build.py           # layers -> drawables and rasters
python3 brand/tools/validate.py        # the six checks
python3 brand/tools/contact_sheet.py   # review/contact-sheet.png
python3 brand/tools/viewer.py          # index.html
```

The tools are lifted from the sibling Controlplaine project, which is also where
the six checks and the reads-as/says/costs table come from. `apply.py` — the
step that promotes a concept into `android/` — is deliberately **not** ported
yet: it gets written once there is something to promote, against the concept
that won rather than against five that might.

## Not decided here

- **The themed (Android 13) layer is its own decision, not a flatten.** A and B
  flatten to a bare 2×2 grid, which is honest but says nothing; if either is
  chosen its themed layer wants redrawing. Flattening the colour art is what
  turned the sibling inzo mark into "a solid bowtie".
- **Which surfaces are generated, which are references, and which are redraws.**
  Whoever writes `apply.py` should decide that split and put the list *in the
  file*. Controlplaine's mark landed on twelve surfaces: six regenerated, three
  hand-wired references to generated art that follow on their own, and three
  drawn for the surface they sit on that do **not** follow — and the only reason
  those three are known to be stale rather than discovered to be is that the
  table exists. Two specifics that would apply here: a notification avatar must
  be the **round** bitmap, not `@mipmap/ic_launcher`, because the shade
  centre-crops a square tile into a circle; and the splash theme in
  `values-v31` is a reference, so it follows for free.
- Whatever ships needs looking at **on the Pad 3's own launcher, over a photo
  wallpaper**, at the size it will actually be. Everything above is measured,
  but nothing has been on the device.
