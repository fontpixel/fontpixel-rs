# Unit A acceptance: WOFF1/WOFF2 full-lifecycle ledger and codec bounds

Scope: `crates/fontpixel/src/woff/**`, additive helpers in
`crates/fontpixel/src/working.rs`, `crates/fontpixel/tests/woff*.rs`, and a
dedicated allocation-measurement test package. Base: main `6297998`.

Design sources: `WORK/owners/w64/NEXT-LEDGER-DESIGN.md` (native WOFF stages 1-6,
exact dependency blocker, WOFF1 backend caveat), `brotli-decoder-alloc-audit.md`,
`brotli-allocation-site-inventory.json`,
`brotli-encoder-helper-allocation-inventory.json`,
`brotli-encoder-fallible-proposal.md` (the vendored/fallible route is **not**
implemented), `woff-mod-batch04.PROPOSED.patch`, `woff-zlib.PROPOSED.rs`.
The phase-6 limitation being closed is the first two bullets of
`docs/w6.1-writer-budget.md` "Known limitations accepted for phase 6".

Hard constraints carried into every item: no default limit or cap is raised; no
strict/loss/report behaviour is weakened and no report entry is removed; no
`#[allow]`; no guessed source credit; resource failures stay
`Error::ResourceLimit`; default output bytes stay identical.

## A1. One ledger per public WOFF operation (goal 1)

Done means:

- `woff1::{encode, decode}` and `woff2::{encode, decode, encode_many}` (and the
  WOFF2 collection paths behind them) create exactly one
  `Budget::new(l.sfnt.resources.max_total_bytes, ...)` per font operation and pass
  it to every stage. No stage creates its own full-size budget any more
  (today `woff1::compress/decompress`, `woff2::compress/decompress`,
  `Sfnt::parse`, `SfntBuilder::build` each start a fresh full ledger).
- Every fontpixel-owned allocation is admitted **before** it is made and its
  lease lives until the storage drops (`Owned`/`Vector`/`Buffer`/`Nested`):
  - WOFF1 decode: directory records, span list, every decompressed/copied table,
    the borrowed table list, assembled sfnt (including the DSIG re-assembly,
    where the old sfnt is dropped before its lease), metadata/private blocks.
  - WOFF1 encode: parsed source directory (`Sfnt::parse_with_budget`), table
    list, canonical sfnt (old canonical dropped before its charge on DSIG
    re-assembly), re-parse of canonical, sorted records, per-table compressed
    output, growable final output (header written through mutable slices only).
  - WOFF2 decode: record directory, tag set, decompressed raw stream (kept
    live while tables copy from it; no credit from `raw.len()`), table map
    (tree nodes + values), glyf/loca/hmtx reconstruction (old table content
    stays charged until the replacement is inserted), builder/table ownership,
    final sfnt.
  - WOFF2 encode: parsed directory, table map, normalized glyf/loca, all seven
    transform streams, bbox bitmap and overlap arrays, source loca offsets,
    per-glyph lengths/points/flags/instructions, assembled transform output,
    hmtx transform, order list, header, raw stream, parallel input clone, the
    `BrotliEncoderMaxCompressedSizeMulti` output buffer and allocator vector,
    final output.
  - Collections: record list, per-face index lists and tag sets, raw stream,
    per-face table maps and outputs, payload list and shared-table map, TTC
    output. A table shared by several faces is charged once (its bytes move into
    one payload); per-face transformed copies remain charged.
- `sfnt/collection.rs` is outside this unit; its `encode` is called with the
  ledger's remaining allowance as its byte cap and its computable output size is
  reserved in the WOFF2 ledger (documented as the interface boundary).

Tests: `woff::ledger_tests` (unit) and `tests/woff_budget.rs` (integration).

## A2. Codec errors keep their class (goal 3)

Done means: any `fontpixel::Error` that travels through `std::io::Error`
(Brotli writer into a ledger `Buffer`, `io::Write for Buffer`) is unwrapped with
`io::Error::downcast::<crate::Error>()`; a ledger failure inside the Brotli
writer is returned as the original `Error::ResourceLimit`, never as
`InvalidInput`. Metadata decode resource failures remain terminal (existing
`read_aux` rule) while invalid streams remain a reported loss.

Test: public-API binary search of `max_total_bytes` for WOFF1/WOFF2 encode and
decode asserting every failure below the boundary is `ResourceLimit`.

## A3. Brotli encoder heap bound (goal 2)

Done means:

- `fontpixel::woff::woff2::brotli_encoder_heap_bound(quality, lgwin, input_len,
  threads) -> Option<usize>` derives a conservative upper bound of the locked
  `brotli 9.0.0` encoder's total heap for fontpixel's call shape
  (`BROTLI_MODE_FONT`, `size_hint = input_len`, all other parameters default,
  single-stream `BrotliCompress` when `threads == 1 || input_len < 128 KiB`,
  otherwise `compress_multi` with `threads` `StandardAlloc` workers). The
  derivation is a doc comment with a per-term table citing `file:line` of each
  allocation site (sum over simultaneously live state, max over strictly
  sequential phases).
