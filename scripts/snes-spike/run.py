#!/usr/bin/env python3
"""Evaluate a pinned external jgenesis checkout without changing app dependencies."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

REVISION = 'b1419eface3147568b2247d33b6bdb6695adfe06'
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--upstream', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    args = parser.parse_args()
    upstream = args.upstream.resolve()
    actual = subprocess.check_output(['git', '-C', str(upstream), 'rev-parse', 'HEAD'], text=True).strip()
    if actual != REVISION:
        raise SystemExit(f'Expected upstream revision {REVISION}; found {actual}')
    if subprocess.check_output(['git', '-C', str(upstream), 'status', '--porcelain', '--untracked-files=no'], text=True).strip():
        raise SystemExit('The upstream checkout must have no source modifications')
    env = dict(os.environ)
    env['PATH'] = str(Path.home() / '.cargo/bin') + os.pathsep + env['PATH']
    with tempfile.TemporaryDirectory(prefix='emulia-snes-probe-') as temporary:
        work = Path(temporary)
        manifest = ('[package]\nname="emulia-snes-probe"\nversion="0.0.0"\nedition="2024"\npublish=false\n'
                    '[workspace]\n[[bin]]\nname="emulia-snes-probe"\npath=' + json.dumps(str(HERE / 'main.rs')) + '\n[dependencies]\n')
        for name, path in [('snes-core', upstream / 'backend/snes-core'),
                           ('jgenesis-common', upstream / 'common/jgenesis-common'),
                           ('emulation-runtime', ROOT / 'emulation-runtime')]:
            manifest += name + '={path=' + json.dumps(str(path)) + '}\n'
        manifest += 'bincode="2"\n'
        (work / 'Cargo.toml').write_text(manifest)
        shutil.copyfile(HERE / 'Cargo.lock', work / 'Cargo.lock')
        subprocess.run(['cargo', '+1.98.1', 'run', '--release', '--locked', '--manifest-path', str(work / 'Cargo.toml'),
                        '--target-dir', str(args.target_dir.resolve())], env=env, check=True)


if __name__ == '__main__':
    main()
