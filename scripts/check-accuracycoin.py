#!/usr/bin/env python3
"""Run the pinned AccuracyCoin ROM and reject lost passes, skipped tests or hangs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def build_runner(root):
    build = subprocess.run(
        ["cargo", "build", "--release", "--locked", "-p", "nes-runner",
         "--message-format=json-render-diagnostics"],
        cwd=root, stdout=subprocess.PIPE, text=True, check=True,
    )
    # Cargo reports the actual executable, including CARGO_TARGET_DIR and any
    # configured target triple. A hard-coded target/release path can run a stale
    # binary instead of the code that was just built.
    for line in build.stdout.splitlines():
        artifact = json.loads(line)
        if (artifact.get("reason") == "compiler-artifact"
                and artifact["target"]["name"] == "nes-runner"
                and artifact.get("executable")):
            return artifact["executable"]
    raise ValueError("Cargo did not report a nes-runner executable")


def check_results(stdout, returncode, expected):
    rows = [line.split("\t") for line in stdout.splitlines() if "\t" in line]
    summary = re.search(r"^AccuracyCoin: (\d+)/144 passed; completed=true$", stdout, re.MULTILINE)
    if returncode not in (0, 1) or len(rows) != 144 or summary is None:
        raise ValueError("AccuracyCoin did not complete")
    actual = {}
    for row in rows:
        if len(row) != 5:
            raise ValueError(f"Malformed result row: {row!r}")
        status, page, name, code, address = row
        if (address in actual or status not in ("PASS", "FAIL")
                or not re.fullmatch(r"\$04[0-9A-F]{2}", address)
                or not re.fullmatch(r"[0-3][0-9A-F]", code)):
            raise ValueError(f"Invalid/unfinished result: {status} {address} {code}")
        actual[address] = {"page": int(page), "name": name, "status": status, "code": code}
    if set(actual) != {t["address"] for t in expected["tests"]}:
        raise ValueError("AccuracyCoin menu differs from the recorded baseline")
    lost = []
    for test in expected["tests"]:
        current = actual[test["address"]]
        if current["name"] != test["name"] or current["page"] != test["page"]:
            raise ValueError("AccuracyCoin menu differs from the recorded baseline")
        if test["after"] == "PASS" and current["status"] != "PASS":
            lost.append(test["name"])
    passed = sum(t["status"] == "PASS" for t in actual.values())
    if int(summary[1]) != passed or returncode != (0 if passed == 144 else 1):
        raise ValueError("AccuracyCoin summary/exit status disagrees with its result rows")
    return {"rom_sha256": expected["rom_sha256"], "passed": passed, "total": 144,
            "regressions": lost, "results": actual}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rom", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    expected = json.loads((root / "docs/accuracycoin-results.json").read_text())
    if hashlib.sha256(args.rom.read_bytes()).hexdigest() != expected["rom_sha256"]:
        parser.error("ROM revision differs from the recorded baseline; use the pinned revision in docs/ACCURACY.md")
    try:
        runner = build_runner(root)
        result = subprocess.run(
            [runner, "accuracycoin", str(args.rom.resolve()), "10000"],
            capture_output=True, text=True, timeout=180,
        )
    except subprocess.TimeoutExpired:
        parser.exit(1, "AccuracyCoin runner timed out after 180 seconds\n")
    except (subprocess.CalledProcessError, ValueError) as error:
        parser.exit(1, f"Cannot build AccuracyCoin runner: {error}\n")
    try:
        report = check_results(result.stdout, result.returncode, expected)
    except ValueError as error:
        parser.exit(1, f"{error}:\n{result.stdout}{result.stderr}\n")
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    baseline = sum(t["after"] == "PASS" for t in expected["tests"])
    lost = report["regressions"]
    print(f"AccuracyCoin: {report['passed']}/144 passed; {len(lost)} regressions against the recorded {baseline}-pass baseline")
    if lost:
        raise SystemExit("Lost passes: " + ", ".join(lost))


if __name__ == "__main__":
    main()
