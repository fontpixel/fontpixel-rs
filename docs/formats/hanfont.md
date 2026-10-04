# Korean Johab component fonts (`hanfont`)

Specification written before implementation (2026-10-04). The implemented native
layout is the documented **Johab 8/4/4** component order, in a headerless 16×16,
MSB-first, two-byte-per-row stream. This is a declared layout, not a universal
interpretation of every DOS `.han` file. No content magic exists; detection never
claims confidence from a 11,520-byte length alone.

Sources: [Unifoundry, Generating Hangul Syllables](https://www.unifoundry.com/hangul/hangul-generation.html),
section “Johab 8/4/4 Fonts”; [Unicode Hangul algorithms](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-3/#G24646).
Only published structural facts and selection rules are used. No GPL program or
font artwork is copied. Other arrangements (Hanterm 529-component 10/3-or-4/4,
Unifont 6/3/1, vendor-specific DOS layouts, optional Latin prefixes) are distinct
layouts and are not silently guessed or treated as 8/4/4.

| Wire data | Model |
| --- | --- |
| slots 0–159 = 8 groups × (filler + 19 initials) | legacy component slots |
| slots 160–247 = 4 groups × (filler + 21 vowels) | legacy component slots |
| slots 248–359 = 4 groups × (filler + 27 finals) | legacy component slots |
| one slot = 32 packed bytes | 16×16 Mono bitmap, origin (0,0), advance 16 |
| layout selection | `hanfont.layout` property |
| entire native component data | `hanfont.components` byte property |

Read options choose component slots or all 11,172 modern Unicode syllables.
Syllables use `AC00 + (initial*21 + vowel)*28 + final`; finals range 0–27,
where 0 means absent. Initial form depends on vowel geometry and final presence;
vowel form depends on initial G/K and final presence; final form depends on
vowel geometry. Bytewise OR combines selected component rasters. Unused fillers
are retained; an absent final does not draw a final component. Ancient jamo and
standalone modern jamo are not invented by this modern syllable projection.

Writing accepts all 360 uniquely labelled legacy component glyphs, or a complete
syllable projection with retained native bytes whose selected components exactly
reproduce every current bitmap. There is no inverse raster decomposition. Edited
or incomplete syllable fonts that cannot be represented fail explicitly. Component
writes require exact 16×16 geometry; optional Gray8 thresholding is reported as
loss. Extra metadata, aliases, names, sequences, metrics and changed native data
receive structured loss diagnostics. Strict registry writes reject these losses.

All input, component, glyph, pixel, table, model and output allocations use one
cumulative budget. Successful output also passes the default syllable reader
under identical limits, even when source mode was Components. Composition uses
linear bytewise OR; model index construction and retained-syllable validation
use ordered-map operations, giving O(glyphs × log(glyphs) + raster bytes) work.
