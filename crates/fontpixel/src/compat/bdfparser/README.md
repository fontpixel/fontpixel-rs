# SPEC-v3 compatibility façade

Enable `fontpixel/compat-bdfparser`; add `cjk-charsets` for the full codec set.
The feature pulls BDF, PCF and ordered/exact JSON metadata, leaving native minimal
BDF/PCF builds unchanged. The `fontpixel-cli/parity` feature selects the full set
and builds `fontpixel-parity`.

`Font` owns one native model behind generation-checked shared attachment state;
`Font::clone` and `subset` deep-copy it. `Glyph` is a metadata snapshot; its edits
write through while the original slot still exists. Two handles merge unchanged
fields rather than reverting one another. `Glyph::clone` is detached. Native
models remain available through `Font::core` and `Font::from_core`.

`Bitmap` is a mutating/chaining façade over native Gray8 labels 0/1/2, with Python
zero-height normalization, substring replacement, all SPEC geometry/effects,
exact todata integer/hex conversion, Pillow byte palettes and exact SVG output.
Ragged intermediate rows are rejected (K12, outside the valid SPEC input domain).
`bindata()` materializes rows only when requested.

Font query/edit methods retain lowercase SPEC names. Rust uses explicit arguments
and typed `CreateOptions`, `PcfWriteOptions`, `TtfOptions` and `SaveOptions`.
Draw keyword maps mirror the operation-script vocabulary (`ucgs`, `r`, `mode`,
`direction`, `linelimit`, `missing`). Transform functions are Rust closures.
`set_converters` accepts numeric `Converter` closures. `cp2char` returns Rust
scalars; `cp2unicode`/`unicode2cp` retain Python's numeric surrogate identity.
There is no global warning formatter: `Font::report` collects structured warnings,
including unknown-charset warnings once per font/name. Fatal syntax errors use
native `Error::BdfParse { line, message }`; other failures use native typed
InvalidInput, ResourceLimit or Io errors. Python/JS exception names are not Rust
class names. Binding-language conveniences such as URLs, canvas drawing and
operator overloading belong to the respective bindings.

Loading supports strings, line iterators, binary streams/files, PCF, glyph-based
TTF and bounded gzip transport. Serialization can target bytes, strings, streams
or files. The exact compatibility outline/TTF/SVG routines are isolated here;
this does not advertise future general native sfnt or SVG-font format support.

See `tests/parity/README.md`, `docs/formats/ttf-compat.md` and `reports/w1.5.md`.
