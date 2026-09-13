#!/usr/bin/env python3
"""Render the established brand SVG; requires librsvg and Pillow only when regenerating."""
from pathlib import Path
import subprocess
import tempfile
from PIL import Image

root = Path(__file__).resolve().parents[1]
out = root / 'desktop/packaging/icons'
out.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory() as tmp:
    master = Path(tmp) / 'master.png'
    subprocess.run(['rsvg-convert', '-w', '1024', '-h', '1024', '-o', str(master),
                    str(root / 'site/assets/mark.svg')], check=True)
    with Image.open(master) as artwork:
        # Inset the existing rounded mark for macOS Dock alignment.
        mac = Image.new('RGBA', (1024, 1024))
        mac.alpha_composite(artwork.resize((824, 824), Image.Resampling.LANCZOS), (100, 100))
        mac.save(out / 'Emulia.icns', format='ICNS')
        artwork.resize((512, 512), Image.Resampling.LANCZOS).save(out / 'Emulia.png')
