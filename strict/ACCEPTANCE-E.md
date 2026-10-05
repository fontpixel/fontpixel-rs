# Unit E acceptance: catalogue parity through explicit options

Scope: explicit bitmap projection (FONT/XLFD, SIZE, bounded property edits,
existing SWIDTH), PCF writer metadata precedence, vector `.notdef` policy, CLI
and batch wiring, and a new catalogue adapter profile. Defaults never change.

Design sources: `WORK/owners/w66/CATALOGUE-BITMAP-PROJECTION-PROPOSAL.txt`
(§4 API, §5 provenance/strict, §6 budget/transaction, §7 a–f),
`WORK/owners/w63/CATALOGUE-ACTUAL-FIX-PROPOSALS.txt` (item 2, `.notdef`),
`docs/LOCAL-PAUSE-20261005.zh.md` §5. `WORK` =
`local-work/phase6-local-20261004`; `E` = `local-work/strict-e`.

Commits: `9db3b7c` (projection, PCF precedence, CLI, adapter), `0e45bc6` and
its follow-up (vector `.notdef`: new `variants/notdef.rs` plus hookup lines in
unit C files `variants.rs`, `variants/export.rs`, CLI `opentype.rs`).

## Checklist

### Library: `fontpixel::projection`
- [x] E1 Default changes nothing — `projection::tests::default_preserves_everything`.
- [x] E2 `font_name` sets `names.xlfd`, resyncs `names.xlfd_fields` (reader
      rule, empty fields kept) and an existing `FONT` property; rejects empty,
      padded, multi-line, NUL, > 4096 bytes —
      `font_name_resyncs_fields_and_font_property_with_audit`,
      `rejects_unsafe_or_ambiguous_edits_without_change`.
- [x] E3 `SizeProjection { point_size, resolution }`, positive only; BDF SIZE
      follows via `reconcile` — `size_replacement_is_loss_and_absent_size_is_repair`,
      `pcf_precedence_keeps_existing_size_properties_and_reports_loss` (BDF part).
- [x] E4 Typed edits `Set{name, EditValue::Integer|Text}` / `Remove{name}`, ≤ 64,
      one per name, token names; rejects COMMENT, notice-like names
      (COPYRIGHT/NOTICE/LICENSE/LICENCE), `bdf.`/`pcf.` provenance, `FONT` set,
      normalized name/charset/spacing/default-char keys; integer metrics
      (PIXEL_SIZE, FONT_ASCENT/DESCENT, UNDERLINE_*, SPACE_WIDTH, LINE_GAP,
      LINE_HEIGHT) sync the model — `property_edits_classify_sync_metrics_and_keep_comments`,
      `rejects_unsafe_or_ambiguous_edits_without_change`.
- [x] E5 Replace/remove = Loss, add = Repair, equal = no change and no entry —
      same tests plus second-apply check in the font-name test.
- [x] E6 Typed `MetadataChange { font_index, field, before, after }` per changed
      record (incl. synced metrics) and per-change report entries; SWIDTH audit
      unchanged — `projects_all_physical_records_with_full_audit`,
      `font_name_sync_precedes_font_property_removal`.
- [x] E7 Validate and stage every strike first, commit only moves staged values
      — `later_strike_failure_and_budget_failure_commit_nothing`,
      `failure_in_a_later_strike_leaves_every_strike_unchanged`.
- [x] E8 Upper bounds (replacement property lists incl. coexisting old/new
      records, staged strings, audit, report) admitted against
      `max_working_bytes` before allocation; exact-bound probe in the budget
      test; CLI charges argument strings, edit slice, live model, earlier
      report and the outcome (`metadata::project`).

### PCF writer
- [x] E9 `PcfWriteOptions::metadata_precedence`, default `NormalizedEdits`;
      existing `tests/pcf.rs`, `pcf_reference.rs`, `pcf_properties.rs` pass;
      default-profile catalogue outputs byte-identical to frozen v2 (below).
- [x] E10 `ExplicitProperties` keeps existing POINT_SIZE/RESOLUTION_X/Y/RESOLUTION,
      generates absent ones from the projected model (Spleen RESOLUTION 103,
      Cozette POINT_SIZE 120), reports `pcf.write.property_precedence` Loss —
      `pcf_precedence_keeps_existing_size_properties_and_reports_loss`,
      CLI `pcf_precedence_is_explicit_and_reports_kept_properties`.

### Vector `.notdef`
- [x] E11 `NotdefPolicy::{Preserve, Empty{advance_width, left_side_bearing}}`
      (design units) in new `variants/notdef.rs`; empty outline at GID 0 with
      exact hmtx; replacing a source `.notdef` (`encode_single`) reports
      `variants.notdef.replaced` Loss — `variants::notdef::tests::*`, CLI
      `variants_empty_notdef_has_exact_design_metrics_and_empty_outline`.
- [x] E12 Hookup lines in unit C files isolated in commit `0e45bc6` (+ follow-up).

### CLI / batch
- [x] E13 `convert`: `--font-name`, `--point-size` + `--resolution X,Y`,
      `--set-integer-property NAME=N`, `--set-text-property NAME=TEXT`,
      `--remove-property NAME` (repeatable), `--pcf-metadata-precedence`
      (`normalized-edits`|`explicit-properties`, PCF only); batch options with
      the same names — `tests/projection_cli.rs` (11 tests).
- [x] E14 `variants --notdef empty --notdef-advance N [--notdef-lsb L]` (hookup commit).
- [x] E15 `// Unit D hookup slot` comment in the `Convert` arguments.
- [x] E16 No-option output unchanged: `projections_equal_to_the_source_leave_output_byte_identical`
      and all 181 default-profile catalogue downloads byte-identical to
      `WORK/full-correctness-v2/catalogue-native/downloads` (`E/catalogue-native-default`).

### Catalogue adapter
- [x] E17 `benches/phase6_catalogue.py --profile explicit` (default profile
      unchanged; `benches/phase6_catalogue_test.py` 9 tests). Per-task values
      come from the application's own parsed model, the same inputs
      `ingest/bdfwrite.py:18-30` uses. Unrenamed, unmerged PCF downloads convert
      the source PCF directly (`downloads.py:261-267`), no charset step.
- [x] E18 Native explicit run `E/catalogue-native` (173/173 tasks), validator
      proof `E/catalogue-native-proof.json`, comparison `E/comparison.json`.
      Unresolved fonts 164 → 132; BDF/PCF pass 8 → 40 (all 34 PCF and Proggy
      BDF pass; the 28 remaining BDF differ only by retained source COMMENT
      lines); archives pass 3 → 5; failed 1 → 1 (Spleen64 consumer-01).

### Gates
- [x] Focused tests, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
      `cargo fmt --all --check`. No `#[allow]`, no default-cap change, no
      deleted report entries, no COMMENT/notice removal.
