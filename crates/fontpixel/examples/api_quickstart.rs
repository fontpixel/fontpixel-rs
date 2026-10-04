//! Filesystem-free native API walkthrough reconstructed from current public APIs.
//!
//! CURRENT STATUS: uncompiled and untested after workspace recovery. The checks
//! below are executable examples for later authorized validation, not pass claims.
//! No fonts or images are read/written on disk; diagnostics go to stdout.
//! Run default, no-default-features and all-features separately when permitted.
use fontpixel::metadata::{NameOverrides, apply_names};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::{Bitmap, Error, Font, FontSet, Glyph, ResourceLimits, Result};

fn missing(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn show_report(label: &str, report: &fontpixel::Report) {
    println!("{label}: {report}");
}

fn source_font() -> Result<Font> {
    let mut glyph_a = Glyph::new(".#.\n###\n#.#".parse::<Bitmap>()?);
    glyph_a.codepoints.push(Codepoint::Unicode(u32::from('A')));
    glyph_a.metrics.advance = Some(Vector::new(4, 0));
    let mut glyph_b = Glyph::new("##.\n###\n##.".parse::<Bitmap>()?);
    glyph_b.codepoints.push(Codepoint::Unicode(u32::from('B')));
    glyph_b.metrics.advance = Some(Vector::new(4, 0));
    let mut font = Font::from_glyphs([glyph_a, glyph_b])?;
    font.names.family = Some("API Example".into());
    font.names.style = Some("Regular".into());
    font.names.license = Some("CC0-1.0".into());
    font.metrics.pixel_size = Some(5);
    font.metrics.ascent = Some(3);
    font.metrics.descent = Some(2);
    font.update_bounds()?;
    let id = font
        .glyph_id(Codepoint::Unicode(u32::from('A')))
        .ok_or_else(|| missing("example A is missing"))?;
    font.edit_glyph(id, |glyph| {
        glyph.name = Some("A".into());
        Ok(())
    })?;
    font.validate(&ResourceLimits::default())?;
    Ok(font)
}

fn model_and_metadata() -> Result<FontSet> {
    let mut fonts = FontSet::from(source_font()?);
    let limits = ResourceLimits::default();
    let usage = fonts.retained_usage(limits.max_total_bytes)?;
    assert_eq!((usage.fonts, usage.glyphs), (1, 2));
    let clone_usage = fonts.single()?.cloned_usage(limits.max_total_bytes)?;
    assert_eq!(clone_usage.glyphs, 2);
    // This clone estimate does not release or discount the original source.
    let audit = apply_names(
        &mut fonts,
        &NameOverrides {
            family: Some("API Renamed".into()),
            ..Default::default()
        },
        &limits,
    )?;
    assert_eq!(fonts.single()?.names.family.as_deref(), Some("API Renamed"));
    assert!(!audit.has_loss());
    show_report("intentional name edits", &audit);

    // An invalid explicit identity must fail atomically.
    let before = fonts.clone();
    assert!(apply_names(
        &mut fonts,
        &NameOverrides {
            family: Some("   ".into()),
            ..Default::default()
        },
        &limits,
    )
    .is_err());
    assert_eq!(fonts, before);
    Ok(fonts)
}

fn render_and_charset(font: &Font) -> Result<()> {
    let rendered = font.draw("ABA", &fontpixel::render::RenderOptions::default())?;
    show_report("render", &rendered.report);
    let svg = fontpixel::render::to_svg(&rendered.bitmap, &Default::default())?;
    assert!(svg.contains("<svg"));
    let codec = fontpixel::charset::CharsetCodec::new("cp1252", None)
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    assert!(codec.known());
    assert_eq!(codec.cp2char(0x80), Some('€'));
    assert_eq!(codec.char2cp('€'), Some(0x80));
    Ok(())
}

fn merge_selected(font: &Font) -> Result<()> {
    use fontpixel::merge::{MergeOptions, MergePart, MergeSpacing, Selection, merge};
    let result = merge(
        &[MergePart::new(font, Selection::parse("U+0041-0042")?)],
        &MergeOptions {
            family: "API Merged".into(),
            spacing: MergeSpacing::Prop,
            strict: true,
            ..Default::default()
        },
    )?;
    assert_eq!(result.font.len(), 2);
    assert!(!result.report.diagnostics.has_loss());
    println!("merge: {}", result.report);
    assert!(result.report.to_json().contains("API Merged"));
    Ok(())
}

#[cfg(feature = "yaff")]
fn lossless_roundtrip(fonts: &FontSet) -> Result<()> {
    use fontpixel::{Input, ReadOptions, ResourcePath, WriteOptions};
    let registry = fontpixel::registry();
    let mut write = WriteOptions::new(ResourcePath::new("example.yaff")?);
    write.strict = true;
    let written = registry.write(fonts, "yaff", &write)?;
    assert!(!written.report.has_loss());
    let bytes = written.output.into_single()?;
    let loaded = registry.read(
        &Input::new(&bytes).with_name("example.yaff"),
        &ReadOptions {
            format: Some("yaff".into()),
            ..Default::default()
        },
    )?;
    assert_eq!(&loaded.fonts, fonts);
    // Forward the reader report so strict writing covers the complete operation.
    let rewritten = registry.write_with_report(&loaded.fonts, "yaff", &write, loaded.report)?;
    assert!(!rewritten.report.has_loss());
    Ok(())
}

#[cfg(feature = "pcf")]
fn typed_pcf(fonts: &FontSet) -> Result<()> {
    use fontpixel::formats::pcf::{Order, PcfWriteOptions};
    use fontpixel::{Input, ReadOptions, ResourcePath, WriteOptions};
    let mut registry = fontpixel::registry();
    registry.set_pcf_write_options(PcfWriteOptions {
        byte_order: Order::Lsb,
        ..Default::default()
    })?;
    let written = registry.write(fonts, "pcf", &WriteOptions::new(ResourcePath::new("example.pcf")?))?;
    show_report("PCF write (metadata may be lost)", &written.report);
    let bytes = written.output.into_single()?;
    let loaded = registry.read(
        &Input::new(&bytes),
        &ReadOptions {
            format: Some("pcf".into()),
            ..Default::default()
        },
    )?;
    show_report("PCF read", &loaded.report);
    let original = fonts.single()?.glyph_for_codepoint(Codepoint::Unicode(65))
        .ok_or_else(|| missing("source A is missing"))?;
    let decoded = loaded.fonts.single()?.glyph_for_codepoint(Codepoint::Unicode(65))
        .ok_or_else(|| missing("PCF A is missing"))?;
    assert_eq!(decoded.bitmap, original.bitmap);
    assert_eq!(decoded.metrics.advance, original.metrics.advance);
    Ok(())
}

#[cfg(feature = "image")]
mod atlas {
    use super::*;
    use fontpixel::{Input, OutputBundle, ReadOptions, ResourcePath, ResourceResolver, WriteOptions};

    struct Assets<'a>(&'a OutputBundle);
    impl ResourceResolver for Assets<'_> {
        fn resolve(&self, path: &ResourcePath, maximum: usize) -> Result<Vec<u8>> {
            let asset = self.0.assets().iter().find(|asset| &asset.path == path)
                .ok_or_else(|| Error::MissingResource(path.to_string()))?;
            let limit_error = || Error::ResourceLimit {
                resource: "example sidecar allocation",
                limit: maximum,
            };
            if asset.bytes.len() > maximum {
                return Err(limit_error());
            }
            let mut copy = Vec::new();
            copy.try_reserve_exact(asset.bytes.len()).map_err(|_| limit_error())?;
            if copy.capacity() > maximum {
                return Err(limit_error());
            }
            copy.extend_from_slice(&asset.bytes);
            Ok(copy)
        }
    }

    pub fn roundtrip(fonts: &FontSet) -> Result<()> {
        let registry = fontpixel::registry();
        let mut options = WriteOptions::new(ResourcePath::new("atlas.png")?);
        options.strict = true;
        let written = registry.write(fonts, "image", &options)?;
        assert!(!written.report.has_loss());
        assert!(written.output.assets().len() >= 2);
        let resources = Assets(&written.output);
        let primary = written.output.assets().iter()
            .find(|asset| &asset.path == written.output.primary())
            .ok_or_else(|| missing("atlas primary asset is missing"))?;
        let input = Input::new(&primary.bytes)
            .with_name(primary.path.as_str())
            .with_resources(&resources);
        let loaded = registry.read(&input, &ReadOptions {
            format: Some("image".into()),
            ..Default::default()
        })?;
        assert_eq!(&loaded.fonts, fonts);
        show_report("atlas read", &loaded.report);
        // A host persistence layer must deliver every asset, not just the PNG.
        Ok(())
    }
}

