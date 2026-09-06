#!/usr/bin/env python3
"""Run public conformance ROMs from an external checkout; never bundle them."""
import argparse
import concurrent.futures
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("roms", type=Path, help="christopherpow/nes-test-roms checkout")
parser.add_argument("--report", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
subprocess.run([str(Path.home() / ".cargo/bin/cargo"), "build", "--release", "-p", "nes-runner"], cwd=root, check=True)
patterns = ["instr_test-v5/rom_singles/*.nes", "apu_test/rom_singles/*.nes", "ppu_vbl_nmi/rom_singles/*.nes", "mmc3_test/[1-5]-*.nes"]
paths = sorted(p for pattern in patterns for p in args.roms.glob(pattern))
if len(paths) != 39:
    parser.error(f"Expected 39 ROMs in the supported suites, found {len(paths)}")
def run(path):
    result = subprocess.run([str(root / "target/release/nes-runner"), "rom-test", str(path), "1800"], capture_output=True, text=True, timeout=120)
    return {"rom": str(path.relative_to(args.roms)), "passed": result.returncode == 0, "output": (result.stdout + result.stderr).strip()}
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    results = list(pool.map(run, paths))
for result in results:
    print(f"{'PASS' if result['passed'] else 'FAIL'} {result['rom']}")
    if not result["passed"]:
        print(result["output"])
if args.report:
    args.report.write_text(json.dumps(results, indent=2) + "\n")
raise SystemExit(0 if all(r["passed"] for r in results) else 1)
