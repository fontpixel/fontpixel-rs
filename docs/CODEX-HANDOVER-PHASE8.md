# Handover to Codex (gpt-6-astra): finish phase 8 of fontpixel-rs

You are taking over phase 8 (WASM/JS bindings) of fontpixel-rs from Claude Code,
which stopped on 2026-10-07. Do phase 8 only. Do not start phase 9 or the final
report; stop when phase 8 is delivered and reported.

## 0. Ground rules (owner's standing instructions — follow exactly)

- Reply to the owner in Chinese (the owner writes Chinese). Keep code, commands,
  paths and identifiers verbatim.
- Local commits on `main` are allowed. **No push, no publish (npm/PyPI), no deploy.**
- **Deletion rule (hard):** delete only with explicit absolute paths, e.g.
  `rm -rf /home/chen/githubprojects/fontpixel/local-work/p8-x/tmp`. Never
  `cd X && rm *`, never `rm -rf "$VAR"/*`, never globs on a directory you did not
  create. (An earlier agent nearly wiped the repository with `rm -f *`.)
- **Disk:** clean build/target dirs, caches and scratch regularly; delete a unit's
  target dir after its work is merged; check `df -h /home` around long jobs.
- Never raise default resource caps; never weaken strict/loss/report behaviour;
  never delete report entries; no `#[allow]` in Rust; no `unsafe`.
- Never write into `/home/chen/githubprojects/fontpixel/reference`, `.../samples`,
  `.../bdfparser-js` or other sibling repos; never run `npm install` there or in
  this repository. Install Node tooling under `/home/chen/githubprojects/fontpixel/local-work/`.
- The bindings never call the CLI as a subprocess. Tests may run the native CLI to
  produce reference bytes.
- Tokens are limited: work sequentially, avoid repeated full gates (iterate with
  fast builds, run the full gates once per milestone), keep logs out of context.
- PLAN.md says gaps in the plan are decided by you and recorded in
  `docs/decisions.md`.

## 1. Read first

1. `TODO.md` — section **「第 8 阶段暂停记录」** is the authoritative state and
   resume list (written 2026-10-06/07). Everything below summarises it.
2. `PLAN.md` phase 8 (search 「第 8 阶段」) and `docs/plan-addendum-merge.md` W8.3.
3. `docs/bindings-scope.md` (requirements), `docs/bindings-design.md` (binding
   contract, incl. the "JS/WASM (phase 8)" section), `docs/decisions.md` entries
   "Phases 7–9 architecture", "`fontpixel-ops`: one typed definition…",
   "wasm32 readiness of core and ops" (note its **correction** block),
   "2026-10-06 — WASM binding foundation".
4. `/home/chen/githubprojects/fontpixel/local-work/p8-common-rules.md` —
   conventions, file ownership, build/test commands for the WASM crate.
5. The per-branch handoffs: `/home/chen/githubprojects/fontpixel/local-work/p8-j{1,3,4,5,6}/HANDOFF.md`.
6. Phase 7 as the model to follow: `reports/phase-7.md`, `benches/PHASE7-REPORT.md`,
   `crates/fontpixel-py/`.

## 2. State at handover

Repository: `/home/chen/githubprojects/fontpixel/fontpixel-rs`, branch `main`
(clean). Phases 0–7 are delivered (reports in `reports/`).

Merged into main for phase 8:
- Unit A (`15cf8af`): wasm32 readiness of core/ops (brotli single-threaded
  spawner on wasm, callback traits without Send+Sync on wasm, `Error::Cancelled`,
  `merge_with_progress`/`convert_with_progress`, Python `progress=`).
- Unit F (`40bc508`): `crates/fontpixel-wasm` foundation (wasm-bindgen 0.2.129
  pinned, JSON plain-data bridge with per-value number/bigint, error classes,
  handles with free/dispose/guard, resources and async re-run protocol, progress
  adapter, `capabilities()`, `tools/bindings/gen_dts.py`, `scripts/build_wasm.sh`,
  `scripts/check_wasm.sh`, JS coverage gate).
- Unit J2 (`f263b9e`): bitmap/render/charset/outline JS APIs.

Five unit branches, committed and clean, **not merged** (worktrees removed,
branches kept):

| Branch | Area | Last commit | Notes |
| --- | --- | --- | --- |
| `p8-js-model` | J1 model/metadata/resources (model/core/root.toml) | `fbebed7` | complete; only fast build re-run after merging J2 |
| `p8-js-formats` | J3 all 99 formats (formats.toml, generator) | `38a53d3` | 1,408 format tests pass on full; 1 failure on the merge profile (`setWriteOptions('bdat')`); clippy/TS smoke/docs pending; full wasm grew to 16.6 MB pre-opt (703-type `normalize` table?) |
| `p8-js-sfnt` | J4 sfnt/WOFF/variants/merge library | `191bbb3` | complete on full; minimal/merge profiles untested; WOFF2 byte test marked TODO |
| `p8-js-ops` | J5 ops (22 ops) + W8.3 `info`/`merge`/`render` + Worker/progress/cancel + Node file helper | `a5cbbd3` | 57k-glyph merge BDF byte-identical to CLI; WOFF2 byte test TODO; check_wasm.sh not completed; `gen_dts` wrong shape for `TaskRequest` |
| `p8-js-compat` | J6 bdfparser-js compat + reference tests | `6f85876` | own tests pass; reference suite 1178/11 failed/3 skipped (all 11 explained); touched the shared Rust compat core (Python still 1262 pass, Python reference 1755/8/10); check.sh/check_wasm.sh/reference JSON re-run pending |

