# Handover to Codex (gpt-6-astra): phase 9 — wasm font tool on fontpixel.com

Phase 8 is delivered (`main` at `3c03899`, `reports/phase-8.md`,
`benches/PHASE8-REPORT.md`). Your task is **phase 9 only**. Do not write the
project's final report (`FINAL-REPORT.md`); stop when phase 9 is delivered and
reported.

The owner changed phase 9 on 2026-10-07. PLAN.md's standalone page
`web/merge/` (no framework) is **not** built. Instead, update the catalogue site
`/home/chen/githubprojects/fontpixel/fontpixel.com` with a client-side wasm font
tool. The authoritative spec is `fontpixel-rs/TODO.md`, resume step 4
(「第 9 阶段（2026-10-07 用户改定）」), and `docs/decisions.md`,
"2026-10-07 — Phase 9 retargeted to fontpixel.com". The W9.1 design in
`docs/plan-addendum-merge.md` (preview, monospace hints, naming and
reserved-name checks, licenses, Worker progress) is folded into the tool.

## 0. Ground rules (owner's standing instructions)

- Reply to the owner in Chinese. Keep code, commands, paths and identifiers verbatim.
- **Local commits only. No push, no publish, no deploy** (Cloudflare deploy is the
  owner's job). Do not touch CI secrets or deploy settings.
- **Deletion rule (hard):** delete only with explicit absolute paths. Never
  `cd X && rm *`, never `rm -rf "$VAR"/*`, never globs on a directory you did not
  create.
- Clean build output, target dirs and caches regularly; check `df -h /home`.
- Never write into `/home/chen/githubprojects/fontpixel/reference`, `samples`,
  `bdfparser-js` or other sibling repos except the two repos below.
- fontpixel-rs rules still hold for changes there: never raise default resource
  caps, never weaken strict/loss/report behaviour, no `#[allow]`, no `unsafe`.
- **UI translation (zh, zh-Hant) is done by a Sol model (gpt-5.6-sol) sub-agent**,
  not by you directly and not by Claude models (owner decision 2026-10-07). Give
  it the English strings, glossary and context; review the result for
  completeness and placeholder safety only.
- Tokens are limited: work sequentially, iterate with fast builds, run full gates
  once per milestone, keep long logs out of context.
- Decisions the spec leaves open: decide, and record them in
  `fontpixel-rs/docs/decisions.md` (and in the site's docs if site-specific).

## 1. Repositories and branches

- `fontpixel.com` (`/home/chen/githubprojects/fontpixel/fontpixel.com`): clean,
  currently on branch `add-tong` at `a7dd031`. Create branch **`tool-page`** from
  that commit and do all site work there. Astro 5 + Svelte 5 islands in `site/`
  (bun, vitest, Playwright e2e, astro check/svelte-check); pipeline in
  `pipeline/`; prebuilt downloads in `dist-downloads/`, staged by
  `site/scripts/stage-downloads.mjs`; previews currently use bdfparser (JS).
  Read its README, docs/ and TODO.md first. Use Svelte 5 runes
  (`$state`, `$derived`, `$props`, `$effect`, `$state.raw` for large data).
  The owner may use another agent session on this repo; never commit to
  `add-tong` or other existing branches.
- `fontpixel-rs` (`/home/chen/githubprojects/fontpixel/fontpixel-rs`): `main`,
  clean at `3c03899`. Small changes only: a slim **`web`** wasm build profile in
  `crates/fontpixel-wasm` + `scripts/build_wasm.sh`, and whatever export the site
  needs. Local commits on `main`.

## 2. What to build (summary; the TODO step is authoritative)

Owner requirements:
1. Works on any catalogue font or any font the user uploads.
2. Convert to the common formats we support.
3. Subset by one or more Unicode blocks or typed ranges; or pick several fonts,
   each with its own blocks/ranges, and subset and/or merge them.
4. Pixelize outline fonts (TTF/OTF/WOFF/WOFF2) at a chosen pixel size via the
   core `vector-raster` path; warn that hinting is not executed.
5. Each font page's download area offers only prebuilt **BDF and WOFF2**; every
   other format is generated in the browser by wasm when the user clicks.

Accepted additions (all 14 in the TODO step), notably:
- OFL Reserved Font Names: subset/merge is modification → renaming is required;
  reserved names cannot be reused; fonts that forbid modification cannot be
  subset/merged.
- Subset by pasted text.
- **Preview before download (owner refinement):** render the result with
  representative characters for every Unicode block and writing system it
  contains, CJK and Latin alike, plus per-block counts/coverage, missing glyphs
  and the loss report.
- Outputs: BDF, PCF, OTB, bitmap TTF, vector TTF/OTF (square/round dots),
  WOFF/WOFF2, PSF, FNT/FON, Unifont hex, YAFF, BMFont (PNG+.fnt), C array.
- wasm loaded only on demand (tool page or "download other formats"), slim `web`
  profile, Web Worker with progress and cancel, generated files cached in the
  browser (Cache API/IndexedDB).
- Shareable URL state; font pages link to a prefilled tool.
- Old prebuilt download URLs (PCF/TTF) must not just 404 (redirect to on-demand
  generation); update README, docs and `llms.txt`; no-JS visitors still get
  BDF/WOFF2.
- Stop prebuilding PCF/TTF in the pipeline.
- License/copyright metadata travels into generated files; zips include LICENSE.
- Mobile memory: catch resource-limit errors with a clear message.
- UI in the site's languages en / zh / zh-Hant (translation by Sol, see §0).
- Unsupported (uncommon) uploaded formats: tell the user to convert with the CLI
  first; do not lazy-load the full wasm.

## 3. Acceptance

- Playwright e2e in Chromium, Firefox and WebKit ("WebKit", not "Safari"):
  monospace and proportional subset+merge exported as BDF and WOFF2 (plus a few
  other common formats) **byte-identical** to the native CLI for the same inputs
  and options (tests may run the CLI; the site never does); monospace-mismatch
  warning; reserved-name rejection; a ~50k-glyph font stays responsive (Worker,
  progress, cancel); pixelize; on-demand download from a font page; old-URL
  redirect.
- The site's own gates pass: `bun run check`, `bun run test`, `bun run build`,
  the existing e2e/a11y suites, SEO audit.
- fontpixel-rs gates for its changes: `scripts/check_wasm.sh` (coverage gate
  `python,js` stays 0 errors), `scripts/check.sh` if Rust changed.
- Report sizes and load times of the `web` profile (pre/post wasm-opt, gzip) and
  generation times for typical fonts.

## 4. Deliverables

- `fontpixel.com` branch `tool-page` with the tool, the download-area change, the
  pipeline change, docs; local commits.
- `fontpixel-rs`: `web` profile and any binding additions on `main`;
  `reports/phase-9.md` (Chinese, style of `reports/phase-8.md`: conclusion, gates,
  every requirement and addition with evidence, sizes/timings, browser range,
  known limitations); update `TODO.md` (phase 9 done; final report not started).
- Stop afterwards.

## 5. Environment

- Rust 1.99, wasm32 target, wasm-pack 0.15, wasm-bindgen 0.2.129, wasm-opt 117
  (`~/.cache/.wasm-pack`), Node 24 (nvm), bun 1.4 for the site, Playwright
  browsers in `~/.cache/ms-playwright`.
- fontpixel-rs target dir: `source scripts/local-env.sh`
  (`/home/chen/githubprojects/fontpixel/local-work/target-fontpixel-rs`); side
  builds in `.../local-work/target-p9-*`, delete afterwards.
- Evidence gates: `/home/chen/githubprojects/fontpixel/local-work/phase6-local-20261004/run_gate.py LABEL "command"`
  (continue the numbering after the last gate in `gates/`).
- The machine is shared with the owner's other agents.
