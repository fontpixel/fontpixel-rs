# W5.D Apple / Qt native format group

Implemented original Rust readers and writers for fbit/HFNT, Lisa font.lib,
MGTK, QPF and QPF2, plus NFNT BinHex 4.0 transport. All production paths are native;
there is no subprocess, Python, Qt, filesystem fallback or font-tool dependency.
The root agent owns shared feature/registry/CLI integration and final long fuzz
and performance campaigns. This report covers this worker's implementation and
short validation only. Specifications were recorded in docs/formats before code.

## Native behavior and model contract

- **fbit/HFNT:** Shift-JIS run ordinals, continuous cells, fdef 0/5/7 storage,
  native HFNT word/code tables, explicit inferred-width override, retained resource
  metadata, all existing Mac envelopes including BinHex, explicit external data
  forks and AppleDouble bundles. Envelope signatures precede the bare fbit tag:
  a MacBinary name beginning `fbit` is valid. HFNT's unknown words can also spell
  fbit; the explicit bare kind and retained resource kind remain authoritative.
  A data fork is retained once per collection, avoiding per-font fork copies.
- **Lisa:** native Pascal directory names, word offsets, bitmap and separate
  metrics resources, 9000/B000 and 9080/B080 rasters, reverse zero-mask expansion,
  row XOR, native compression generation, unknown printer entries retained with
  explicit read Loss. Exact shared resource ranges are permitted, partial overlap
  is rejected, and every expansion is charged cumulatively. Native in-place
  expansion must never overwrite the unread compressed prefix. Explicit compressed
  output rejects an unsafe stream; default Preserve can emit plain native records.
- **MGTK:** both seven-bit strip layouts, row ordering, width/advance semantics,
  zero-width cells, ignored high bits and trailing bytes retained in source.
  Canonical output explicitly audits geometry and source changes.
- **QPF:** all three preorder passes, iterative bounded trees, signed metrics,
  Mono/Gray8 and padded stride, BMP and native surrogate positions, balanced
  canonical trees. Filename-only family identity is an explicit read option.
- **QPF2:** native tagged headers, sfnt cmap, GMap holes and stable IDs, aliases and
  supplementary Unicode mappings, native signed geometry and advance, cached
  rasters, fractional metrics and external linkage retained with explicit read
  Loss. No external outline is implicitly opened. A cached glyph zero remains
  unencoded data, not an invented default character. Default Gray8 is consumable
  by Qt 4.8.7; explicit packed Mono is wire support, since that engine's alpha
  renderer ignores the format tag and assumes Gray8.
- **NFNT BinHex:** HQX alphabet, 90 RLE, all three CRCs, forks and Finder metadata,
  historical optional final `!`, same-asset multipart separators, bounded lengths,
  truncation and decompression checks. Prefix/suffix text and header/fork semantics
  are preserved; whitespace and RLE choices are canonicalized. Separate segment
  files are caller-managed resources. Existing NFNT functionality remains tested.

All codecs expose Default/Clone/Debug typed options and registry traits. New
writers audit every supplied model field through an actual same-budget reread,
including ordered properties, glyph IDs, trailing removed slots, aliases,
sequences, geometry and metrics. Required Gray8 reduction is explicit. Unsupported
fields produce Loss or Error and registry strict writing rejects Loss. Missing
optional input values may acquire format-defined defaults. Native properties are
opaque format semantics rather than private extensions for unrelated model data.

Common allocation accounting covers source, model, retained metadata, temporary
raster/table data, output, readback and report entries; Lisa and BinHex also apply
cumulative decompression limits before allocation. The shared original helper is
`formats/fbit/common.rs`, exposed privately as `w5d_common`. fbit/lisa reuse the
existing bounded NFNT raster/envelope layer via a private bridge; QPF2 reuses the
bounded sfnt cmap layer. No new dependency was added. Hot paths are linear or
O(n log n) table/range operations; no whole-fork copy occurs per fbit strike.

## Verification

The final tests include 71 native regressions: fbit 8, Lisa 8, MGTK 7, QPF 6,
QPF2 8, and NFNT 34 (30 existing plus four BinHex tests). Each isolated feature
passes all-target tests, all-target Clippy with `-D warnings`, and rustdoc with
`RUSTDOCFLAGS='-D warnings'`. The combined example/test Clippy gate also passes.
`gates.json`, `alltargets-gates.json`, `lisa-final-gates.json` and `commands.json`
record commands/exits and raw log hashes. The last Lisa test adds shared-range
expansion budgets and opaque printer-resource preservation.

Independent crosscheck: **73 checks** in `crosscheck.json` with command stdout,
stderr, binaries and source hashes:

| Consumer | Checks | Actual scope |
|---|---:|---|
| Unmodified monobit 0.54.0 | 42 | fbit/HFNT, native envelopes, Lisa compatible records, MGTK, NFNT |
| Qt 4.8.7 original font engines | 12 | QPF/QPF2 metrics, alpha pixels, aliases and supplementary mappings |
| Original a2d dump_font.pl | 9 | MGTK original/replayed/canonical raster rows |
| Original a2d make_font.pl | 3 | Independently generated MGTK fonts read by Rust and monobit |
| Lisa document's original f.c | 5 | Original and Rust-generated compressed records |
| Unmodified monobit BinHex decoder | 2 | Both forks, with exact resource bytes on replay and semantic comparison after canonical NFNT regeneration |