Known conflicts when merging: J3 defines `Written`/`OutputBundle` (may clash with
J1); J4's `merge` namespace clashes with J5's top-level W8.3 `merge()`; J1's
`__fpStash()`/`model_args::handle_arg` lets handles go into arrays/unions (J2 uses
`copy()` instead — consider switching); shared files (`src/lib.rs`,
`capabilities.rs`, `js/index.js`/`index.d.ts`, `ts/aliases.d.ts`,
`ts/types.d.ts` (regenerate, don't hand-merge), `scripts/build_wasm.sh` copy list,
`Cargo.toml`/`Cargo.lock`, `tests/typing/smoke.ts`, `package.json` exports).

## 3. Owner decision to implement: WOFF2 `portable-float`

Wasm and native WOFF2 bytes differ (decoded tables identical): brotli 9.0.0 uses
the platform `f32::log2`, which differs in the last bit between native Linux and
wasm. The owner decided (2026-10-06) to **enable brotli's `portable-float`
feature** in `crates/fontpixel/Cargo.toml` so they are identical. Then:
re-check the brotli heap admission (`crates/fontpixel/src/woff/woff2/brotli_encode.rs`
empirical envelope, `crates/fontpixel-heap-check`); rebuild the frozen reference
CLI used by tests/evidence; update WOFF2 byte expectations and any reported
WOFF2 sizes/hashes (state they are post-change values); turn the J4/J5 WOFF2
TODO tests into real assertions (input ≥ 128 KiB, e.g. Tong SC from
`/home/chen/githubprojects/fontpixel/samples/tong-pixel-sans/`); update the
decisions.md correction block to "implemented".

## 4. Work to do (in order)

1. Merge the branches into main one at a time: `p8-js-model` → `p8-js-formats`
   → `p8-js-sfnt` → `p8-js-ops` → `p8-js-compat`, resolving the conflicts above
   and the open items in each HANDOFF.md. After each merge:
   `scripts/build_wasm.sh full` and `node --test crates/fontpixel-wasm/tests/*.test.mjs`.
2. Implement `portable-float` (section 3).
3. Gates on the merged state: `scripts/check_wasm.sh` (all profiles; its last
   step `tools/bindings/gate.sh python,js` must report **0 errors, 0 unmapped**),
   `scripts/check.sh` (full native gate), `scripts/check_python.sh`
   (venv `/home/chen/githubprojects/fontpixel/local-work/py310-venv`), the Python
   bdfparser reference run (needs `freetype-py`; must stay 1755 passed / 8 failed /
   10 skipped) and the JS bdfparser-js reference run (refresh its results JSON).
4. Remaining phase 8 requirements:
   - Browser tests with Playwright in Chromium, Firefox and WebKit (browsers are
     cached in `~/.cache/ms-playwright`; install the `playwright` npm package under
     `local-work/`): loading, core APIs, W8.3 Worker merge with progress and
     cancel, byte identity of merge outputs (BDF and WOFF2) with the native CLI.
     Say "WebKit", not "Safari".
   - Node: verify loading on Node 18 as well as 24 (or record why not).
   - Benchmarks vs pure-JS bdfparser-js 3.0.0, per category (whole workloads and
     small calls), in Node and headless Chrome; report slower cases honestly
     (PLAN risk table: never change the method to look better). Method like
     `benches/phase7_python.py`; shared desktop → record load, use A/B pairs.
   - Wasm size per feature combination (full / merge / minimal, with and without
     WOFF2/brotli, CJK charsets, vectorization), pre/post wasm-opt and gzip, and
     initialization time in Node and the browser.
   - JS documentation (like `docs/python-api.md` + generated reference) and
     runnable examples; TypeScript `tsc --strict` smoke kept green.
   - Document threading, Promise/sync behaviour, progress/cancellation, handle
     release, memory copies, Number/BigInt, host persistence (bindings-scope.md
     phase 8 list).
5. Deliver: `reports/phase-8.md` (Chinese, same style as `reports/phase-7.md`:
   conclusion, gates table, W-items incl. W8.3, units, coverage summary, compat
   results, benchmarks, sizes, known limitations) and `benches/PHASE8-REPORT.md`;
   update `TODO.md` (phase 8 done; phase 9 is the fontpixel.com tool page already
   specified there — do not start it). Commit locally. Clean up target dirs.

## 5. Environment

- Rust 1.99 (rustup), targets wasm32-unknown-unknown and aarch64 installed;
  wasm-pack 0.15 (`~/.cargo/bin`), wasm-bindgen CLI 0.2.129 and wasm-opt 117
  under `~/.cache/.wasm-pack`; Node 24.21 (nvm); TypeScript read-only at
  `/home/chen/githubprojects/fontpixel/bdfparser-js/node_modules/.bin/tsc`.
- Target dirs: main `/home/chen/githubprojects/fontpixel/local-work/target-fontpixel-rs`
  (`source scripts/local-env.sh`); use your own `.../local-work/target-p8-*` for
  side builds and delete them afterwards. `CARGO_BUILD_JOBS=6`.
- Evidence: `/home/chen/githubprojects/fontpixel/local-work/phase6-local-20261004/run_gate.py LABEL "command"`
  records a gate under `gates/LABEL/` (labels carry their own number; the last
  one is `212-p7-wheels-final`, continue at 213).
- The machine is shared with the owner's other agents (load is often 3–15).
