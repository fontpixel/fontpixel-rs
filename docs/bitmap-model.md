# Bitmap and model API (W1.1)

This is the core used by readers, writers, rendering, and compatibility
adapters. W1.2 now registers actual feature-gated BDF hooks in
`fontpixel::registry()` (`Registry<FontSet>`); other catalogue entries remain
honestly unimplemented. Custom readers
and writers implement the existing `FormatReader<FontSet>` and
`FormatWriter<FontSet>` contracts, including sidecars and combined strict reports.

## Bitmap representation and resource limits

`Bitmap` is `Bitmap<Mono>` by default. Each row has `ceil(width/8)` bytes; bit 7
is leftmost. Right padding bits are always zero, including after inversion,
flipping, compositing and importing dirty padding. `Bitmap<Gray8>` has one byte
per pixel, with 0 empty and 255 full ink. No conversion happens implicitly.
`to_gray8()` expands 0/1 to 0/255; `to_gray8_palette(paper, ink)` is explicit.
`to_mono(threshold)` selects values **greater than or equal to** the threshold;
pass 128 for the recommended font-import default. A writer performing this
lossy conversion must add an `Impact::Loss` report entry.

Constructors and allocating operations return `Result`. `BitmapLimits` defaults
are 1,048,576 per dimension, 16,777,216 pixels, and 67,108,864 output bytes.
A bitmap carries its allocation policy; derived outputs inherit it. Callers can
supply tighter limits or explicitly raise them. `set_limits` validates before
changing the policy. Whole-array allocation is fallible; sizes are checked before
allocation. The bounds are per result, not a process-wide/peak-resident-memory
budget. Some transforms keep the source and one or more temporary results alive.
Codecs must use the read context's budgets before constructing data, rather than
first constructing an unbounded model and only then calling `FontSet::validate`.

Width and height are both explicit, even for `W×0` or `0×H`. They participate in
equality. Pixel reads/writes beyond the rectangle fail; extreme signed offsets
clip correctly rather than wrapping. A zero-ink `trim()` returns precisely 0×0.

## Coordinate systems and operations

- `get_pixel`, `set_pixel`, `paste`, and `composite`: top-left origin, y down
- `crop`, `shift`, `shadow`, and `bbox`: bottom-left origin, y up
- `rotate(angle)`: clockwise, signed multiples of 90 only
- `extend(left,right,top,bottom)` and `byte_pad(multiple)`: zero margins
- `enlarge(x,y)`: nearest-neighbor integer factors >=1
- `concat` / `concat_all`: four directions, perpendicular start/end alignment,
  signed gaps (including overlaps and gaps extending beyond preceding content)
- `invert`: binary predicate inverse; `invert_with(1)` preserves label-1 output
- Gray8 `negate`: continuous-tone 255-value inversion
- `bold`, `italic`, `shadow`, `glow`, and sample/pattern `replace`
- `bbox`, `trim`, `has_ink`, `PartialEq` / `Eq`, and owned `Clone`

Geometry/effects that change dimensions return a new bitmap; source data is not
mutated. `paste`, `overlay`, `composite`, `invert`, `replace`, and `set_pixel` are
in place. There is no trait-based `+`/`*` hiding fallible allocation. The later
compatibility facade will expose SPEC's mutating/chaining method signatures.

The compositing kernel works a row/byte at a time, not by pixel. Packed full-byte
runs select operation and alignment once; aligned copies use slice copies, and
other runs can be vectorized. Only the leading/trailing partial bytes need masks.
Gray8 composition works directly on byte slices. `concat_all` places all inputs
in one output allocation; no intermediate growing raster. Mono bold uses
logarithmic bytewise dilation, Gray8 bold a linear sliding nonzero window.

`Over` is SPEC's **nonzero source wins** rule. It is separate from `AlphaOver`
(the coverage union `a+b-round(a*b/255)`). Other rules are Replace, bitwise
Or/And/Xor/Clear, saturating Add/Subtract, Min/Max, and coverage Multiply. Rules
apply only within the source/destination intersection; pixels outside it remain
unchanged. Integer coverage multiplication uses nearest rounding. All Mono
rules reduce to their exact Boolean counterparts.

## SPEC-v3 0/1/2 labels

The core does not squeeze three labels into one bit, nor silently map label 2
through the usual 128 threshold. A compatibility bitmap can use Gray8 **as an
explicit label raster** with byte values 0, 1 and 2:

- `from_text(..., [('.',0),('#',1),('&',2),('0',0),('1',1),('2',2)], limits)`
- `shadow(dx,dy,2)`; original nonzero pixels win
- `glow(corners,1,2)`; only original label 1 generates glow, so label 2 does not
  spread again on repeated calls
- `invert_with(1)` and explicit output text/RGB/RGBA palettes
- Convert labels to ink using `to_mono(1)`, not the grayscale recommendation 128

The 3,841 checked-in Python-oracle cases exercise the shared operations and exact
pixel labels, plus ink bounds. Byte/string conversion edge cases have direct
Rust tests. Boundaries for W1.5 adapters are deliberate and documented:

1. Python represents `W×0` as an empty row list and therefore loses W; core keeps it
2. Python `enlarge` factors <=1 are no-ops; its adapter uses `max(factor,1)`
3. Core rejects ragged pattern-replacement output. If full compatibility cases
   require transient ragged legacy rows, W1.5 must preserve them in its adapter
   (e.g. as individually validated rows), rather than substitute an error for a
   successful reference operation or corrupt a rectangular Glyph bitmap
4. Core `overlay` is silent about size mismatch; compatibility attaches warnings
5. `to_row_values` supports <=64 bits; arbitrary-width exact rows use numeric hex,
   with no truncation. Binding/JSON numeric representation belongs to W1.5/W7/W8
6. Pillow mode-1 export's mapped background/padding semantics are adapter-level;
   core font wire bytes always start with zero padding before order transforms
7. `tosvg`/outlines belong to phase 2; JS-only canvas drawing belongs to bindings

## Wire bytes and text

`ByteLayout` supports MSB/LSB bits, big/little scan-unit byte significance, 1/2/4/8
byte row alignment and scan units, and top-down/bottom-up rows. It follows X11:
bytes within scan units are reversed when bit/byte orders disagree. A scan unit
larger than row alignment is rejected in that case, avoiding partial/cross-row
swaps. This is also SPEC-v3's PCF safety rule. PCF itself permits only its own
subset of these options; the PCF codec must validate format-level restrictions.
Gray8 accepts row alignment/order but rejects irrelevant packed-bit settings.
Import requires exact byte length and normalizes padding. Export is deterministic.

`to_hex_rows` is byte-padded uppercase BDF representation; `to_numeric_hex_rows`
is lowercase, right-aligned, ceil(width/4) digits like SPEC `todata(4)`. Text palettes
support Unicode characters. Unknown characters/values, ragged rows, duplicate
palette keys, zero padding multiples and incompatible layouts return errors.
CR is ignored and blank outer lines skipped when parsing text. A plain textual
representation cannot unambiguously preserve all degenerate zero-area dimensions;
use explicit width/height plus bytes for lossless transport of those shapes.

## Glyph, Font and FontSet

`Glyph` preserves ordered zero/multiple codepoint aliases, Unicode sequences,
optional names, horizontal/vertical/scalable advances, BBX, VVECTOR, ABC widths,
independent bearings and image-relative offsets. Mono and Gray8 are explicit
`GlyphBitmap` variants. `set_bitmap` optionally trims and adjusts bearings without
changing advance; `bitmap_in_cell` aligns ink to an arbitrary origin-relative cell.
Glyph validation requires the BBX dimensions to match the raster.

Codes are tagged `Codepoint::Unicode` or `Legacy`; legacy codes use the strike's
`FontCharset`. Unicode-tagged numeric surrogates and out-of-range source codes (e.g. 0x110000)
are preserved for tolerant SPEC/BDF parity; `Codepoint::scalar()` returns None
for them. Writers and scalar consumers must reject/report them as appropriate. Empty Unicode
sequences are invalid; duplicate aliases and duplicate source glyph codes survive.

`Font` owns file-order slots and BTree indexes for codes and whole Unicode
sequences. Lookup/ranges are O(log n), ordered traversal O(n); no full scan is
needed for lookup. The effective mapping is the **last file-order glyph**;
`glyph_ids_for_codepoint` exposes every duplicate for diagnostics. `GlyphId` is a
stable insertion index; deletion leaves a hole rather than retargeting IDs.
`edit_glyph` and `replace_glyph` validate and update indexes transactionally.
An adapter that wants BDF's replacement behavior must explicitly remove/replace
the target, rather than assuming core insertion discards duplicates.

Global metrics distinguish absent from zero. Exact `Rational` point sizes avoid
float artifacts. Names retain family/style/full/PostScript/version/copyright/
license/foundry and full XLFD plus ordered fields. Charset metadata preserves raw
registry/encoding, explicit GL choice, Windows byte/code page and code ranges;
`codec()` uses W0.2's exact tables and explicit disabled-feature errors.

`Properties` is an ordered sequence, not a unique-key map: original spelling,
duplicates, key-only flags, integers, decimal text, strings, lists and opaque bytes
survive. Optional raw record bytes retain quoting/line endings/provenance. Semantic
edits clear raw bytes for that property. Raw bytes are **not automatically
canonical** after any model edit; format writers must reconcile normalized fields
and source records, and report any unavoidable loss. `headers`, per-font and
per-glyph properties keep their respective source order independently.

`FontSet` preserves ordered strikes plus container metadata. `single`/
`into_single` reject zero or multiple fonts. `Loaded<FontSet>::into_single` keeps
read diagnostics and format identity alongside the extracted `Font`. `validate`
checks references, glyph consistency, aggregate glyph count/bitmap bytes and
per-glyph pixel budgets; it does not replace parse-time allocation guards.

See [benchmark results](../benches/REPORT.md) and [decisions](decisions.md).
