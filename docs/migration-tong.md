# Tong migration — reconstructed recovery document

**Recovery status (2026-10-04):** this phase-6 documentation/source reconstruction is uncompiled and untested. Historical observations are not current acceptance. See [recovery status](recovery-status.md) for known gaps and catalogue metadata/metric work.

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
| `--font-version V` | Generator `--version`; variants `--font-version`/Options.version |
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

Generic OTB output maps version/name metadata from the input. It does not
silently implement old export-rs's version override; edit the typed Font metadata
when that behavior is required.

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

These commands are documentation only; they have not been run during recovery.
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

The original `tong_export.rs` example parsed 20 BDFs once, prepared four regional
plans, encoded both shapes/flavors, reused sfnt for WOFF2 and emitted PCF/OTB plus
byte-copied BDFs. Its positional arguments were BDF directory, output directory,
font jobs, quality, compressor threads, version, dot reuse. That example's source
is not reconstructed here; recover it separately rather than invent its internals.

For website subsetting, retain page-text extraction, CSS unicode-range, naming,
cache stamps and byte-size accounting in the site builder; replace the fontTools
subset operation with `fontpixel subset ... --text-file ... -o ...` or
`--unicodes U+0020-007E,U+4E00-4EFF`. Supported locl/composite dependencies and
retained hint programs/CFF subroutines survive; unsupported glyph-indexed tables
must fail or be explicitly dropped with a loss report.
