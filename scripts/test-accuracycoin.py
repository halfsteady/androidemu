#!/usr/bin/env python3
"""Test the regression gate without external ROMs or a Rust build."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("accuracycoin", Path(__file__).with_name("check-accuracycoin.py"))
accuracycoin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(accuracycoin)


class RegressionGateTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parent.parent
        self.expected = json.loads((root / "docs/accuracycoin-results.json").read_text())
        self.baseline = sum(t["after"] == "PASS" for t in self.expected["tests"])
        self.rows = [
            [t["after"], str(t["page"]), t["name"], t["after_code"], t["address"]]
            for t in self.expected["tests"]
        ]

    def output(self, rows=None, completed=True):
        rows = self.rows if rows is None else rows
        passes = sum(row[0] == "PASS" for row in rows)
        return "\n".join("\t".join(row) for row in rows) + f"\nAccuracyCoin: {passes}/144 passed; completed={str(completed).lower()}\n"

    def test_baseline_and_improvements_pass(self):
        report = accuracycoin.check_results(self.output(), 1, self.expected)
        self.assertEqual(report["passed"], self.baseline)
        self.assertEqual(report["regressions"], [])
        next(row for row in self.rows if row[0] == "FAIL")[0] = "PASS"
        report = accuracycoin.check_results(self.output(), 1, self.expected)
        self.assertEqual(report["passed"], self.baseline + 1)
        self.assertEqual(report["regressions"], [])

    def test_regressions_are_reported_even_when_total_does_not_decrease(self):
        lost = next(row for row in self.rows if row[0] == "PASS")
        next(row for row in self.rows if row[0] == "FAIL")[0] = "PASS"
        lost[0] = "FAIL"
        report = accuracycoin.check_results(self.output(), 1, self.expected)
        self.assertEqual(report["passed"], self.baseline)
        self.assertEqual(report["regressions"], [lost[2]])

    def test_unfinished_invalid_and_duplicate_rows_are_rejected(self):
        for status in ["PENDING", "RUNNING", "SKIP", "INVALID"]:
            with self.subTest(status=status):
                rows = [row[:] for row in self.rows]
                rows[0][0] = status
                with self.assertRaises(ValueError):
                    accuracycoin.check_results(self.output(rows), 1, self.expected)
        for index, value in [(4, self.rows[1][4]), (2, "Wrong test"), (1, "23"), (3, "GG")]:
            rows = [row[:] for row in self.rows]
            rows[0][index] = value
            with self.assertRaises(ValueError):
                accuracycoin.check_results(self.output(rows), 1, self.expected)

    def test_truncation_timeout_and_wrong_exit_code_are_rejected(self):
        for output, code in [(self.output(self.rows[:-1]), 1), (self.output(completed=False), 1),
                             (self.output(), 2), (self.output(), 0),
                             (self.output().replace(f"{self.baseline}/144", "144/144"), 1),
                             (self.output().replace("\t", " ", 1), 1)]:
            with self.assertRaises(ValueError):
                accuracycoin.check_results(output, code, self.expected)

    def test_uses_cargo_executable_instead_of_assuming_target_directory(self):
        executable = "/tmp/custom target/x86_64-unknown-linux-gnu/release/nes-runner"
        output = json.dumps({"reason": "compiler-artifact", "target": {"name": "nes-runner"}, "executable": executable})
        with patch.object(accuracycoin.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, output)):
            self.assertEqual(accuracycoin.build_runner(Path("/repo")), executable)


if __name__ == "__main__":
    unittest.main()
