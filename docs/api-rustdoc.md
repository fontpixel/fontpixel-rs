# Native API quick start

Recovery status: the present phase-6 source is reconstructed, uncompiled and untested.
Historical test reports are not acceptance of this source. See the repository
docs/recovery-status.md for known gaps.

The native API keeps data, diagnostics and output assets separate. It performs
no implicit filesystem reads, writes, process execution or network requests.
Canonical format identifiers are strings such as `"bdf"`, `"pcf"` and `"yaff"`.
Enabled capabilities can be queried with [`FontRegistry::formats`].

## Construct and edit a glyph

```
use fontpixel::{Bitmap, Font, FontSet, Glyph};
use fontpixel::model::{Codepoint, Vector};
let mut glyph = Glyph::new(".#.\n###".parse::<Bitmap>()?);
glyph.codepoints.push(Codepoint::Unicode(65));
glyph.metrics.advance = Some(Vector::new(4, 0));
let mut font = Font::from_glyphs([glyph])?;
font.update_bounds()?;
let id = font.glyph_id(Codepoint::Unicode(65)).unwrap();
font.edit_glyph(id, |g| { g.name = Some("A".into()); Ok(()) })?;
assert_eq!(FontSet::from(font).single()?.len(), 1);
# Ok::<(), fontpixel::Error>(())
```

[`Font`] owns stable glyph slots and effective code/sequence indexes. Use its
editing methods to keep indexes valid. Duplicate mappings remain observable;
ordinary lookup chooses the last source glyph. [`model::Codepoint::Legacy`]
requires the owning font's charset. A Unicode-tagged integer is not automatically
a valid scalar; use [`model::Codepoint::scalar`].

## Read and write with complete reports

```
# #[cfg(feature = "yaff")] {
use fontpixel::{Bitmap, Font, FontSet, Glyph, Input, ResourcePath, WriteOptions};
let font = Font::from_glyphs([Glyph::new("#".parse::<Bitmap>()?)])?;
let fonts = FontSet::from(font);
let registry = fontpixel::registry();
let mut options = WriteOptions::new(ResourcePath::new("example.yaff")?);
options.strict = true;
let written = registry.write(&fonts, "yaff", &options)?;
let bytes = written.output.into_single()?;
let loaded = registry.read(&Input::new(&bytes).with_name("example.yaff"),
                           &Default::default())?;
assert_eq!(loaded.fonts, fonts);
let rewritten = registry.write_with_report(&loaded.fonts, "yaff", &options, loaded.report)?;
assert!(!rewritten.report.has_loss());
# }
# Ok::<(), fontpixel::Error>(())
```

[`read`] is the standalone-byte convenience operation. A reusable [`registry`]
allows typed per-codec configuration. [`Input::with_resources`] supplies explicit
sidecars, and [`Input::directory`] represents a native directory font. The caller
owns the [`ResourceResolver`] authority boundary and must enforce filesystem
symlink safety separately from [`ResourcePath`]'s lexical checks.

[`formats::Loaded`] contains the complete model, diagnostics and selected format.
[`FontSet::single`] refuses empty or multiple strikes instead of silently choosing
one. [`resources::Written`] holds every output asset plus its report;
[`OutputBundle::into_single`] refuses to discard sidecars or flatten directories.
Strict writes reject accumulated information loss before returning output assets.
A persistence layer must stage and protect destinations itself.

`Font::retained_usage` and `FontSet::retained_usage` count retained model storage;
`Font::cloned_usage` preflights one additional owned strike without allocating it.
Keep the original source charged separately. These include index storage and
deleted slots, and fail on the caller's byte bound or excessive property nesting.
The audited Rust 1.99.0 commit on x86-64 Linux permits tighter B-tree and cloned
outer-vector accounting; unverified builds retain conservative fallback charges.
The resource limits are unchanged, and these owned-storage estimates exclude
allocator overhead and process-wide RSS. See `docs/bitmap-model.md` for the exact
toolchain/custom-sysroot guard and proof.

## Render text and map charsets

```
use fontpixel::{Bitmap, Font, Glyph};
use fontpixel::model::{Codepoint, Vector};
let mut glyph = Glyph::new("#".parse::<Bitmap>()?);
glyph.codepoints.push(Codepoint::Unicode(65));
glyph.metrics.advance = Some(Vector::new(2, 0));
let mut font = Font::from_glyphs([glyph])?;
font.update_bounds()?;
let rendered = font.draw("AAA", &Default::default())?;
let svg = fontpixel::render::to_svg(&rendered.bitmap, &Default::default())?;
assert!(svg.contains("<svg"));
# Ok::<(), fontpixel::Error>(())
```

```
let codec = fontpixel::charset::CharsetCodec::new("cp1252", None)?;
assert_eq!(codec.cp2char(0x80), Some('€'));
assert_eq!(codec.char2cp('€'), Some(0x80));
# Ok::<(), fontpixel::charset::CharsetError>(())
```

[`bitmap`] documents packed Mono and Gray8 coverage, fallible transforms, wire
orders and explicit thresholds. [`render`] documents directions, missing glyphs,
cell/advance/ink modes and rendering budgets. [`charset`] provides exact generated
per-glyph mappings; CJK tables require `cjk-charsets`. Unknown charset identity
fallback remains explicitly distinguishable through `CharsetCodec::known`.

## API layers and feature choices

- [`model`]: loss-preserving strikes, glyphs, metrics, names, properties and codes.
- [`formats`]: registry, structural detection, typed codec profiles and reports.
- [`resources`]: caller-owned inputs, directory listings, assets and budgets.
- [`report`] and [`error`]: stable structured diagnostics and typed failures.
- [`outline`]: pixel boundary tracing and explicit outline/raster geometry.
- [`merge`](mod@merge): ordered Unicode selections, baseline/spacing and license reports.
- With `sfnt`: table/container APIs and regional `variants` planning.
- With `woff`/`woff2`: container-level packaging without raster projection.
- With `compat-bdfparser`: the separate Rust SPEC-v3 compatibility facade.

Format features install actual native profiles, not unlimited historical-dialect
support. BDF/PCF/YAFF are defaults; `all-formats` selects the 99 codec IDs. Native
color input projects into Mono/Gray8 coverage, not arbitrary RGBA preservation.
OpenType raster conversion, table-preserving container coding and byte-preserving
subsetting serve different purposes. Python/WASM bindings belong to later phases.

The repository's `docs/api.md` contains a longer guide and migration links.
Run `cargo run -p fontpixel --all-features --example api_quickstart` for a
filesystem-free tour including sidecar bundles, regional plans, subsets and WOFF2.

Both planned Python and WASM/JS bindings must expose the complete native public
capabilities as independent developer APIs, alongside separate legacy-compatible
APIs. Exhaustive operation/type/option mappings and native coverage tests are
required. Authorized work is phase 7 then phase 8 after phase-6 delivery; phase 9
and package publication are excluded.
