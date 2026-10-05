#!/usr/bin/env python3
"""PLAN 9.6 phase-6 ASAN cargo-fuzz campaign runner (stdlib only).

Subcommands
  inventory  derive targets / reader IDs / feature profiles / gaps from source
  run        build every target with ASAN, then fuzz each one in its own
             libFuzzer process on a private copy of its seed corpus
  summary    re-evaluate a finished run label (pass = exit 0, no crash/timeout/
             oom/leak artifacts, elapsed >= requested seconds, final stats seen)

Results are written under <out-root>/<label>/ and an existing label is never
overwritten. The committed/ignored fuzz/corpus tree is only read, never written.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import glob
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FUZZ = REPO / "fuzz"
LOCAL_WORK = Path(os.environ.get("FONTPIXEL_WORK_ROOT", REPO.parent / "local-work"))
DEFAULT_OUT_ROOT = LOCAL_WORK / "phase6-local-20261004" / "fuzz-final"
DEFAULT_TARGET_DIR = LOCAL_WORK / "target-fuzz-asan"
CATALOGUE = REPO / "crates" / "fontpixel" / "src" / "formats" / "catalogue.rs"
PHASE5_INDEX = REPO / "reports" / "phase5-validation" / "long-fuzz-index.json"
TRIPLE = "x86_64-unknown-linux-gnu"

# Feature profile per target (fuzz/Cargo.toml [features] names). Verified by the
# per-target ASAN build: a profile that is too narrow fails to compile.
PROFILES = {
    "bdf": "baseline", "pcf": "baseline", "yaff": "baseline", "sfnt": "baseline",
    "glyf": "baseline", "compat_ttf": "baseline", "cff": "baseline",
    "embedded_bitmap": "embedded-only", "color_bitmap": "color-only",
    "woff1": "baseline", "woff2": "baseline", "gsub": "baseline", "subset": "baseline",
    "vector_raster": "baseline", "collection": "baseline", "svg": "baseline",
    "hex": "baseline", "psf": "baseline", "fnt": "baseline", "fon": "fon-only",
    "image": "image-c", "c": "c-only", "imageset_bitmap": "imageset-only",
    "transport": "transport-only",
    "rockbox_bitmap": "rockbox-only", "u8m_bitmap": "u8m-only",
    "cybiko_bitmap": "cybiko-only", "playdate_bitmap": "playdate-only",
    "hmzk_bitmap": "hmzk-only", "sbf_bitmap": "sbf-only",
    "pilfont_bitmap": "pilfont-only", "sfont_bitmap": "sfont-only",
    "vlw_bitmap": "vlw-only",
}
for _t in (
    "kbitx pbf hbf draw raw cpi pf2 fontx2 hzk nfnt bmfont sfd kbits pk gf plan9 amiga "
    "gem snf vfont fzx figlet hanfont iigs drcs palm gdr dosstart edwin mkwinfont psf2txt "
    "pcr plus3dos drhalo writeon daisy polyprint printshop tasprint pxl signum fbit lisa "
    "mgtk qpf qpf2 beos oberon symbos wsfont bbc wyse grasp hrcg xbin consoleet zapfont "
    "comfont vtfont geos gpi pcgeos ssfn pfr catalyst chiwriter dashen hppcl prebuilt "
    "riscos xerox"
).split():
    PROFILES[_t] = f"{_t}-only"

# Registry reader IDs reached by each target (reviewed against the harness
# source). "primary" = the target drives that reader's own entry point;
# "component" = it fuzzes a parser layer used by that reader.
_SUFFIX_READER = {
    "rockbox_bitmap": "rockbox", "u8m_bitmap": "u8m", "cybiko_bitmap": "cybiko",
    "playdate_bitmap": "playdate", "hmzk_bitmap": "hmzk", "sbf_bitmap": "sbf",
    "pilfont_bitmap": "pilfont", "sfont_bitmap": "sfont", "vlw_bitmap": "vlw",
    "color_bitmap": "color",
}
READERS: dict[str, dict[str, list[str]]] = {
    "embedded_bitmap": {"primary": ["otb", "bdat"], "component": []},
    "vector_raster": {"primary": ["vector-raster", "ttf", "otf", "woff", "woff2", "ttc"], "component": []},
    "sfnt": {"primary": [], "component": ["ttf", "otf"]},
    "glyf": {"primary": [], "component": ["ttf"]},
    "cff": {"primary": [], "component": ["otf"]},
    "woff1": {"primary": ["woff"], "component": []},
    "woff2": {"primary": ["woff2"], "component": []},
    "collection": {"primary": ["ttc"], "component": ["woff", "woff2"]},
    "compat_ttf": {"primary": [], "component": ["ttf"]},
    "gsub": {"primary": [], "component": ["ttf", "otf"]},
    "subset": {"primary": [], "component": ["ttf", "otf"]},
    "imageset_bitmap": {"primary": ["image"], "component": []},
    "transport": {"primary": ["raw", "bdf", "ssfn"], "component": []},
    "fon": {"primary": ["fon"], "component": ["fnt"]},
}
for _t in PROFILES:
    READERS.setdefault(_t, {"primary": [_SUFFIX_READER.get(_t, _t)], "component": []})

# Pure-copy seed fallbacks for targets that have no fuzz/corpus/<target> dir.
FALLBACK_SEEDS = {
    "bdf": ["tests/data/bdf/*.bdf"],
    "pcf": ["tests/data/pcf/*.pcf*"],
    "yaff": ["tests/data/yaff/*.yaff"],
    "sfnt": ["tests/data/sfnt/*.cmap", "tests/data/compat/*.ttf", "tests/data/embedded-bitmap/*.ttf"],
    "glyf": ["tests/data/compat/*.ttf"],
    "compat_ttf": ["tests/data/compat/*.ttf"],
    "cff": ["tests/data/cff/*.cff"],
    "gsub": ["tests/data/compat/*.ttf"],
    "subset": ["tests/data/compat/*.ttf"],
    "vector_raster": ["tests/data/compat/*.ttf", "tests/data/embedded-bitmap/*.ttf"],
    "collection": ["tests/data/compat/*.ttf"],
    "woff1": ["tests/data/compat/*.ttf"],
    "woff2": ["tests/data/compat/*.ttf"],
}

FAIL_ARTIFACT_PREFIXES = ("crash-", "timeout-", "oom-", "leak-")
DEFAULT_MAX_LEN = 131072


# ----------------------------------------------------------------- helpers
def now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def run_text(argv, cwd=REPO, env=None) -> str:
    return subprocess.run(argv, cwd=cwd, env=env, check=True, capture_output=True, text=True).stdout


def clean_env() -> dict:
    env = dict(os.environ)
    env.pop("RUSTC_BOOTSTRAP", None)
    env.setdefault("CARGO_INCREMENTAL", "0")
    return env


def source_snapshot() -> dict:
    """Source identity: HEAD + sha256(git diff --binary HEAD) + untracked hashes."""
    head = run_text(["git", "rev-parse", "HEAD"]).strip()
    diff = subprocess.run(["git", "diff", "--binary", "HEAD"], cwd=REPO, check=True, capture_output=True).stdout
    untracked = {}
    out = subprocess.run(["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=REPO,
                         check=True, capture_output=True).stdout
    for name in sorted(x for x in out.decode().split("\0") if x):
        p = REPO / name
        if p.is_file():
            untracked[name] = sha256_file(p)
    dirty = sorted(x[3:] for x in run_text(["git", "status", "--porcelain=v1", "--untracked-files=no"]).splitlines())
    ident = hashlib.sha256(json.dumps([head, hashlib.sha256(diff).hexdigest(), untracked],
                                      sort_keys=True).encode()).hexdigest()
    return {
        "git_head": head,
        "diff_binary_sha256": hashlib.sha256(diff).hexdigest(),
        "diff_binary_bytes": len(diff),
        "modified_tracked_paths": dirty,
        "untracked_sha256": untracked,
        "fuzz_cargo_lock_sha256": sha256_file(FUZZ / "Cargo.lock"),
        "identity_sha256": ident,
    }


def toolchain_info() -> dict:
    env = clean_env()
    info = {}
    for key, argv in {
        "rustc_nightly": ["rustc", "+nightly", "-vV"],
        "cargo_nightly": ["cargo", "+nightly", "-V"],
        "cargo_fuzz": ["cargo", "+nightly", "fuzz", "--version"],
    }.items():
        try:
            info[key] = run_text(argv, cwd=FUZZ, env=env).strip()
        except (subprocess.CalledProcessError, FileNotFoundError) as e:
            info[key] = f"unavailable: {e}"
    info["RUSTC_BOOTSTRAP_set"] = "RUSTC_BOOTSTRAP" in os.environ
    return info


# ----------------------------------------------------------------- inventory
def fuzz_bins() -> list[tuple[str, str]]:
    c = tomllib.loads((FUZZ / "Cargo.toml").read_text())
    return [(b["name"], b["path"]) for b in c.get("bin", [])]


def fuzz_features() -> dict:
    return tomllib.loads((FUZZ / "Cargo.toml").read_text()).get("features", {})


def catalogue_ids() -> list[str]:
    return re.findall(r'\bid:\s*"([a-z0-9-]+)"', CATALOGUE.read_text())


def harness_source(path: str) -> tuple[str, list[str]]:
    p = FUZZ / path
    src = p.read_text()
    helpers = []
    for h in re.findall(r'#\[path\s*=\s*"([^"]+)"\]', src):
        hp = (p.parent / h).resolve()
        helpers.append(str(hp.relative_to(FUZZ)))
        src += "\n" + hp.read_text()
    return src, helpers


def referenced_format_modules(src: str) -> set[str]:
    flat = re.sub(r"\s+", " ", src)
    mods = set(re.findall(r"formats::([a-z0-9_]+)", flat))
    for grp in re.findall(r"formats::\{([^}]*)\}", flat):
        for item in grp.split(","):
            item = item.strip().split("::")[0].split(" ")[0]
            if item and item[0].islower():
                mods.add(item)
    return mods


def source_input_limit(src: str) -> int | None:
    vals = []
    for expr in re.findall(r"max_input_bytes\s*[:=]\s*([0-9_][0-9_\s\*]*)", src):
        try:
            n = 1
            for part in expr.replace("_", "").split("*"):
                if part.strip():
                    n *= int(part.strip())
            vals.append(n)
        except ValueError:
            pass
    return max(vals) if vals else None


def phase5_max_len() -> dict[str, int]:
    out = {}
    if not PHASE5_INDEX.exists():
        return out
    for c in json.loads(PHASE5_INDEX.read_text()).get("campaigns", []):
        if c.get("status") != "passed":
            continue
        for a in c.get("command", []):
            if a.startswith("-max_len="):
                out[c["target"]] = int(a.split("=", 1)[1])
    return out


def choose_max_len(target: str, limit: int | None, p5: dict[str, int]) -> tuple[int, str]:
    if target in p5:
        return p5[target], "phase-5 campaign argv"
    if limit:
        return min(max(2 * limit, 4096), 262144), "2 x harness max_input_bytes (cap 262144)"
    return DEFAULT_MAX_LEN, "default"


def seed_plan(target: str) -> dict:
    d = FUZZ / "corpus" / target
    if d.is_dir():
        files = [p for p in d.rglob("*") if p.is_file()]
        return {"source": str(d.relative_to(REPO)), "kind": "fuzz/corpus", "files": len(files)}
    files = sorted({f for g in FALLBACK_SEEDS.get(target, []) for f in glob.glob(str(REPO / g))})
    return {"source": FALLBACK_SEEDS.get(target, []), "kind": "fallback-copy" if files else "empty",
            "files": len(files)}


def build_inventory() -> dict:
    bins = fuzz_bins()
    feats = fuzz_features()
    ids = catalogue_ids()
    p5 = phase5_max_len()
    problems, targets = [], []
    names = [n for n, _ in bins]
    if len(set(names)) != len(names):
        problems.append("duplicate [[bin]] names")
    for missing in sorted(set(names) - set(PROFILES)):
        problems.append(f"bin {missing} has no feature profile in the runner")
    for extra in sorted(set(PROFILES) - set(names)):
        problems.append(f"runner profile {extra} has no [[bin]]")
    for name, path in bins:
        if name not in PROFILES:
            continue
        prof = PROFILES[name]
        for f in prof.split(","):
            if f not in feats:
                problems.append(f"{name}: profile feature {f} missing from fuzz/Cargo.toml")
        src, helpers = harness_source(path)
        mods = referenced_format_modules(src)
        r = READERS[name]
        for rid in r["primary"] + r["component"]:
            if rid not in ids:
                problems.append(f"{name}: reader id {rid} not in catalogue")
        expand = sorted({x for f in prof.split(",") for x in feats.get(f, [])})
        limit = source_input_limit(src)
        max_len, max_len_src = choose_max_len(name, limit, p5)
        targets.append({
            "target": name,
            "harness": f"fuzz/{path}",
            "harness_sha256": sha256_file(FUZZ / path),
            "helpers": [f"fuzz/{h}" for h in helpers],
            "feature_profile": prof,
            "feature_expansion": expand,
            "readers_primary": r["primary"],
            "readers_component": r["component"],
            "referenced_format_modules": sorted(mods),
            "harness_max_input_bytes": limit,
            "max_len": max_len,
            "max_len_source": max_len_src,
            "seeds": seed_plan(name),
        })
    primary = {rid for t in targets for rid in t["readers_primary"]}
    any_cov = primary | {rid for t in targets for rid in t["readers_component"]}
    by_reader = {rid: {"primary": [t["target"] for t in targets if rid in t["readers_primary"]],
                       "component": [t["target"] for t in targets if rid in t["readers_component"]]}
                 for rid in ids}
    return {
        "kind": "phase-6 PLAN 9.6 fuzz inventory derived from source",
        "generated_at_utc": now(),
        "source": source_snapshot(),
        "counts": {
            "fuzz_targets": len(targets),
            "fuzz_target_source_files": len(list((FUZZ / "fuzz_targets").glob("*.rs"))),
            "catalogue_reader_ids": len(ids),
            "readers_with_primary_target": len(primary & set(ids)),
            "readers_with_any_target": len(any_cov & set(ids)),
            "distinct_feature_profiles": len({t["feature_profile"] for t in targets}),
            "helper_only_targets": sorted(t["target"] for t in targets if not t["readers_primary"]),
            "targets_without_seed_corpus_dir": sorted(t["target"] for t in targets if t["seeds"]["kind"] != "fuzz/corpus"),
        },
        "gaps": {
            "reader_ids_without_any_target": sorted(set(ids) - any_cov),
            "reader_ids_without_primary_target": sorted(set(ids) - primary),
        },
        "problems": problems,
        "reader_ids": ids,
        "readers": by_reader,
        "targets": targets,
    }


def inventory_markdown(inv: dict) -> str:
    c, g = inv["counts"], inv["gaps"]
    lines = [
        "# Phase 6 fuzz inventory (PLAN 9.6)", "",
        f"Generated {inv['generated_at_utc']} from source at HEAD `{inv['source']['git_head'][:12]}` "
        f"(diff sha256 `{inv['source']['diff_binary_sha256'][:16]}`, identity `{inv['source']['identity_sha256'][:16]}`).", "",
        f"- cargo-fuzz `[[bin]]` targets: **{c['fuzz_targets']}** "
        f"({c['fuzz_target_source_files']} top-level `.rs` harness files in `fuzz/fuzz_targets`, plus shared"
        " `#[path]` helper dirs `pilfont_bitmap/` and `rockbox_bitmap/`)",
        f"- catalogue reader IDs: **{c['catalogue_reader_ids']}**; with a primary target: "
        f"**{c['readers_with_primary_target']}**; with any target: **{c['readers_with_any_target']}**",
        f"- reader IDs without any target: {g['reader_ids_without_any_target'] or 'none'}",
        f"- reader IDs without a primary (entry-point) target: {g['reader_ids_without_primary_target'] or 'none'}",
        f"- helper/component-only targets: {', '.join(c['helper_only_targets'])}",
        f"- distinct feature profiles (= distinct ASAN fontpixel builds): {c['distinct_feature_profiles']}",
        f"- targets without a `fuzz/corpus/<target>` dir (fallback copy or empty start): "
        f"{', '.join(c['targets_without_seed_corpus_dir'])}",
        f"- problems: {inv['problems'] or 'none'}", "",
        "| target | primary readers | component readers | profile | max_len | seeds |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for t in inv["targets"]:
        s = t["seeds"]
        lines.append(f"| {t['target']} | {', '.join(t['readers_primary']) or '-'} | "
                     f"{', '.join(t['readers_component']) or '-'} | {t['feature_profile']} | {t['max_len']} | "
                     f"{s['kind']} ({s['files']}) |")
    return "\n".join(lines) + "\n"


def cmd_inventory(a) -> int:
    inv = build_inventory()
    out = Path(a.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    (out / "inventory.json").write_text(json.dumps(inv, indent=1) + "\n")
    (out / "inventory.md").write_text(inventory_markdown(inv))
    print(json.dumps({"counts": inv["counts"], "gaps": inv["gaps"], "problems": inv["problems"]}, indent=1))
    return 1 if inv["problems"] else 0


# ----------------------------------------------------------------- parsing
_STAT = re.compile(r"^stat::([a-z_]+):\s*(\d+)\s*$", re.M)
_STATUS = re.compile(r"^#(\d+)\s+([A-Z]+)\s+cov:\s*(\d+)\s+ft:\s*(\d+)(.*)$", re.M)
_DONE_RUNS = re.compile(r"^Done (\d+) runs in (\d+) second", re.M)
_SEEDS = re.compile(r"INFO:\s+seed corpus: files: (\d+)")
_ARTIFACT = re.compile(r"Test unit written to (\S+)")


def parse_libfuzzer_log(text: str) -> dict:
    stats = {k: int(v) for k, v in _STAT.findall(text)}
    status = _STATUS.findall(text)
    r: dict = {"final_stats_present": "number_of_executed_units" in stats, "stats": stats}
    done = [s for s in status if s[1] == "DONE"]
    last = done[-1] if done else (status[-1] if status else None)
    r["cov"] = int(last[2]) if last else None
    r["ft"] = int(last[3]) if last else None
    r["cov_source"] = ("DONE line" if done else "last status line") if last else None
    m = re.search(r"exec/s:\s*(\d+)", last[4]) if last else None
    r["last_exec_per_sec"] = int(m.group(1)) if m else None
    m = re.search(r"rss:\s*(\d+)Mb", last[4]) if last else None
    r["last_rss_mb"] = int(m.group(1)) if m else None
    if "number_of_executed_units" in stats:
        r["execs"], r["execs_source"] = stats["number_of_executed_units"], "stat::number_of_executed_units"
    elif last:
        r["execs"], r["execs_source"] = int(last[0]), "last status line (partial)"
    else:
        r["execs"], r["execs_source"] = None, None
    d = _DONE_RUNS.findall(text)
    r["done_runs"] = int(d[-1][0]) if d else None
    r["done_seconds"] = int(d[-1][1]) if d else None
    s = _SEEDS.findall(text)
    r["seed_files_reported"] = int(s[-1]) if s else None
    r["artifacts_reported"] = _ARTIFACT.findall(text)
    markers = [m for m in ("==ERROR: AddressSanitizer", "ERROR: LeakSanitizer", "ERROR: libFuzzer:",
                           "panicked at", "SUMMARY: AddressSanitizer", "==ABORTING")
               if m in text]
    r["error_markers"] = markers
    return r


def failing_artifacts(artifact_dir: Path) -> list[str]:
    if not artifact_dir.is_dir():
        return []
    return sorted(p.name for p in artifact_dir.iterdir() if p.is_file() and p.name.startswith(FAIL_ARTIFACT_PREFIXES))


def evaluate(record: dict, required_seconds: float) -> dict:
    reasons = []
    if record.get("exit_code") != 0:
        reasons.append(f"exit code {record.get('exit_code')}")
    if record.get("failing_artifacts"):
        reasons.append(f"artifacts {record['failing_artifacts']}")
    if record.get("watchdog_killed"):
        reasons.append("watchdog kill")
    el = record.get("elapsed_seconds")
    if el is None or el < required_seconds:
        reasons.append(f"elapsed {el} < required {required_seconds}")
    p = record.get("parsed") or {}
    if not p.get("final_stats_present"):
        reasons.append("no libFuzzer final stats")
    if not p.get("execs"):
        reasons.append("no executions")
    return {"passed": not reasons, "reasons": reasons}


def summarize(run_dir: Path) -> dict:
    manifest = json.loads((run_dir / "manifest.json").read_text())
    req = manifest["required_seconds"]
    rows, passed, failed, missing = [], [], [], []
    for t in manifest["targets"]:
        rp = run_dir / "results" / f"{t}.json"
        if not rp.exists():
            missing.append(t)
            continue
        rec = json.loads(rp.read_text())
        ev = evaluate(rec, req)
        p = rec.get("parsed") or {}
        rows.append({"target": t, "passed": ev["passed"], "reasons": ev["reasons"],
                     "exit_code": rec.get("exit_code"), "elapsed_seconds": rec.get("elapsed_seconds"),
                     "execs": p.get("execs"), "cov": p.get("cov"), "ft": p.get("ft"),
                     "peak_rss_mb": (p.get("stats") or {}).get("peak_rss_mb"),
                     "seed_files": rec.get("seed_files"), "failing_artifacts": rec.get("failing_artifacts")})
        (passed if ev["passed"] else failed).append(t)
    builds = {}
    bp = run_dir / "builds.json"
    if bp.exists():
        for b in json.loads(bp.read_text()):
            builds[b["target"]] = b["exit_code"]
    return {
        "label": manifest["label"], "required_seconds": req, "evaluated_at_utc": now(),
        "limits": {k: manifest.get(k) for k in ("max_total_time", "rss_limit_mb", "timeout", "parallel",
                                                "asan_options")},
        "criterion": "exit==0 and no crash-/timeout-/oom-/leak- artifacts and elapsed>=required "
                     "and libFuzzer final stats present with executions>0",
        "counts": {"targets": len(manifest["targets"]), "passed": len(passed), "failed": len(failed),
                   "missing_result": len(missing),
                   "build_failed": sorted(t for t, rc in builds.items() if rc != 0)},
        "all_passed": not failed and not missing and len(passed) == len(manifest["targets"]),
        "failed": failed, "missing": missing,
        "total_execs": sum(r["execs"] or 0 for r in rows),
        "rows": rows,
    }


def cmd_summary(a) -> int:
    run_dir = Path(a.out_root) / a.label
    s = summarize(run_dir)
    (run_dir / "summary.json").write_text(json.dumps(s, indent=1) + "\n")
    print(f"{'target':18} {'ok':4} {'exit':>4} {'elapsed':>8} {'execs':>12} {'cov':>6} {'ft':>7} {'rssMB':>6}  reasons")
    for r in s["rows"]:
        el = f"{r['elapsed_seconds']:.1f}" if r["elapsed_seconds"] is not None else "-"
        print(f"{r['target']:18} {'PASS' if r['passed'] else 'FAIL':4} {str(r['exit_code']):>4} {el:>8} "
              f"{str(r['execs']):>12} {str(r['cov']):>6} {str(r['ft']):>7} {str(r['peak_rss_mb']):>6}  "
              f"{'; '.join(r['reasons'])}")
    print(json.dumps({k: s[k] for k in ("label", "limits", "counts", "all_passed", "failed", "missing", "total_execs")}))
    return 0 if s["all_passed"] else 1


# ----------------------------------------------------------------- build
def build_target(target: str, target_dir: Path, jobs: int, log_dir: Path) -> dict:
    prof = PROFILES[target]
    argv = ["cargo", "+nightly", "fuzz", "build", target, "-O", "-a", "--sanitizer", "address",
            "--no-default-features", "--features", prof, "--target-dir", str(target_dir)]
    env = clean_env()
    env["CARGO_BUILD_JOBS"] = str(jobs)
    start = time.monotonic()
    started = now()
    with open(log_dir / f"{target}.log", "wb") as log:
        rc = subprocess.run(argv, cwd=FUZZ, env=env, stdout=log, stderr=subprocess.STDOUT).returncode
    rec = {"target": target, "argv": argv, "cwd": str(FUZZ), "feature_profile": prof, "started_utc": started,
           "seconds": round(time.monotonic() - start, 2), "exit_code": rc,
           "CARGO_BUILD_JOBS": jobs}
    return rec


def do_builds(targets, run_dir: Path, a) -> list[dict]:
    log_dir = run_dir / "build-logs"
    bin_dir = run_dir / "bin"
    log_dir.mkdir()
    bin_dir.mkdir()
    base = Path(a.target_dir)
    workers = max(1, a.build_workers)
    # keep same-profile targets on one worker so fontpixel is compiled once per profile
    by_prof: dict[str, list[str]] = {}
    for t in targets:
        by_prof.setdefault(PROFILES[t], []).append(t)
    queues: list[list[str]] = [[] for _ in range(workers)]
    for prof in sorted(by_prof, key=lambda p: -len(by_prof[p])):
        min(queues, key=len).extend(by_prof[prof])
    results, lock = [], threading.Lock()

    def worker(i: int, q: list[str]):
        td = base if workers == 1 else base / f"w{i}"
        for t in q:
            rec = build_target(t, td, a.build_jobs, log_dir)
            src = td / TRIPLE / "release" / t
            if rec["exit_code"] == 0 and src.is_file():
                dst = bin_dir / t
                shutil.copy2(src, dst)
                rec["binary"] = str(dst)
                rec["binary_sha256"] = sha256_file(dst)
            with lock:
                results.append(rec)
                print(f"[build] {t:18} rc={rec['exit_code']} {rec['seconds']}s", flush=True)

    with concurrent.futures.ThreadPoolExecutor(workers) as ex:
        list(ex.map(lambda iq: worker(*iq), enumerate(queues)))
    results.sort(key=lambda r: targets.index(r["target"]))
    (run_dir / "builds.json").write_text(json.dumps(results, indent=1) + "\n")
    return results


# ----------------------------------------------------------------- run
def prepare_corpus(target: str, dest: Path) -> dict:
    plan = seed_plan(target)
    if plan["kind"] == "fuzz/corpus":
        shutil.copytree(REPO / plan["source"], dest)
    else:
        dest.mkdir(parents=True)
        for g in FALLBACK_SEEDS.get(target, []):
            for f in sorted(glob.glob(str(REPO / g))):
                shutil.copy2(f, dest / Path(f).name)
    files = [p for p in dest.rglob("*") if p.is_file()]
    h = hashlib.sha256()
    for p in sorted(files):
        h.update(str(p.relative_to(dest)).encode() + b"\0" + sha256_file(p).encode() + b"\n")
    return {**plan, "copied_files": len(files), "copied_bytes": sum(p.stat().st_size for p in files),
            "copy_manifest_sha256": h.hexdigest()}


def fuzz_one(target: str, binary: Path, run_dir: Path, inv_t: dict, a, snapshot_id: str) -> dict:
    corpus = run_dir / "corpus" / target
    art = run_dir / "artifacts" / target
    art.mkdir(parents=True)
    seeds = prepare_corpus(target, corpus)
    max_total = a.seconds + a.margin
    argv = [str(binary), str(corpus),
            f"-max_total_time={max_total}", f"-timeout={a.timeout}", f"-rss_limit_mb={a.rss_limit_mb}",
            f"-max_len={inv_t['max_len']}", "-print_final_stats=1", f"-artifact_prefix={art}/"]
    env = clean_env()
    if a.asan_options:
        env["ASAN_OPTIONS"] = a.asan_options
    log_path = run_dir / "logs" / f"{target}.log"
    started = now()
    t0 = time.monotonic()
    killed = False
    with open(log_path, "wb") as log:
        proc = subprocess.Popen(argv, cwd=run_dir, env=env, stdout=log, stderr=subprocess.STDOUT,
                                start_new_session=True)
        try:
            rc = proc.wait(timeout=max_total + a.watchdog_grace)
        except subprocess.TimeoutExpired:
            killed = True
            os.killpg(proc.pid, signal.SIGKILL)
            rc = proc.wait()
    elapsed = time.monotonic() - t0
    text = log_path.read_text(errors="replace")
    rec = {
        "target": target, "argv": argv, "cwd": str(run_dir),
        "env_ASAN_OPTIONS": env.get("ASAN_OPTIONS"),
        "feature_profile": PROFILES[target], "binary_sha256": sha256_file(binary),
        "source_identity_sha256": snapshot_id,
        "started_utc": started, "ended_utc": now(), "elapsed_seconds": round(elapsed, 3),
        "requested_seconds": a.seconds, "max_total_time": max_total,
        "rss_limit_mb": a.rss_limit_mb, "timeout": a.timeout, "max_len": inv_t["max_len"],
        "exit_code": rc, "watchdog_killed": killed,
        "seed_files": seeds["copied_files"], "seeds": seeds,
        "parsed": parse_libfuzzer_log(text),
        "failing_artifacts": failing_artifacts(art),
        "all_artifacts": sorted(p.name for p in art.iterdir()) if art.is_dir() else [],
        "log": str(log_path), "log_sha256": sha256_file(log_path),
    }
    rec["evaluation"] = evaluate(rec, a.seconds)
    if a.discard_corpus and rec["evaluation"]["passed"]:
        shutil.rmtree(corpus, ignore_errors=True)
        rec["corpus_discarded"] = True
    (run_dir / "results" / f"{target}.json").write_text(json.dumps(rec, indent=1) + "\n")
    p = rec["parsed"]
    print(f"[fuzz]  {target:18} {'PASS' if rec['evaluation']['passed'] else 'FAIL'} rc={rc} "
          f"{elapsed:.1f}s execs={p['execs']} cov={p['cov']} ft={p['ft']}", flush=True)
    return rec


def cmd_run(a) -> int:
    run_dir = Path(a.out_root) / a.label
    if run_dir.exists():
        print(f"refusing to overwrite existing label {run_dir}", file=sys.stderr)
        return 2
    inv = build_inventory()
    if inv["problems"]:
        print("inventory problems: " + "; ".join(inv["problems"]), file=sys.stderr)
        return 2
    inv_by = {t["target"]: t for t in inv["targets"]}
    targets = [t["target"] for t in inv["targets"]]
    if a.targets:
        want = [x for x in a.targets.split(",") if x]
        unknown = set(want) - set(targets)
        if unknown:
            print(f"unknown targets {sorted(unknown)}", file=sys.stderr)
            return 2
        targets = [t for t in targets if t in want]
    run_dir.mkdir(parents=True)
    for sub in ("results", "logs", "corpus", "artifacts"):
        (run_dir / sub).mkdir()
    snap = inv["source"]
    manifest = {
        "label": a.label, "created_utc": now(), "runner_argv": sys.argv,
        "runner_sha256": sha256_file(Path(__file__)),
        "required_seconds": a.seconds, "max_total_time": a.seconds + a.margin,
        "parallel": a.parallel, "timeout": a.timeout, "rss_limit_mb": a.rss_limit_mb,
        "asan_options": a.asan_options, "target_dir": a.target_dir, "reuse_build_from": a.reuse_build_from,
        "build_flags": ["-O", "-a", "--sanitizer", "address", "--no-default-features"],
        "toolchain": toolchain_info(), "source_before": snap, "targets": targets,
        "machine": {"nproc": os.cpu_count(), "loadavg": os.getloadavg()},
    }
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
    (run_dir / "inventory.json").write_text(json.dumps(inv, indent=1) + "\n")

    if a.reuse_build_from:
        prev = Path(a.out_root) / a.reuse_build_from
        pm = json.loads((prev / "manifest.json").read_text())
        if pm["source_before"]["identity_sha256"] != snap["identity_sha256"]:
            print("refusing --reuse-build-from: source identity differs", file=sys.stderr)
            return 2
        prev_builds = {b["target"]: b for b in json.loads((prev / "builds.json").read_text())}
        (run_dir / "bin").mkdir()
        builds = []
        for t in targets:
            b = dict(prev_builds[t])
            if b["exit_code"] == 0:
                dst = run_dir / "bin" / t
                shutil.copy2(b["binary"], dst)
                if sha256_file(dst) != b["binary_sha256"]:
                    print(f"binary hash mismatch for {t}", file=sys.stderr)
                    return 2
                b["binary"], b["reused_from"] = str(dst), a.reuse_build_from
            builds.append(b)
        (run_dir / "builds.json").write_text(json.dumps(builds, indent=1) + "\n")
    else:
        builds = do_builds(targets, run_dir, a)
    after_build = source_snapshot()
    manifest["source_after_build"] = after_build
    manifest["source_changed_during_build"] = after_build["identity_sha256"] != snap["identity_sha256"]
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
    if manifest["source_changed_during_build"]:
        print("WARNING: source identity changed during build", file=sys.stderr)
    ok = {b["target"]: Path(b["binary"]) for b in builds if b["exit_code"] == 0 and b.get("binary")}
    if a.build_only:
        failed = [b["target"] for b in builds if b["exit_code"] != 0]
        print(json.dumps({"built": len(ok), "build_failed": failed}))
        return 1 if failed else 0

    # longest expected first: large seed corpora take longest to replay
    order = sorted(ok, key=lambda t: -inv_by[t]["seeds"]["files"])
    with concurrent.futures.ThreadPoolExecutor(max(1, a.parallel)) as ex:
        futs = [ex.submit(fuzz_one, t, ok[t], run_dir, inv_by[t], a, snap["identity_sha256"]) for t in order]
        for f in futs:
            f.result()
    end = source_snapshot()
    manifest["source_after_run"] = end
    manifest["source_changed_during_run"] = end["identity_sha256"] != snap["identity_sha256"]
    manifest["finished_utc"] = now()
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
    return cmd_summary(a)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out-root", default=str(DEFAULT_OUT_ROOT))
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("inventory")
    p.add_argument("--out-dir", default=str(DEFAULT_OUT_ROOT))
    p.set_defaults(func=cmd_inventory)
    p = sub.add_parser("run")
    p.add_argument("--label", required=True)
    p.add_argument("--seconds", type=int, required=True, help="required fuzzing seconds per target (pass threshold)")
    p.add_argument("--margin", type=int, default=10, help="-max_total_time = seconds + margin (phase 5: 600+10)")
    p.add_argument("--parallel", type=int, default=max(1, (os.cpu_count() or 8) - 4))
    p.add_argument("--timeout", type=int, default=10)
    p.add_argument("--rss-limit-mb", type=int, default=2048,
                   help="libFuzzer -rss_limit_mb (phase 5 used 1024; psf peaked at 700 MB in a 15 s smoke)")
    p.add_argument("--asan-options", default=None, help="ASAN_OPTIONS for fuzz processes (default: inherit)")
    p.add_argument("--watchdog-grace", type=int, default=1800,
                   help="extra seconds past max_total_time before SIGKILL (initial corpus replay)")
    p.add_argument("--targets", default="", help="comma-separated subset")
    p.add_argument("--target-dir", default=str(DEFAULT_TARGET_DIR))
    p.add_argument("--build-workers", type=int, default=1, help="parallel builds (each gets its own target dir)")
    p.add_argument("--build-jobs", type=int, default=6, help="CARGO_BUILD_JOBS per build worker")
    p.add_argument("--build-only", action="store_true")
    p.add_argument("--reuse-build-from", default=None, help="label whose binaries to reuse (identical source only)")
    p.add_argument("--discard-corpus", action="store_true", help="delete per-target corpus copy after a pass")
    p.set_defaults(func=cmd_run)
    p = sub.add_parser("summary")
    p.add_argument("--label", required=True)
    p.set_defaults(func=cmd_summary)
    a = ap.parse_args(argv)
    return a.func(a)


if __name__ == "__main__":
    sys.exit(main())
