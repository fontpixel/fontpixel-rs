# SPEC-v3 compatibility TrueType/SVG dependencies (W1.5)

## Basis and scope

MIT `reference/bdfparser/src/bdfparser/{outline,ttf,bitmap,font}.py` and
`docs/SPEC-v3.md` §§7–8,11. These algorithms are needed by the unchanged phase-1
parity suite. They are private to the opt-in compatibility namespace; no native
format catalogue hooks claim general TTF/SVG availability yet.

## Field and behavior mapping

- Model source/edited metadata maps to the 17 SPEC slots; raw source hex rows are
  validated on drawing and TrueType export. Native PCF retains its lenient hex rule.
- Glyph order is .notdef, numeric ascending encoded glyphs, then unencoded records.
  .notdef copies DEFAULT_CHAR when available. cmap 4/12 excludes surrogates and
  out-of-Unicode values, with deterministic first-gid mapping and run formation.
- Table order is `[BDFP] OS/2 cmap glyf head hhea hmtx loca maxp name post prep`.
  Signed/unsigned field checks occur before emission. head checksum adjustment and
  table checksums follow SPEC; timestamp defaults to Unix zero.
- Merged squares follow exact edge emission/turn/rotation order; other squares
  and circles follow exact IEEE evaluation order and floor(v+0.5) rounding.
  Rasterization uses nonzero winding and 16 quadratic chords, pixel-centre sampled.
- BDFP preserves ordered headers, properties/spellings and complete metric slots,
  including unencoded alternate codes. Pixels are reconstructed from outlines,
  never copied from a saved source font. Small dots may therefore rasterize blank.
- External TrueType requires pixelsize. Reader supports cmap 0/4/6/12, name/post,
  short/long loca, simple glyphs and composites (offset/point match/transforms).
  Nesting and pixel/point/table budgets fail explicitly; CFF is refused.
- SVG fonts match exact source strings and Unicode mappings, including control
  exclusions, XML escapes and arbitrary-width property integers. SVG import and
  general native SVG-font codecs remain W2.8 work.

## Ambiguities and losses

Executable Python behavior wins when prose differs. Encoding overrides affect
character conversion but not serialized PCF properties. Dynamic partially-null
vectors are projected per consumer, without losing their source slots.

BDFP is optional; without it, original non-Unicode codes, property comments,
source spellings/bitmap padding and all unsupported metrics cannot be reconstructed.
With it, bitmap border/padding is canonicalized by the reference rasterizer, and
properties comments are intentionally excluded from BDFP. No byte-preserving source
shortcut is used. Rust numeric Unicode access preserves lone surrogate values;
Rust char conveniences cannot represent them. Invalid ragged raster operations
are the explicitly assessed K12 difference, not a valid-font conversion mode.

## Validation

Direct Rust Mini BDF/SVG goldens; eight Python-produced TTF byte goldens and
round-trip BDF goldens; literal hole/diagonal cases; complete default reference
random/probe/corpus differential suite. This is not yet the phase-2 fontTools,
OTS, FreeType, HarfBuzz or ten-minute reader-fuzz certification.
