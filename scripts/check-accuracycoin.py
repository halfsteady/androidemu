#!/usr/bin/env python3
"""Run the pinned AccuracyCoin ROM and reject lost passes, skipped tests or hangs."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rom", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    expected = json.loads((root / "docs/accuracycoin-results.json").read_text())
    if hashlib.sha256(args.rom.read_bytes()).hexdigest() != expected["rom_sha256"]:
        parser.error("ROM revision differs from the recorded baseline; use the pinned revision in docs/ACCURACY.md")
    subprocess.run(["cargo", "build", "--release", "--locked", "-p", "nes-runner"], cwd=root, check=True)
    result = subprocess.run(
        [str(root / "target/release/nes-runner"), "accuracycoin", str(args.rom.resolve()), "10000"],
        capture_output=True, text=True, timeout=180,
    )
    rows = [line.split("\t") for line in result.stdout.splitlines() if "\t" in line]
    if result.returncode not in (0, 1) or len(rows) != 144 or "completed=true" not in result.stdout:
        raise SystemExit("AccuracyCoin did not complete:\n" + result.stdout + result.stderr)
    actual = {}
    for status, page, name, code, address in rows:
        if address in actual or status not in ("PASS", "FAIL"):
            raise SystemExit(f"Invalid/unfinished result: {status} {address}")
        actual[address] = {"page": int(page), "name": name, "status": status, "code": code}
    lost = []
    for test in expected["tests"]:
        current = actual.get(test["address"])
        if current is None or current["name"] != test["name"] or current["page"] != test["page"]:
            raise SystemExit("AccuracyCoin menu differs from the recorded baseline")
        if test["after"] == "PASS" and current["status"] != "PASS":
            lost.append(test["name"])
    report = {"rom_sha256": expected["rom_sha256"], "passed": sum(t["status"] == "PASS" for t in actual.values()), "total": 144, "regressions": lost, "results": actual}
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(f"AccuracyCoin: {report['passed']}/144 passed; {len(lost)} regressions against the recorded 103-pass baseline")
    if lost:
        raise SystemExit("Lost passes: " + ", ".join(lost))


if __name__ == "__main__":
    main()