Qt was built from the official 4.8.7 source archive, using embedded x86_64 and
configure flags disabling obsolete dialogs. QtCore, QtNetwork and QtGui plus the
actual QPF1/QPF2 engines built successfully. The overall `make sub-src` later
failed in the unrelated accessibility plugin (exit 2); this is not described as
a successful complete Qt build. An original thin C++ test driver links the actual
engines; their parsing/rendering implementations were neither copied into Rust
nor adapted. `build-oracles.sh` reproduces the external consumer compilation.
Original large build logs are losslessly gzip-archived under `external-logs/`;
their original-byte hashes and paths are in `external-logs.json`.

Lisa's public document is explicitly empirical, not an official Apple guarantee.
Its original f.c appendix was extracted after the appendix marker, with PDF form
feeds removed and no implementation changes, then compiled externally. Plain
Lisa resources can validly omit Macintosh's extra FFFF terminator. Unmodified
monobit rejects two such original/replay cases; these are recorded separately in
`upstream_failures`, not counted as passes. For the original C consumer's output,
a separate compatibility copy adds only that terminator before the independent
NFNT parser; the C output itself is preserved and hashed. The five C checks test
compressed records; the original plain layout also has a specification regression.

BinHex's upstream decoder logs CRC errors rather than reliably raising; the
crosscheck captures error-level logs and asserts there are none. Its unsupported
writer is not claimed as an oracle. fbit's Apple technote mirror was unavailable;
the fact declaration quoted by the MIT monobit reader and independent read/write
comparison are the available evidence. HFNT unknown words stay explicitly opaque.

Short ASAN runs for six targets each requested 20 seconds (actual 21 seconds),
with 64 KiB inputs, bounded models/decompression, 1 GiB RSS and three-second case
timeout. `commands.json` records final execution counts, exits and log hashes.
These short runs are not the root's required >=610-second campaigns. Fuzz harnesses
exercise bounded detection, native replay, canonical regeneration, same-limit
readability and no-loss model equality after removing regenerated source blobs.

`seed_w5d.py` authors all fixtures from field descriptions, including the
MacBinary filename collision. It also creates seven separately marked benchmark
fixtures: 256 glyphs for each format and 2,048 glyphs for QPF/QPF2. The debug driver
validated canonical read/write geometry for all seven; no formal timing was taken
by this worker. The root can run five quiet release `time` and `time-canonical`
measurements through `w5d_timing` for native replay and canonical encoding.

## Preserved failures and corrections

The first Lisa all-literal compressed fixture exceeded its native in-place body
buffer. The original C consumer aborted; its log and original bad fixture remain
`lisa-native-consumer-initial.log` and `lisa-unsafe-compression.bin`. Production now
checks every reverse cursor transition, rejects this stream, and emits only safe
compressed output. Native validation subsequently passed all five cases.

An original MacBinary collision seed used an arbitrary creator `TEST`, which
monobit's automatic transport detector did not recognize. Rust decoded it. The
final independently authored collision seed uses the usual FFIL/DMOV font creator,
and all three original/replay/canonical checks pass; the exploratory failure log
is retained. The production reader does not impose that creator restriction.

Early test-only failures (wrong field path and expansion bound excluding the type
word) and the shared extension-catalogue assertion are retained as initial logs.
The final tests fix those expectations, and the root updated the shared catalogue
assertion. No failed result is counted as a passing gate.

## Attribution and root integration notes

All Rust implementations, tests, seed data, crosscheck orchestration and the C++
driver in this change are original MIT project material. No historical glyph
artwork is redistributed. `sources.json` records consulted documents and external
consumer bytes, URLs and SHA-256 hashes. monobit 0.54.0 is MIT (Rob Hagemans and
contributors). Qt sources carry LGPL/commercial alternatives and remain external;
no Qt implementation is redistributed. The a2d repository has no established
repository-wide license for its font scripts; its tests/LICENSE does not license
bin/*.pl. Those scripts and the Lisa appendix consumer are executed externally,
not copied into this repository. No license is invented for public documents.

Suggested NOTICE addition: cite Peter N. Lewis's BinHex definition, Apple Japan
Technote 100004 via the MIT monobit declaration, KreativeKorp's empirical Lisa
format description, a2d's MGTK record documentation, and Qt's published packed
format declarations; explain original implementation and external-only consumers.
Suggested decision entries: envelope precedence for fbit filenames, scoped
AppleDouble companions, cumulative decompression including the Lisa type word,
Lisa native in-place compression safety, glyph-zero QPF2 semantics, Gray8 default
for Qt interoperability, and full slot_count auditing. Shared Cargo/registry/CLI,
NOTICE and decisions are root-owned. Final long fuzz, performance and repository
acceptance remain root responsibilities; no further production work is pending
in this owned group at freeze.