- Before calling the encoder, the WOFF2 ledger reserves that bound as a lease
  that lives for the whole encoder call (all threads joined). The dependency is
  neither patched nor vendored; quality, threads, transforms and output bytes
  are unchanged.
- Empirical proof in the dedicated package `crates/fontpixel-heap-check`
  (see A6): counting `#[global_allocator]`, peak bytes during the encoder call,
  qualities 0-11, lgwin 22 (plus 10/16/24 for the formula), threads 1 and 2,
  inputs: empty/tiny, 64 KiB, ~1 MiB, and an optional multi-MB sfnt from
  `FONTPIXEL_HEAP_CHECK_FONT` (skipped with a message when unset). Assert
  observed peak <= bound for every case; print bound/observed table.

**Decision point for the coordinator (expected outcome, not an implementation
choice).** From the locked source, at quality >= 10 the encoder's
`BrotliBuildMetaBlock` -> `BrotliSplitBlock` -> `ClusterBlocks`
(`brotli-9.0.0/src/enc/block_splitter.rs:419-660`) allocates histogram arrays
and pair queues proportional to `num_blocks`, and `FindBlocks`
(`block_splitter.rs:225-330`) does not bound `num_blocks` below the symbol count
of the metablock. At quality 11 `BrotliCreateHqZopfliBackwardReferences`
(`backward_references/hq.rs:1259-1315`) additionally grows a `u64` match array to
about 100 entries per input position of a 2^18-byte block with old/new overlap.
A sound bound therefore exceeds the 256 MiB default for metablocks of a few tens
of KiB at q >= 10. Admitting it makes default (q11) WOFF2 encodes of real fonts
fail with `ResourceLimit`. This unit does **not** shrink the bound, add a
toggle, or change test budgets to hide that; it reports every affected default
call site/test and the full-Tong number (A7).

## A4. Brotli decoder and deflate bounds (goal 2, continued)

- Brotli decoder: every decoder cell already goes through the ledger allocator
  (`woff2/brotli_decode.rs`). Done means the empirical package confirms that the
  heap peak of `woff2::decode` is <= the smallest `max_total_bytes` that
  succeeds plus a small documented constant for unledgered report strings.
- Deflate (WOFF1): `woff/zlib.rs` calls `miniz_oxide =0.9.1` directly, so flate2
  feature unification (`zlib-rs`/C backends) cannot select a different backend
  for WOFF1. Done means `cargo tree -e features -i flate2` and
  `-i miniz_oxide` for the default and all-features profiles are recorded below
  and the empirical package shows WOFF1 encode/decode heap peak <= admitted
  ledger capacity (+ the same constant) for levels 0-9.

## A5. Exact-capacity regressions (goal 4)

For each of: WOFF1 encode, WOFF1 decode, WOFF2 encode with transforms on, WOFF2
encode with transforms off, WOFF2 decode, WOFF2 collection encode and decode:
measure the ledger high-water mark `P` of a successful run (additive
`Budget::peak()` helper), then assert success with `Budget::new(P)` and
`Error::ResourceLimit` with `Budget::new(P - 1)`, and that the budget is fully
released after both. WOFF2 cases use quality <= 9 so the exact boundary is not
dominated by the A3 bound; one additional case covers q11 to show the bound is
part of the admitted peak.

Default bytes: existing byte-identity tests (`tests/woff.rs`, zlib flate2
equality, fixture round trips) keep passing for every case whose bound fits;
WOFF2 output for any case that still succeeds is byte-identical to `main`
(checked against encodings produced with an unlimited ledger).

## A6. Measurement package

`crates/fontpixel-heap-check` (publish = false), a workspace member that is
**not** in `default-members`. Reason: the workspace lint table sets
`unsafe_code = "forbid"`, which cannot be relaxed inside `crates/fontpixel`
without `#[allow]`; a `GlobalAlloc` implementation requires `unsafe impl`. The
package keeps its own lint table (missing_docs, clippy::all) and contains only
the counting allocator and its tests. Run it by name:
`cargo test --locked --release -p fontpixel-heap-check`. Cargo.lock gains one
path-package entry and no registry packages (same `brotli`/`miniz_oxide` pins).

## A7. Default workload report (goal 5)

Report the bound and the observed heap for the default full-Tong square TTF ->
WOFF2 path (`full-correctness-v2/vector-native/font.ttf`, release build), the
extra budget needed beyond the 256 MiB default, and the list of default-limit
tests/call sites that now fail.

## Verification commands

- `cargo test --locked -p fontpixel --features woff,woff2,ttc --lib woff`
- `cargo test --locked -p fontpixel --features woff,woff2,ttc --test woff --test woff_budget`
- `cargo test --locked --release -p fontpixel-heap-check`
- `cargo clippy --locked -p fontpixel --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -p fontpixel-heap-check --all-targets -- -D warnings`
- `cargo fmt --all --check`

## Status

(filled in at the end of the unit)
