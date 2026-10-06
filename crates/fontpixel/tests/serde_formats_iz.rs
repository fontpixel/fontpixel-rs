//! serde forms of the typed option, codec and value types of the formats whose
//! module names start with i–z, plus `vector_raster`. Bindings accept these as
//! Python dicts / JS objects, so every documented default must survive a JSON
//! round trip, an empty object must mean the default, and malformed input must
//! be rejected instead of silently ignored.
#![cfg(feature = "serde")]
use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;

/// `T::default()` round-trips, and `{}` deserializes to that same default.
fn defaults<T: Serialize + DeserializeOwned + Default>() -> String {
    let json = serde_json::to_string(&T::default()).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    let empty: T = serde_json::from_str("{}").unwrap();
    assert_eq!(serde_json::to_string(&empty).unwrap(), json);
    json
}

/// Explicit values round-trip unchanged.
fn same<T: Serialize + DeserializeOwned>(value: &T) {
    let json = serde_json::to_string(value).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
}

/// Every variant has the given snake_case name and parses back; Rust-cased and
/// unknown names are rejected.
fn variants<T: Serialize + DeserializeOwned + PartialEq + Debug>(all: &[(T, &str)]) {
    for (value, name) in all {
        assert_eq!(serde_json::to_string(value).unwrap(), format!("\"{name}\""));
        let back: T = serde_json::from_str(&format!("\"{name}\"")).unwrap();
        assert_eq!(&back, value);
    }
    rejects::<T>("\"no_such_variant\"");
    rejects::<T>("7");
}

fn rejects<T: DeserializeOwned>(json: &str) {
    assert!(
        serde_json::from_str::<T>(json).is_err(),
        "{} accepted {json}",
        std::any::type_name::<T>()
    );
}

