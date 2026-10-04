#!/usr/bin/env python3
"""Recorder/source guard tests only; never run Rust, Cargo or native font tools."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest import mock

import replay_documentation_smokes as smoke
import check_api_documentation as api_docs


class RecorderTests(unittest.TestCase):
    def setUp(self):
        work = smoke.ROOT.parent / "local-work"
        work.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="docs-recorder-test-", dir=work)
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.evidence = self.base / "evidence"
        self.evidence.mkdir()
        (self.evidence / "temp").mkdir()

    def test_nonzero_process_keeps_complete_raw_streams_and_status(self):
        recorder = smoke.Recorder(self.evidence)
        with self.assertRaisesRegex(ValueError, "got 7"):
            recorder.run("failure", [sys.executable, "-c",
                "import sys;sys.stdout.buffer.write(b'out\\xff');sys.stderr.write('problem');sys.exit(7)"])
        directory = self.evidence / "commands/failure"
        result = json.loads((directory / "result.json").read_text())
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["exit_code"], 7)
        self.assertEqual((directory / "stdout.log").read_bytes(), b"out\xff")
        self.assertEqual((directory / "stderr.log").read_bytes(), b"problem")
        self.assertEqual(result["stdout"]["sha256"], smoke.identity(directory / "stdout.log")["sha256"])

    def test_timeout_preserves_planned_and_finished_records(self):
        recorder = smoke.Recorder(self.evidence, timeout=0.05)
        with self.assertRaises(subprocess.TimeoutExpired):
            recorder.run("timeout", [sys.executable, "-c", "import time;time.sleep(5)"])
        directory = self.evidence / "commands/timeout"
        self.assertTrue((directory / "command.json").is_file())
        result = json.loads((directory / "result.json").read_text())
        self.assertEqual(result["status"], "timeout")
        self.assertIn("stdout", result)
        self.assertIn("stderr", result)

    def test_success_and_duplicate_command_refuses_replacement(self):
        recorder = smoke.Recorder(self.evidence)
        stdout = recorder.run("success", [sys.executable, "-c", "print('original')"])
        with self.assertRaises(FileExistsError):
            recorder.run("success", [sys.executable, "-c", "print('replacement')"])
        self.assertEqual(stdout.read_text(), "original\n")

    def test_evidence_rejects_existing_and_escaping_symlink(self):
        with self.assertRaises(ValueError):
            smoke.new_evidence(self.evidence, self.base)
        alias = self.base / "alias"
        alias.symlink_to(self.base / "not-created")
        with self.assertRaises(ValueError):
            smoke.new_evidence(alias, self.base)
        outside = self.base / "outside"
        outside.mkdir()
        work = self.base / "work"
        work.mkdir()
        (work / "escape").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            smoke.new_evidence(work / "escape/new", work)
        self.assertFalse((outside / "new").exists())

    def test_build_snapshot_refuses_dirty_or_additional_source(self):
        expected = {"head": "same-head", "files": {"lib.rs": {"bytes": 1, "sha256": "a"}}}
        for files in [{"lib.rs": {"bytes": 1, "sha256": "b"}},
                      {**expected["files"], "new.rs": {"bytes": 1, "sha256": "c"}}, {}]:
            with self.assertRaisesRegex(ValueError, "file identities"):
                smoke.check_source({"head": "same-head", "files": files}, expected)

    def test_real_generator_all_84_tasks_and_changed_routes_are_rejected(self):
        inputs, outputs = self.base / "inputs", self.base / "outputs"
        inputs.mkdir()
        outputs.mkdir()
        mini = smoke.ROOT / "tests/data/compat/mini.bdf"
        for size in ["Large", "Small"]:
            for spacing in ["", "Mono"]:
                for region in ["SC", "TC", "JP", "KR", "Latin"]:
                    shutil.copyfile(mini, inputs / f"TongPixelSans{spacing}14{size}-{region}.bdf")
        job_path = self.base / "tong.toml"
        recorder = smoke.Recorder(self.evidence)
        recorder.run("generator", [sys.executable, "-B", smoke.ROOT / "examples/tong_batch.py",
                     "--bdf-dir", inputs, "--out", outputs, "--version", "2026.10.02",
                     "--jobs", "2", "--write", job_path])
        job = tomllib.loads(job_path.read_text())
        self.assertEqual(len(smoke.check_tong_plan(job, inputs, outputs)), 84)
        altered = copy.deepcopy(job)
        variant = next(t for t in altered["tasks"] if t["kind"] == "variants")
        variant["languages"]["ZHT,ZHH"] = variant["languages"]["ja"]
        with self.assertRaisesRegex(ValueError, "paths/order/options"):
            smoke.check_tong_plan(altered, inputs, outputs)
        altered = copy.deepcopy(job)
        altered["tasks"][-1]["output"] = altered["tasks"][0]["output"]
        with self.assertRaises(ValueError):
            smoke.check_tong_plan(altered, inputs, outputs)

    def test_final_manifest_is_failed_on_mutated_input(self):
        repo = self.base / "repo"
        (repo / "tests/data/compat").mkdir(parents=True)
        (repo / "examples").mkdir()
        shutil.copyfile(smoke.ROOT / "tests/data/compat/mini.bdf", repo / "tests/data/compat/mini.bdf")
        shutil.copyfile(smoke.ROOT / "examples/documentation-smoke-cases.json",
                        repo / "examples/documentation-smoke-cases.json")
        binary = self.base / "binary-never-executed"
        binary.write_bytes(b"Not a native binary; workflow is replaced for this recorder test.")
        snapshot = {"head": "test-source", "files": {str(p.relative_to(repo)): smoke.identity(p)
                                                    for p in repo.rglob("*") if p.is_file()}}
        manifest = self.base / "source.json"
        smoke.write_json(manifest, snapshot)
        evidence = self.base / "local-work/mutated-input"

        def fake_workflow(recorder, binary, contract, evidence, protected):
            original = evidence / "input.bin"
            original.write_bytes(b"original")
            protected[str(original)] = smoke.identity(original)
            original.write_bytes(b"changed")
            return {"cli_smokes": 0, "tiny_tong_tasks": 0}

        with mock.patch.object(smoke, "ROOT", repo), \
             mock.patch.object(smoke, "source_snapshot", return_value=snapshot), \
             mock.patch.object(smoke, "run_workflow", side_effect=fake_workflow):
            code = smoke.main(["--binary", str(binary), "--expected-binary-sha256",
                smoke.identity(binary)["sha256"], "--expected-source-commit", "test-source",
                "--source-manifest", str(manifest), "--evidence", str(evidence)])
        self.assertEqual(code, 1)
        result = json.loads((evidence / "result.json").read_text())
        self.assertEqual(result["status"], "failed")
        self.assertIn("input bytes changed", result["identity_error"]["message"])
        self.assertTrue((evidence / "inputs-after.json").is_file())

    def test_new_geometry_fixture_has_required_complete_glyph_records(self):
        records = smoke.fixture_glyphs(smoke.GEOMETRY_BDF)
        self.assertEqual(set(records), {0x4E00, 0x2500})
        for glyph in records.values():
            self.assertEqual(glyph["DWIDTH"], (8, 0))
            self.assertEqual(glyph["BBX"], (8, 8, 0, 0))
            self.assertEqual(glyph["BITMAP"], ["00", "00", "00", "FF", "00", "00", "00", "00"])

    def test_api_source_check_survives_formatter_call_layout(self):
        self.assertTrue(api_docs.source_fragment_present(
            "p3.apply(\n    &id,\n    options,\n)", "p3.apply(&id,"))
        self.assertTrue(api_docs.source_fragment_present(
            "pub fn\n    from_glyphs<T>(\n", "pub fn from_glyphs"))

    def test_api_source_check_preserves_identifiers_and_string_contents(self):
        self.assertFalse(api_docs.source_fragment_present(
            "p3.apply(\n    &identifier,\n)", "p3.apply(&id,"))
        self.assertFalse(api_docs.source_fragment_present(
            'resource: \n "plainoutputassetbytes"', 'resource: "plain output asset bytes"'))
        self.assertTrue(api_docs.source_fragment_present(
            'resource: \n "plain output asset bytes"', 'resource: "plain output asset bytes"'))


if __name__ == "__main__":
    unittest.main()
