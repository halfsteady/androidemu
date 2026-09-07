#!/usr/bin/env python3
"""Compile-check the GLSL in ScreenRenderer.kt without a device.

A shader that fails to compile is a black screen, and the only place that shows
up is on hardware — the Kotlin compiles either way, because the shader is a
string. This pulls the two shader sources out of the Kotlin literal and runs them
through glslangValidator, which does enforce the ES 3.00 rules that matter here
(reserved words like `sample`, undeclared identifiers, type mismatches).

Usage:
    python3 scripts/check-shaders.py [--validator PATH]

The validator is looked up on PATH, then in the Android SDK's emulator, which
ships one. With none available this skips rather than fails, the same way
check-roms.py skips without its ROMs.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "android/app/src/main/java/dev/androidemu/ScreenRenderer.kt"
SHADERS = [("VERTEX", "const val FRAGMENT", "vert"), ("FRAGMENT", "}", "frag")]


def find_validator(explicit: str | None) -> str | None:
    if explicit:
        return explicit if os.access(explicit, os.X_OK) else None
    found = shutil.which("glslangValidator")
    if found:
        return found
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT") or str(Path.home() / "Android/Sdk")
    candidate = Path(sdk) / "emulator/lib64/vulkan/glslangValidator"
    return str(candidate) if candidate.exists() else None


def extract(name: str, stop: str) -> str:
    """The Kotlin string-concatenation literal for one shader, comments dropped."""
    out: list[str] = []
    inside = False
    for line in SOURCE.read_text().splitlines():
        stripped = line.strip()
        if stripped.startswith(f"const val {name} ="):
            inside = True
            continue
        if inside and stripped.startswith(stop):
            break
        if not inside or stripped.startswith("//"):
            continue
        for piece in re.findall(r'"((?:[^"\\]|\\.)*)"', line):
            out.append(piece.encode().decode("unicode_escape"))
    if not out:
        raise SystemExit(f"could not find the {name} shader in {SOURCE}")
    return "".join(out)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--validator", help="path to glslangValidator")
    args = parser.parse_args()

    validator = find_validator(args.validator)
    if not validator:
        print("skipped: no glslangValidator found (PATH or $ANDROID_HOME/emulator/lib64/vulkan)")
        return 0

    # The SDK's copy is not executable where it sits, so run a copy.
    with tempfile.TemporaryDirectory() as work:
        runnable = Path(work) / "glslangValidator"
        shutil.copy2(validator, runnable)
        runnable.chmod(0o755)
        failed = False
        for name, stop, suffix in SHADERS:
            path = Path(work) / f"screen.{suffix}"
            path.write_text(extract(name, stop))
            done = subprocess.run([str(runnable), str(path)], capture_output=True, text=True)
            if done.returncode == 0:
                print(f"ok: {name.lower()} shader")
            else:
                failed = True
                print(f"FAILED: {name.lower()} shader")
                print(done.stdout.strip() or done.stderr.strip())
        return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
