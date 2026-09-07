#!/usr/bin/env python3
"""Compile-check the GLSL in ScreenRenderer.kt without a device.

A shader that fails to compile is a black screen, and the only place that shows
up is on hardware — the Kotlin compiles either way, because the shader is a
string. This pulls every shader source out of the Kotlin literals and runs them
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


def find_validator(explicit: str | None) -> str | None:
    if explicit:
        return explicit if os.access(explicit, os.X_OK) else None
    found = shutil.which("glslangValidator")
    if found:
        return found
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT") or str(Path.home() / "Android/Sdk")
    candidate = Path(sdk) / "emulator/lib64/vulkan/glslangValidator"
    return str(candidate) if candidate.exists() else None


def shaders() -> list[tuple[str, str, str]]:
    """Every GLSL literal in the file, as (name, source, suffix).

    Found rather than listed, so a shader added to the renderer is checked
    without anyone remembering to add it here. A constant counts as a shader
    when its literals begin with a #version line; the stage comes from whether
    it writes gl_Position.
    """
    found: list[tuple[str, str, str]] = []
    name: str | None = None
    pieces: list[str] = []

    def flush() -> None:
        if name and pieces:
            source = "".join(pieces)
            if source.startswith("#version"):
                suffix = "vert" if "gl_Position" in source else "frag"
                found.append((name, source, suffix))

    for line in SOURCE.read_text().splitlines():
        stripped = line.strip()
        start = re.match(r"const val (\w+) =", stripped)
        if start:
            flush()
            name, pieces = start.group(1), []
            continue
        if name is None:
            continue
        # A comment can quote things too, and its quotes are not shader source.
        if stripped.startswith("//"):
            continue
        quoted = re.findall(r'"((?:[^"\\]|\\.)*)"', re.sub(r"\s*//.*$", "", line))
        if quoted:
            pieces.extend(piece.encode().decode("unicode_escape") for piece in quoted)
        elif pieces:
            flush()
            name, pieces = None, []
    flush()
    if not found:
        raise SystemExit(f"no shaders found in {SOURCE}")
    return found


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
        for name, source, suffix in shaders():
            path = Path(work) / f"{name.lower()}.{suffix}"
            path.write_text(source)
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
