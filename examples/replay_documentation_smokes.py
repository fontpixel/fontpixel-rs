#!/usr/bin/env python3
"""Record 18 real CLI smokes and 84 tiny Tong tasks using an already built CLI.

No Cargo, compilation, installation or performance measurement. Requires a new
evidence directory below sibling local-work and the coordinator's build-source
manifest: {"head": "...", "files": {"relative/path": {"bytes": N, "sha256": "..."}}}.
The manifest must include tracked and nonignored untracked files, including dirty
source, as recorded by git ls-files --cached --others --exclude-standard.
The original geometric fixture was lost; this runner generates a new CC0 fixture.
Tiny inputs are twenty copies of mini.bdf, not full Tong or distinct locl faces.
"""
from __future__ import annotations

import argparse
from collections import Counter
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MINI_SHA = "bff2ac191f67c2586e6a40c4635b26451986e8bd3f7fed39e61eae57bb3e618d"
COPYRIGHT = ("Tong Pixel Sans. Derived from Source Han Sans, Source Sans 3, "
             "Source Code Pro (Adobe), the Noto fonts (Google), Plangothic P1; "
             "Han bitmaps partly after TUMBLED (TsFreddie).")
LICENSE = "This Font Software is licensed under the SIL Open Font License, Version 1.1."
GEOMETRY_BDF = b"""STARTFONT 2.1
FONT -fontpixel-SmokeGeometry-Medium-R-Normal--8-80-75-75-C-80-ISO10646-1
SIZE 8 75 75
FONTBOUNDINGBOX 8 8 0 0
COMMENT New synthetic documentation smoke fixture; not the lost historical fixture.
STARTPROPERTIES 7
FAMILY_NAME "Smoke Geometry"
PIXEL_SIZE 8
FONT_ASCENT 8
FONT_DESCENT 0
SPACING "C"
COPYRIGHT "CC0-1.0; generated for fontpixel documentation testing"
LICENSE "CC0-1.0"
ENDPROPERTIES
CHARS 2
STARTCHAR uni4E00
ENCODING 19968
SWIDTH 1000 0
DWIDTH 8 0
BBX 8 8 0 0
BITMAP
00
00
00
FF
00
00
00
00
ENDCHAR
STARTCHAR uni2500
ENCODING 9472
SWIDTH 1000 0
DWIDTH 8 0
BBX 8 8 0 0
BITMAP
00
00
00
FF
00
00
00
00
ENDCHAR
ENDFONT
"""


def require(condition, message):
    if not condition:
        raise ValueError(message)


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def identity(path):
    with Path(path).open("rb") as stream:
        return {"bytes": os.fstat(stream.fileno()).st_size,
                "sha256": hashlib.file_digest(stream, "sha256").hexdigest()}


