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

Semantic validation then runs exactly as it does for Rust callers. One serde
quirk remains: a struct also deserializes from a JSON array of its fields in
declaration order, so `[]` produces the defaults. Bindings should pass
dicts/objects.

## Naming notes

- Enum names come from mechanical `snake_case`. `RawPreset::HarlekinIII` has an
  explicit `"harlekin_iii"`, because the mechanical form would be `harlekin_i_i_i`.
- `PrebuiltDepth` serializes by name (`"gray4"`), not by its `repr(u8)` value.
- `RawPreset`'s `FromStr` CLI aliases (`"64c"`, `"vga-8x16"`, `"harlekin-iii"`, …)
  are not serde aliases. Its serde names are the variant names (`"c64_prg"`,
  `"vga16"`, `"harlekin_iii"`). The CLI tokens `binscii`, `fontsubst` and
  `freetype` likewise correspond to the serde names `bin_scii`, `font_subst` and
  `free_type`.
