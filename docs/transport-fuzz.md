# Public gzip transport fuzz target

`fuzz/fuzz_targets/transport.rs` exercises the real public `Registry::read`, `write` and `write_with_report` paths with RAW, BDF and SSFN codecs. These are the actual API names; no separate `read_font`/`write_font` API or substitute mock codec is introduced.

Enable `transport-only` in the fuzz package. Inputs are at most 256 KiB. Shared profiles bound total resources to 2 MiB, inflation to 256 KiB, tables/resources to 256 KiB, glyphs to 256, individual rasters to 4096 pixels and resource count to eight. An original model branch generates at most four 16×16 monochrome glyphs. The wire branch feeds arbitrary native/gzip bytes through both independently varied and generous bounded read policies before attempting writes.

A 16-byte control prefix selects codec, native layout, binary/ASCII representation, typed/common limits, strict diagnostic behavior and original glyph dimensions. Remaining bytes are either native/gzip wire input or generated raster entropy. This prefix is harness metadata and is never submitted as part of a font.

RAW profiles cover bit-aligned and byte-aligned layouts, bit/row orders, row padding, byte columns, horizontal strikes and optional load-address prefixes. Writer registries deliberately never receive RAW read options. The independently configured reader knows the layout. Generous generated cases must succeed, so accidentally validating the writer through a default layout-less RAW reader causes a failure rather than being silently allowed as a writer error.

SSFN covers binary and ASCII typed writer profiles. RAW and SSFN vary actual typed budgets independently from common transport limits. BDF has no public typed registry setter: its built-in older writer/transport adapter is tested with common limits, including the selectors labelled `typed` in generic seed names. The harness does not replace the real BDF adapter to invent a setter.

For every successful gzip write, the target checks:

- A single actual gzip asset within the intersected wire/output limits.
- Successful public reading under the identical effective limits and native profile.
- Complete `FontSet` equality against separately serialized native output, including metadata/provenance.
- No-loss source equality for RAW/SSFN; all effective glyph fields for BDF, whose stored source lines may be recanonicalized. BDF pixels, complete glyph metrics, codepoints, sequences and names are checked, while the native-versus-gzip model comparison remains complete.
- Exactly one original incoming diagnostic, including report-bearing error paths.
- Strict rejection of loss before output; loss-free strict output must have identical deterministic bytes and report. A prior incoming loss can reject before native serialization, so its strict report is correctly the original report alone.

Native writes without `.gz` are also exercised with common envelope limits. The public implementation review and existing `tests/transport_limits.rs` counter tests establish that this branch does not repeat serialization; the fuzz target does not duplicate those mock-unit assertions.

`fuzz/seed_transport.py` uses only Python's standard library. It regenerates 302 original seeds and checks them read-only with `--check`; unrelated mutation files are left untouched. Seeds include legal native/gzip/multi-member payloads for RAW, BDF, SSFN ASCII and independently assembled SSFN binary, plus CRC/ISIZE corruption, invalid flags/method, trailing junk, nested envelopes, truncation and inflation over the cap. Gzip field construction follows [RFC 1952](https://www.rfc-editor.org/rfc/rfc1952); DEFLATE is produced independently by Python zlib. No external font artwork is used.

Run:

```sh
source scripts/local-env.sh
export CARGO_TARGET_DIR=$FONTPIXEL_WORK_ROOT/target-transport-fuzz
export CARGO_BUILD_JOBS=2
python3 fuzz/seed_transport.py --root "$FONTPIXEL_WORK_ROOT/transport-corpus"
cargo +nightly fuzz run transport --no-default-features --features transport-only \
  "$FONTPIXEL_WORK_ROOT/transport-corpus" -- \
  -max_total_time=610 -timeout=2 -max_len=262144 -rss_limit_mb=1024
```

The coordinator owns that long run. The worker performed short ASAN smoke and final seed/mutation replay, not performance measurements. Exact evidence and freeze hashes are in `reports/w5.transport-harness*`, `w5.transport-smoke.json`, `w5.transport-seeds.json` and `w5.transport-external-check.json`.
