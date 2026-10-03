# W2.4 embedded bitmap recovery

Date: 2026-10-03 UTC. All completed checks pass; final two reader fuzz runs still in progress.

## Implementation and history

Recovery branch: `recovery/w2.4-20261003`, based on validated W2.1–W2.3 recovery `e5a4d298`. Production reconstruction is committed at `0c4bfdd`. Original archived `main` (`caaeecd`) and W2.1 (`76ab7eb`) refs remain untouched. No merge or push is claimed.

The complete retained initial snapshots were combined with all recorded later code blocks and replacements. All 68 audited replacements matched, with two whitespace-only anchor adjustments for compact pre-rustfmt text. Rust 1.99.0 supplied formatting. The baseline CLI test changes were described semantically rather than retained as full code, so equivalent capability assertions were reconstructed and explicitly are not claimed as original text.

Original W2.4 Git objects (`113539d`, `ece15ea`, `e08d69c`), final original source-byte hashes, complete evidence and lockfiles are unavailable. This is new recovery history. Fresh source fingerprint is `d8adc6b27c3354dd`; it is not represented as an old final-code fingerprint. The old dirty pre-refinement observation is preserved only in historical-status.md. No production-code repair, redesign or additional feature was needed after replay.

## Recovered scope

- Bounded EBDT/EBLC and Apple bdat/bloc read/write, locator formats 1–5, images 1/2/5/6/7/8/9, and 1/2/4/8-bit coverage.
- EBSC read/write, sparse and multiple physical strikes, shared metrics, bounded composites and bitwise-OR gray overlap.
- CBDT/CBLC version 3 reads with PNG 17/18/19 and premultiplied BGRA; PNG/dupe sbix reads.
- Honest native OTB/bdat read/write hooks, color read/detect-only hooks, typed options, explicit projection/loss diagnostics, native names/mappings/metrics and source-table provenance.
- Final per-font raster/model/diagnostic budgets and the recorded sbix duplicate-placement correction.

## Fresh validation

- All-feature workspace library/binary/example build passes.
- `scripts/check.sh`: exit 0; 561 aggregate test executions, including all 15 embedded-bitmap tests. Generator checks, rustfmt, Clippy, feature matrices and production dependency audit pass.
- Complete default parity: 0 NEW divergences; all 67 charset tables, 2,000 random cases / 35,941 operations, 1,000 random_all / 17,588, 41 probes / 89 and all 251 corpus cases / 5,221 ran. Existing K12 cases remain six random_all plus one probe. Python fingerprint remains `938d9ed560ef29ba`; crate sources are clean and unchanged during parity.
- Independent crosscheck: 140 cases / 380,808 glyph comparisons. It covers all seven Tong BDFs as OTB and bdat, both supplied OTB reference round trips, 121 synthetic cases, monobit round trips/EBSC, HarfBuzz mappings and the applicable sfnt-container OTS checks.
- Synthetic fixture regeneration independently passes 121 cases / 126 glyph comparisons. All 37 tiny binary fixtures are regenerated from the recovered deterministic generator and labelled accordingly in SOURCES.json, with current hashes and fixed timestamps. Original binary hashes are unavailable.
- Separate no-default `sfnt`, `otb`, `bdat` and `color` configurations pass. Strict all-feature rustdoc passes. BDF/PCF-only dependency tree excludes Fontations and png.
- Root and isolated-fuzz dependency audits pass after the narrow separately documented policy reconciliation below. No independent claim of online advisory-database freshness is made.
- 690 protected reference/sample files remain identical, with no missing or extra files. All 24 W2.4 salvage-manifest entries remain unchanged.

Final-reader robustness results (each target uses generated synthetic seeds, a 610-second minimum, 10-second case timeout, 2 GiB RSS limit and 64 KiB maximum input):
- embedded_bitmap: running, not counted as passed.
- color_bitmap: running, not counted as passed.

Leak detection alone is disabled under the recorded sandbox configuration; AddressSanitizer remains enabled. Historical initial fuzz runs are not used as substitutes for these fresh final-code runs.

## New recovery reconciliation, kept separate

The first isolated fuzz dependency audit failed its duplicate-crate ban: png 0.18.1 requires miniz_oxide 0.8.9 while flate2 1.1.10 also requires 0.9.1. The root policy already contained the exact 0.8.9 exception in the retained W2.4 ledger, but the isolated fuzz policy did not. The original final W2.4 fuzz audit was never observed.

After verifying both dependency paths and obtaining coordinator authorization, commit `cbcbed4` copied only the same exact-version exception into fuzz/deny.toml. No advisory ignore, license expansion, dependency downgrade or broad duplicate exception was introduced. The initial failure and successful rechecks are preserved; this maintenance change is not claimed as recovered original content.

Cargo regenerated the missing main/fuzz lockfile additions from crates.io, selecting the same recorded versions: png 0.18.1, fdeflate 0.3.7, miniz_oxide 0.8.9 and optional zlib-rs 0.6.8, retaining flate2 1.1.10 and its miniz_oxide 0.9.1. Registry checksums are regenerated evidence, not recovered old lockfile blobs.

## Independent-tool and scope limits

- FreeType 2.13.3 shifts width-3 bit-aligned 2/4-bit rows in the known synthetic case. The driver asserts the exact measured discrepancy; it does not generally weaken pixel equality. Canonical byte-aligned writes match pixels strictly.
- freetype-py's bundled library lacks PNG support here. The test wrapper is copied to a workspace-local directory and points to the installed system FreeType 2.13.3 library; no system library or security setting is changed.
- OTS 9.2.0 rejects bitmap-only sfnt and strips EBDT/EBLC in outline-bearing fonts. OTS success is not claimed as bitmap-payload validation; fontTools, FreeType, monobit and structural tests supply that evidence.
- monobit bdat+EBSC raises the recorded upstream EBLC KeyError; EBDT scaling and native EBSC are independently tested.
- Apple Huffman image 4, obsolete image 3, nonzero colorRef, and non-PNG/non-dupe sbix encodings are explicitly rejected. There is no color writer, arbitrary vector FontSet rasterization, TTC/resource-fork support or native TTF/OTF hook activation in this item.
- Color projects to alpha coverage or explicit darkness/threshold with loss reporting; BGRA composite darkness is rejected because RGB composition needs an RGBA model. Vertical pixel metrics survive, but design-space vhea/vmtx generation remains outside this item.

Full fresh evidence is under `reports/recovery-w2.4-20261003/`; old observations are labelled separately. Full historical raw logs and original Git history remain missing. Build outputs, generated corpora, large temporary fonts and installed dependencies are excluded from delivery. Logs are normalized only for trailing whitespace at final packaging.

## Restore and continue

The complete all-branches bundle contains original archived refs and both recovery branches. Clone it, then select `recovery/w2.4-20261003`. The original package supplies the unchanged reference/sample inputs. W2.5–W2.8 and later phases remain outside this restoration; phase 2 as a whole is not complete.

Requested worker configuration: gpt-6-astra, xhigh, fast. Actual model and tier are unverified.
