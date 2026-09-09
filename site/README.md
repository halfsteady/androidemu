# site/

The Emulia website: <https://emulia.website>

This folder is the source of truth. It is plain static HTML with no build step —
open `index.html` in a browser and that is the site.

```
index.html      the marketing page
privacy.html    the privacy policy, which is also the URL Google Play requires
assets/         screenshots of the app, and the launcher mark
robots.txt      allows everything, points at the sitemap
sitemap.xml     the two pages
```

## How it reaches the web

This repository is private, and GitHub Pages will not serve a private repository
on a free plan. Making this one public to publish a marketing page would publish
the emulator with it, so the site is mirrored instead:

```
androidemu/site/  ──publish-site.yml──▶  bsteinfeld/emulia-site  ──pages.yml──▶  emulia.website
      (private)         on push to main          (public)            GitHub Pages
```

[`.github/workflows/publish-site.yml`](../.github/workflows/publish-site.yml)
runs on any push to `main` that touches `site/**`. It checks the site is whole,
then pushes the folder into the public repository over an SSH deploy key held in
the `EMULIA_SITE_DEPLOY_KEY` secret. That push triggers the Pages deploy there.

**Never edit the copy in `emulia-site`** — the next sync overwrites it.

## Working on it

```sh
cd site && python3 -m http.server 8000
python3 scripts/check-site.py        # every local href and src resolves
```

`check-site.py` is what CI runs before publishing. With no build step nothing
else would notice a renamed screenshot until it 404s on the live page.

## Design notes

The colours are not invented. `--ink`, `--panel`, `--leaf` and `--paper` are the
values the app ships in `Ui.kt`, and `--q1`–`--q4` are the four quadrants of the
launcher mark. Each section owns one of the four in order, because that is what
the mark is about: the same thing, every palette.

Screenshots are real, cropped to remove the Android status and navigation bars,
and converted to WebP. `assets/mark.svg`, `favicon-32.png` and
`apple-touch-icon.png` come out of `python3 brand/tools/build.py` rather than
being redrawn — the site's mark and the launcher icon are the same object.

The Google Play button is deliberately inert while the listing is a draft. When
the app is live, both `<span class="badge pending">` elements become links to the
store and lose the `pending` class.
