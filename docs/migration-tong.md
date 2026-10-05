# Tong migration

**Status (2026-10-05):** the phase-6 source compiles and `scripts/check.sh` passed (3,453 tests, clippy `-D warnings`, cargo-deny). The full 20-BDF Tong paired validation, performance and ASAN reruns are not reported here. See [recovery status](recovery-status.md) for history.

This text preserves the earlier migration contract; it is not the original
byte-identical guide. Keep Tong glyph sources, `tools/build.py`, regional
borrowing/corrections and website assembly in Tong. Convert its built BDFs with
fontpixel. The read-only samples and complete `validation-inputs/phase2/bdf`
baseline have different provenance; full comparisons used the latter's 20 files.

## Export option mapping

| Old export-rs option/stage | Native replacement and boundary |
| --- | --- |
| `[OUT]` | Explicit output paths; no implicit site mutation |
| `--sizes Large Small` | Same selection in `examples/tong_batch.py` |
| `--only square` / `--only dot` | BDF/PCF/OTB plus selected vector shape, as old CLI |
| `--only otb` | Only OTB tasks |
| `--skip-otf` | Retain TTF/WOFF2, omit OTF |
| `--bdf-dir DIR --only otb` | Generator inputs; no Python/bdftopcf codec subprocess |
| `--font-version V` | Generator `--version`; variants `--font-version`/Options.version; OTB needs convert `--font-version` (not set by the generator) |
| hidden `--plan FILE` | Native Variants/prepared plan or version-1 batch; no old JSON bridge |
| hidden `--unpack FILE OUT` | WOFF2 container decode preserving original sfnt bytes |
| `TONG_EXPORT_PYTHON` | Retain only for Tong's source/BDF build |
| prepare.py | Host source build plus native task generation; no prepared glyph JSON/FEA |
| bdftopcf | Native PCF writer with explicit wire options |
| `otb::convert` | Native embedded-bitmap writer |
| export_plan | Native variants and encoders |
| WOFF2 source flavor | auto uses CFF for square, TrueType for dot |
| rayon parallelism | Batch jobs separately bounded from WOFF2 compression threads |
| BDF byte copy | Keep checked host copy for exact bytes; generic conversion canonicalizes |
| manifest/cleanup | Remain host publication policy; native batch never deletes stale downloads |

