# TrueType outlines (`ttf`), W2.2

## Sources and scope

- OpenType [glyf](https://learn.microsoft.com/en-us/typography/opentype/spec/glyf),
  [loca](https://learn.microsoft.com/en-us/typography/opentype/spec/loca),
  [maxp](https://learn.microsoft.com/en-us/typography/opentype/spec/maxp).
- Read-only `reference/bdfparser/docs/SPEC-v3.md` §§7–8 and its MIT implementation:
  exact compatibility point ordering, rounding, metadata, and byte serialization.
- Read-only Tong `tools/export-rs/src/outlines.rs` and `python_reference.py`:
  boundary turns, independent corner-touching pixels, and reusable round-dot glyph.
- Fontations `read-fonts` 0.44 typed readers, with independent structural validation.

This item provides native outline generation and editable glyf/loca tables. The
existing bdfparser facade retains its exact TTF reader/writer. W2.7 now supplies
the native `ttf` registry codec, embedded preference, exhaustive grid detection,
phantom-aware rasterization and single-face writing; see [vector-raster](vector-raster.md).
The table-level contracts below remain independent of that model projection.

## Field mapping

| Wire field | Native representation |
| --- | --- |
| simple glyph contour endpoints and coordinate flags | ordered closed contours, signed coordinates and on-curve markers |
| simple overlap flag | explicit `overlap` flag |
| glyph instructions | preserved opaque bytes, never executed |
| composite component IDs | stable glyph indices |
| signed offsets / unsigned point arguments | distinct `Anchor` variants |
| F2DOT14 transforms | exact signed 2.14 matrix words |
| component metrics, rounding, overlap, offset-scaling flags | explicit semantic flags |
| glyf bounding box | checked record field; recomputed from geometry on encode |
| loca short/long | checked monotone offsets; explicit output choice or auto |
| maxp outline counts | calculated simple/composite maxima and recursion depth |

Table reads are confined to each loca-delimited glyph; counts and instruction
lengths are checked before expansion. Whole-table and expanded geometry budgets
are independent. Component graphs reject cycles, out-of-range IDs and excessive
nesting/work, including graphs made solely of empty glyphs.

## Geometry and choices

Merged squares follow SPEC's stable clockwise outer contours, counterclockwise
holes, lower-left start, and right turns at shared corners. Independent squares
stay independent even at diameter 1. Native dots use Tong's four quadratic arcs,
rounded to integer design units. One reusable dot glyph is translated once per
ink pixel in composite output. SPEC circles keep their existing eight off-curve
control-point approximation and exact bytes; the two contracts are deliberately
explicit. Design-unit geometry uses y up; source bitmap rows use y down.

Transforms are unhinted. Offsets without either scaling flag use the OpenType
recommended unscaled convention; explicit scaled offsets are transformed. Point
attachment applies after the child transform. Phantom-point attachment requires
metrics/hinting outside this table layer and errors rather than approximating.
`ROUND_XY_TO_GRID` is retained but not applied in design space: grid fitting needs
a device scale. `USE_MY_METRICS` is retained for the metrics consumer.

## Preservation and loss

Decode/edit/encode preserves contours, component relationships, transforms,
instructions and semantic flags, not source packing/padding or stale bounds.
Canonical output recomputes bounds and maxp geometry; compression uses short
coordinate deltas and repeated flags. Empty glyphs use equal loca offsets.
No instructions or variation deltas are executed by outline flattening.
Rasterization is unhinted nonzero-winding pixel-centre fill; quadratic intersections
are evaluated on monotone curve sections without chord approximation. A successful geometric projection does not assert a
whole-font lossless conversion. Existing compatibility BDFP metadata behavior and
full default differential tests remain mandatory regression checks.

The compatibility reader additionally limits whole-font raster/model expansion:
64 Mi pixels, 256 MiB estimated raster/provenance/row storage, 64 Mi expanded
points, and 32 Mi component visits. These are charged cumulatively, including
zero-width rows, before raster allocation. Per-glyph bounds remain in force.
