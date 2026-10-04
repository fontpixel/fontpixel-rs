# Unifont hexdraw

Primary reference: [GNU Unifont manual, hexdraw](https://unifoundry.com/unifont/doc/unifont.pdf).
A hexadecimal Unicode label and colon start each glyph. Pixel rows use `#` and
`-`, with leading whitespace; the first row may follow the colon. Blank lines
separate glyphs. Width and height are inferred from rectangular rows. Production
code implements this text grammar independently; GNU GPL utility source is not
copied. The MIT monobit reader is an independent interoperability oracle.

The typed writer reports all mappings, metrics, properties, and source metadata
that the text representation cannot retain. Legacy codes are never guessed to be
Unicode. Resource limits bound text lines, glyphs, pixels, and aggregate model.

`DrawCodec`, `read(bytes, limits)`, and `write(set, DrawWriteOptions)` operate on
one strike. `gray_threshold` must be supplied for Gray8. Input labels have four
to six hex digits and must be Unicode scalars. Ragged rows, empty glyphs, and
unexpected syntax fail. The bounded detector returns `Possible`, never a
filename-derived content match. `%` comments are retained in ordered
`draw.comment` font headers; their original line placement is not retained.

Widths and heights can vary per glyph; writing retains its rectangular raster
and normalizes its origin to `(0,0)` with a width-sized advance. Only the first
valid Unicode scalar per source glyph is emitted; every other alias, legacy
mapping, sequence, omitted glyph, changed metric, property, name, charset, and
source-format marker is reported. This includes stable glyph-table holes.
The writer validates the emitted text under the same limits while charging the
retained source model. No-loss output therefore decodes to the same full model.

Validation: hand-authored wire examples and randomized rectangular Unicode
fonts in `crates/fontpixel/tests/draw.rs`; bidirectional monobit `hexdraw`, the
catalogue BDF parser, and Tom Thumb catalogue raster equality in
`tests/crosscheck/w41.py`; `fuzz/fuzz_targets/draw.rs`.

The writer preflights text/raster capacity and reserves its read-back model.
Diagnostic entries and formatted strings share the source/output peak budget;
resource failures return typed errors without expanding a full failure report.