Generic OTB output maps version/name metadata from the input BDF. The generator's
OTB tasks pass no naming options, so they do not apply old export-rs's version,
copyright, licence or PostScript name. Add them per task when needed:
`convert --font-version`, `--copyright`, `--license`, `--license-url`,
`--postscript-name` (batch `[tasks.options]` `font_version`, `copyright`,
`license`, `license_url`, `postscript_name`). On a Tong BDF these add name IDs 6,
13 and 14 and set ID 5 literally (release binary, 2026-10-05). See
[output differences](#output-differences-from-export-rs) for what they do not cover.

## Routing and commands

SC is base. TC supplies both zh-Hant/ZHT and zh-HK/ZHH, JP is ja/JAN, KR is ko/KOR.
Default language scripts are DFLT,hani,kana,hang,bopo. Latin is a script override
for latn,grek,cyrl, including narrow quotes/ellipsis. Alternative-only characters
join the default cmap. Equal complete glyph/metric alternatives may share glyphs.

```sh
fontpixel variants --base TongPixelSans14Large-SC.bdf \
  --lang zh-Hant,zh-HK=TongPixelSans14Large-TC.bdf \
  --lang ja=TongPixelSans14Large-JP.bdf \
  --lang ko=TongPixelSans14Large-KR.bdf \
  --script latn,grek,cyrl=TongPixelSans14Large-Latin.bdf \
  --family 'Tong Pixel Sans 14 Large' --font-version 2026.10.02 \
  --vendor TONG --ppu 100 --upm 1400 --shape square \
  -o TongPixelSans14Large-Square.woff2
```

The exact recovered generator carries full original notice strings. Batch maps
iterate lexical keys; it uses `ZHT,ZHH`, `ja`, `ko` to retain TC → JP → KR order.
Direct repeated `--lang` keeps supplied argument order. Mono is a source-face
choice (`TongPixelSansMono14Large-*`), not a command to flatten advances. Small
uses its own BDFs. Dots use diameter 0.9. Explicit `--lang-scripts TAG=...` changes
routing only when required.

```sh
mkdir -p ../local-work/tong-native/{bdf,pcf,otb,ttf,otf,woff2}
python3 -B examples/tong_batch.py \
  --bdf-dir ../validation-inputs/phase2/bdf \
  --out ../local-work/tong-native --version 2026.10.02 \
  --jobs 2 --write ../local-work/tong-native.toml
fontpixel batch ../local-work/tong-native.toml --jobs 2 \
  > ../local-work/tong-native-report.json
```

The flags above match the current `--help`; the generator's task matrix is checked by
`examples/check_documentation.py`. Results of the full 84-task run belong to the pending
validation rerun and are not reported here.
The generator creates 84 independent tasks: 20 each BDF/PCF/OTB plus eight
regional/shape variants × TTF/OTF/WOFF2. `--only otb`:20; either shape:72;
`--skip-otf`:76; Large only:42. It validates input existence and exclusively
creates the requested TOML; the host creates output directories. Output names
use uppercase Square/Dot, matching the original CLI (the older shared Rust
example used lowercase). Every task reads original sources: batch has no task
dependencies, so website subsetting runs later. PCF supplementary-character losses
are explicit; full exports are not globally strict without resolving them.

`variants --max-input-bytes` defaults to 64 MiB and `--max-total-bytes` to 256 MiB.
Its cumulative model charge includes the extra TC clone for the second language
route. Before loss, the full Large proportional square-TTF case passed 256 MiB
with the exact audited Rust accounting and byte-matched a 512 MiB run. That was
one configuration, not evidence for a whole recovered 84-artifact run. Other
toolchains retain conservative fallback estimates; a known larger case can opt
into `--max-total-bytes 536870912` or batch options.max_total_bytes. Do not raise
the default or use budget overrides to hide an accounting bug.

The baseline [tong_export.rs](../crates/fontpixel/examples/tong_export.rs) example
is present in the repository. It parses 20 BDFs once, prepares four regional
plans, encodes both shapes/flavors, reuses sfnt for WOFF2 and emits PCF/OTB plus
byte-copied BDFs. Its positional arguments are BDF directory, output directory,
font jobs, quality, compressor threads, version, dot reuse. The reconstructed
[phase6_tong_export.rs](../crates/fontpixel/examples/phase6_tong_export.rs) adds the
full-report workflow, with report construction and file I/O inside its timing.
Both are compiled by `cargo clippy --all-targets` in `scripts/check.sh`. Their
shared-plan API workflow does not replace the default full CLI batch correctness or
performance gate.

For website subsetting, retain page-text extraction, CSS unicode-range, naming,
cache stamps and byte-size accounting in the site builder; replace the fontTools
subset operation with `fontpixel subset ... --text-file ... -o ...` or
`--unicodes U+0020-007E,U+4E00-4EFF`. Supported locl/composite dependencies and
retained hint programs/CFF subroutines survive; unsupported glyph-indexed tables
must fail or be explicitly dropped with a loss report.

## Output differences from export-rs

PLAN §11 asks what differs from the current Tong output and why. The evidence is
a paired run over all 20 OTB files (two sizes × two spacings × five regions;
`local-work/phase6-local-20261004/owners/w63/tong-actual-20-otb-fields.json`) and
a source review of `tools/export-rs/src/otb.rs` (`TONG-BITMAP-METADATA-GAP.txt`).
In all 20 files every encoded glyph's pixels matched, cmap-to-GID mappings were
equal, and per-GID hmtx, bitmap presence and raster had no differences. The
remaining differences are metadata. Values are native vs export-rs.

| What differs | Files | export-rs | fontpixel `convert --to otb` | Option |
| --- | ---: | --- | --- | --- |
| Name IDs | 20 | Windows IDs 0–6, 13, 14, 16, 17 (`otb.rs:276`): Tong copyright, style Regular, unique ID `{family} Regular {version}`, PostScript name, licence text and URL | IDs 0, 1, 2, 4, 5, 8 from the source BDF (its own copyright, FOUNDRY as ID 8, `Version 1.0`) | IDs 5, 6, 13, 14 and the copyright text via the convert naming options above; IDs 3, 16, 17 have no option and ID 8 is always written |
| `OS/2.achVendID` | 20 | `TONG` | `NONE` | none for OTB (`variants --vendor` exists for outlines) |
| `OS/2.panose` | 20 | family type 2, serif style 11, weight 5, proportion 3 (proportional) or 9 (Mono) | all 0 | none |
| Mono detection, `post.isFixedPitch` | 10 | Mono decided from the `TongPixelSansMono` family prefix, because the Tong BDFs declare `SPACING "P"` | 0 from the source spacing | none |
| `OS/2.ulUnicodeRange1–3` | 20 | only bit 57 (supplementary planes) where applicable | every range bit the cmap covers | none |
| `head.flags`, `head.lowestRecPPEM` | 20 | 3, 14 | 11, 8 | none |
| `OS/2.usBreakChar` | 20 | 32 | 0 | none |
| `OS/2.xAvgCharWidth` | 10 | floor of the average | rounded average; 1 unit higher | none |
| `post` format | 20 | 3.0 | 2.0 with glyph names | none |
| EBLC `widthMax` | 20 | maximum advance (`otb.rs:251`) | maximum bitmap width: Large 21 vs 22, Small 18 or 19 vs 20, Mono Large 19 vs 14, Mono Small 18 or 19 vs 14 | none |
| EBLC index subtables | 20 | many shared-metric subtables (for example 1,723 in TongPixelSans14Large-JP) | one subtable in that file | representation |
| `head.created`/`modified` | 20 | wall-clock time of the run | 0 | none; native output is deterministic |
| `head.checkSumAdjustment` | 20 | derived from bytes | derived from bytes | not a semantic field |

The design units per em match: export-rs uses 1400 and native automatic scaling
gives 100 units per pixel at 14 ppem. Changing `--otb-upm` is not a fix for any row.
The regional square/dot TTF/OTF/WOFF2 outputs have no complete field inventory
yet; the comparison so far selects hhea, OS/2 and head fields only.
