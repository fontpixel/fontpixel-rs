# fontpixel W2.1–W2.3 validated recovery

Date: 2026-10-03 UTC. Status: recorded sources reconstructed and fresh validation passed.

## Result and history boundary

The accepted W2.1 implementation plus W2.2 native outline/glyf and W2.3 CFF work are reconstructed on `recovery/w2.1-w2.3-20261003`. Original archived branches remain untouched: `main` at `caaeecd`, `w2.1-sfnt` at `76ab7eb`.

This is new recovery history, not recreation of the lost original Git objects. The archive contains a complete all-branches Git bundle and this report. W2.4 is not included in this checkpoint; it has a separate salvage/integration scope.

- `4f1bfd6`: 12 surviving W2.1 acceptance documentation/evidence files. The original acceptance revision changed no production source or tests. Ten original evidence files remain missing.
- `34d8482`: W2.2 initial source payloads, recorded sequential edits, and intervening pinned Rust 1.99.0 formatting. All 67 audited replacement anchors matched.
- `bb31cf4`: W2.3 recovered source/test/example/fuzz/docs plus recorded feature/module/append-only integration changes.
- `aecdb9a`: first buildable checkpoint, saved to Library before long tests. It deliberately lacked regenerated CFF fixtures and fresh acceptance results.
- `5c81ede`: interim durable checkpoint retaining all completed checks while the slow full-SC CFF comparison continued.
- Final recovery commit adds the completed validation evidence. Six regenerated fixtures are explicitly labelled. No production-code repair, redesign or new feature was needed.

## Historical production-source fingerprints match

The exact original parity fingerprint algorithm gives W2.2 `63c4972f231fa4b4` and W2.3 `873ec98e9affe795`, both matching retained historical observations. Fresh full parity independently records W2.3 `873ec98e9affe795` at start/end with clean crate sources; Python remains `938d9ed560ef29ba`.

These are 16-hex truncated SHA-256 fingerprints over relative pathnames and bytes under `crates/fontpixel/src/**/*.rs` plus `crates/fontpixel-cli/src/parity.rs`. Matching them strongly corroborates exact production-source recovery in that scope, but does not authenticate every test/doc/fuzz input or restore old Git objects and evidence.

Every W2.3 production/test/example Rust file equals the Rust 1.99.0-formatted recovered payload. The separate fuzz target retains the recovered raw payload. All W2.2 recorded edits matched their anchors. An initial diagnostic incorrectly sorted Git pathname strings instead of Path objects componentwise and therefore reported a mismatch; the actual harness and corrected diagnostic establish the matches above. No mismatch claim remains outstanding.

## Fresh validation

- Locked all-feature workspace libraries, binaries and examples build successfully.
- `scripts/check.sh`: exit 0, 546 aggregate test executions; generated data checks, rustfmt, both Clippy configurations, feature matrices, and cargo-deny pass.
- Full default Python/Rust parity: 0 NEW divergences. All 67 charset tables, 2,000 random cases/35,941 operations, 1,000 random_all cases/17,588 operations, 41 probes/89 operations, and all 251 corpus cases/5,221 operations ran. Existing K12 counts remain six random_all and one probe.
- Native sfnt external check and sfnt generator check: pass. Separate sfnt-only configuration and strict all-feature rustdoc: pass.
- Native glyf external default: 16 cases / 30,091 glyph checks. Full SC: 4 cases / 77,317 checks. Both pass.
- CFF external default: 20 cases / 30,094 glyph checks. Full SC: 8 cases / 77,326 checks. Both pass. Synthetic fixture generation independently passes six cases / 18 glyph checks.
- External checks include fontTools, OTS, FreeType and HarfBuzz as implemented by the recovered drivers; full-SC tests cover every SC source glyph in square and dot forms.
- Fuzz workspace builds and dependency checks pass. Each restored reader completed 611 seconds with AddressSanitizer enabled, leak detection disabled under the recorded sandbox configuration:
  - sfnt: 6,124,739 executions
  - glyf: 6,132,071 executions
  - compatibility TTF: 314,472 executions
  - CFF: 18,109,323 executions
- 690 protected reference/sample files: zero changed, missing or extra files. Salvage inputs were never edited. Cargo.lock and fuzz/Cargo.lock are unchanged from the restored baseline.

Full fresh logs, exit codes, structured crosscheck results, parity summaries, source hashes, audited replay anchors and commands are committed under `reports/recovery-20261003/`. These new results are distinct from historical W2.1–W2.3 reports and surviving old evidence. Cargo-deny passed; no separate assertion of online advisory-database freshness is made.

## Regenerated and still-missing material

Six tiny CFF fixtures were regenerated from the retained original recipe using fontTools 4.66.1. Their new `tests/data/cff/SOURCES.json` explicitly labels them regenerated and records their current SHA-256 values. The original binary hashes are unavailable, so these bytes are not claimed as recovered originals.

Lost original commits, complete historical logs, original fixture manifest and old full evidence hash manifests remain unavailable. The historical missing-artifact inventories are preserved beside this report. Historical reports retain original references and old results; they do not turn missing old files into recovered evidence. Original W2.1 source hashes refer to its historical stage, not later integrated source changes.

Fresh logs have only trailing line whitespace and trailing blank lines normalized for Git whitespace checks. Test content, statuses and counts are unchanged. Build outputs, temporary fonts, dependencies and generated fuzz corpora are excluded from the bundle.

## Toolchain and restoration

Rust/cargo 1.99.0, nightly 1.101.0-nightly (2026-10-02), Python 3.12.14, cargo-fuzz 0.13.2 and cargo-deny 0.20.2 were installed from official toolchain/registry sources into the authorized writable workspace. The default home install initially failed because the home directory is read-only; no home or security permissions were changed. One broad temporary-directory metadata listing encountered an unrelated denied daemon directory; no access was retried and test execution was unaffected.

Restore with `git clone fontpixel-rs-all-branches.bundle fontpixel-rs`, then select `recovery/w2.1-w2.3-20261003`. The verified bundle includes the original archived branches and all new recovery commits. Protected reference/sample inputs remain in the original task package.

Requested worker configuration: gpt-6-astra, xhigh, fast. Actual model/tier cannot be verified from this environment.
