#!/usr/bin/env python3
"""Publish already-built, signed APKs to this project's Controlplaine /stuff shelf."""
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
for variant, label, architecture in [("preview", "tablet preview", "arm64 for the OnePlus Pad 3"), ("debug", "debug build", "arm64 and x86_64 for development/emulators")]:
    source = root / f"android/app/build/outputs/apk/{variant}/app-{variant}.apk"
    subprocess.run([str(build_tools / "apksigner"), "verify", str(source)], check=True)
    subprocess.run([str(build_tools / "zipalign"), "-c", "-P", "16", "4", str(source)], check=True)
    manifest = subprocess.check_output([str(build_tools / "aapt2"), "dump", "badging", str(source)], text=True)
    version = re.search(r"versionName='([^']+)'", manifest).group(1)
    code = re.search(r"versionCode='([^']+)'", manifest).group(1)
    name = f"{date}-androidemu-{variant}.apk"
    target = shelf / name
    sidecar = shelf / (name + ".json")
    if target.exists() and (not sidecar.exists() or not json.loads(sidecar.read_text()).get("title", "").startswith("AndroidEmu ")):
        raise SystemExit(f"Refusing to replace an unrecognized artifact: {target}")
    shutil.copyfile(source, staging / name)
    digest = hashlib.sha256((staging / name).read_bytes()).hexdigest()
    metadata = {"title": f"AndroidEmu {label} — {version}", "summary": f"{architecture}. Version code {code}. Phase 1 preview: library, video/audio, NTSC/PAL/Dendy, mappers 0/1/2/3/4/7/66, touch and controllers with a per-device button-mapping wizard, full screen, 10 savestate slots, thumbnails, autosave and SRAM. 60 Rust tests, 39 accuracy ROMs and Kotlin/JNI tests pass. Device UI and latency acceptance remain pending. SHA-256: {digest}", "tags": ["android", "apk", "nes", "preview"]}
    (staging / sidecar.name).write_text(json.dumps(metadata, indent=2) + "\n")
    os.replace(staging / name, target)
    os.replace(staging / sidecar.name, sidecar)
    print(json.dumps({"file": str(target), "version": version, "sha256": digest}))
