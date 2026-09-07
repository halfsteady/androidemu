#!/usr/bin/env python3
"""Publish the signed release APK to this project's /stuff shelf.

Keeps exactly two rows: the installable APK, and a link to the GitHub Release
it came from. Anything this script published earlier is pruned, so the shelf
does not quietly grow a build per session. The AAB is deliberately not shelved
- it is only ever uploaded to Play Console from a desktop, and it sits on the
release the link points at.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

if os.environ.get("JAVA_HOME"):
    os.environ["PATH"] = str(Path(os.environ["JAVA_HOME"]) / "bin") + os.pathsep + os.environ.get("PATH", "")
root = Path(__file__).resolve().parent.parent
sdk = Path(os.environ.get("ANDROID_HOME", str(Path.home() / "Android/Sdk")))
build_tools = sdk / "build-tools/36.0.0"
shelf = root / ".harness/artifacts"
staging = shelf / ".staging"
staging.mkdir(parents=True, exist_ok=True)
date = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d")

# Rows this script owns, and may therefore replace or prune, identified by the
# sidecar title it wrote. Someone else's entry is never touched.
OWNED_TITLE_PREFIXES = ("Emulia", "Amelia’s NES", "AndroidEmu ")
RELEASES_URL = "https://github.com/bsteinfeld/androidemu/releases"

# Defaults to what Gradle just built. Pass a path to shelve the artifact a
# GitHub Release actually shipped instead, so the shelf and the release are the
# same bytes rather than two builds of the same commit:
#   gh release download v0.1.0 -D /tmp/rel
#   python3 scripts/publish-apks.py /tmp/rel/*.apk
source = next(
    (Path(a).resolve() for a in sys.argv[1:] if a.endswith(".apk")),
    root / "android/app/build/outputs/apk/release/app-release.apk",
)


def owned(entry):
    """True when a shelf row came from this script, by its sidecar's title."""
    sidecar = entry.with_name(entry.name + ".json")
    if not sidecar.exists():
        return False
    return json.loads(sidecar.read_text()).get("title", "").startswith(OWNED_TITLE_PREFIXES)


manifest = subprocess.check_output([str(build_tools / "aapt2"), "dump", "badging", str(source)], text=True)
version = re.search(r"versionName='([^']+)'", manifest).group(1)
code = re.search(r"versionCode='([^']+)'", manifest).group(1)

subprocess.run([str(build_tools / "apksigner"), "verify", str(source)], check=True)
subprocess.run([str(build_tools / "zipalign"), "-c", "-P", "16", "4", str(source)], check=True)
# Whatever signed it must not be the debug key; this gets handed out.
certificate = subprocess.check_output(
    [str(build_tools / "apksigner"), "verify", "--print-certs", str(source)], text=True
)
if "CN=Android Debug" in certificate:
    raise SystemExit(f"Refusing to publish a debug-signed artifact: {source}")

name = f"{date}-emulia-{version}.apk"
target = shelf / name
sidecar = shelf / (name + ".json")
if target.exists() and not owned(target):
    raise SystemExit(f"Refusing to replace an unrecognized artifact: {target}")
shutil.copyfile(source, staging / name)
digest = hashlib.sha256((staging / name).read_bytes()).hexdigest()
summary = (
    f"Tablet APK, arm64. Version code {code}. NTSC/PAL/Dendy, mappers 0/1/2/3/4/7/66, "
    "five-channel audio, touch and controllers with a per-device button-mapping wizard, "
    "full screen, rewind, 10 savestate slots with thumbnails, autosave and SRAM. "
    f"Signed with the upload key. SHA-256: {digest}"
)
(staging / sidecar.name).write_text(
    json.dumps(
        {
            "title": f"Emulia — {version}",
            "summary": summary,
            "tags": ["android", "apk", "nes", "emulia"],
        },
        indent=2,
    )
    + "\n"
)
os.replace(staging / name, target)
os.replace(staging / sidecar.name, sidecar)
print(json.dumps({"file": str(target), "version": version, "sha256": digest}))

# The releases page carries every version, the AAB and the checksums. It needs
# a GitHub login, which the APK row above does not, so it complements that row
# rather than replacing it.
link = shelf / "emulia-releases.link.json"
link.write_text(
    json.dumps(
        {
            "title": "Emulia releases",
            "url": RELEASES_URL,
            "summary": "Every tagged release, with its APK, Play bundle and checksums. Private repository, so downloading needs a GitHub login.",
            "tags": ["android", "nes", "emulia", "source"],
        },
        indent=2,
    )
    + "\n"
)
print(json.dumps({"file": str(link), "url": RELEASES_URL}))

# Prune what this script published before. Keeping one build per session turns
# the shelf into a changelog nobody asked for.
for stale in sorted(shelf.iterdir()):
    if stale.is_dir() or stale.suffix == ".json" or stale in (target, link):
        continue
    if owned(stale):
        stale.with_name(stale.name + ".json").unlink()
        stale.unlink()
        print(json.dumps({"pruned": stale.name}))