#[cfg(feature = "otb")]
fn embedded_metrics(fonts: &FontSet) -> Result<()> {
    use fontpixel::formats::embedded::{EmbeddedReferenceStrike, EmbeddedRounding, EmbeddedWriteOptions};
    use fontpixel::{ResourcePath, WriteOptions};
    let mut registry = fontpixel::registry();
    registry.set_embedded_write_options(EmbeddedWriteOptions {
        units_per_em: Some(1000),
        reference_strike: EmbeddedReferenceStrike::LargestPpem,
        rounding: EmbeddedRounding::NearestTiesEven,
        ..Default::default()
    });
    let written = registry.write(fonts, "otb", &WriteOptions::new(ResourcePath::new("example.otb")?))?;
    show_report("OTB write", &written.report);
    let bytes = written.output.into_single()?;
    let parsed = fontpixel::sfnt::Sfnt::parse(&bytes, &Default::default())?;
    assert!(parsed.table(*b"EBDT").is_some());
    assert!(parsed.table(*b"EBLC").is_some());
    Ok(())
}

#[cfg(feature = "sfnt")]
fn regional_subset(font: &Font) -> Result<()> {
    use fontpixel::sfnt::{self, name::NameTable};
    use fontpixel::variants::{Options, ScriptVariant, Variants, script_tag};
    let mut variants = Variants::new(font.clone());
    let mut japanese = font.clone();
    let a = japanese.glyph_id(Codepoint::Unicode(65))
        .ok_or_else(|| missing("regional A is missing"))?;
    japanese.edit_glyph(a, |glyph| glyph.shift(1, 0))?;
    japanese.update_bounds()?;
    variants.add_language("ja", japanese)?;
    variants.scripts.push(ScriptVariant {
        scripts: vec![script_tag("latn")?],
        font: font.clone(),
    });
    let options = Options {
        family: Some("API Regional".into()),
        style: Some("Square".into()),
        legacy_style: Some("Regular".into()),
        full_name: Some("API Regional Square".into()),
        postscript_name: Some("APIRegional-Square".into()),
        ..Default::default()
    };
    let prepared = variants.prepare(&options.limits)?;
    assert!(prepared.plan().cmap.contains_key(&65));
    let encoded = prepared.encode(&options)?;
    show_report("regional output", &encoded.report);
    let parsed = sfnt::Sfnt::parse(&encoded.bytes, &options.limits.sfnt)?;
    let names = NameTable::decode(
        parsed.table(*b"name").ok_or_else(|| missing("name table is missing"))?,
        &options.limits.sfnt,
    )?;
    let full_name = names.records.iter().find(|record| record.platform_id == 3 && record.name_id == 4)
        .ok_or_else(|| missing("full-name record is missing"))?;
    assert_eq!(full_name.text()?.as_deref(), Some("API Regional Square"));
    let subset = sfnt::subset::subset(&encoded.bytes, &sfnt::subset::Options::text("A"))?;
    show_report("subset", &subset.report);
    assert!(subset.glyph_mapping.contains_key(&0));
    sfnt::Sfnt::parse(&subset.bytes, &Default::default())?;
    #[cfg(feature = "woff2")]
    webfont(&subset.bytes)?;
    Ok(())
}