/// Unknown fields and wrongly typed or out-of-range common fields are rejected.
fn rejects_common<T: DeserializeOwned>() {
    rejects::<T>(r#"{"no_such_field": 1}"#);
    rejects::<T>("\"options\"");
    rejects::<T>("1");
    rejects::<T>("null");
}

/// Shared checks for write options with the usual threshold/limit fields.
fn rejects_write<T: DeserializeOwned>() {
    rejects_common::<T>();
    rejects::<T>(r#"{"gray_threshold": 256}"#);
    rejects::<T>(r#"{"gray_threshold": -1}"#);
    rejects::<T>(r#"{"limits": {"max_glyphs": -1}}"#);
    rejects::<T>(r#"{"limits": {"max_glyphs": "many"}}"#);
    rejects::<T>(r#"{"limits": {"no_such_limit": 1}}"#);
}

fn rejects_output<T: DeserializeOwned>() {
    rejects::<T>(r#"{"max_output_bytes": -1}"#);
    rejects::<T>(r#"{"max_output_bytes": 1.5}"#);
}

/// Sidecar paths deserialize through `ResourcePath` validation.
fn rejects_path<T: DeserializeOwned>(field: &str) {
    for bad in ["../escape", "/abs", "a//b", "c:\\x", ""] {
        let json = serde_json::json!({ field: bad }).to_string();
        rejects::<T>(&json);
    }
    let json = serde_json::json!({ field: "fonts/side.car" }).to_string();
    serde_json::from_str::<T>(&json).unwrap();
}

/// Helper self-check on a local probe shaped like the format options, so the
/// helpers are exercised (and must fail on bad input) in every feature profile.
#[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
struct Probe {
    path: Option<fontpixel::resources::ResourcePath>,
    gray_threshold: Option<u8>,
    max_output_bytes: usize,
    limits: fontpixel::ResourceLimits,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProbeKind {
    First,
    SecondKind,
}

#[test]
fn helpers() {
    defaults::<Probe>();
    same(&Probe {
        gray_threshold: Some(128),
        ..Probe::default()
    });
    variants(&[
        (ProbeKind::First, "first"),
        (ProbeKind::SecondKind, "second_kind"),
    ]);
    rejects_write::<Probe>();
    rejects_output::<Probe>();
    rejects_path::<Probe>("path");
    let accepted = std::panic::catch_unwind(|| rejects::<Probe>("{}"));
    assert!(accepted.is_err(), "rejects must fail on valid input");
}

#[cfg(feature = "iigs")]
#[test]
fn iigs() {
    use fontpixel::formats::iigs::*;
    defaults::<IigsReadOptions>();
    defaults::<IigsWriteOptions>();
    defaults::<IigsCodec>();
    variants(&[
        (Version::Auto, "auto"),
        (Version::V1_1, "1.1"),
        (Version::V1_5, "1.5"),
    ]);
    variants(&[(Container::Bare, "bare"), (Container::BinScii, "binscii")]);
    rejects_common::<IigsReadOptions>();
    rejects_write::<IigsWriteOptions>();
    rejects_output::<IigsWriteOptions>();
    rejects::<IigsWriteOptions>(r#"{"version": "V1_5"}"#);
    rejects::<IigsCodec>(r#"{"write_options": {"container": "zip"}}"#);
}

#[cfg(feature = "image")]
#[test]
fn image() {
    use fontpixel::formats::image::*;
    defaults::<ImageSetOptions>();
    defaults::<ImageReadOptions>();
    defaults::<ImageWriteOptions>();
    defaults::<ImageCodec>();
    variants(&[
        (ImageLayout::Atlas, "atlas"),
        (ImageLayout::Imageset, "imageset"),
    ]);
    variants(&[
        (ImageSetDomain::Legacy, "legacy"),
        (ImageSetDomain::Unicode, "unicode"),
    ]);
    variants(&[
        (ImageSetBackground::Black, "black"),
        (ImageSetBackground::White, "white"),
        (ImageSetBackground::Alpha, "alpha"),
    ]);
    variants(&[(MappingFormat::Json, "json"), (MappingFormat::Toml, "toml")]);
    variants(&[
        (Encoding::Png, "png"),
        (Encoding::Bmp, "bmp"),
        (Encoding::Gif, "gif"),
        (Encoding::Tga, "tga"),
        (Encoding::Pbm, "pbm"),
        (Encoding::Pgm, "pgm"),
    ]);
    // The embedded sidecar mapping keeps its existing serialized form.
    let read: ImageReadOptions = serde_json::from_str(
        r#"{"mapping": {"image": "a.png", "cell_size": [8, 8], "cells": [{"rect": [0, 0, 8, 8], "char": "A"}]}}"#,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_string(&read.mapping).unwrap(),
        r#"{"image":"a.png","cell_size":[8,8],"cells":[{"rect":[0,0,8,8],"char":"A"}],"omit_blank":false}"#
    );
    same(&read);
    rejects_common::<ImageSetOptions>();
    rejects_common::<ImageReadOptions>();
    rejects_path::<ImageReadOptions>("mapping_path");
    rejects::<ImageReadOptions>(r#"{"mapping": {"cells": [{"rect": [0, 0, 8]}]}}"#);
    rejects_write::<ImageWriteOptions>();
    rejects::<ImageWriteOptions>(r#"{"encoding": "jpeg"}"#);
    rejects::<ImageWriteOptions>(r#"{"columns": -1}"#);
    rejects::<ImageSetOptions>(r#"{"radix": 4294967296}"#);
}

#[cfg(feature = "kbits")]
#[test]
fn kbits() {
    use fontpixel::formats::kbits::*;
    defaults::<KbitsWriteOptions>();
    defaults::<KbitsCodec>();
    rejects_common::<KbitsWriteOptions>();
    rejects::<KbitsWriteOptions>(r#"{"limits": {"max_glyphs": -1}}"#);
}

#[cfg(feature = "kbitx")]
#[test]
fn kbitx() {
    use fontpixel::formats::kbitx::*;
    defaults::<KbitxReadOptions>();
    defaults::<KbitxWriteOptions>();
    defaults::<KbitxCodec>();
    rejects_common::<KbitxReadOptions>();
    rejects::<KbitxReadOptions>(r#"{"gray_threshold": 256}"#);
    rejects_common::<KbitxWriteOptions>();
    rejects_output::<KbitxWriteOptions>();
}

#[cfg(feature = "lisa")]
#[test]
fn lisa() {
    use fontpixel::formats::lisa::*;
    defaults::<LisaReadOptions>();
    defaults::<LisaWriteOptions>();
    defaults::<LisaCodec>();
    variants(&[
        (Compression::Preserve, "preserve"),
        (Compression::Uncompressed, "uncompressed"),
        (Compression::Compressed, "compressed"),
    ]);
    rejects_common::<LisaReadOptions>();
    rejects::<LisaReadOptions>(r#"{"charset": 1}"#);
    rejects_write::<LisaWriteOptions>();
    rejects_output::<LisaWriteOptions>();
}

#[cfg(feature = "mgtk")]
#[test]
fn mgtk() {
    use fontpixel::formats::mgtk::*;
    defaults::<MgtkReadOptions>();
    defaults::<MgtkWriteOptions>();
    defaults::<MgtkCodec>();
    variants(&[
        (Columns::Auto, "auto"),
        (Columns::Single, "single"),
        (Columns::Double, "double"),
    ]);
    rejects_common::<MgtkReadOptions>();
    rejects_write::<MgtkWriteOptions>();
    rejects_output::<MgtkWriteOptions>();
}

#[cfg(feature = "mkwinfont")]
#[test]
fn mkwinfont() {
    use fontpixel::formats::mkwinfont::*;
    assert_eq!(defaults::<MkwinfontReadOptions>(), "{}");
    defaults::<MkwinfontWriteOptions>();
    defaults::<MkwinfontCodec>();
    rejects_common::<MkwinfontReadOptions>();
    rejects_common::<MkwinfontCodec>();
}

#[cfg(feature = "nfnt")]
#[test]
fn nfnt() {
    use fontpixel::formats::nfnt::*;
    defaults::<NfntReadOptions>();
    defaults::<NfntWriteOptions>();
    defaults::<NfntCodec>();
    variants(&[
        (Container::Bare, "bare"),
        (Container::ResourceFork, "resource-fork"),
        (Container::Dfont, "dfont"),
        (Container::Suitcase, "suitcase"),
        (Container::MacBinary, "macbinary"),
        (Container::AppleSingle, "apple-single"),
        (Container::AppleDouble, "apple-double"),
        (Container::BinHex, "binhex"),
    ]);
    variants(&[(ResourceType::Nfnt, "nfnt"), (ResourceType::Font, "font")]);
    rejects_common::<NfntReadOptions>();
    rejects_path::<NfntReadOptions>("apple_double_resource");
    rejects_write::<NfntWriteOptions>();
    rejects_output::<NfntWriteOptions>();
}

#[cfg(feature = "oberon")]
#[test]
fn oberon() {
    use fontpixel::formats::oberon::*;
    defaults::<OberonReadOptions>();
    defaults::<OberonWriteOptions>();
    defaults::<OberonCodec>();
    rejects_common::<OberonReadOptions>();
    rejects_write::<OberonWriteOptions>();
    rejects_output::<OberonWriteOptions>();
    rejects::<OberonWriteOptions>(r#"{"family": 300}"#);
}

#[cfg(feature = "palm")]
#[test]
fn palm() {
    use fontpixel::formats::palm::*;
    defaults::<PalmReadOptions>();
    defaults::<PalmWriteOptions>();
    defaults::<PalmCodec>();
    variants(&[
        (PalmContainer::Bare, "bare"),
        (PalmContainer::Pdb, "pdb"),
        (PalmContainer::Prc, "prc"),
    ]);
    variants(&[
        (PalmVersion::V1, "1"),
        (PalmVersion::V2, "2"),
        (PalmVersion::Afnx, "afnx"),
        (PalmVersion::FontSubst, "fontsubst"),
        (PalmVersion::Gray1, "gray1"),
        (PalmVersion::Gray2, "gray2"),
    ]);
    variants(&[
        (PalmRounding::Exact, "exact"),
        (PalmRounding::Floor, "floor"),
    ]);
    rejects_common::<PalmReadOptions>();
    rejects_write::<PalmWriteOptions>();
    rejects_output::<PalmWriteOptions>();
    rejects::<PalmWriteOptions>(r#"{"densities": [72, 70000]}"#);
}

#[cfg(feature = "pbf")]
#[test]
fn pbf() {
    use fontpixel::formats::pbf::*;
    defaults::<PbfReadOptions>();
    defaults::<PbfWriteOptions>();
    defaults::<PbfCodec>();
    variants(&[
        (Version::One, "1"),
        (Version::Two, "2"),
        (Version::Three, "3"),
    ]);
    variants(&[(Compression::Raw, "raw"), (Compression::Rle4, "rle4")]);
    variants(&[
        (OffsetWidth::Auto, "auto"),
        (OffsetWidth::U16, "16"),
        (OffsetWidth::U32, "32"),
    ]);
    rejects_common::<PbfReadOptions>();
    rejects_write::<PbfWriteOptions>();
    rejects_output::<PbfWriteOptions>();
    rejects::<PbfWriteOptions>(r#"{"wildcard": 65536}"#);
}

#[cfg(feature = "pcf")]
#[test]
fn pcf() {
    use fontpixel::formats::pcf::*;
    defaults::<PcfWriteOptions>();
    defaults::<PcfCodec>();
    variants(&[(Order::Msb, "msb"), (Order::Lsb, "lsb")]);
    variants(&[
        (GlyphPad::One, "1"),
        (GlyphPad::Two, "2"),
        (GlyphPad::Four, "4"),
        (GlyphPad::Eight, "8"),
    ]);
    variants(&[
        (ScanUnit::One, "1"),
        (ScanUnit::Two, "2"),
        (ScanUnit::Four, "4"),
    ]);
    variants(&[
        (MetadataPrecedence::NormalizedEdits, "normalized-edits"),
        (
            MetadataPrecedence::ExplicitProperties,
            "explicit-properties",
        ),
    ]);
    let explicit: PcfWriteOptions =
        serde_json::from_str(r#"{"byte_order": "lsb", "glyph_pad": "1"}"#).unwrap();
    assert_eq!(explicit.byte_order, Order::Lsb);
    assert_eq!(explicit.glyph_pad, GlyphPad::One);
    assert_eq!(explicit.scan_unit, PcfWriteOptions::default().scan_unit);
    rejects_write::<PcfWriteOptions>();
    rejects_output::<PcfWriteOptions>();
    rejects::<PcfWriteOptions>(r#"{"scan_unit": "8"}"#);
}

#[cfg(feature = "pcgeos")]
#[test]
fn pcgeos() {
    use fontpixel::formats::pcgeos::*;
    defaults::<PcgeosReadOptions>();
    defaults::<PcgeosWriteOptions>();
    defaults::<PcgeosCodec>();
    variants(&[(PcgeosVariant::Sbcs, "sbcs"), (PcgeosVariant::Dbcs, "dbcs")]);
    rejects_common::<PcgeosReadOptions>();
    rejects_write::<PcgeosWriteOptions>();
    rejects_output::<PcgeosWriteOptions>();
    rejects::<PcgeosWriteOptions>(r#"{"font_id": 65536}"#);
}

#[cfg(feature = "pcr")]
#[test]
fn pcr() {
    use fontpixel::formats::pcr::*;
    assert_eq!(defaults::<PcrReadOptions>(), "{}");
    defaults::<PcrWriteOptions>();
    defaults::<PcrCodec>();
    rejects_common::<PcrReadOptions>();
    rejects_common::<PcrCodec>();
}

#[cfg(feature = "pf2")]
#[test]
fn pf2() {
    use fontpixel::formats::pf2::*;
    defaults::<Pf2ReadOptions>();
    defaults::<Pf2WriteOptions>();
    defaults::<Pf2Codec>();
    rejects_common::<Pf2ReadOptions>();
    rejects_write::<Pf2WriteOptions>();
    rejects_output::<Pf2WriteOptions>();
}

#[cfg(feature = "pfr")]
#[test]
fn pfr() {
    use fontpixel::formats::pfr::*;
    defaults::<PfrReadOptions>();
    defaults::<PfrWriteOptions>();
    defaults::<PfrCodec>();
    variants(&[
        (PfrCompression::Raw, "raw"),
        (PfrCompression::Rle4, "rle4"),
        (PfrCompression::Rle8, "rle8"),
    ]);
    variants(&[
        (PfrDialect::Davic, "davic"),
        (PfrDialect::FreeType, "freetype"),
    ]);
    rejects_common::<PfrReadOptions>();
    rejects_write::<PfrWriteOptions>();
    rejects_output::<PfrWriteOptions>();
}

#[cfg(feature = "pilfont")]
#[test]
fn pilfont() {
    use fontpixel::formats::pilfont::*;
    defaults::<PilfontReadOptions>();
    defaults::<PilfontWriteOptions>();
    defaults::<PilfontCodec>();
    rejects_common::<PilfontReadOptions>();
    rejects_path::<PilfontReadOptions>("atlas");
    rejects_common::<PilfontWriteOptions>();
    rejects_output::<PilfontWriteOptions>();
    rejects::<PilfontWriteOptions>(r#"{"max_width": -1}"#);
}

#[cfg(feature = "pk")]
#[test]
fn pk() {
    use fontpixel::formats::pk::*;
    defaults::<PkWriteOptions>();
    defaults::<PkCodec>();
    let explicit: PkWriteOptions =
        serde_json::from_str(r#"{"design_size": {"numerator": 10, "denominator": 1}}"#).unwrap();
    same(&explicit);
    rejects_write::<PkWriteOptions>();
    rejects_output::<PkWriteOptions>();
    rejects::<PkWriteOptions>(r#"{"design_size": 10}"#);
}

#[cfg(feature = "plan9")]
#[test]
fn plan9() {
    use fontpixel::formats::plan9::*;
    defaults::<Plan9WriteOptions>();
    defaults::<Plan9Codec>();
    variants(&[(Container::Font, "font"), (Container::Subfont, "subfont")]);
    variants(&[(Compression::Raw, "raw"), (Compression::Literal, "literal")]);
    rejects_write::<Plan9WriteOptions>();
    rejects_output::<Plan9WriteOptions>();
}

#[cfg(feature = "playdate")]
#[test]
fn playdate() {
    use fontpixel::formats::playdate::*;
    defaults::<PlaydateReadOptions>();
    defaults::<PlaydateWriteOptions>();
    defaults::<PlaydateCodec>();
    variants(&[
        (PlaydateFlavor::FntEmbedded, "fnt-embedded"),
        (PlaydateFlavor::FntPng, "fnt-png"),
        (PlaydateFlavor::Pft, "pft"),
    ]);
    rejects_common::<PlaydateReadOptions>();
    rejects_path::<PlaydateReadOptions>("atlas");
    rejects_write::<PlaydateWriteOptions>();
    rejects_output::<PlaydateWriteOptions>();
    rejects::<PlaydateWriteOptions>(r#"{"tracking": 40000}"#);
}

#[cfg(feature = "plus3dos")]
#[test]
fn plus3dos() {
    use fontpixel::formats::plus3dos::*;
    assert_eq!(defaults::<Plus3dosReadOptions>(), "{}");
    defaults::<Plus3dosWriteOptions>();
    defaults::<Plus3dosCodec>();
    rejects_common::<Plus3dosReadOptions>();
    rejects_common::<Plus3dosCodec>();
}

#[cfg(feature = "polyprint")]
#[test]
fn polyprint() {
    use fontpixel::formats::polyprint::*;
    defaults::<PolyprintReadOptions>();
    defaults::<PolyprintWriteOptions>();
    defaults::<PolyprintCodec>();
    rejects_common::<PolyprintReadOptions>();
    rejects::<PolyprintReadOptions>(r#"{"baseline": "low"}"#);
    rejects_write::<PolyprintWriteOptions>();
    rejects_output::<PolyprintWriteOptions>();
}

#[cfg(feature = "prebuilt")]
#[test]
fn prebuilt() {
    use fontpixel::formats::prebuilt::*;
    defaults::<PrebuiltReadOptions>();
    defaults::<PrebuiltWriteOptions>();
    defaults::<PrebuiltCodec>();
    variants(&[
        (PrebuiltDepth::Mono, "mono"),
        (PrebuiltDepth::Gray2, "gray2"),
        (PrebuiltDepth::Gray4, "gray4"),
        (PrebuiltDepth::Gray8, "gray8"),
    ]);
    rejects_common::<PrebuiltReadOptions>();
    rejects::<PrebuiltReadOptions>(r#"{"byte_order": "middle"}"#);
    rejects_write::<PrebuiltWriteOptions>();
    rejects_output::<PrebuiltWriteOptions>();
    rejects::<PrebuiltWriteOptions>(r#"{"depth": 8}"#);
}

#[cfg(feature = "printshop")]
#[test]
fn printshop() {
    use fontpixel::formats::printshop::*;
    defaults::<PrintshopReadOptions>();
    defaults::<PrintshopWriteOptions>();
    defaults::<PrintshopCodec>();
    rejects_common::<PrintshopReadOptions>();
    rejects_write::<PrintshopWriteOptions>();
    rejects_output::<PrintshopWriteOptions>();
}

#[cfg(feature = "psf")]
#[test]
fn psf() {
    use fontpixel::formats::psf::*;
    defaults::<PsfWriteOptions>();
    defaults::<PsfCodec>();
    variants(&[(Version::One, "1"), (Version::Two, "2")]);
    variants(&[
        (UnicodeTable::Auto, "auto"),
        (UnicodeTable::Include, "include"),
        (UnicodeTable::Omit, "omit"),
    ]);
    rejects_write::<PsfWriteOptions>();
    rejects_output::<PsfWriteOptions>();
    rejects::<PsfWriteOptions>(r#"{"psf1_count": 65536}"#);
}

#[cfg(feature = "psf2txt")]
#[test]
fn psf2txt() {
    use fontpixel::formats::psf2txt::*;
    assert_eq!(defaults::<Psf2txtReadOptions>(), "{}");
    defaults::<Psf2txtWriteOptions>();
    defaults::<Psf2txtCodec>();
    rejects_common::<Psf2txtReadOptions>();
    rejects_common::<Psf2txtCodec>();
}

#[cfg(feature = "pxl")]
#[test]
fn pxl() {
    use fontpixel::formats::pxl::*;
    defaults::<PxlReadOptions>();
    defaults::<PxlWriteOptions>();
    defaults::<PxlCodec>();
    variants(&[(PxlVersion::V1001, "1001"), (PxlVersion::V1002, "1002")]);
    rejects_common::<PxlReadOptions>();
    rejects_path::<PxlReadOptions>("tfm");
    rejects_path::<PxlWriteOptions>("tfm");
    rejects_write::<PxlWriteOptions>();
    rejects_output::<PxlWriteOptions>();
}

#[cfg(feature = "qpf")]
#[test]
fn qpf() {
    use fontpixel::formats::qpf::*;
    defaults::<QpfReadOptions>();
    defaults::<QpfWriteOptions>();
    defaults::<QpfCodec>();
    rejects_common::<QpfReadOptions>();
    rejects_write::<QpfWriteOptions>();
    rejects_output::<QpfWriteOptions>();
}

#[cfg(feature = "qpf2")]
#[test]
fn qpf2() {
    use fontpixel::formats::qpf2::*;
    defaults::<Qpf2ReadOptions>();
    defaults::<Qpf2WriteOptions>();
    defaults::<Qpf2Codec>();
    variants(&[(GlyphFormat::Gray8, "gray8"), (GlyphFormat::Mono, "mono")]);
    rejects_common::<Qpf2ReadOptions>();
    rejects_write::<Qpf2WriteOptions>();
    rejects_output::<Qpf2WriteOptions>();
}

#[cfg(feature = "raw")]
#[test]
fn raw() {
    use fontpixel::formats::raw::*;
    defaults::<RawReadOptions>();
    defaults::<RawWriteOptions>();
    defaults::<RawCodec>();
    variants(&[
        (RawPacking::ByteAlignedRows, "byte-rows"),
        (RawPacking::BitAligned, "bit-aligned"),
    ]);
    variants(&[
        (RawByteOrder::RowMajor, "row-major"),
        (RawByteOrder::ColumnMajor, "column-major"),
    ]);
    variants(&[
        (RawPreset::Vga8, "vga8"),
        (RawPreset::Vga14, "vga14"),
        (RawPreset::Vga16, "vga16"),
        (RawPreset::C64, "c64"),
        (RawPreset::C64Prg, "64c"),
        (RawPreset::Spectrum, "spectrum"),
        (RawPreset::Ch8, "ch8"),
        (RawPreset::Msx, "msx"),
        (RawPreset::HarlekinIII, "harlekin"),
        (RawPreset::Harlekin8, "harlekin8"),
        (RawPreset::Harlekin16, "harlekin16"),
    ]);
    // Every spelling `RawPreset::from_str` (the CLI) accepts also deserializes.
    for (alias, preset) in [
        ("vga-8x8", RawPreset::Vga8),
        ("vga-8x14", RawPreset::Vga14),
        ("vga-8x16", RawPreset::Vga16),
        ("c64-prg", RawPreset::C64Prg),
        ("zx-spectrum", RawPreset::Spectrum),
        ("harlekin-iii", RawPreset::HarlekinIII),
        ("harlekin512", RawPreset::HarlekinIII),
    ] {
        let json = format!("\"{alias}\"");
        assert_eq!(serde_json::from_str::<RawPreset>(&json).unwrap(), preset);
        assert_eq!(alias.parse::<RawPreset>().unwrap(), preset);
    }
    for (preset, name) in [
        (RawPreset::C64Prg, "64c"),
        (RawPreset::HarlekinIII, "harlekin"),
    ] {
        assert_eq!(name.parse::<RawPreset>().unwrap(), preset);
    }
    for preset in [RawPreset::Vga16, RawPreset::C64Prg, RawPreset::HarlekinIII] {
        let layout = preset.layout();
        let json = serde_json::to_string(&layout).unwrap();
        assert_eq!(serde_json::from_str::<RawLayout>(&json).unwrap(), layout);
    }
    // A layout has no documented default: its geometry is required.
    let layout: RawLayout =
        serde_json::from_str(&serde_json::to_string(&RawLayout::new(8, 8)).unwrap()).unwrap();
    assert_eq!(layout, RawLayout::new(8, 8));
    rejects::<RawLayout>("{}");
    rejects::<RawLayout>(r#"{"width": 8, "height": 8}"#);
    let mut value = serde_json::to_value(RawLayout::new(8, 8)).unwrap();
    value["bit_order"] = "middle".into();
    rejects::<RawLayout>(&value.to_string());
    let mut value = serde_json::to_value(RawLayout::new(8, 8)).unwrap();
    value["extra"] = 1.into();
    rejects::<RawLayout>(&value.to_string());
    rejects_common::<RawReadOptions>();
    rejects::<RawReadOptions>(r#"{"layout": {"width": 8}}"#);
    rejects_write::<RawWriteOptions>();
    rejects_output::<RawWriteOptions>();
}

#[cfg(feature = "riscos")]
#[test]
fn riscos() {
    use fontpixel::formats::riscos::*;
    defaults::<RiscosReadOptions>();
    defaults::<RiscosWriteOptions>();
    defaults::<RiscosCodec>();
    variants(&[
        (RiscosVersion::Xy, "xy"),
        (RiscosVersion::V4, "4"),
        (RiscosVersion::V5, "5"),
        (RiscosVersion::V6, "6"),
        (RiscosVersion::V7, "7"),
        (RiscosVersion::V8, "8"),
    ]);
    variants(&[(RiscosDepth::Mono, "mono"), (RiscosDepth::Gray4, "gray4")]);
    variants(&[
        (RiscosCompression::Raw, "raw"),
        (RiscosCompression::Packed, "packed"),
    ]);
    variants(&[
        (RiscosMetricsVersion::V0, "0"),
        (RiscosMetricsVersion::V2, "2"),
    ]);
    rejects_common::<RiscosReadOptions>();
    rejects_path::<RiscosReadOptions>("metrics_path");
    rejects_path::<RiscosWriteOptions>("metrics_path");
    rejects_write::<RiscosWriteOptions>();
    rejects_output::<RiscosWriteOptions>();
}

#[cfg(feature = "rockbox")]
#[test]
fn rockbox() {
    use fontpixel::formats::rockbox::*;
    defaults::<RockboxReadOptions>();
    defaults::<RockboxWriteOptions>();
    defaults::<RockboxCodec>();
    variants(&[(RockboxFlavor::Rb11, "rb11"), (RockboxFlavor::Rb12, "rb12")]);
    variants(&[(RockboxDepth::Mono, "mono"), (RockboxDepth::Gray4, "gray4")]);
    rejects_common::<RockboxReadOptions>();
    rejects_write::<RockboxWriteOptions>();
    rejects_output::<RockboxWriteOptions>();
    rejects::<RockboxCodec>(r#"{"read_options": {"x": 1}}"#);
}

#[cfg(feature = "sbf")]
#[test]
fn sbf() {
    use fontpixel::formats::sbf::*;
    defaults::<SbfReadOptions>();
    defaults::<SbfWriteOptions>();
    defaults::<SbfCodec>();
    rejects_common::<SbfReadOptions>();
    rejects_write::<SbfWriteOptions>();
    rejects_output::<SbfWriteOptions>();
}

#[cfg(feature = "sfd")]
#[test]
fn sfd() {
    use fontpixel::formats::sfd::*;
    defaults::<SfdWriteOptions>();
    defaults::<SfdCodec>();
    rejects_common::<SfdWriteOptions>();
    rejects::<SfdWriteOptions>(r#"{"limits": {"max_glyphs": -1}}"#);
}

#[cfg(feature = "sfont")]
#[test]
fn sfont() {
    use fontpixel::formats::sfont::*;
    defaults::<SfontReadOptions>();
    defaults::<SfontWriteOptions>();
    defaults::<SfontCodec>();
    variants(&[(SfontFlavor::Sfont, "sfont"), (SfontFlavor::Rfont, "rfont")]);
    let explicit: SfontReadOptions =
        serde_json::from_str(r#"{"codepoints": [{"Unicode": 65}, {"Legacy": 66}]}"#).unwrap();
    same(&explicit);
    rejects_common::<SfontReadOptions>();
    rejects::<SfontReadOptions>(r#"{"codepoints": [65]}"#);
    rejects_common::<SfontWriteOptions>();
    rejects_output::<SfontWriteOptions>();
}

#[cfg(feature = "signum")]
#[test]
fn signum() {
    use fontpixel::formats::signum::*;
    defaults::<SignumReadOptions>();
    defaults::<SignumWriteOptions>();
    defaults::<SignumCodec>();
    variants(&[
        (SignumVersion::E24, "e24"),
        (SignumVersion::P9, "p9"),
        (SignumVersion::P24, "p24"),
        (SignumVersion::L30, "l30"),
    ]);
    rejects_common::<SignumReadOptions>();
    rejects_write::<SignumWriteOptions>();
    rejects_output::<SignumWriteOptions>();
}

#[cfg(feature = "snf")]
#[test]
fn snf() {
    use fontpixel::formats::snf::*;
    defaults::<SnfReadOptions>();
    defaults::<SnfWriteOptions>();
    defaults::<SnfCodec>();
    rejects_common::<SnfReadOptions>();
    rejects::<SnfReadOptions>(r#"{"byte_order": "Big"}"#);
    rejects::<SnfReadOptions>(r#"{"bitmap": {"row_alignment": 256}}"#);
    rejects_write::<SnfWriteOptions>();
    rejects_output::<SnfWriteOptions>();
}

#[cfg(feature = "ssfn")]
#[test]
fn ssfn() {
    use fontpixel::formats::ssfn::*;
    defaults::<SsfnReadOptions>();
    defaults::<SsfnWriteOptions>();
    defaults::<SsfnCodec>();
    variants(&[
        (SsfnEncoding::Binary, "binary"),
        (SsfnEncoding::Ascii, "ascii"),
    ]);
    rejects_common::<SsfnReadOptions>();
    rejects_write::<SsfnWriteOptions>();
    rejects_output::<SsfnWriteOptions>();
}

#[cfg(feature = "svg")]
#[test]
fn svg() {
    use fontpixel::formats::svg::*;
    defaults::<Limits>();
    defaults::<ReadOptions>();
    defaults::<WriteOptions>();
    defaults::<ImageOptions>();
    defaults::<SvgCodec>();
    rejects_common::<Limits>();
    rejects::<Limits>(r#"{"max_nodes": 4294967296}"#);
    rejects::<Limits>(r#"{"geometry": {"no_such_limit": 1}}"#);
    rejects_common::<ReadOptions>();
    rejects::<ReadOptions>(r#"{"ppem": 65536}"#);
    rejects_common::<WriteOptions>();
    rejects::<WriteOptions>(r#"{"gray_threshold": 256}"#);
    rejects::<WriteOptions>(r#"{"shape": "circle"}"#);
    rejects::<WriteOptions>(r#"{"dot_size": "big"}"#);
    rejects_output::<WriteOptions>();
    rejects_common::<ImageOptions>();
    rejects::<ImageOptions>(r#"{"image": {"scale": -1}}"#);
}

#[cfg(feature = "symbos")]
#[test]
fn symbos() {
    use fontpixel::formats::symbos::*;
    defaults::<SymbosReadOptions>();
    defaults::<SymbosWriteOptions>();
    defaults::<SymbosCodec>();
    variants(&[
        (SymbosVersion::Medium, "medium"),
        (SymbosVersion::Small, "small"),
        (SymbosVersion::Big, "big"),
    ]);
    for (value, json) in [
        (
            SymbosExtendedInfo::WritingSystem(3),
            r#"{"writing_system":3}"#,
        ),
        (
            SymbosExtendedInfo::Layout { wysiwyg: true },
            r#"{"layout":{"wysiwyg":true}}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<SymbosExtendedInfo>(json).unwrap(),
            value
        );
    }
    rejects::<SymbosExtendedInfo>(r#"{"writing_system":256}"#);
    rejects::<SymbosExtendedInfo>(r#"{"layout":{"wysiwyg":true,"extra":1}}"#);
    rejects::<SymbosExtendedInfo>(r#"{"layout":{}}"#);
    rejects_common::<SymbosReadOptions>();
    rejects_write::<SymbosWriteOptions>();
    rejects_output::<SymbosWriteOptions>();
}

#[cfg(feature = "tasprint")]
#[test]
fn tasprint() {
    use fontpixel::formats::tasprint::*;
    defaults::<TasprintReadOptions>();
    defaults::<TasprintWriteOptions>();
    defaults::<TasprintCodec>();
    variants(&[
        (TasprintVersion::Spectrum48k, "48k"),
        (TasprintVersion::Plus3, "plus3"),
        (TasprintVersion::Cpc, "cpc"),
        (TasprintVersion::Pcw, "pcw"),
    ]);
    rejects_common::<TasprintReadOptions>();
    rejects_write::<TasprintWriteOptions>();
    rejects_output::<TasprintWriteOptions>();
}

#[cfg(feature = "u8m")]
#[test]
fn u8m() {
    use fontpixel::formats::u8m::*;
    defaults::<U8mReadOptions>();
    defaults::<U8mWriteOptions>();
    defaults::<U8mCodec>();
    rejects_common::<U8mReadOptions>();
    rejects_write::<U8mWriteOptions>();
    rejects_output::<U8mWriteOptions>();
    rejects::<U8mWriteOptions>(r#"{"load_address": 65536}"#);
}

#[cfg(feature = "vector-raster")]
#[test]
fn vector_raster() {
    use fontpixel::formats::vector_raster::*;
    defaults::<Limits>();
    defaults::<Options>();
    defaults::<WriteOptions>();
    let explicit: Options = serde_json::from_str(r#"{"pixel_offset": [0.25, 0.5]}"#).unwrap();
    assert_eq!(explicit.pixel_offset, (0.25, 0.5));
    rejects_common::<Limits>();
    rejects_common::<Options>();
    rejects::<Options>(r#"{"pixel_offset": [0.25]}"#);
    rejects::<Options>(r#"{"ppem": -1}"#);
    rejects::<Options>(r#"{"gray_threshold": 256}"#);
    rejects_common::<WriteOptions>();
    rejects::<WriteOptions>(r#"{"outline": {"shape": "circle"}}"#);
}

#[cfg(feature = "vfont")]
#[test]
fn vfont() {
    use fontpixel::formats::vfont::*;
    defaults::<VfontReadOptions>();
    defaults::<VfontWriteOptions>();
    defaults::<VfontCodec>();
    rejects_common::<VfontReadOptions>();
    rejects::<VfontReadOptions>(r#"{"byte_order": "middle"}"#);
    rejects_write::<VfontWriteOptions>();
    rejects_output::<VfontWriteOptions>();
}

#[cfg(feature = "vlw")]
#[test]
fn vlw() {
    use fontpixel::formats::vlw::*;
    defaults::<VlwReadOptions>();
    defaults::<VlwWriteOptions>();
    defaults::<VlwCodec>();
    variants(&[
        (VlwVersion::V8, "8"),
        (VlwVersion::V9, "9"),
        (VlwVersion::V10, "10"),
        (VlwVersion::V11, "11"),
    ]);
    rejects_common::<VlwReadOptions>();
    rejects_common::<VlwWriteOptions>();
    rejects_output::<VlwWriteOptions>();
    rejects::<VlwWriteOptions>(r#"{"smooth": 1}"#);
}

#[cfg(feature = "vtfont")]
#[test]
fn vtfont() {
    use fontpixel::formats::vtfont::*;
    defaults::<VtfontReadOptions>();
    defaults::<VtfontWriteOptions>();
    defaults::<VtfontCodec>();
    rejects_common::<VtfontReadOptions>();
    rejects_write::<VtfontWriteOptions>();
    rejects_output::<VtfontWriteOptions>();
}

#[cfg(feature = "writeon")]
#[test]
fn writeon() {
    use fontpixel::formats::writeon::*;
    assert_eq!(defaults::<WriteonReadOptions>(), "{}");
    defaults::<WriteonWriteOptions>();
    defaults::<WriteonCodec>();
    rejects_common::<WriteonReadOptions>();
    rejects_common::<WriteonCodec>();
}

#[cfg(feature = "wsfont")]
#[test]
fn wsfont() {
    use fontpixel::formats::wsfont::*;
    defaults::<WsfontReadOptions>();
    defaults::<WsfontWriteOptions>();
    defaults::<WsfontCodec>();
    variants(&[
        (WsfontOrder::Known, "known"),
        (WsfontOrder::LeftToRight, "left"),
        (WsfontOrder::RightToLeft, "right"),
    ]);
    variants(&[(WsfontDepth::Mono, "mono"), (WsfontDepth::Alpha, "alpha")]);
    rejects_common::<WsfontReadOptions>();
    rejects_write::<WsfontWriteOptions>();
    rejects_output::<WsfontWriteOptions>();
    rejects::<WsfontWriteOptions>(r#"{"known_orders": {"known_bit_order": "up"}}"#);
}

#[cfg(feature = "wyse")]
#[test]
fn wyse() {
    use fontpixel::formats::wyse::*;
    defaults::<WyseReadOptions>();
    defaults::<WyseWriteOptions>();
    defaults::<WyseCodec>();
    rejects_common::<WyseCodec>();
}

#[cfg(feature = "xbin")]
#[test]
fn xbin() {
    use fontpixel::formats::xbin::*;
    defaults::<XbinReadOptions>();
    defaults::<XbinWriteOptions>();
    defaults::<XbinCodec>();
    variants(&[
        (CharacterCount::Auto, "auto"),
        (CharacterCount::Chars256, "256"),
        (CharacterCount::Chars512, "512"),
    ]);
    variants(&[
        (ScreenMode::Preserve, "preserve"),
        (ScreenMode::FontOnly, "font-only"),
    ]);
    rejects_write::<XbinWriteOptions>();
    rejects::<XbinWriteOptions>(r#"{"cell": {"width": 8}}"#);
}

#[cfg(feature = "xerox")]
#[test]
fn xerox() {
    use fontpixel::formats::xerox::*;
    defaults::<XeroxReadOptions>();
    defaults::<XeroxWriteOptions>();
    defaults::<XeroxCodec>();
    variants(&[
        (XeroxVariant::Alto, "alto"),
        (XeroxVariant::Bitblt, "bitblt"),
        (XeroxVariant::Prepress, "prepress"),
    ]);
    variants(&[
        (XeroxBitbltLayout::Plain, "plain"),
        (XeroxBitbltLayout::Kerned, "kerned"),
        (XeroxBitbltLayout::PlainIndex, "plain-index"),
        (XeroxBitbltLayout::KernedIndex, "kerned-index"),
    ]);
    rejects_common::<XeroxReadOptions>();
    rejects_write::<XeroxWriteOptions>();
    rejects_output::<XeroxWriteOptions>();
}

#[cfg(feature = "yaff")]
#[test]
fn yaff() {
    use fontpixel::formats::yaff::*;
    defaults::<YaffWriteOptions>();
    defaults::<YaffCodec>();
    rejects_common::<YaffWriteOptions>();
    rejects_output::<YaffWriteOptions>();
    rejects::<YaffWriteOptions>(r#"{"limits": {"max_glyphs": -1}}"#);
}

#[cfg(feature = "zapfont")]
#[test]
fn zapfont() {
    use fontpixel::formats::zapfont::*;
    defaults::<ZapfontReadOptions>();
    defaults::<ZapfontWriteOptions>();
    defaults::<ZapfontCodec>();
    variants(&[(ZapVariant::Old, "old"), (ZapVariant::Redraw, "redraw")]);
    rejects_common::<ZapfontReadOptions>();
    rejects_write::<ZapfontWriteOptions>();
}
