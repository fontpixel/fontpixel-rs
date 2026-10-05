#!/usr/bin/env python3
"""Default-limit workload sweep for one fontpixel binary (stdlib only).

Every case runs the binary with its DEFAULT limits (no budget option) in a
fresh output directory and records exit status, wall time and the peak RSS of
the child (``wait4`` ``ru_maxrss``). Cases:

* the full Tong SC BDF converted to every writable format that
  ``fontpixel formats --json`` lists;
* full Tong square TTF and OTF converted to WOFF and WOFF2;
* ``subset`` of a full Tong TTF to WOFF2 (ASCII and every mapped scalar);
* the Unifont 13 plus Tong Latin merge to TTF and WOFF2;
* one regional ``variants`` family to TTF and WOFF2.

A non-zero exit whose message is not a resource limit is classified as a
format rejection (the reason is recorded). A resource-limit failure is rerun
with ``--max-total-bytes`` = ``--probe-budget`` (1 GiB by default): if that
run's peak RSS exceeds the 256 MiB default cap the failure is "genuine",
otherwise the ledger over-counts. With ``--compare BIN`` every case is also
run by BIN with ``--max-total-bytes`` = ``--compare-budget`` (commands
without that option run at BIN's defaults) and the output trees are compared
byte for byte.

Writes ``sweep.json`` and ``sweep.md`` into ``--output`` (which must be empty
or absent). Example:

  python3 scripts/phase6_default_sweep.py --binary target/release/fontpixel \\
      --compare ../frozen/fontpixel --output ../local-work/sweep-new
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
# The workspace holding samples/ and local-work/ (also from a worktree).
WORK = next((d for d in ROOT.parents if (d / "samples").is_dir()), ROOT.parent)
DEFAULT_CAP = 256 * 1024 * 1024
LIMIT_MARKERS = ("exceeds configured limit", "ResourceLimit", "limit exceeded")
BUDGET_COMMANDS = {"convert", "vectorize", "merge", "variants"}


def run_measured(command: list[str], cwd: Path) -> dict:
    """Run one child in ``cwd``, reaping it with ``wait4`` to read its own peak
    RSS; return exit, wall seconds, peak RSS bytes and the stderr tail."""
    cwd.mkdir(parents=True, exist_ok=True)
    start = time.perf_counter()
    with open(os.devnull, "wb") as sink:
        err_path = cwd / ".stderr"
        with open(err_path, "wb") as err:
            child = subprocess.Popen([str(c) for c in command], cwd=cwd, stdout=sink, stderr=err)
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
    wall = time.perf_counter() - start
    text = err_path.read_text("utf-8", "replace").strip().splitlines()
    err_path.unlink()
    return {"exit": child.returncode, "wall_s": round(wall, 3),
            "peak_rss_bytes": usage.ru_maxrss * 1024,
            "stderr_tail": [line for line in text if not line.startswith("Warning ")][-3:]}


def tree(path: Path) -> dict[str, bytes]:
    return {str(p.relative_to(path)): p.read_bytes() for p in sorted(path.rglob("*")) if p.is_file()}


def with_budget(command: list[str], budget: int) -> list[str]:
    return command + ["--max-total-bytes", str(budget)] if command[0] in BUDGET_COMMANDS else command


def writable_formats(binary: Path) -> list[tuple[str, str]]:
    listing = json.loads(subprocess.run([str(binary), "formats", "--json"], check=True,
                                        capture_output=True).stdout)
    formats = []
    for entry in listing["formats"]:
        info = entry["format"]
        if entry["write"] and info["feature_enabled"]:
            extensions = [e for e in info["extensions"] if not e.endswith(".gz")]
            formats.append((info["id"], extensions[0] if extensions else info["id"]))
    return formats


def cases(a: argparse.Namespace) -> list[tuple[str, list[str]]]:
    out = []
    for fid, ext in writable_formats(a.binary):
        out.append((f"sc-bdf.{fid}", ["convert", a.sc_bdf, "--to", fid, "-o", f"sc.{ext}"]))
    for label, source in [("tong-ttf", a.tong_ttf), ("tong-otf", a.tong_otf)]:
        for kind in ["woff", "woff2"]:
            out.append((f"{label}.{kind}", ["convert", source, "-o", f"font.{kind}"]))
    for label, ranges in [("ascii", "U+0020-007E"), ("all-mapped", "U+0000-D7FF,U+E000-10FFFF")]:
        out.append((f"subset.tong-ttf.{label}.woff2", ["subset", a.tong_ttf, "--unicodes", ranges,
                                                       "-o", "subset.woff2"]))
    for kind in ["ttf", "woff2"]:
        out.append((f"merge.unifont-latin.{kind}", ["merge", "--family", "Unifont Western Mix",
                    "--part", a.latin_bdf, "--take", "western", "--part", a.unifont, "--take", "all",
                    "--spacing", "prop", "-o", f"merged.{kind}"]))
        samples = a.samples
        out.append((f"variants.tong-large.{kind}", ["variants", "--base", samples / "TongPixelSans14Large-SC.bdf",
                    "--lang", f"zh-Hant,zh-HK={samples / 'TongPixelSans14Large-TC.bdf'}",
                    "--lang", f"ja={samples / 'TongPixelSans14Large-JP.bdf'}",
                    "--lang", f"ko={samples / 'TongPixelSans14Large-KR.bdf'}",
                    "--script", f"latn,grek,cyrl={samples / 'TongPixelSans14Large-Latin.bdf'}",
                    "--family", "Tong Pixel Sans 14 Large", "-o", f"family.{kind}"]))
    return [(name, [str(c) for c in command]) for name, command in out]


def classify(result: dict) -> str:
    if result["exit"] == 0:
        return "ok"
    message = " ".join(result["stderr_tail"])
    return "resource-limit" if any(m in message for m in LIMIT_MARKERS) else "rejected"


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--compare", type=Path, help="second binary, run with --compare-budget")
    p.add_argument("--compare-budget", type=int, default=1 << 30)
    p.add_argument("--probe-budget", type=int, default=1 << 30,
                   help="budget for rerunning resource-limit failures")
    p.add_argument("--samples", type=Path, default=WORK / "samples/tong-pixel-sans")
    p.add_argument("--sc-bdf", type=Path)
    p.add_argument("--latin-bdf", type=Path)
    p.add_argument("--tong-ttf", type=Path,
                   default=WORK / "local-work/phase6-local-20261004/full-correctness-v2/vector-native/font.ttf")
    p.add_argument("--tong-otf", type=Path,
                   default=WORK / "local-work/strict-c/tong-batch-base/export/otf/TongPixelSans14Large-square.otf")
    p.add_argument("--unifont", type=Path,
                   default=WORK / "local-work/phase6-local-20261004/performance-work/reference/bdfparser/tests/fonts/unifont-13.0.04.bdf")
    p.add_argument("--only", action="append", default=[], help="run only cases whose id contains this text")
    a = p.parse_args()
    a.binary = a.binary.resolve()
    a.sc_bdf = (a.sc_bdf or a.samples / "TongPixelSans14Large-SC.bdf").resolve()
    a.latin_bdf = (a.latin_bdf or a.samples / "TongPixelSans14Large-Latin.bdf").resolve()
    for path in [a.binary, a.sc_bdf, a.latin_bdf, a.tong_ttf, a.tong_otf, a.unifont] + ([a.compare] if a.compare else []):
        if not Path(path).exists():
            p.error(f"missing input: {path}")
    if a.output.exists() and any(a.output.iterdir()):
        p.error("--output must be empty or absent")
    a.output.mkdir(parents=True, exist_ok=True)
    results = []
    for name, command in cases(a):
        if a.only and not any(o in name for o in a.only):
            continue
        record = {"case": name, "command": command}
        record.update(run_measured([a.binary] + command, a.output / "default" / name))
        record["status"] = classify(record)
        if record["status"] == "resource-limit":
            probe = run_measured([a.binary] + with_budget(command, a.probe_budget), a.output / "probe" / name)
            genuine = probe["exit"] == 0 and probe["peak_rss_bytes"] > DEFAULT_CAP
            record["probe"] = probe
            record["verdict"] = ("genuine: peak RSS with a larger budget exceeds the 256 MiB default cap"
                                 if genuine else
                                 "ledger over-count: succeeds with a larger budget below the 256 MiB cap"
                                 if probe["exit"] == 0 else "fails even with the probe budget")
        if a.compare and record["exit"] == 0:
            other = run_measured([a.compare.resolve()] + with_budget(command, a.compare_budget),
                                 a.output / "compare" / name)
            record["compare"] = {k: other[k] for k in ("exit", "wall_s", "peak_rss_bytes", "stderr_tail")}
            record["identical"] = other["exit"] == 0 and tree(a.output / "default" / name) == tree(a.output / "compare" / name)
        results.append(record)
        rss = record["peak_rss_bytes"] / 2**20
        print(f"{name}: exit {record['exit']} {record['wall_s']}s {rss:.1f} MiB {record['status']}"
              + (f" identical={record['identical']}" if "identical" in record else "")
              + (f" {record['stderr_tail'][-1:]}" if record["exit"] else ""), flush=True)
    summary = {"binary": str(a.binary), "compare": str(a.compare) if a.compare else None,
               "compare_budget": a.compare_budget if a.compare else None, "default_cap": DEFAULT_CAP,
               "cases": results,
               "nonzero_at_defaults": [r["case"] for r in results if r["exit"] != 0],
               "resource_limit_failures": [r["case"] for r in results if r["status"] == "resource-limit"],
               "compare_mismatches": [r["case"] for r in results if r.get("identical") is False]}
    (a.output / "sweep.json").write_text(json.dumps(summary, indent=2) + "\n")
    lines = ["| case | exit | wall s | peak RSS MiB | status | identical | note |",
             "| --- | ---: | ---: | ---: | --- | --- | --- |"]
    for r in results:
        note = r.get("verdict") or (" ".join(r["stderr_tail"])[-160:] if r["exit"] else "")
        lines.append(f"| {r['case']} | {r['exit']} | {r['wall_s']:.2f} | {r['peak_rss_bytes'] / 2**20:.1f} | "
                     f"{r['status']} | {r.get('identical', '')} | {note.replace('|', '/')} |")
    (a.output / "sweep.md").write_text("\n".join(lines) + "\n")
    print(json.dumps({k: summary[k] for k in ("nonzero_at_defaults", "resource_limit_failures", "compare_mismatches")}))


if __name__ == "__main__":
    main()