def write_json(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write("\n")


def source_snapshot(repo):
    def git(*args):
        return subprocess.check_output(["git", "-C", str(repo), *args])
    names = sorted(set(git("ls-files", "-z", "--cached", "--others",
                           "--exclude-standard").split(b"\0")) - {b""})
    files = {}
    for raw in names:
        name = os.fsdecode(raw)
        path = repo / name
        files[name] = identity(path) if path.is_file() else {"missing": True}
    return {"head": git("rev-parse", "HEAD").decode().strip(), "files": files}


def check_source(actual, expected):
    require(actual["head"] == expected["head"], "source HEAD differs from build manifest")
    require(actual["files"] == expected["files"],
            "source file identities differ from build manifest (including dirty/untracked source)")


def new_evidence(path, work):
    # Resolve parent symlinks before granting any output authority.
    require(not path.exists() and not path.is_symlink(), "evidence path already exists")
    path, work = path.resolve(), work.resolve()
    require(path != work and path.is_relative_to(work),
            "evidence must be a new directory below sibling local-work")
    require(not path.is_relative_to(ROOT), "evidence must not be inside source")
    path.mkdir(parents=True, exist_ok=False)
    return path


class Recorder:
    def __init__(self, evidence, repo=ROOT, timeout=180):
        self.evidence, self.repo, self.timeout = evidence, repo, timeout
        self.records = []
        self.env = dict(os.environ)
        self.overrides = {"TMPDIR": str(evidence / "temp"),
                          "PYTHONDONTWRITEBYTECODE": "1"}
        self.env.update(self.overrides)

    def run(self, name, argv, expected_exit=0):
        require(re.fullmatch(r"[a-zA-Z0-9_-]+", name), "invalid command record name")
        directory = self.evidence / "commands" / name
        directory.mkdir(parents=True, exist_ok=False)
        argv = list(map(str, argv))
        record = {"name": name, "argv": argv, "cwd": str(self.repo),
                  "environment_overrides": self.overrides, "started_utc": utc(),
                  "expected_exit": expected_exit, "status": "running"}
        self.records.append(record)
        write_json(directory / "command.json", record)
        started = time.monotonic()
        stdout, stderr = directory / "stdout.log", directory / "stderr.log"
        try:
            with stdout.open("xb") as out, stderr.open("xb") as err:
                result = subprocess.run(argv, cwd=self.repo, env=self.env, stdout=out,
                                        stderr=err, timeout=self.timeout, check=False)
            record["exit_code"] = result.returncode
            require(result.returncode == expected_exit,
                    f"{name}: expected exit {expected_exit}, got {result.returncode}")
            record["status"] = "expected_exit"
        except BaseException as error:
            record["status"] = "timeout" if isinstance(error, subprocess.TimeoutExpired) else "failed"
            record["error"] = {"type": type(error).__name__, "message": str(error)}
            raise
        finally:
            record.update(finished_utc=utc(), elapsed_seconds=time.monotonic() - started)
            for label, path in [("stdout", stdout), ("stderr", stderr)]:
                if path.exists():
                    record[label] = {"path": str(path.relative_to(self.evidence)), **identity(path)}
            write_json(directory / "result.json", record)
        return stdout

    def json(self, name, argv):
        return json.loads(self.run(name, argv).read_text())


def rebase_batch(original, mini, output):
    rebased = original.replace("../tests/data/compat/mini.bdf", str(mini))
    rebased = rebased.replace("../../local-work/docs-example-output/", str(output) + "/")
    before, after = tomllib.loads(original), tomllib.loads(rebased)
    require(before.keys() == after.keys(), "batch top-level keys changed")
    require(len(before["tasks"]) == len(after["tasks"]) == 3, "batch must contain three tasks")
    for a, b in zip(before["tasks"], after["tasks"]):
        require({k: v for k, v in a.items() if k not in ("inputs", "output")} ==
                {k: v for k, v in b.items() if k not in ("inputs", "output")},
                "batch changed more than paths")
        require(b["inputs"] == [str(mini)], "unexpected batch input")
        if "output" in b:
            require(Path(b["output"]).parent == output, "batch output escapes evidence")
    return rebased


def check_batch(data, tasks, committed):
    require(data["success"] and data["exit_code"] == 0, "batch failed")
    require(data["summary"] == {"total": len(tasks), "succeeded": len(tasks),
                                "failed": 0, "committed": committed}, "wrong batch summary")
    require(data["jobs"] == data["workers"] == 2, "expected two requested/actual workers")
    require([r["id"] for r in data["tasks"]] == [t["id"] for t in tasks], "batch task order changed")
    for row, task in zip(data["tasks"], tasks):
        require(row["success"] and row["exit_code"] == 0, "task failed")
        require(row["committed"] == ("output" in task), "wrong task commit state")
        require(not row["diagnostics_truncated"], "successful task diagnostics were truncated")
        require(isinstance(row["result"], dict), "task result must be an object")


def check_tong_plan(job, inputs, outputs):
    require(job["version"] == 1 and job["jobs"] == 2, "wrong Tong job header")
    tasks = job["tasks"]
    require(len(tasks) == 84, "expected 84 tiny Tong tasks")
    expected = []
    for size in ["Large", "Small"]:
        for spacing in ["", "Mono"]:
            stem = f"TongPixelSans{spacing}14{size}"
            for region in ["SC", "TC", "JP", "KR", "Latin"]:
                for kind in ["bdf", "pcf", "otb"]:
                    expected.append({"id": f"{stem}-{region}-{kind}", "kind": "convert",
                                     "inputs": [str(inputs / f"{stem}-{region}.bdf")],
                                     "output": str(outputs / kind / f"{stem}-{region}.{kind}"),
                                     "from": "bdf", "to": kind})
            for shape in ["square", "dot"]:
                for kind in ["ttf", "otf", "woff2"]:
                    expected.append({"id": f"{stem}-{shape}-{kind}", "kind": "variants",
                        "base": str(inputs / f"{stem}-SC.bdf"),
                        "family": f"Tong Pixel Sans{' Mono' if spacing else ''} 14 {size}",
                        "languages": {"ZHT,ZHH": str(inputs / f"{stem}-TC.bdf"),
                                      "ja": str(inputs / f"{stem}-JP.bdf"),
                                      "ko": str(inputs / f"{stem}-KR.bdf")},
                        "scripts": {"latn,grek,cyrl": str(inputs / f"{stem}-Latin.bdf")},
                        "output": str(outputs / kind / f"{stem}-{shape.capitalize()}.{kind}"),
                        "to": kind,
                        "options": {"shape": shape, "dotsize": 0.9 if shape == "dot" else 1.0,
                                    "ppu": 100, "upm": 1400, "style": shape.capitalize(),
                                    "font_version": "2026.10.02", "vendor": "TONG",
                                    "copyright": COPYRIGHT, "license": LICENSE,
                                    "license_url": "https://openfontlicense.org", "dot_reuse": "patterns",
                                    "woff2_source": "auto", "woff2_quality": 11, "woff2_threads": 1}})
    require(tasks == expected, "Tong task paths/order/options differ from the documented contract")
    require(len({t["id"] for t in tasks}) == len({t["output"] for t in tasks}) == 84,
            "Tong task ID/output collision")
    return tasks


def check_signature(path):
    signatures = {".png": b"\x89PNG\r\n\x1a\n", ".ttf": b"\0\1\0\0",
                  ".otb": b"\0\1\0\0", ".otf": b"OTTO", ".woff2": b"wOF2",
                  ".pcf": b"\x01fcp", ".bdf": b"STARTFONT"}
    require(path.is_file() and not path.is_symlink() and path.stat().st_size > 0,
            f"missing, empty or symlink artifact: {path}")
    if path.suffix in signatures:
        with path.open("rb") as stream:
            require(stream.read(len(signatures[path.suffix])) == signatures[path.suffix],
                    f"wrong artifact signature: {path}")


def fixture_glyphs(data):
    """Read only the simple BDF records used by our two deterministic fixtures."""
    result = {}
    for block in data.decode("ascii").split("STARTCHAR ")[1:]:
        fields, rows = {}, []
        bitmap = False
        for line in block.splitlines()[1:]:
            if line == "ENDCHAR":
                break
            if bitmap:
                rows.append(line)
            elif line == "BITMAP":
                bitmap = True
            elif line.startswith(("ENCODING ", "DWIDTH ", "BBX ")):
                key, values = line.split(" ", 1)
                fields[key] = tuple(map(int, values.split()))
        cp, = fields.pop("ENCODING")
        require(cp not in result, "unexpected duplicate fixture mapping")
        result[cp] = {**fields, "BITMAP": rows}
    require(result, "fixture contains no glyphs")
    return result


def run_workflow(recorder, binary, contract, evidence, protected):
    out, inputs, outputs = (evidence / n for n in ["cli", "tiny-inputs", "tiny-output"])
    for directory in [out, out / "each", inputs, outputs, evidence / "example-output"]:
        directory.mkdir()
    for kind in ["bdf", "pcf", "otb", "ttf", "otf", "woff2"]:
        (outputs / kind).mkdir()
    mini = inputs / "mini.bdf"
    shutil.copyfile(ROOT / "tests/data/compat/mini.bdf", mini)
    for name in ["first.bdf", "second.bdf"]:
        shutil.copyfile(mini, out / name)
    geometry = out / "han-geometry.bdf"
    geometry.write_bytes(GEOMETRY_BDF)
    write_json(evidence / "fixture-provenance.json", {
        "mini": {"source": "tests/data/compat/mini.bdf", **identity(mini)},
        "geometry": {"origin": contract["geometry_provenance"], "license": "CC0-1.0",
                     "codepoints": [19968, 9472], "advances": [8, 8], **identity(geometry)}})
    for size in ["Large", "Small"]:
        for spacing in ["", "Mono"]:
            for region in ["SC", "TC", "JP", "KR", "Latin"]:
                shutil.copyfile(mini, inputs / f"TongPixelSans{spacing}14{size}-{region}.bdf")
    for path in [geometry, out / "first.bdf", out / "second.bdf", *inputs.iterdir()]:
        protected[str(path)] = identity(path)
    write_json(evidence / "inputs-before.json", protected)
    batch = evidence / "batch-example.toml"
    batch.write_text(rebase_batch((ROOT / "examples/batch.toml").read_text(), mini,
                                 evidence / "example-output"))
    paths = {"mini": str(mini), "out": str(out), "rebased_batch_toml": str(batch)}
    cases = contract["cases"]
    require(len(cases) == 18 and len({c["id"] for c in cases}) == 18, "expected 18 unique cases")
    for index, case in enumerate(cases, 1):
        argv = [arg.format_map(paths) for arg in case["argv"]]
        stdout = recorder.run("cli-" + case["id"], [binary, *argv], case["expected_exit"])
        data = None
        if "--json" in argv or ("--report" in argv and "json" in argv) or argv[0] == "batch":
            data = json.loads(stdout.read_text())
        if index == 1:
            ids = [name for line in (ROOT / "FORMATS.md").read_text().splitlines()
                   if line.startswith("| `") for name in re.findall(r"`([^`]+)`", line.split("|")[1])]
            require(len(data["formats"]) == len(set(ids)) == 99, "expected 99 format IDs")
            require({r["format"]["id"] for r in data["formats"]} == set(ids), "format IDs changed")
            require(all(r["read"] and r["write"] for r in data["formats"]), "CLI needs all format features")
        elif index == 2:
            require(data["font_count"] == 1 and data["fonts"][0]["glyph_count"] == 3,
                    "mini font info changed")
        elif index == 3:
            require(b"#" in stdout.read_bytes(), "preview has no ink")
        elif index == 4:
            pixels = (out / "a.png").read_bytes()
            width, height = struct.unpack(">II", pixels[16:24])
            require(width > 0 and height > 0 and width % 4 == height % 4 == 0, "invalid 4x PNG dimensions")
        elif index == 9:
            require(data["equal"] and data["difference_count"] == 0 and not data["truncated"],
                    "equal diff reported differences")
        elif index == 10:
            require(data["valid"], "generated TTF validation failed")
        elif index == 12:
            require(data["merge"]["family"] == "Example Mix", "merge family changed")
        elif index == 13:
            check_batch(data, tomllib.loads(batch.read_text())["tasks"], 2)
        elif index == 18:
            require(data["merge"]["geometric_width"] == "full", "geometric policy changed")
            require(data["merge"]["full_width"] == 8, "new fixture must establish full width 8")
        write_json(evidence / f"case-{index:02d}-checks.json", {"case": case["id"], "status": "pass"})
    for name in ["a.png", "a.yaff", "a.ttf", "regional.ttf", "subset.woff2", "cp1252.bdf",
                 "mix.yaff", "two.yaff", "selected.bdf", "single-thread.woff2", "geometric.yaff",
                 "each/first.pcf", "each/second.pcf"]:
        check_signature(out / name)
    for task in tomllib.loads(batch.read_text())["tasks"]:
        if "output" in task:
            check_signature(Path(task["output"]))
    # Extra artifact checks are recorded separately, not counted as the 18 cases.
    for label, target in [("yaff", out / "a.yaff"), ("selected", out / "selected.bdf")]:
        equal = recorder.json("verify-" + label, [binary, "diff", mini, target, "--json"])
        require(equal["equal"] and equal["difference_count"] == 0 and not equal["truncated"],
                f"{label} changed mini glyph/model data")
    two = recorder.json("verify-two-strikes", [binary, "info", out / "two.yaff", "--json"])
    require(two["font_count"] == 2 and all(f["glyph_count"] == 3 for f in two["fonts"]),
            "multi-input conversion lost a strike")
    geometry_readback = out / "geometry-readback.bdf"
    recorder.run("verify-geometry-readback", [binary, "convert", out / "geometric.yaff",
                                            "-o", geometry_readback, "--report", "json"])
    require(fixture_glyphs(geometry_readback.read_bytes()) == fixture_glyphs(GEOMETRY_BDF),
            "geometric-width=full changed fixture mappings, advances, bounds or pixels")
    job = evidence / "tong-mini.toml"
    recorder.run("tong-generate", [sys.executable, "-B", ROOT / "examples/tong_batch.py",
                 "--bdf-dir", inputs, "--out", outputs, "--version", "2026.10.02",
                 "--jobs", "2", "--write", job])
    tasks = check_tong_plan(tomllib.loads(job.read_text()), inputs, outputs)
    data = recorder.json("tong-mini-batch", [binary, "batch", job, "--jobs", "2"])
    check_batch(data, tasks, 84)
    for task in tasks:
        check_signature(Path(task["output"]))
    require({str(p) for p in outputs.rglob("*") if p.is_file()} == {t["output"] for t in tasks},
            "unexpected or missing tiny output artifacts")
    # Each subprocess, including every help query, has its own retained record.
    recorder.run("documentation-source-audit", [sys.executable, "-B", ROOT / "examples/check_documentation.py"])
    recorder.run("api-source-audit", [sys.executable, "-B", ROOT / "examples/check_api_documentation.py",
                                    "--require-rustdoc-link"])
    helptext = recorder.run("cli-root-help", [binary, "--help"]).read_text()
    commands = {case["argv"][0] for case in cases}
    require(len(commands) == 13, "expected 13 command spellings")
    for command in sorted(commands):
        require(re.search(rf"^  {command}\s", helptext, re.M) or
                (command == "vectorize" and "vectorize" in helptext), f"root help omits {command}")
        recorder.run("help-" + command, [binary, command, "--help"])
    return {"cli_smokes": 18, "tiny_tong_tasks": 84, "tiny_formats": dict(Counter(t["to"] for t in tasks)),
            "command_spellings": 13, "static_checks": "source/lexical only"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--expected-binary-sha256", required=True)
    parser.add_argument("--expected-source-commit", required=True)
    parser.add_argument("--source-manifest", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=180, help="per-command seconds; default 180")
    args = parser.parse_args(argv)
    require(args.timeout > 0, "timeout must be positive")
    evidence = new_evidence(args.evidence, ROOT.parent / "local-work")
    (evidence / "temp").mkdir()
    recorder = Recorder(evidence, timeout=args.timeout)
    result = {"schema": 1, "status": "failed", "started_utc": utc(), "native_smokes": "not completed",
              "binary": str(args.binary.resolve()), "commands": recorder.records,
              "limitations": ["No full Tong/catalogue correctness or performance acceptance.",
                              "Identical tiny region inputs do not establish locl equivalence.",
                              "Small reports do not test the >=1MiB spool.",
                              "Success-path smokes do not replace negative/resource/transaction tests."]}
    write_json(evidence / "planned-run.json", {**result, "arguments": vars(args) | {
        k: str(v) for k, v in vars(args).items() if isinstance(v, Path)}})
    protected = {}
    expected = None
    try:
        binary = args.binary.resolve(strict=True)
        result["binary_identity"] = identity(binary)
        require(result["binary_identity"]["sha256"] == args.expected_binary_sha256, "binary SHA mismatch")
        expected = json.loads(args.source_manifest.read_text())
        require(expected["head"] == args.expected_source_commit, "manifest source commit mismatch")
        before = source_snapshot(ROOT)
        write_json(evidence / "source-before.json", before)
        check_source(before, expected)
        shutil.copyfile(args.source_manifest, evidence / "build-source-manifest.json")
        mini = ROOT / "tests/data/compat/mini.bdf"
        require(identity(mini)["sha256"] == MINI_SHA, "mini fixture SHA mismatch")
        contract = json.loads((ROOT / "examples/documentation-smoke-cases.json").read_text())
        require(contract["schema"] == 1 and contract["mini_sha256"] == MINI_SHA, "wrong smoke contract")
        for path in [mini, ROOT.parent / "reference/fontpixel/ingest/manifest.toml"]:
            if path.is_file():
                protected[str(path)] = identity(path)
        result.update(run_workflow(recorder, binary, contract, evidence, protected))
        require(not list((evidence / "temp").iterdir()), "CLI temporary files remain")
        result.update(status="pass", native_smokes="completed")
    except BaseException as error:
        result["error"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        try:
            after = source_snapshot(ROOT)
            write_json(evidence / "source-after.json", after)
            require(expected is not None, "build-source manifest was not established")
            check_source(after, expected)
            require(identity(args.binary.resolve())["sha256"] == args.expected_binary_sha256,
                    "binary changed during run")
            checked = {path: identity(path) for path in protected}
            write_json(evidence / "inputs-after.json", checked)
            require(checked == protected, "input bytes changed during run")
        except BaseException as error:
            result["status"] = "failed"
            result["identity_error"] = {"type": type(error).__name__, "message": str(error)}
        result["finished_utc"] = utc()
        result["artifacts"] = {str(p.relative_to(evidence)): identity(p)
                               for p in sorted(evidence.rglob("*")) if p.is_file()}
        write_json(evidence / "result.json", result)
    print(json.dumps({"status": result["status"], "evidence": str(evidence)}, ensure_ascii=False))
    return 0 if result["status"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
