# Catalogue migration — mapping and documented output differences

**Status (2026-10-05):** the phase-6 source compiles and `scripts/check.sh` passed
(3,453 tests, clippy `-D warnings`, cargo-deny); `cargo doc` with
`RUSTDOCFLAGS=-D warnings` is clean. That is a build/unit/integration result only.
The full catalogue and Tong paired validation, performance and ASAN reruns are not
reported here. See [recovery status](recovery-status.md) for history.

By the user's 2026-10-05 decision, catalogue output differences from the original
Python download pipeline are **documented** here, as PLAN §11 requires ("what
differs from the current output, if anything, and why"), rather than all removed.
One explicit option was added to reproduce a catalogue convention:
`fontpixel convert --swidth-ppem N [--swidth-units U]` (see
[SWIDTH](#bdf-and-pcf-downloads-68-files)). Everything else listed below is either
reproducible with an existing option or an intentional difference.

## Status of the earlier comparison

The first catalogue comparison omitted bitmap family, style and copyright. The
reference-side inventory then identified FontTools defaults including fsType=4,
fsSelection=0 and usBreakChar=32, and the explicit metadata/UPM/reference-strike
policies. These defaults are not licence instructions. The complete paired
classification in [Catalogue download differences](#catalogue-download-differences)
now covers every field category observed in that run. Do not invent an intentional
difference to explain a mismatch that the classification does not list; a new
mismatch needs its own source-backed explanation.

## Reference inventory retained from the earlier audit

- `reference/fontpixel/ingest/manifest.toml`: 442 active `[[family]]` and 71
  `[[todo]]` entries, 513 combined; 41 active family keys.
- Active `convert` values: absent/none 115, otb 133, ttf 178, kbitx 7,
  spritesheet 8, font8x8 1. `run.py` also supports pbf and t-mat-8x8, absent
  from active entries.
- 124 declared instances in 64 families; 12 axis instances across Pixelify Sans,
  Bitcount, Doto and Handjet; six Bitcount per-glyph-offset instances.
- 24 families use `grid_units`; one uses `pixel_offset`.
- Eight sprite families, 12 map objects, 1,175 explicit cells: 1,156 char cells
  and 19 name-only cells.
- Known license keys: spdx, name, name_en, confidence, file, note, note_en.
  Known variant overrides: weight, size, display. Additional run-only fields:
  name_en, authors_en, form, merge_subsets; family_name, foundry, glyph_offsets.
  Pending-item fields include source, reason, reason_en.
- Original manifest SHA-256:
  `fcf8b612cbb197298291bf3c16de03fb88ddba6bd7fb8176c532e95bfbbf874e`.

The exact current key lists were regenerated in
[migration-manifest-inventory.json](migration-manifest-inventory.json) from that
same manifest SHA. This is a data-derived inventory, not evidence that each
mapping has passed conversion tests.

## Every active family parameter

Host policy means the catalogue retains the parameter; it is not silently discarded
or claimed as a fontpixel codec option. The table covers all 41 active keys.

| Parameter | Native mapping or retained host responsibility |
| --- | --- |
| `added` | Catalogue chronology; host metadata |
| `aliases` | Catalogue search/display aliases; host metadata |
| `all_glyphs` | Explicit glyph inclusion policy; reconcile unencoded/native mappings, not an assumed CLI flag |
| `author` | Author attribution in catalogue/README; retain source notices separately |
| `authors` | Ordered author records; host attribution |
| `convert` | Select native format importer/rasterization; see [`convert` values](#manifest-convert-values); unsupported variable/hinted cases retain fallback |
| `convert_take` | Host source glob selection before native conversion |
| `default_variant` | Host default download/display variant |
| `description` | Host localized catalogue description |
| `description_en` | Host English description |
| `export_name` | Explicit family/output naming policy; normalized family overrides do not rewrite original XLFD automatically |
| `form_verified` | Host provenance/review flag, not native inference |
| `forms` | Host language/form classification |
| `grid_units` | Explicit vector-raster grid override (`--grid-units`, requires `--ppem`), resolved per source when a table |
| `hinting` | Original hint interpreter fallback; native rasterizer does not execute hint programs |
| `homepage` | Host source/reference URL |
| `instances` | Resolve per-file instance lists; map names/weights explicitly and retain axis/offset fallbacks |
| `license` | Nested host licence assessment and notices; see keys below |
| `license_files` | Host selection/copy of licence documents |
| `license_from` | Host separate file/archive selection relative to source/root |
| `license_take` | Host licence archive member patterns |
| `merge` | Catalogue grouping policy; do not equate it automatically with native glyph selection/merge |
| `name` | Host family display identity; explicitly map native names when exporting |
| `name_zh` | Host Chinese display identity; no automatic localized-name synthesis |
| `pixel_offset` | Explicit global raster offset (`--pixel-offset-x`/`--pixel-offset-y`); not per-glyph offset support |
| `ppem` | Explicit native raster size (`--ppem`), resolving filename tables before conversion; automatic grid detection can still fail |
| `provenance_url` | Host provenance link |
| `provenance_url_en` | Host English provenance link |
| `repository` | Host source repository metadata; native codec does not fetch it |
| `slug` | Host stable catalogue/output directory identity |
| `source` | Host input-root path resolution |
| `source_formats` | Host input provenance/classification; actual format selection uses native detection/explicit options |
| `source_kind` | Host source-kind metadata |
| `sprite_maps` | Translate atlas mapping geometry/cells explicitly; strict native validation differences remain |
| `take` | Host checked source copy/glob selection |
| `take_from` | Host prefixed copy/glob groups to prevent basename collisions |
| `variants` | Host per-filename display/size/weight overrides |
| `vibes` | Host search/style tags |
| `zip` | Host single archive extraction and selection |
| `zip_take` | Host archive member patterns, preserving original default policy |
| `zips` | Host ordered multiple archives |

The 124 resolved instances use `instances[].name` for output stem/name,
`instances[].weight` for explicit weight metadata, `instances[].axes` for variable
instantiation and `instances[].glyph_offsets` for per-glyph outline adjustments.
The latter two require the original/external fallback. Instance declarations may
be a list or filename-to-list table; resolving the table is host responsibility.

The seven nested licence keys are `license.spdx`, `license.name`,
`license.name_en`, `license.confidence`, `license.file`, `license.note` and
`license.note_en`. Preserve identification, assessment, selected document and
localized explanatory text; editing a native licence string does not establish
permission. Per-file variant overrides `weight`, `size` and `display` remain
explicit host mapping. Sprite-map keys are `baseline`, `cell_size`, `cells`,
`characters`, `foreground`, `name`, `omit_blank` and `pixel_size`; preserve
geometry and named/unencoded cells while reconciling the stricter native rules.
Pending entries retain `source`, `reason` and `reason_en`.

## Pipeline boundaries and known substitutions

Map BDF/PCF parsing through the native registry and explicit charset APIs. Map
OTB/bitmap and vector export through the corresponding codec or typed OpenType
API. Keep catalogue orchestration, download/archive extraction, licence selection,
family/variant grouping, naming, generated README, ZIP membership, metadata
indexes, cache/stamp policy and site publication in the host application.

Native variable-font instancing, hint interpreter execution and per-glyph outline
offsets were not implemented. Keep the explicit original/external fallback for
the axis/hinted/per-glyph cases. Global grid and pixel-offset options exist; they
do not imply the unsupported per-glyph operation. Native sprite-sheet input
strictly validates both dimension divisibility, overlaps and unmapped visible
pixels; do not silently adopt the original producer's permissive behavior.

No `reference/fontpixel/pipeline/rust` or other `opf_vector` Rust source/binary
was supplied. Only `vectorize.py` and `_build_vector_ttf_reference` were present.
The old wrapper signature is `build_ttf(glyph tuples, pixel_size, ascent, descent,
shape, name records, OS/2 ranges, timestamp)`; each glyph tuple is
`(cp, bbw, bbh, bbx, bby, dwidth, rows)`. This is an ABI mapping, not a source
audit or benchmark of the missing production extension. Label any comparison
against the pure fontTools oracle precisely.

## Catalogue download differences

### Final status after the strict phase 6 (2026-10-06)

Defaults are unchanged: the tables in the subsections below describe the **default profile** (the
first paired run), where "intentional" or "no option" means "not reproduced by default". Every
difference the original application makes on purpose can now be reproduced with an explicit
option. `benches/phase6_catalogue.py --profile explicit` passes them, with values read from the
application's own policy inputs:

| Difference category | Explicit option(s) |
| --- | --- |
| BDF/PCF `FONT` XLFD, `SIZE` point size and resolution | `--font-name`, `--point-size` with `--resolution X,Y` |
| BDF/PCF properties added, removed or rewritten by `bdfwrite.py` | `--set-integer-property`, `--set-text-property`, `--remove-property` |
| Glyph `SWIDTH` from the device advance | `--swidth-ppem` (`--swidth-units`, default 1000) |
| PCF `POINT_SIZE`/`RESOLUTION*` taken from existing properties | `--pcf-metadata-precedence explicit-properties` |
| Proggy PCF download copied from the source PCF | the adapter converts the source PCF directly (no charset→BDF detour) |
| Vector `.notdef` (empty outline, original advance and LSB) | `--notdef`, `--notdef-advance`, `--notdef-lsb` (`variants` and `convert --to ttf/otf/woff/woff2`) |
| Vector name table, OS/2 `fsType`/`fsSelection`/break char/code pages, timestamps, fixed pitch, `maxZones`, `gasp`, cmap subtables, glyph order, dot glyph placement | `variants --name-record/--name-record-hex/--name-language-tag --fs-type --fs-selection --break-char --code-page-ranges --created --modified --fixed-pitch --max-zones --gasp --cmap-unicode-full --glyph-order --dot-glyph`, `--vendor` |
| Bitmap TTF name table, OS/2, head, post, EBLC `widthMax` and index layout, empty `glyf`/`loca` with maxp 1.0, `.notdef`, line metrics, underline, hmtx, glyph order | the `--otb-*` options (`--otb-name-record`, `--otb-outline-tables empty-glyf`, `--otb-width-max`, `--otb-index-layout`, `--otb-glyph-order`, `--otb-underline`, …; see `convert --help`) |

With the explicit profile, 99 of the 173 fonts are identical in every compared field. That covers all
34 PCF, 6 BDF, all 42 vector TTF and 17 of the 21 bitmap TTF. The **73 remaining files are all
classified**, and none is unexplained:

- **Original-lossy, not copied (32 files).** 28 BDF differ only by source `COMMENT` lines, which
  fontpixel keeps and the original never stores (`parsers/bdf.py`) or writes
  (`ingest/bdfwrite.py:16-54`). 4 bitmap TTF (3x5 Block ×2, Capsigrany, Tinypixel) differ because the
  original keeps only encoded glyphs (`ttfexport.py:65-71`) while fontpixel keeps the unencoded ones;
  their glyph counts, `numberOfHMetrics`, `xAvgCharWidth` and EBLC ranges follow from that.
- **Derived (41 files).** The WOFF2 downloads differ only in `head.checkSumAdjustment`; the
  matching TTF files are identical.
- **Consumer limitation (1 file).** `spleen-64px-round.woff2`: FreeType raster overflow (error 98)
  at 14 ppem on U+2502.

Five family ZIPs are identical; the other three differ only through their BDF members' `COMMENT`
lines. In the default profile the bitmap `.ttf` downloads are written as OTB, which has no outline
table and therefore fails OTS. Browsers need `--otb-outline-tables empty-glyf`; with it, all 21 native
bitmap TTF pass OTS, the same as the original.

### Evidence and scope

The numbers in this section come from one paired run of the original Python
download builder and the native adapter over the eight sample families
(34 source files), and from its complete classification:

- run proof: `local-work/phase6-local-20261004/full-correctness-v1/catalogue-proof.json`
  (SHA-256 `c994df99858d4e53e6cae4b554fabfca76ff13cf291c8eb56e14ebe121930a7d`)
  and its per-file `catalogue-proof.json.details/`;
- classification: `owners/w63/catalogue-all181-classification.json` with rule
  table `owners/w63/SEMANTIC-DIFFERENCE-CLASSIFICATION.json`;
- BDF/PCF detail: `owners/w66/catalogue-bitmap-actual-summary.json` and
  `owners/w66/CATALOGUE-BITMAP-PROJECTION-PROPOSAL.txt`.

These paths are under `local-work/phase6-local-20261004/` outside this
repository. Python line numbers below refer to the read-only copy in
`performance-work/reference/fontpixel/pipeline/opf/` (SHA-256 prefixes:
`downloads.py` 4eed6809, `ttfexport.py` 2c1aaa1f, `vectorize.py` 74a01ff0,
`ingest/bdfwrite.py` 61f0711b, `parsers/bdf.py` bba29456, `ingest/run.py`
02e4858b).

The vector comparison side is `_build_vector_ttf_reference`, the fontTools oracle
in `vectorize.py:271-341`, because the production `opf_vector` extension
(`vectorize.py:231-268`) was not supplied. Differences against production vector
downloads may therefore include oracle-versus-extension differences that this run
cannot see.

Totals: 181 download files = 173 font files + 8 ZIPs; 61 field categories; no
unclassified field. Of the 173 font files, 8 are an observed strict pass, 164 have
at least one field category in the tables below, and 1
(`spleen-64px-round.woff2`) stopped on the consumer limitation described under
[Packaging and consumer limitation](#packaging-and-consumer-limitation). The
classification found no proven error in a generic native default; it does not
make any difference "equivalent". This run predates the `--swidth-ppem` option.

| Download kind | Files | Strict pass | With documented differences | Other |
| --- | ---: | ---: | ---: | --- |
| `bdf` (`.bdf.gz`) | 34 | 4 | 30 | |
| `pcf` (`.pcf.gz`) | 34 | 4 | 30 | |
| `ttf` bitmap (EBDT/EBLC) | 21 | 0 | 21 | |
| `ttf-square`/`ttf-round` | 42 | 0 | 42 | |
| `woff2-square`/`woff2-round` | 42 | 0 | 41 | 1 consumer limitation |
| `zip` | 8 | 3 byte-identical | 5 inherit BDF members | |

The 8 passing font files are the BDF and PCF of the two 3x5-block faces,
Capsigrany and Tinypixel.

### Download kind to command

`downloads.build_downloads` (`downloads.py:321-471`) writes these kinds per family.
The native column is what the comparison adapter (`benches/phase6_catalogue.py`,
`jobfile`) generates as batch tasks; the equivalent single commands are shown.

| Kind and original producer | fontpixel command |
| --- | --- |
| `bdf`: `write_bdf` into a temporary file (`downloads.py:218-227`, `ingest/bdfwrite.py:16-54`), gzip level 9, mtime 0 (`downloads.py:156-157`) | `fontpixel convert SRC -o OUT.bdf --swidth-ppem PX` (add `--from pcf` for PCF sources); gzip is host packaging |
| `pcf`, source is PCF and not merged/renamed: the source bytes are returned unchanged (`downloads.py:264-267`) | host byte copy, or `fontpixel convert SRC.pcf -o OUT.pcf` without a charset step (re-encodes; not compared in this run) |
| `pcf`, otherwise: the BDF above compiled by `bdftopcf SRC -o DST` (`downloads.py:268-278`) | `fontpixel convert SRC -o OUT.pcf --swidth-ppem PX` |
| `ttf` (bitmap): all sizes of one face as EBDT/EBLC strikes, `build_ttf` (`downloads.py:363-421`, `ttfexport.py:52-186`) | `fontpixel convert A.bdf B.bdf --to otb -o FACE.ttf --family F --style S --full-name … --postscript-name … --font-version 1.000 --copyright … --otb-upm 1000 --otb-reference largest --otb-rounding ties-even` |
| `ttf-square`, `ttf-round`: largest size only; round only when the face has at most 20,000 glyphs (`downloads.py:23`, `423-440`) | `fontpixel variants --base LARGEST.bdf -o FACE-PXpx-SHAPE.ttf --family F --style S --legacy-style S --full-name … --postscript-name … --font-version 1.000 --copyright … --shape square\|dot --dotsize 1.0 --dot-reuse literal --ppu U --upm U*PX --woff2-source ttf`, with `U = floor(1000 / PX)` |
| `woff2-square`, `woff2-round`: the vector TTF recompressed by `_woff2` (`downloads.py:177-215`, `441-450`) | the same `variants` task with a `.woff2` output |
| `zip`: every BDF, licence files, extra files and `README.txt`, entries dated 1980-01-01 (`downloads.py:454-470`) | host packaging (the adapter's `native_archives`) |

Notes on the table:

- `PX` is the face's pixel size. For BDF outputs and for PCF outputs that the
  original rebuilds through `bdftopcf`, `--swidth-ppem PX` reproduces the
  original SWIDTH; omit it for the PCF byte-copy branch.
- In batch files the same options go in `[tasks.options]`, for example
  `swidth_ppem = 13`, `otb_upm = 1000`, `otb_reference = "largest"`,
  `otb_rounding = "ties-even"`. The family name is the structural `family` task
  field, not an option.
- `export_name` renaming (`downloads.py:230-258`, `340-354`) rewrites the XLFD
  `FONT` family field, `FAMILY_NAME`, and `FULL_NAME`/`FACE_NAME` when present.
  `convert --family` only changes normalized names: on a Cozette BDF it rewrote
  `FAMILY_NAME`, removed the stale `FACE_NAME` and left the `FONT` line unchanged
  (checked with the 2026-10-05 release binary). The comparison adapter uses the
  separate `crates/fontpixel/examples/phase6_catalogue_export_name.rs` helper for
  this; there is no CLI flag for an XLFD rewrite.
- WOFF2: the original disables the glyf/hmtx table transforms and uses Brotli
  quality 9 when the TTF exceeds 1,500,000 bytes, otherwise 11
  (`downloads.py:173-215`). `fontpixel variants` applies the transforms and
  defaults to quality 11; `--woff2-quality` sets the quality. Transforms can be
  turned off only through the library (`Woff2Options::transform_glyf` /
  `transform_hmtx`); there is no CLI flag. Both are valid WOFF2. The comparison
  decodes the fonts, so it does not measure this difference.
- Native `--shape dot` defaults to `--dotsize 0.9` (Tong); the catalogue's round
  shape corresponds to `--dotsize 1.0`.

### Manifest `convert` values

`ingest/run.py:270-373` turns each source into a catalogue BDF before downloads
are built. The download differences above apply after this step.

| `convert =` | Original importer | fontpixel reader |
| --- | --- | --- |
| absent/none (115) | source BDF/PCF used as taken | `--from bdf` / `--from pcf` (detected by default) |
| `otb` (133) | `ingest/otb.py` `convert_otb`, one BDF per strike; `all_glyphs` adds unencoded glyphs | `fontpixel convert SRC.otb -o OUT.bdf --font-index N` per strike; compare its unencoded-glyph handling with the family's `all_glyphs` setting |
| `ttf` (178) | `ingest/rasterize.py` `rasterize_ttf` at `ppem` (or detected native grid), optional `grid_units`, `pixel_offset`, `hinting`, `instances` | `fontpixel convert SRC.ttf -o OUT.bdf --ppem P [--grid-units G] [--pixel-offset-x X --pixel-offset-y Y] [--face-index I] [--no-embedded-bitmaps]`; `hinting`, `instances[].axes` and `instances[].glyph_offsets` have no native equivalent: keep the original importer |
| `kbitx` (7) | `ingest/kbitx.py`, grayscale value >= 0x80 is ink | `fontpixel convert SRC.kbitx -o OUT.bdf --kbitx-read-threshold 128`; without the flag native keeps grayscale samples |
| `spritesheet` (8) | `ingest/spritesheet.py`, cells from `sprite_maps` | `--from image --image-mapping MAP.json`; the manifest `sprite_maps` table is not that mapping format and must be translated; native validation is stricter (see above) |
| `font8x8` (1), `t-mat-8x8` (0) | `ingest/header8x8.py` | `--from c --c-read-family font8x8` / `--c-read-family tmat` |
| `pbf` (0) | `ingest/pebble.py`, baseline placed at the PBF line height | `--from pbf`; check the baseline against the original before replacing it |

Reader options appear in `fontpixel convert --help`. These mappings were checked
against the current help text; they were not run over the manifest's sources.

### BDF and PCF downloads (68 files)

Rule `app-06`. Of 68 files, 8 pass and 60 differ. Value pairs in this and the
following tables are written native vs original. Field categories: header fields
(30 BDF, 29 PCF), raw header records (30 BDF), properties (30 PCF, 1 BDF), glyph
records (6 BDF, 5 PCF; plus unencoded records in 2 PCF), default character
(1 PCF), property spelling (1 BDF, 1 PCF), raw property records (1 BDF).

| What differs | Files | Original behaviour | fontpixel |
| --- | ---: | --- | --- |
| `FONT` XLFD | 30 BDF, 29 PCF | for BDF sources, `parsers/bdf.py:78-89` reads only the pixel size and setwidth from `FONT` and never stores it, so unless `export_name` supplies one (`downloads.py:230-258`) `bdfwrite.py:18-21` writes `-opf-{family_slug}-medium-r-normal--{px}-{px*10}-75-75-p-{px*10}-iso10646-1`; weight, slant and spacing are not derived from the source (Terminus Bold becomes `medium`, monospaced fonts become `p`). The PCF parser keeps `FONT` as a property, so Proggy keeps its source name | keeps the source XLFD. Intentional; no option. The three families with `export_name` (3x5-block, Capsigrany, Tinypixel) go through the rename path on both sides and match |
| `SIZE` point size | 3 BDF | `bdfwrite.py:23` writes `SIZE {px} 75 75` | keeps the source `SIZE` (Cozette ×2: 12 vs 13; Proggy: 8 vs 13). Intentional |
| `SIZE` resolution | 27 BDF | same line, always 75 dpi | keeps the source (26 files 72 dpi, 1 file 96 dpi). Intentional |
| PCF `RESOLUTION` property | 26 PCF | `bdftopcf` derives it from the rewritten 75-dpi BDF (inferred from the values) | 99 vs 103 in all 26. Intentional |
| `COMMENT` lines | 28 BDF | `parsers/bdf.py` has no `COMMENT` handling, so the model never holds them, and the writer emits none (`bdfwrite.py:16-54`) | kept. Cozette's comments include `(c) 2020-2025 Ines @ the.moonwit.ch`; Spleen's carry the copyright notice and licence text. Intentional: fontpixel does not drop notice text. Native PCF output cannot store header comments and reports `pcf.write.header` loss |
| Glyph `SWIDTH` | 6 BDF, 5 PCF | `bdfwrite.py:44` writes `round(DWIDTH * 1000 / max(1, pixel_size))` (Python `round` is ties-to-even) for every encoded and unencoded glyph | keeps the source value by default. **Reproduced by `--swidth-ppem PX`**, reported as loss, so `--strict` rejects it |
| `DEFAULT_CHAR`, `CHARSET_*` | 1 PCF (Proggy) | PCF byte copy keeps `DEFAULT_CHAR 8223` and no charset properties | the comparison adapter first ran `fontpixel charset --source-charset iso8859-1 --to unicode` on every PCF source (`benches/phase6_catalogue.py`, `prepare_native_inputs`), which reset the default character to 0 and added `CHARSET_REGISTRY`/`CHARSET_ENCODING`. An adapter step, not a writer default; follow the PCF byte-copy branch instead |
| Proggy BDF properties | 1 BDF | `bdfwrite.py:26-34` drops the `FONT` property and adds `FONT_ASCENT`, `FONT_DESCENT`, `PIXEL_SIZE`, `CHARSET_REGISTRY "ISO10646"`, `CHARSET_ENCODING "1"`; the PCF-derived `FONT` keeps `iso8859-1` | native keeps a `FONT` property, has no `PIXEL_SIZE` property, and its `FONT` ends in `ISO10646-1` after the charset step. Intentional |

SWIDTH detail (W66 summary; every original value matches round-half-even of
`DWIDTH * 1000 / pixel_size`, including PCF unencoded records):

| Font | Records changed | Source → original values |
| --- | --- | --- |
| Cozette, CozetteCrossedSeven (13 px) | BDF: 6,045 of 6,046 each; PCF: 4,599 encoded + 1,446 unencoded each | 461→0 ×1, 461→462 ×177, 500→462 ×4,649, 500→538 ×1, 922→462 ×65, 922→923 ×1,152 |
| ProggyCleanSZ (13 px) | BDF: 256 | 7000→538 ×256 (the PCF copy keeps 7000) |
| Terminus u18b, u18n (18 px) | 891 each, BDF and PCF | 555→556 |
| Tom Thumb (6 px) | 201, BDF and PCF | 1000→667 ×191, 666→667 ×10 |

`fontpixel convert cozette.bdf -o out.bdf --swidth-ppem 13` changed 6,045 of
6,046 SWIDTH values on the sample source and reported
`projection.scalable_advance.replaced` as loss; adding `--strict` exited 3 with no
output (release binary, 2026-10-05). The full catalogue comparison has not been
rerun with the option. The other differences in this table (FONT, SIZE,
COMMENT, properties) have no option.

### Bitmap TTF downloads (21 files)

The original builds a TrueType file with empty `glyf` outlines plus EBDT/EBLC
strikes (`ttfexport.py:1-8`, `73-80`). The native task writes the OTB profile
(no `glyf`/`loca`) to the same file name. 34 strikes in 21 files.

| What differs | Files | Original (`ttfexport.py`) | fontpixel |
| --- | ---: | --- | --- |
| Tables | 21 | `glyf` + `loca` with empty outlines (`:76-80`) | no `glyf`/`loca`; `maxp` version 0.5 instead of 1.0 with its 13 outline fields (21) |
| Glyph set and `.notdef` | 21 | glyph order is `.notdef` plus encoded code points only (`:65-71`); `.notdef` has advance `round(px * 1000/px / 2)` = 500 and no bitmap (`:84`, `:117-134`) | keeps the source `.notdef` and unencoded glyphs. `.notdef` advance: 0 vs 500 in 18 files, 667 and 167 in the two 3x5-block faces, 312 in Capsigrany. Four files keep extra unencoded source glyphs, so `maxp.numGlyphs` differs: 3x5-block ×2 (`.null`, `nonmarkingreturn`; 102 vs 100), Capsigrany (`nonmarkingreturn`; 99 vs 98), Tinypixel (two unnamed glyphs; 101 vs 99) |
| Left side bearings | 6 | every hmtx LSB is 0 (`:84-98`) | design LSB from the bitmap (example: U+007C in 3x5 proportional, 167 vs 0) |
| `head`/`hhea` extents | 21 | empty outlines give 0 for `head.xMin/yMin/xMax/yMax` | design extents from the bitmaps (`head.yMin` 21, `xMax`/`yMax` 21, `xMin` 3; `hhea.xMaxExtent` 21, `minLeftSideBearing` 3, `minRightSideBearing` 5) |
| `hhea.numberOfHMetrics` | 19 | fontTools trailing-advance compression over its glyph set | different glyph set and compression. Representation |
| EBLC line metrics | 21 (34 strikes) | `:140-154` writes the same values into `hori` and `vert`, with `caretSlopeNumerator 0`, `caretSlopeDenominator 1` and `minOriginSB`, `minAdvanceSB`, `maxBeforeBL`, `minAfterBL` all 0 | upright caret (1/0), computed min/max fields, zero `vert` metrics. FreeType therefore reports a vertical advance for every encoded glyph of the original only (21 files) |
| EBLC `widthMax` | 7 strikes in 7 files | maximum DWIDTH (`:144`) | maximum bitmap width (native vs original: 3x5 ×2: 3 vs 4; Capsigrany 16 vs 17; Cozette ×2: 12 vs 13; Tinypixel 5 vs 6; Tom Thumb 3 vs 6). Intentional; no option |
| Strike order, index subtables | 1 + 2 strikes | strikes sorted by pixel size (`:62`); one format-1 index subtable per strike (`:164-169`) | Spleen strikes in input order (24, 32, 64, 8, 12, 16); two strikes use 17 and 19 format-1 subtables. Representation |
| `post` | 21 | format 3.0, `isFixedPitch=0` always (`:106-107`) | format 2.0; `isFixedPitch=1` for monospaced sources (15 files) |
| `post` underline | 2 | 0 | Cozette ×2: source `UNDERLINE_POSITION -19`, `UNDERLINE_THICKNESS 13` scaled to −1462/1000 |
| `OS/2.version`, `head.flags`, `head.lowestRecPPEM` | 21 each | fontTools `FontBuilder` defaults 3, 3, 3 (not set by `ttfexport.py`) | 4, 11, 8 |

The table above records the default-profile run. Every row except the dropped
unencoded glyphs now has an explicit option; see
[Explicit sfnt profile](#explicit-sfnt-profile) below.

### Vector TTF and WOFF2 downloads (42 + 41 files)

| What differs | Files (TTF + WOFF2) | Original (`vectorize.py` oracle) | fontpixel |
| --- | ---: | --- | --- |
| `.notdef` | 42 + 41 | empty outline, advance `max(unit * px // 2, 1)`, LSB 0 (`:301-302`, `:322-326`) | the Tong box from `variants/export.rs:185`: hollow 5×10 pixels, bearing 1 pixel, advance 7 pixels (for example 1162/166 vs 498/0 at 166 units per pixel). Intentional; no option |
| Extents from `.notdef` | `head.xMax`/`yMax` and `hhea.xMaxExtent` 8 + 8; `hhea.advanceWidthMax` 10 + 10 | — | the box is wider/taller than any glyph in those faces (for example `head.yMax` 1660 vs 830, `advanceWidthMax` 1162 vs 664) |
| Round-shape helper glyph | round files | the dot is GID 1 (`:294`, `:303-305`) | the dot helper is the last GID. Representation |
| `maxp.maxContours` | 21 + 20 (all round files) | 1: the only simple glyph is the dot | 2: the box `.notdef` has two contours |
| `maxp.maxZones` | 42 + 41 | 2 (fontTools default) | 1 (no instructions) |
| `gasp` table | 42 + 41 | absent | present |
| `post.isFixedPitch` | 30 + 29 | 0 always (`:335`) | 1 for monospaced sources |
| `OS/2.ulCodePageRange1` | 28 + 28 | 0 (`setupOS2` receives only vertical metrics, `:333-334`) | computed: 159 (24 + 24) or 415 (4 + 4) |
| `hhea.numberOfHMetrics` | 32 + 31 | fontTools compression | different glyph order/compression. Representation |

Encoded glyph outlines and advances are not among the differing fields.

### All sfnt downloads (21 bitmap + 42 vector TTF + 41 WOFF2 = 104)

| What differs | Files | Original | fontpixel | Reproducible? |
| --- | ---: | --- | --- | --- |
| `name` text | 83 vector | `name_records` (`ttfexport.py:24-41`): ID3 `{family}-{style}-{version}`, ID5 the literal `1.000` | ID3 `{family} {style} {version}`, ID5 `Version 1.000` | text of IDs 0–2, 4, 6 via `--family`, `--style`, `--legacy-style`, `--full-name`, `--postscript-name`, `--copyright`; the ID3 format and the `Version ` prefix are fixed |
| `name` platforms | 104 | Windows Unicode records plus Mac Roman records whenever the text is Mac Roman encodable (`vectorize.py:243-256`, fontTools `setupNameTable`) | vector: Mac records only for ASCII text (`variants/export.rs:765`); every copyright in the run contains an em dash, so Mac ID 0 is missing in 83 files. Bitmap: no Mac records and no ID3 (21) | no |
| Extra name IDs | 83 vector, 16 bitmap | IDs 0–6 only | vector adds typographic IDs 16/17 on both platforms; bitmap adds ID 8 (foundry) in 16 of 21 | no |
| `OS/2.fsType` | 104 | 4 | 0 | no. Both are library defaults, not a licensing statement by the font author |
| `OS/2.fsSelection` | 104 | 0 | 64 (REGULAR) | no. Library default |
| `OS/2.achVendID` | 104 | `????` | `NONE` | vector: `variants --vendor '????'` (checked with the release binary); bitmap: no |
| `OS/2.usBreakChar` | 104 | 32 | 0 | no. Library default |
| `OS/2.xAvgCharWidth` | 5 bitmap + 12 + 11 | fontTools average over its hmtx | native average over its glyph set; difference 1 to 7 units | no |
| `head.created`/`modified` | 104 | fixed 3660681600, 2020-01-01 (`ttfexport.py:17-20`, `:180`; `vectorize.py:338`) so rebuilds are byte-stable | raw 0 (fontTools displays it as 2082840000 with a plausibility warning); also deterministic | no option |
| `head.checkSumAdjustment` | 104 | derived from the file bytes | derived from the file bytes | not a semantic field: it differs whenever any byte differs. The validator recomputes it per file (rule `repr-01`, demonstrated on six real sfnt) |
| `cmap` subtables | 104 | (0,3) and (3,1) format 4 in 94 files, plus (3,10) format 12 in 10 | (0,3) and (3,1) format 4 plus (0,4) and (3,10) format 12 in all 104 | no; the mapped code points are identical in 104 of 104 files |

The "Reproducible?" column describes the default-profile run; the options below
now reproduce every row except `head.checkSumAdjustment`.

### Explicit sfnt profile

`benches/phase6_catalogue.py --profile explicit` (unit F) adds these options,
with values taken from the original's own policy inputs: `name_records()`
(`ttfexport.py:24-41`) through fontTools `setupNameTable`
(`fontBuilder.py:440-490`, Windows 3/1/0x409 plus Mac 1/0/0 when Mac Roman
encodes the text), and the fontTools `FontBuilder` defaults
(`fontBuilder.py:140-192`, `_getOS2Defaults` `:267-308`). Line numbers of
fontTools refer to the oracle copy in `local-work/oracles`.

| Original policy | Bitmap TTF (`convert --to otb`) | Vector (`variants`) |
| --- | --- | --- |
| Name IDs 0–6 on both platforms | `--otb-name-record P/E/L/ID=TEXT` | `--name-record P/E/L/ID=TEXT` (same syntax and parser) |
| `fsType` 4, `fsSelection` 0, `usBreakChar` 32, `achVendID` `????` | `--otb-fs-type 4 --otb-fs-selection 0 --otb-break-char 32 --otb-vendor '????'` | `--fs-type 4 --fs-selection 0 --break-char 32 --vendor '????'` |
| Code pages not computed (0) | — (already equal) | `--code-page-ranges 0,0` |
| OS/2 v3, `head.flags` 3, `lowestRecPPEM` 3 | `--otb-os2-version 3 --otb-head-flags 3 --otb-lowest-rec-ppem 3` | — (already equal) |
| Fixed timestamps (`ttfexport.py:17-20`, `:180`) | `--otb-created/--otb-modified 3660681600` | `--created/--modified 3660681600` |
| `post` format 3, `isFixedPitch` 0 (`:107`) | `--otb-post-names omit --otb-fixed-pitch false` (omitting names is loss) | `--fixed-pitch false` |
| `post` underline 0 (`fontBuilder.py:185-186`) | `--otb-underline 0,0` (loss when the source has underline metrics) | — |
| cmap: format 12 only above U+FFFF, under (3,10) (`fontBuilder.py:399-421`) | `--otb-cmap-unicode-full when-needed` | `--cmap-unicode-full when-needed` |
| `maxp.maxZones` 2 (`fontBuilder.py:167`), no `gasp` | `--otb-outline-tables empty-glyf --otb-max-zones 2` (empty glyf/loca, maxp 1.0, zero outline extents and bearings, `:76-80`) | `--max-zones 2 --gasp omit` |
| Glyph order `.notdef` + sorted code points (`:65-71`; `vectorize.py:290-292`) | `--otb-glyph-order codepoint` | `--glyph-order codepoint`, round: `--dot-glyph after-notdef` |
| `.notdef` without bitmap, advance 500 (`:84`) | `--otb-notdef empty --otb-notdef-advance 500` (replacing a source `.notdef` is loss) | `--notdef empty --notdef-advance N --notdef-lsb 0` (unit E) |
| hmtx trailing-advance compaction (`_h_m_t_x.py:93-104`) | `--otb-hmtx compact-trailing` | — (already compacted) |
| EBLC line metrics (`:140-154`) | `--otb-width-max advance --otb-caret-slope 0,1 --otb-line-extremes zero --otb-vertical-line-metrics mirror-horizontal` | — |
| One index subtable per strike (`:164-169`); strikes by size (`:62`) | `--otb-index-layout spanning`; the adapter passes the inputs in pixel-size order | — |

`convert --to ttf/otf/woff/woff2/ttc` also accepts `--notdef`,
`--notdef-advance` and `--notdef-lsb`. Batch files use the underscore names.

Results (2026-10-05, `local-work/strict-f/`): unresolved fonts 132 (unit E) →
73; all 42 vector TTFs and 17 of 21 bitmap TTFs pass. Every remaining field is
classified: 28 BDF keep source `COMMENT` lines (original-lossy, as above); 41
WOFF2 differ only in `head.checkSumAdjustment` (derived, rule `repr-01`: the
original's WOFF2 value differs from its own TTF's, for example 2694500144 vs
2962985748 for `tom-thumb-6px-square`, while native WOFF2 carries its TTF's
value; both TTFs pass); 4 bitmap TTFs (3x5-block ×2, Capsigrany,
Tinypixel) keep unencoded source glyphs that `ttfexport.py:65-71` drops
(original-lossy), and their glyph count, `numberOfHMetrics`,
`xAvgCharWidth`, unencoded glyph records and EBLC glyph ranges equal the
original when recomputed without those glyphs; `spleen-64px-round.woff2`
stays the consumer limitation below.

### Packaging and consumer limitation

- ZIPs: all 8 pass packaging and their licence and README bytes match. 3x5-block,
  Capsigrany and Tinypixel ZIPs are byte-identical. Cozette, Proggy Clean, Spleen,
  Terminus and Tom Thumb inherit the BDF differences of their members. ZIP
  assembly is host packaging; the comparison adapter reproduces the original
  member order and 1980-01-01 entry dates (`downloads.py:454-470`).
- `spleen-64px-round.woff2` (rule `consumer-01`): FreeType 2.13.2 returns error
  98 for U+2502 at 14 ppem on the native and the original TTF and WOFF2 alike;
  at 64 and 128 ppem the four rasters are identical. This is a consumer limitation
  common to both outputs, recorded without a full metadata inventory for that file.

### Other differences that need a policy

- Old BDF ingestion keeps the first duplicate code point and drops later ones;
  native effective lookup takes the last file-order glyph while retaining all
  mappings. Choose the ingestion policy explicitly.
- Native legacy codes remain tagged and use an explicit charset; do not mistake
  them for Unicode or silently copy an old repair/remapping heuristic.
- Reader, transformation and writer loss reports must all survive. Strict mode
  rejects accumulated loss before exposing output assets, so a catalogue job that
  uses `--swidth-ppem` cannot also be `--strict`.

## Naming and embedded-metric controls

`Font.names.family/style/copyright` and the typed native metadata options must be
mapped deliberately. Convert has `metadata::NameOverrides`/`metadata::apply_names`
behind `--family`, `--style`, `--full-name`, `--postscript-name`,
`--font-version`, `--copyright`, `--license`, `--license-url` and `--foundry`.
They change normalized names with a structured audit; original raw properties,
XLFD and provenance remain separate. Explicit family/style changes clear stale
derived full/PostScript names unless replacements are provided. Do not infer
weight, permissions or raw-record rewriting from them.

Variants has separate legacy subfamily/full/PostScript controls. Embedded
OTB/bdat output has typed upem/reference-strike/rounding controls through
`EmbeddedWriteOptions`, exposed by `convert` and `merge` as `--otb-upm 16..16384`,
`--otb-reference first|largest|N` and `--otb-rounding half-away|ties-even`.
Batch scalar options are `otb_upm`, `otb_reference` and `otb_rounding`.
Defaults remain automatic upem, first-strike priority and half-away rounding.
Explicit flags require OTB/bdat with the corresponding feature; color and other
outputs reject them. A numeric reference is a zero-based writer FontSet index,
not a catalogue file index or a strike-selection operation. See the
[native guide](api.md#explicit-output-names-and-embedded-metrics) and
[CLI example](w6.1-cli.md#conversion-naming-and-metric-controls) for exact priority
and range semantics. These controls compile and are covered by the test suite;
their use in the catalogue run is what the difference tables above measure.

The SWIDTH projection is `fontpixel::projection::apply_bitmap_projection` with
`BitmapProjection::scalable_from_device(units, ppem)` in the library, and
`--swidth-ppem`/`--swidth-units` (batch `swidth_ppem`/`swidth_units`) in the CLI.
It is applied after naming and before writing, transactionally over every live
glyph including unencoded ones; see [the API guide](api.md).
