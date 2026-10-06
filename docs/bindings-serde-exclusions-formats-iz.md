# serde exclusions: formats i–z

Scope: the public types in `crates/fontpixel/src/formats/` whose module file or
directory starts with i–z (and the `*_common` modules they own),
plus `vector_raster`, `registry_codecs`, `transport` and `x11`. Every other public
struct and enum in that scope derives `Serialize`/`Deserialize` under the `serde`
feature. Each struct has `deny_unknown_fields`, and also `default` when it
implements `Default`. Enums use `snake_case` names. The exceptions are listed below.

## Excluded public types

| Type | Reason | How bindings expose it |
| --- | --- | --- |
| `formats::vector_raster::Codec` | `id: &'static str` borrows the canonical format ID, so it cannot be deserialized from owned input. It is a registry codec handle, not an options value. | Bindings take the format ID as a string and call the registry's `set_vector_read_options(Options)` / `set_vector_write_options(WriteOptions)`. Both option types derive serde. `Codec::new(id)` only builds the default handle the registry already registers for `ttf`/`otf`/`woff`/`woff2`/`ttc`. |

## Not public API (no derive needed)

These items are declared `pub`, but they live inside `pub(crate)` modules and are
not re-exported, so the binding coverage inventory does not list them:

| Item | Module | Note |
| --- | --- | --- |
| `Cursor<'a>` | `ssfn/common.rs` (`formats::w5e2_outline_common`) | A borrowed byte cursor used by the PFR reader. |
| `Raster` | `pilfont/common.rs` (`formats::image_bundle_common`) | A decoded atlas buffer that holds an `image::ColorType`. |

`registry_codecs.rs`, `transport.rs` and `x11.rs` declare no public structs or
enums. They contain only crate-internal macros, `impl` blocks and functions.

## Existing serialized forms kept unchanged

- `formats::image::{Mapping, Cell, NativeMetadata, NativeGlyph}` keep their existing
  sidecar-mapping serde attributes byte for byte. They compile only with the
  `image` feature, which already pulls in `serde`. The tests pin the
  `{"image":…,"cells":[{"rect":…,"char":…}],"omit_blank":false}` form.
- `Codepoint` inside `sfont` options keeps the model's existing external tagging
  (`{"Unicode": 65}` / `{"Legacy": 66}`).

## Validation policy

Every field of every type in this scope is `pub`, so a Rust struct literal can
already hold any value. Reads and writes validate semantic ranges (for example
`ppem` 1..=4096, `strike_count > 0`, `dot_size` range, PCF pad/unit
combinations), and serde follows that design. Deserialization rejects:

- unknown fields;
- wrong JSON types;
- out-of-range integers (`gray_threshold: 256`, negative sizes);
- unknown enum names, including Rust-cased names such as `"V1_5"`;
- unsafe sidecar paths, through `ResourcePath`'s validating `try_from`.

Semantic validation then runs exactly as it does for Rust callers. The
coordinator accepted this policy on 2026-10-06. One serde
quirk remains, and it was left as is by decision: a struct also deserializes from
a JSON array of its fields in declaration order, so `[]` produces the defaults.
Bindings should pass dicts/objects.

## Enum value names

The coordinator decided (2026-10-06) that a serialized enum value must equal the
CLI value name wherever the CLI has one. The CLI names are the `possible values`
of `fontpixel convert --help`. Batch TOML `options` pass through to the same
flags, so they use the same names. Variants with no CLI value, and variants whose
CLI name already equals the `snake_case` form, keep `snake_case`. Explicit
renames:

| Type | Variant → serialized name |
| --- | --- |
| `iigs::Version` | `V1_1` → `1.1`, `V1_5` → `1.5` |
| `iigs::Container` | `BinScii` → `binscii` |
| `nfnt::Container` | `ResourceFork` → `resource-fork`, `MacBinary` → `macbinary`, `AppleSingle` → `apple-single`, `AppleDouble` → `apple-double`, `BinHex` → `binhex` |
| `palm::PalmVersion` | `V1` → `1`, `V2` → `2`, `FontSubst` → `fontsubst` |
| `pbf::Version` | `One`/`Two`/`Three` → `1`/`2`/`3` |
| `pbf::OffsetWidth` | `U16` → `16`, `U32` → `32` |
| `pcf::GlyphPad` | `One`/`Two`/`Four`/`Eight` → `1`/`2`/`4`/`8` |
| `pcf::ScanUnit` | `One`/`Two`/`Four` → `1`/`2`/`4` |
| `pcf::MetadataPrecedence` | `NormalizedEdits` → `normalized-edits`, `ExplicitProperties` → `explicit-properties` |
| `pfr::PfrDialect` | `FreeType` → `freetype` |
| `playdate::PlaydateFlavor` | `FntEmbedded` → `fnt-embedded`, `FntPng` → `fnt-png` |
| `psf::Version` | `One`/`Two` → `1`/`2` |
| `pxl::PxlVersion` | `V1001` → `1001`, `V1002` → `1002` |
| `raw::RawPacking` | `ByteAlignedRows` → `byte-rows`, `BitAligned` → `bit-aligned` |
| `raw::RawByteOrder` | `RowMajor` → `row-major`, `ColumnMajor` → `column-major` |
| `raw::RawPreset` | `Vga8`/`Vga14`/`Vga16` → `vga8`/`vga14`/`vga16`, `C64Prg` → `64c`, `HarlekinIII` → `harlekin`. Every other `FromStr` spelling is also a serde alias: `vga-8x8`, `vga-8x14`, `vga-8x16`, `c64-prg`, `zx-spectrum`, `harlekin-iii`, `harlekin512`. |
| `riscos::RiscosVersion` | `V4`…`V8` → `4`…`8` (`Xy` stays `xy`) |
| `riscos::RiscosMetricsVersion` | `V0` → `0`, `V2` → `2` |
| `tasprint::TasprintVersion` | `Spectrum48k` → `48k` |
| `vlw::VlwVersion` | `V8`…`V11` → `8`…`11` |
| `wsfont::WsfontOrder` | `LeftToRight` → `left`, `RightToLeft` → `right` |
| `xbin::CharacterCount` | `Chars256` → `256`, `Chars512` → `512` |
| `xbin::ScreenMode` | `FontOnly` → `font-only` |
| `xerox::XeroxBitbltLayout` | `PlainIndex` → `plain-index`, `KernedIndex` → `kerned-index` |

Numeric CLI values serialize as JSON strings (`"1.5"`, `"16"`), not as numbers.
`PrebuiltDepth` serializes by name (`"gray4"`), not by its `repr(u8)` value.
`SymbosExtendedInfo` stays externally tagged (`{"writing_system": 3}` /
`{"layout": {"wysiwyg": true}}`). The CLI's `--symbos-layout screen|wysiwyg`
chooses the `wysiwyg` boolean and does not name a variant.

Core enums embedded here belong to S1. Their CLI names differ from mechanical
`snake_case`, so S1 needs to apply the same rule to them: `BitOrder` (`msb`/`lsb`),
`RowOrder` (`top-down`/`bottom-up`), `outline::Shape` (`square-separate`) and
`cmap::FullRecords` (`non-bmp`/`when-needed`).
