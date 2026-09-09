#!/usr/bin/env python3
"""Check that site/ is internally whole before it is published.

  python3 scripts/check-site.py [--site site]

The site is plain static HTML with no build step, so nothing else ever looks at
it: a renamed screenshot or a mistyped href is a silent 404 on a live page. This
walks every local href/src in the HTML and fails if the file is not there. It
deliberately does not touch the network -- external links are not this script's
business, and a publish step must not depend on someone else's uptime.

Also checks the two things a static site gets wrong quietly: an <img> without
alt text, and a page with no <title>.
"""

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse

ROOT = Path(__file__).resolve().parent.parent

# src/href on any element, plus the og:image/twitter:image content attributes.
REF = re.compile(r'(?:src|href)\s*=\s*"([^"]+)"', re.I)
META_IMG = re.compile(r'<meta[^>]+(?:og:image|twitter:image)"[^>]+content="([^"]+)"', re.I)
IMG_TAG = re.compile(r"<img\b[^>]*>", re.I)
TITLE = re.compile(r"<title>\s*\S", re.I)


def local_refs(html):
    """Every reference that should resolve to a file in the site directory."""
    for m in REF.finditer(html):
        yield m.group(1)
    for m in META_IMG.finditer(html):
        yield m.group(1)


def check(site):
    problems = []
    pages = sorted(site.glob("*.html"))
    if not pages:
        return ["no HTML pages found in %s" % site]

    for page in pages:
        html = page.read_text(encoding="utf-8")
        where = page.relative_to(ROOT)

        if not TITLE.search(html):
            problems.append("%s: no <title>" % where)

        for tag in IMG_TAG.findall(html):
            if 'alt="' not in tag.lower():
                problems.append("%s: <img> with no alt attribute: %s" % (where, tag[:70]))

        for ref in local_refs(html):
            if ref.startswith(("http://", "https://", "mailto:", "data:", "#")):
                continue
            path = urlparse(ref).path
            if not path:
                continue
            # "/" is the index, and "/foo" is site-absolute rather than filesystem-absolute.
            target = site / "index.html" if path == "/" else \
                (site / path.lstrip("/") if path.startswith("/") else page.parent / path)
            if not Path(unquote(str(target))).exists():
                problems.append("%s: %s does not exist" % (where, ref))

    return problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--site", default="site", help="the site directory (default: site)")
    args = ap.parse_args()

    site = (ROOT / args.site).resolve()
    if not site.is_dir():
        print("no such directory: %s" % site, file=sys.stderr)
        return 2

    problems = check(site)
    if problems:
        for p in problems:
            print("site: %s" % p, file=sys.stderr)
        print("\n%d problem(s)." % len(problems), file=sys.stderr)
        return 1

    pages = len(list(site.glob("*.html")))
    print("site: %d page(s), every local reference resolves." % pages)
    return 0


if __name__ == "__main__":
    sys.exit(main())
