#!/usr/bin/env python3
"""Stdlib unit tests for the phase-6 fuzz campaign parse/summary logic."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import phase6_fuzz_campaign as fc  # noqa: E402

CLEAN = """INFO: Running with entropic power schedule (0xFF, 100).
INFO: Seed: 123
INFO:       12 files found in /x/corpus/bdf
INFO: seed corpus: files: 12 min: 1b max: 900b total: 4000b rss: 40Mb
#13\tINITED cov: 410 ft: 900 corp: 11/3900b exec/s: 0 rss: 45Mb
#4096\tpulse  cov: 500 ft: 1200 corp: 30/9Kb lim: 4096 exec/s: 2048 rss: 60Mb
#812345\tDONE   cov: 1234 ft: 5678 corp: 210/88Kb lim: 65536 exec/s: 1330 rss: 412Mb
Done 812345 runs in 611 second(s)
stat::number_of_executed_units: 812345
stat::average_exec_per_sec:     1330
stat::new_units_added:          1999
stat::slowest_unit_time_sec:    0
stat::peak_rss_mb:              412
"""

CRASH = """INFO: seed corpus: files: 3 min: 1b max: 9b total: 20b rss: 40Mb
#3\tINITED cov: 100 ft: 120 corp: 3/20b exec/s: 0 rss: 41Mb
#77\tNEW    cov: 140 ft: 170 corp: 4/30b lim: 4 exec/s: 0 rss: 41Mb L: 9/9 MS: 1 ChangeBit-
==4242==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x602000000011
SUMMARY: AddressSanitizer: heap-buffer-overflow
MS: 1 ChangeBit-; base unit: 0000
artifact_prefix='/r/artifacts/bdf/'; Test unit written to /r/artifacts/bdf/crash-abc123
"""


def record(**kw):
    base = {"exit_code": 0, "failing_artifacts": [], "watchdog_killed": False,
            "elapsed_seconds": 611.0, "parsed": fc.parse_libfuzzer_log(CLEAN)}
    base.update(kw)
    return base


class ParseTests(unittest.TestCase):
    def test_clean_final_stats(self):
        p = fc.parse_libfuzzer_log(CLEAN)
        self.assertTrue(p["final_stats_present"])
        self.assertEqual(p["execs"], 812345)
        self.assertEqual(p["execs_source"], "stat::number_of_executed_units")
        self.assertEqual((p["cov"], p["ft"]), (1234, 5678))
        self.assertEqual(p["cov_source"], "DONE line")
        self.assertEqual(p["stats"]["peak_rss_mb"], 412)
        self.assertEqual((p["done_runs"], p["done_seconds"]), (812345, 611))
        self.assertEqual(p["seed_files_reported"], 12)
        self.assertEqual(p["error_markers"], [])

    def test_crash_without_stats_falls_back(self):
        p = fc.parse_libfuzzer_log(CRASH)
        self.assertFalse(p["final_stats_present"])
        self.assertEqual(p["execs"], 77)
        self.assertEqual(p["execs_source"], "last status line (partial)")
        self.assertEqual((p["cov"], p["ft"]), (140, 170))
        self.assertEqual(p["artifacts_reported"], ["/r/artifacts/bdf/crash-abc123"])
        self.assertIn("==ERROR: AddressSanitizer", p["error_markers"])

    def test_empty_log(self):
        p = fc.parse_libfuzzer_log("")
        self.assertIsNone(p["execs"])
        self.assertIsNone(p["cov"])


class EvaluateTests(unittest.TestCase):
    def test_clean_pass(self):
        self.assertTrue(fc.evaluate(record(), 600)["passed"])

    def test_short_run_fails_even_with_exit_zero(self):
        ev = fc.evaluate(record(elapsed_seconds=599.9), 600)
        self.assertFalse(ev["passed"])
        self.assertTrue(any("elapsed" in r for r in ev["reasons"]))

    def test_crash_fails(self):
        ev = fc.evaluate(record(exit_code=1, failing_artifacts=["crash-abc"],
                                parsed=fc.parse_libfuzzer_log(CRASH)), 600)
        self.assertFalse(ev["passed"])
        joined = "; ".join(ev["reasons"])
        self.assertIn("exit code 1", joined)
        self.assertIn("crash-abc", joined)
        self.assertIn("no libFuzzer final stats", joined)
        self.assertNotIn("elapsed", joined)

    def test_artifact_alone_fails(self):
        ev = fc.evaluate(record(failing_artifacts=["oom-1"]), 600)
        self.assertFalse(ev["passed"])

    def test_watchdog_fails(self):
        self.assertFalse(fc.evaluate(record(watchdog_killed=True), 600)["passed"])

    def test_missing_stats_fails(self):
        p = fc.parse_libfuzzer_log(CLEAN.split("stat::")[0])
        self.assertFalse(fc.evaluate(record(parsed=p), 600)["passed"])


class ArtifactTests(unittest.TestCase):
    def test_failing_prefixes_only(self):
        with tempfile.TemporaryDirectory() as d:
            for n in ("crash-1", "timeout-2", "oom-3", "leak-4", "slow-unit-5", "other"):
                (Path(d) / n).write_bytes(b"x")
            self.assertEqual(fc.failing_artifacts(Path(d)), ["crash-1", "leak-4", "oom-3", "timeout-2"])
            self.assertEqual(fc.failing_artifacts(Path(d) / "missing"), [])


class SummaryTests(unittest.TestCase):
    def test_summary_marks_pass_fail_missing(self):
        with tempfile.TemporaryDirectory() as d:
            run = Path(d)
            (run / "results").mkdir()
            (run / "manifest.json").write_text(json.dumps(
                {"label": "t", "required_seconds": 600, "targets": ["a", "b", "c"],
                 "max_total_time": 610, "rss_limit_mb": 2048, "timeout": 10, "parallel": 16}))
            (run / "results" / "a.json").write_text(json.dumps(record()))
            (run / "results" / "b.json").write_text(json.dumps(record(elapsed_seconds=15.0)))
            (run / "builds.json").write_text(json.dumps(
                [{"target": "a", "exit_code": 0}, {"target": "c", "exit_code": 101}]))
            s = fc.summarize(run)
            self.assertEqual(s["counts"]["passed"], 1)
            self.assertEqual(s["failed"], ["b"])
            self.assertEqual(s["missing"], ["c"])
            self.assertEqual(s["counts"]["build_failed"], ["c"])
            self.assertFalse(s["all_passed"])
            self.assertEqual(s["total_execs"], 2 * 812345)
            self.assertEqual(s["limits"]["rss_limit_mb"], 2048)
            self.assertEqual(s["limits"]["max_total_time"], 610)

    def test_default_rss_limit_is_2048(self):
        import argparse  # noqa: F401
        captured = {}
        orig = fc.cmd_run
        fc.cmd_run = lambda a: captured.setdefault("a", a) and 0
        try:
            fc.main(["run", "--label", "x", "--seconds", "1"])
        finally:
            fc.cmd_run = orig
        self.assertEqual(captured["a"].rss_limit_mb, 2048)


class InventoryHelperTests(unittest.TestCase):
    def test_input_limit_expression(self):
        src = "max_input_bytes: 256 * 1024,\n x.max_input_bytes=65536;"
        self.assertEqual(fc.source_input_limit(src), 262144)
        self.assertIsNone(fc.source_input_limit("nothing"))

    def test_format_module_refs(self):
        src = "use fontpixel::{\n formats::{Confidence, fnt::FntWriteOptions, fon},\n};"
        self.assertEqual(fc.referenced_format_modules(src), {"fnt", "fon"})

    def test_every_profile_target_has_readers(self):
        self.assertEqual(set(fc.PROFILES), set(fc.READERS))


if __name__ == "__main__":
    unittest.main()
