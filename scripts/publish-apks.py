#!/usr/bin/env python3
"""Publish the already-built, signed release APK and AAB to this project's /stuff shelf."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

if os.environ.get("JAVA_HOME"):
    os.environ["PATH"] = str(Path(os.environ["JAVA_HOME"]) / "bin") + os.pathsep + os.environ.get("PATH", "")
root = Path(__file__).resolve().parent.parent
sdk = Path(os.environ.get("ANDROID_HOME", str(Path.home() / "Android/Sdk")))
build_tools = sdk / "build-tools/36.0.0"
shelf = root / ".harness/artifacts"
staging = shelf / ".staging"
staging.mkdir(parents=True, exist_ok=True)
date = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d")
# The release build is the only artifact worth shelving: it is what a GitHub
# Release carries and what gets sideloaded, signed with the upload key. The AAB
# rides along because that is the file Play Console wants.
outputs = [
    ("apk", Path("android/app/build/outputs/apk/release/app-release.apk"), "tablet APK", "arm64, sideload onto the OnePlus Pad 3"),
    ("aab", Path("android/app/build/outputs/bundle/release/app-release.aab"), "Play bundle", "arm64 app bundle for Play Console"),
]
manifest = subprocess.check_output([str(build_tools / "aapt2"), "dump", "badging", str(root / outputs[0][1])], text=True)
version = re.search(r"versionName='([^']+)'", manifest).group(1)
code = re.search(r"versionCode='([^']+)'", manifest).group(1)
for kind, relative, label, architecture in outputs:
    source = root / relative
    if kind == "apk":
        subprocess.run([str(build_tools / "apksigner"), "verify", str(source)], check=True)
        subprocess.run([str(build_tools / "zipalign"), "-c", "-P", "16", "4", str(source)], check=True)
    # Whatever signed it must not be the debug key; these get handed out.
    certificate = subprocess.check_output(["keytool", "-printcert", "-jarfile", str(source)], text=True)
    if "CN=Android Debug" in certificate:
        raise SystemExit(f"Refusing to publish a debug-signed artifact: {source}")
    name = f"{date}-amelias-nes-{version}.{kind}"
    target = shelf / name
    sidecar = shelf / (name + ".json")
    if target.exists() and (not sidecar.exists() or not json.loads(sidecar.read_text()).get("title", "").startswith("Amelia\u2019s NES ")):
        raise SystemExit(f"Refusing to replace an unrecognized artifact: {target}")
    shutil.copyfile(source, staging / name)
    digest = hashlib.sha256((staging / name).read_bytes()).hexdigest()
    metadata = {
        "title": f"Amelia\u2019s NES {label} \u2014 {version}",
        "summary": f"{architecture}. Version code {code}. NTSC/PAL/Dendy, mappers 0/1/2/3/4/7/66, five-channel audio, touch and controllers with a per-device button-mapping wizard, full screen, rewind, 10 savestate slots with thumbnails, autosave and SRAM. Signed with the upload key. SHA-256: {digest}",
        "tags": ["android", kind, "nes", "amelias-nes"],
    }
    (staging / sidecar.name).write_text(json.dumps(metadata, indent=2) + "\n")
    os.replace(staging / name, target)
    os.replace(staging / sidecar.name, sidecar)
    print(json.dumps({"file": str(target), "version": version, "sha256": digest}))