#[cfg(feature = "woff2")]
fn webfont(sfnt: &[u8]) -> Result<()> {
    use fontpixel::woff::{Limits, woff2};
    let limits = Limits::default();
    let encoded = woff2::encode(sfnt, &woff2::Options {
        compression_threads: 1,
        ..Default::default()
    }, &limits)?;
    show_report("WOFF2 encode", &encoded.report);
    let decoded = woff2::decode(&encoded.bytes, &limits)?;
    show_report("WOFF2 decode", &decoded.report);
    fontpixel::sfnt::Sfnt::parse(&decoded.sfnt, &limits.sfnt)?;
    // Container coding may canonicalize tables/head flags; raw-byte equality is
    // not a general WOFF2 round-trip promise. Preserve decoded.auxiliary on rewrap.
    Ok(())
}

fn main() -> Result<()> {
    let fonts = model_and_metadata()?;
    render_and_charset(fonts.single()?)?;
    merge_selected(fonts.single()?)?;
    #[cfg(feature = "yaff")]
    lossless_roundtrip(&fonts)?;
    #[cfg(feature = "pcf")]
    typed_pcf(&fonts)?;
    #[cfg(feature = "image")]
    atlas::roundtrip(&fonts)?;
    #[cfg(feature = "otb")]
    embedded_metrics(&fonts)?;
    #[cfg(feature = "sfnt")]
    regional_subset(fonts.single()?)?;
    println!("Native API walkthrough finished for the enabled features.");
    Ok(())
}
