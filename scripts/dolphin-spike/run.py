#!/usr/bin/env python3
"""Launch the local Dolphin embedding experiment with a dedicated Emulia library."""
import argparse
import os
from pathlib import Path
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--app', type=Path, required=True)
p.add_argument('--data-dir', type=Path, required=True)
p.add_argument('game', type=Path)
a = p.parse_args()
app = a.app.resolve()
env = dict(os.environ, EMULIA_DOLPHIN_PROBE_LIB=str(app / 'Contents/Frameworks/libemulia_dolphin_probe.dylib'))
raise SystemExit(subprocess.call([str(app / 'Contents/MacOS/Emulia'), str(a.game.resolve()),
                                 '--data-dir', str(a.data_dir.resolve())], env=env))
