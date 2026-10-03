# Vector rasterization (W2.7 design)

## Sources and scope

PLAN §§5.1, 8.1 and FORMATS `vector-raster`; the read-only catalogue importer
`reference/fontpixel/pipeline/opf/ingest/rasterize.py`; OpenType's
[font organization](https://learn.microsoft.com/en-us/typography/opentype/spec/otff),
[head](https://learn.microsoft.com/en-us/typography/opentype/spec/head),
[hmtx](https://learn.microsoft.com/en-us/typography/opentype/spec/hmtx) and
[vmtx](https://learn.microsoft.com/en-us/typography/opentype/spec/vmtx) specifications.
Existing bounded TrueType/CFF1, bitmap and webfont codecs remain authoritative.

## Projection and choices

* Auto detection examines every expanded outline coordinate, including component
  translations and quadratic/cubic controls. Integral coordinates must share a
  divisor greater than one, dividing unitsPerEm, yielding ppem 4–64. No sampling
  may silently accept a late off-grid glyph. Small generated fonts are valid;
  the importer's heuristic minimum of twenty sampled glyphs is not required.
* Explicit ppem permits reviewed off-grid/curved fonts. Optional grid-unit and
  pixel-origin overrides are explicit. Hinting and variable-axis selection are
  not silently approximated. Unsupported outline programs fail through existing
  readers; variable outlines must fail until instantiated externally.
* Matching physical embedded strikes take precedence over outlines. Auto with
  embedded strikes preserves their sizes without requiring a detectable outline
  grid. Explicit ppem selects exact x/y sizes; missing glyphs use outlines at that
  same size when available, preserving all glyph IDs, aliases and unencoded glyphs.
* TrueType quadratics and CFF cubics are filled at pixel centers with nonzero
  winding, with analytic monotonic splitting and deterministic root bisection.
  No FreeType runtime dependency, hint execution, flattening tolerance or dropout
  control. Bounds are tightened to actual lit pixels; blank glyphs retain advance.
* Font names, horizontal/vertical metrics and Unicode cmap aliases map into the
  native Font. Glyph order is source glyph-ID order. Design metrics use nearest,
  ties-to-even integer rounding and report fractional rounding. Table/layout,
  hinting and color projection losses are reported; original binary editing is
  the separate low-level/subset API.
* TTF/OTF/WOFF/WOFF2/TTC registry hooks use this path. Explicit `vector-raster`
  accepts all these wrappers without competing against canonical detectors.
  Writes vectorize through the existing native exporter; TTC maps each Font to
  one face rather than guessing that multiple Fonts are strikes of one face.

## Resource and validation contract

Per-input, table, glyph, expanded-point, intersection, model-byte and collection
aggregate budgets apply before allocations. Tests cover off-grid late glyphs,
transformed composites, holes, quadratic/cubic curves, embedded priority/sparse
fallback, negative origins, aliases, empty outlines, malformed inputs and limits.
External checks use fontTools, FreeType, HarfBuzz and OTS, with source bitmap grid
truth kept separate from FreeType's dropout policy on deliberately off-grid fonts.

## Additional fidelity boundaries

TrueType raster origins use the horizontal phantom-point displacement from the
stored glyf box and hmtx bearing. USE_MY_METRICS resolves recursively to the last
marked component, retaining its untransformed phantom metrics; FreeType-generated
pixel/advance evidence covers differing bearings, scale and translation. CFF
vertical origins honor VORG; otherwise they use exact Bézier extrema plus vmtx.

Only a preferred nominal Unicode cmap and normalized Unicode/English names enter
the native model. UVS, non-Unicode encoding tables, localized records, hinting,
layout and arbitrary sfnt metadata are not preserved as editable semantics.
General outline conversion carries a projection-loss report; use the low-level
container/subset APIs for binary-table preservation. Variable outlines and CFF2,
and the inherited CFF1 unsupported operators documented in `cff.md`, fail explicitly.
Hinting, antialiasing and FreeType dropout control are not implemented. A reusable
dot component centered at the origin can have no lit pixel centers although
FreeType's dropout policy paints a pixel; translated source glyphs remain exact.

Complete anisotropic physical strikes are retained. Sparse anisotropic strikes
currently require explicit square-ppem selection or bitmap-only extraction;
unified fallback rejects them rather than silently applying the y scale to x.
The first bitmap family at a size wins: EBDT/EBLC, then bdat/bloc, then color.


The inferred ppem is the coarsest grid demonstrated by the outline coordinates;
it need not equal an original BDF PIXEL_SIZE. Geometrically indistinguishable
coarser grids cannot reveal hidden source pixel subdivisions. Embedded strike
metadata or an explicit ppem supplies that extra information.

Embedded EBSC instructions survive native wrapper reads as `sfnt.EBSC`; records
whose physical source strike was not selected are omitted with a loss report.
For collections, face-local instructions use `sfnt.face.N.sfnt.EBSC`, together
with each Font's face-index annotation. Unsupported raw common/outline-table
provenance is explicitly reported rather than silently discarded. Native outline
writers regenerate tables and report container-property loss; use the bitmap
codec or low-level collection API when preserving those instructions is required.
