# Serde coverage of core types: shapes and exclusions

This page covers the public structs and enums of `crates/fontpixel/src` outside
`formats/`. With the `serde` feature, bindings accept them as Python dicts or JS
objects and return them in the same form. Format option types are documented with
their formats.

## Shapes

- Structs reject unknown fields. Option and limit types that have a documented
  `Default` fill omitted fields from that default, so `{}` is the default value.
  Value, table, report and result types require every field.
- New enums use `snake_case` variant names. Types that were already serialized
  keep their earlier shapes: model enums such as `Codepoint`, `Spacing` and
  `PropertyValue` keep their variant spelling, which the YAFF native metadata
  also uses. `Report` and the merge report keep the exact JSON the CLI prints.
- `Bitmap<Mono>` and `Bitmap<Gray8>` are `{width, height, data}`, where `data` is
  the canonical row storage from `as_bytes`. Deserialization calls `from_raw`
  under default `BitmapLimits`: a length mismatch or an over-limit raster is an
  error, and Mono padding bits are cleared. Resource limits are not serialized.
- `Font` is `{glyphs, metrics, names, charset, source_format, headers,
  properties}`. `glyphs` holds every glyph slot in ID order, with `null` for a
  removed slot, so stable `GlyphId`s survive. Code and sequence indexes are
  rebuilt. Each glyph is validated as by `Font::add_glyph`, and
  `metrics.default_glyph` must name a live slot.
- `Rational`, `ResourcePath`, `Selection` and `OutputBundle` deserialize through
  their validating constructors (`Rational::new`, `ResourcePath::new`,
  `Selection::parse`, `OutputBundle::new` or `OutputBundle::directory`).
  `ResourcePath` and `Selection` are plain strings.
- OpenType tags (`[u8; 4]`, including OS/2 `vendor_id` and the variant export
  `vendor`) are four-character strings, one character per byte read as Latin-1,
  so `"liga"` stays readable and every byte value round-trips. Tag-keyed maps
  (`Gsub.scripts`, `Script.languages`) are objects keyed by the same strings.
  Other lengths and characters above U+00FF are rejected.
- `MergeOptions.family` is required. The other fields default as in
  `MergeOptions::default()`.

## Partly supported

| Type | Supported | Not supported, and why | Binding approach |
|---|---|---|---|
| `render::MissingGlyph<'a>` | Serialize; Deserialize of `"blank"` and `"error"` | `{"glyph": …}` holds a borrowed `&Glyph`, so deserializing it fails with an explicit error | The replacement glyph is passed as a separate `Glyph` handle argument |
| `render::RenderOptions<'a>` | Serialize and Deserialize, with `missing` handled as above | Same as `MissingGlyph` | Options dict plus an optional replacement-glyph handle |
| `charset::CharsetInfo` | Serialize (the private table id is skipped) | Deserialize: the type holds `&'static` registry data | Read-only records from `charset_table()` |
| `charset::merge_categories::UnicodeRange` | Serialize | Deserialize: `name` is `&'static str` from built-in tables | Read-only constants (`CJK_RANGES`, `UNICODE_BLOCKS`, …) |
| `charset::WindowsCharset` | Serialize | Deserialize: `CodePage` holds a `&'static str` | Returned by `windows_charset(byte)`; input is the byte |

## Excluded

| Type | Reason | Binding approach |
|---|---|---|
| `bitmap::Mono`, `bitmap::Gray8` | Zero-sized pixel-format markers with no data | Bitmap depth is selected by the binding class or the `GlyphBitmap` variant |
| `charset::CharsetCodec` | Codec handle holding `&'static` mapping tables | Handle class built from a charset name and GL flag (`CharsetCodec::new`) |
| `error::Error` | Holds `io::Error` and `&'static str`; `#[non_exhaustive]` | Mapped to exception or error classes that carry the variant fields (`Operation` and `CharsetError` do have serde) |
| `merge::MergePart<'a>` | Borrows `&'a Font` | Font handle plus a part dict (`selection`, `dy`, `label`, `license`), where `Selection` and `PartLicense` have serde |
| `projection::EditValue<'a>`, `PropertyEdit<'a>`, `BitmapProjection<'a>` | Borrowed `&'a str` and `&'a [PropertyEdit]` | Bindings build owned edit lists and borrow them for the call |
| `resources::Input<'a>`, `ReadContext<'a>` | Borrowed input bytes and a `&dyn ResourceResolver` | Bytes/buffer arguments plus a resolver or named-file-set object |
| `resources::NoResources` | Resolver implementation, not data | Used implicitly when no resolver is passed |
| `sfnt::Sfnt<'a>`, `sfnt::TableRecord<'a>` | Borrow the parsed font bytes | Handle class over owned bytes; table records are returned as owned dicts |
| `sfnt::SfntBuilder` | Builder handle; tables are added through validated `insert` | Handle class with `insert(tag, bytes)` and `build(limits)` |
| `sfnt::collection::Collection<'a>`, `Face<'a>` | Borrow the collection bytes | Handle class over owned bytes |
| `sfnt::cff::Cff<'a>` | Borrowed CFF parser state | Handle class over owned bytes |
| `variants::PreparedVariants<'a>` | Borrows or owns the base font through `Cow`, plus private plan state | Handle returned by the prepare call and consumed by export |
| `compat::bdfparser::Font`, `Glyph` | Shared `Rc<RefCell<…>>` handles with mutation semantics from the bdfparser API | Compatibility handle classes |
| `compat::bdfparser::Converter` | Wraps a closure | Python or JS callable adapter |
| `compat::bdfparser::Bitmap` | Compatibility handle with chaining mutators | Compatibility class; data moves through `bindata()` and `Bitmap(rows)` |

## Pending

`compat::bdfparser::SaveOptions` (`Pcf(PcfWriteOptions)`) can get serde as soon
as `formats::pcf::PcfWriteOptions` does. That type belongs to the format option
work, not to this page.
