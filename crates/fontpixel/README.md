# fontpixel

Bitmap font library for Rust: detect, read, edit, render and write bitmap
fonts in 99 formats (BDF, PCF, PSF, YAFF, GNU Unifont HEX, Windows FNT/FON,
BMFont, image atlases, C headers, OpenType bitmap and outline fonts,
WOFF/WOFF2, TTC, and many historical formats), with exact charset tables,
OpenType `locl` regional variants, subsetting and merging.

- Every conversion returns a structured report; information that a target
  format cannot keep is reported as a loss, and strict mode refuses it.
- Bounded readers: every input, sidecar, allocation and output has an explicit
  resource limit (`ResourceLimits`).
- No implicit filesystem, network or process access. Inputs are byte buffers
  plus an optional caller-owned resolver for sidecar files; writers return an
  in-memory `OutputBundle`.
- No `unsafe` code (`#![forbid(unsafe_code)]`).

```toml
[dependencies]
fontpixel = "0.1"                                              # bdf, pcf, yaff
# fontpixel = { version = "0.1", features = ["all-formats"] }  # all 99 formats
```

```rust
use fontpixel::{Bitmap, Font, FontSet, Glyph};
use fontpixel::model::Codepoint;

fn main() -> fontpixel::Result<()> {
    let mut glyph = Glyph::new(".#.\n###\n#.#".parse::<Bitmap>()?);
    glyph.codepoints.push(Codepoint::Unicode(65));
    let font = Font::from_glyphs([glyph])?;
    let set = FontSet::from(font);
    assert_eq!(set.single()?.len(), 1);
    Ok(())
}
```

## Features

Each format has its own Cargo feature (`bdf`, `pcf`, `psf`, `hex`, `fnt`,
`ttf`, `woff2`, ...). The defaults are `bdf`, `pcf` and `yaff`;
`all-formats` selects all 99. With `--no-default-features` the crate has no
dependencies. Other features:

- `cjk-charsets`: exact CJK legacy charset tables (GB2312, GBK, GB18030, Big5,
  Big5-HKSCS, Shift_JIS, EUC-JP, EUC-KR, ...), generated from CPython's codecs
- `serde`: serializable reports and options
- `compat-bdfparser`: the Rust core of the bdfparser-compatible API used by
  the Python and JavaScript bindings

## Related packages

- [`fontpixel-cli`](https://crates.io/crates/fontpixel-cli): the `fontpixel`
  command-line tool
- [`fontpixel-ops`](https://crates.io/crates/fontpixel-ops): the command-line
  operations as a typed Rust API
- Python: [`fontpixel` on PyPI](https://pypi.org/project/fontpixel/)
- JavaScript/WebAssembly: [`fontpixel` on npm](https://www.npmjs.com/package/fontpixel)

Documentation: [API reference](https://docs.rs/fontpixel), the
[repository](https://github.com/fontpixel/fontpixel-rs) (format documents in
`docs/formats/`, design decisions in `docs/decisions.md`).

## License

MIT. Generated charset tables and some format semantics derive from
third-party sources; see `THIRD_PARTY_NOTICES.md` and `LICENSES/`.
