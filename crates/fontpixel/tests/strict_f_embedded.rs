//! Unit F: explicit bitmap-TTF controls (outline tables, empty `.notdef`,
//! line metrics, underline, hmtx, glyph order, index span, cmap records).
#![cfg(feature = "otb")]

use fontpixel::formats::embedded::{
    self, EmbeddedCmapUnicodeFull as CmapFull, EmbeddedGlyphOrder as GlyphOrder,
    EmbeddedHmtx as Hmtx, EmbeddedIndexLayout as Layout, EmbeddedLineExtremes as Extremes,
    EmbeddedMetadataOptions as Metadata, EmbeddedNotdef as Notdef,
    EmbeddedOutlineTables as Outlines, EmbeddedReadOptions,
    EmbeddedVerticalLineMetrics as Vertical, EmbeddedWriteOptions,
};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::report::Impact;
use fontpixel::sfnt::{
    Sfnt,
    bitmap::{self as sbit, Kind},
    cmap,
    metrics::{self, Axis},
    post::PostTable,
};
use fontpixel::{Bitmap, Font, FontSet, Glyph, Report, ResourcePath, WriteOptions};

fn glyph(cp: u32, rows: &str, advance: i64) -> Glyph {
    let mut g = Glyph::new(rows.parse::<Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Unicode(cp));
    g.name = Some(format!("u{cp:04X}"));
    g.metrics.advance = Some(Vector::new(advance, 0));
    g.metrics.bounds.x = 1;
    g
}
/// Two strikes with gaps: the 8 px strike lacks U+0042, the 16 px strike
/// lacks U+0043; glyphs appear out of codepoint order and the source has a
/// `.notdef` and underline metrics.
fn sample(notdef: bool) -> FontSet {
    let mut fonts = Vec::new();
    for (ppem, codes) in [(8u32, [0x44u32, 0x41, 0x43]), (16, [0x41, 0x42, 0x44])] {
        let mut f = Font::default();
        f.names.family = Some("Sample".into());
        f.names.style = Some("Regular".into());
        f.metrics.pixel_size = Some(ppem);
        f.metrics.ascent = Some(i64::from(ppem) * 3 / 4);
        f.metrics.descent = Some(i64::from(ppem) / 4);
        f.metrics.underline_position = Some(-1);
        f.metrics.underline_thickness = Some(1);
        if notdef {
            let mut g = Glyph::new("##\n##".parse::<Bitmap>().unwrap());
            g.name = Some(".notdef".into());
            g.metrics.advance = Some(Vector::new(3, 0));
            f.add_glyph(g).unwrap();
        }
        for cp in codes {
            f.add_glyph(glyph(cp, "#.\n##", i64::from(ppem) / 2))
                .unwrap();
        }
        fonts.push(f);
    }
    FontSet {
        fonts,
        ..Default::default()
    }
}
fn write(set: &FontSet, metadata: Metadata) -> (Vec<u8>, Report) {
    embedded::write(
        set,
        Kind::OpenType,
        &EmbeddedWriteOptions {
            metadata,
            ..Default::default()
        },
    )
    .unwrap()
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}
fn strikes(bytes: &[u8]) -> Vec<sbit::Strike> {
    let sfnt = Sfnt::parse(bytes, &Default::default()).unwrap();
    let n = u16_at(sfnt.table(*b"maxp").unwrap(), 4);
    sbit::decode(
        sfnt.table(*b"EBLC").unwrap(),
        sfnt.table(*b"EBDT").unwrap(),
        Kind::OpenType,
        n,
        &Default::default(),
    )
    .unwrap()
}
/// One decoded encoded glyph: code points, pixels and horizontal metrics.
type Decoded = (Vec<Codepoint>, String, i64, Option<Vector>);
/// Decoded encoded glyphs of every strike, sorted by code points.
fn glyphs(bytes: &[u8]) -> Vec<Vec<Decoded>> {
    let (set, _) = embedded::read(bytes, Kind::OpenType, &EmbeddedReadOptions::default()).unwrap();
    set.fonts
        .iter()
        .map(|f| {
            let mut glyphs: Vec<_> = f
                .glyphs()
                .filter(|(_, g)| !g.codepoints.is_empty())
                .map(|(_, g)| {
                    (
                        g.codepoints.clone(),
                        format!("{:?}", g.bitmap),
                        g.metrics.bounds.x,
                        g.metrics.advance,
                    )
                })
                .collect();
            glyphs.sort_by(|a, b| a.0.cmp(&b.0));
            glyphs
        })
        .collect()
}

#[test]
fn default_controls_are_byte_and_report_identical() {
    let set = sample(true);
    let baseline = embedded::write(&set, Kind::OpenType, &Default::default()).unwrap();
    let explicit = Metadata {
        outline_tables: Outlines::None,
        notdef: Notdef::Preserve,
        line_extremes: Extremes::Computed,
        vertical_line_metrics: Vertical::Zero,
        hmtx: Hmtx::Full,
        glyph_order: GlyphOrder::FirstSeen,
        index_layout: Layout::Canonical,
        cmap_unicode_full: CmapFull::Always,
        ..Default::default()
    };
    assert_eq!(write(&set, explicit), baseline);
    let sfnt = Sfnt::parse(&baseline.0, &Default::default()).unwrap();
    assert!(sfnt.table(*b"glyf").is_none() && sfnt.table(*b"loca").is_none());
    assert_eq!(sfnt.table(*b"maxp").unwrap().len(), 6);
}

#[test]
fn empty_glyf_profile_writes_outline_tables_and_zero_outline_extents() {
    let set = sample(false);
    let baseline = write(&set, Metadata::default()).0;
    for zones in [None, Some(2)] {
        let (bytes, report) = write(
            &set,
            Metadata {
                outline_tables: Outlines::EmptyGlyf,
                max_zones: zones,
                ..Default::default()
            },
        );
        let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
        sfnt.validate_checksums().unwrap();
        let maxp = sfnt.table(*b"maxp").unwrap();
        let n = u16_at(maxp, 4);
        assert_eq!(maxp.len(), 32);
        assert_eq!(u16_at(maxp, 0), 1);
        assert_eq!(u16_at(maxp, 14), zones.unwrap_or(1));
        assert!(maxp[6..14].iter().chain(&maxp[16..]).all(|&b| b == 0));
        assert_eq!(sfnt.table(*b"glyf").unwrap(), [0]);
        assert_eq!(
            sfnt.table(*b"loca").unwrap(),
            vec![0; 2 * (usize::from(n) + 1)]
        );
        let head = sfnt.table(*b"head").unwrap();
        assert!(head[36..44].iter().all(|&b| b == 0), "zero outline bounds");
        assert_eq!(u16_at(head, 50), 0, "short loca");
        let (header, hm) = metrics::decode(
            Axis::Horizontal,
            sfnt.table(*b"hhea").unwrap(),
            sfnt.table(*b"hmtx").unwrap(),
            n,
            &Default::default(),
        )
        .unwrap();
        assert!(hm.iter().all(|m| m.bearing == 0));
        assert_eq!(
            (
                header.min_side_bearing,
                header.min_other_side_bearing,
                header.max_extent
            ),
            (0, 0, 0)
        );
        // Bitmaps and their exact metrics are unchanged.
        assert_eq!(glyphs(&bytes), glyphs(&baseline));
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.code == "embedded.metadata_configuration")
        );
    }
}

#[test]
fn empty_notdef_drops_glyph_zero_bitmaps_and_reports_replaced_source() {
    for source_notdef in [false, true] {
        let set = sample(source_notdef);
        let (bytes, report) = write(
            &set,
            Metadata {
                notdef: Notdef::Empty { advance_width: 500 },
                ..Default::default()
            },
        );
        for strike in strikes(&bytes) {
            assert!(!strike.glyphs.contains_key(&0));
        }
        let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
        let hmtx = sfnt.table(*b"hmtx").unwrap();
        assert_eq!((u16_at(hmtx, 0), u16_at(hmtx, 2)), (500, 0));
        let replaced = report
            .entries
            .iter()
            .any(|e| e.code == "embedded.notdef_replaced" && e.impact == Impact::Loss);
        assert_eq!(replaced, source_notdef);
    }
}

#[test]
fn line_metric_controls_set_caret_extremes_and_mirror_vertical() {
    let set = sample(false);
    let (bytes, _) = write(
        &set,
        Metadata {
            caret_slope: Some([0, 1]),
            line_extremes: Extremes::Zero,
            vertical_line_metrics: Vertical::MirrorHorizontal,
            ..Default::default()
        },
    );
    let default = strikes(&write(&set, Metadata::default()).0);
    for (strike, native) in strikes(&bytes).iter().zip(&default) {
        let h = strike.horizontal.0;
        assert_eq!(h[..3], native.horizontal.0[..3]);
        assert_eq!((h[3], h[4]), (0, 1));
        assert_eq!(h[6..10], [0; 4]);
        assert_eq!(strike.vertical, strike.horizontal);
        assert_eq!(native.vertical.0, [0; 12]);
        assert_eq!((native.horizontal.0[3], native.horizontal.0[4]), (1, 0));
    }
}

#[test]
fn underline_override_is_loss_only_when_replacing_source_values() {
    let mut set = sample(false);
    let (bytes, report) = write(
        &set,
        Metadata {
            underline: Some([0, 0]),
            ..Default::default()
        },
    );
    let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
    let post = PostTable::decode(sfnt.table(*b"post").unwrap(), 5, &Default::default()).unwrap();
    assert_eq!(
        (
            post.header.underline_position,
            post.header.underline_thickness
        ),
        (0, 0)
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "embedded.underline" && e.impact == Impact::Loss)
    );
    for f in &mut set.fonts {
        f.metrics.underline_position = None;
        f.metrics.underline_thickness = None;
    }
    let (_, report) = write(
        &set,
        Metadata {
            underline: Some([-100, 50]),
            ..Default::default()
        },
    );
    assert!(
        !report
            .entries
            .iter()
            .any(|e| e.code == "embedded.underline")
    );
    // Strict registry writes reject the replacement.
    let mut registry = fontpixel::registry();
    registry.set_embedded_write_options(EmbeddedWriteOptions {
        metadata: Metadata {
            underline: Some([0, 0]),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut options = WriteOptions::new(ResourcePath::new("out.otb").unwrap());
    options.strict = true;
    assert!(matches!(
        registry.write(&sample(false), "otb", &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}

#[test]
fn hmtx_compaction_codepoint_order_spanning_index_and_cmap_records() {
    let set = sample(false);
    let baseline = write(&set, Metadata::default()).0;
    let (bytes, _) = write(
        &set,
        Metadata {
            hmtx: Hmtx::CompactTrailing,
            glyph_order: GlyphOrder::Codepoint,
            index_layout: Layout::Spanning,
            cmap_unicode_full: CmapFull::WhenNeeded,
            ..Default::default()
        },
    );
    let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
    let n = u16_at(sfnt.table(*b"maxp").unwrap(), 4);
    let mapping = cmap::decode(sfnt.table(*b"cmap").unwrap(), n, &Default::default()).unwrap();
    // Glyph IDs follow the code points.
    assert_eq!(
        mapping.into_iter().collect::<Vec<_>>(),
        [(0x41, 1), (0x42, 2), (0x43, 3), (0x44, 4)]
    );
    // Format 4 only: (0,3) and (3,1).
    let table = sfnt.table(*b"cmap").unwrap();
    assert_eq!(u16_at(table, 2), 2);
    // The 16 px strike's advances (8) end the table: one long metric covers
    // the trailing glyphs that share it.
    let hhea = sfnt.table(*b"hhea").unwrap();
    assert!(u16_at(hhea, 34) < n, "compacted hmtx");
    let (_, decoded) = metrics::decode(
        Axis::Horizontal,
        hhea,
        sfnt.table(*b"hmtx").unwrap(),
        n,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(decoded.len(), usize::from(n));
    // One index subtable per strike, with a zero-length entry for the gap.
    let eblc = sfnt.table(*b"EBLC").unwrap();
    for i in 0..2 {
        assert_eq!(u16_at(eblc, 8 + i * 48 + 10), 1, "one subtable");
    }
    assert_eq!(glyphs(&bytes), glyphs(&baseline));
}

#[test]
fn invalid_controls_are_rejected_before_writing() {
    let set = sample(false);
    for (metadata, message) in [
        (
            Metadata {
                max_zones: Some(2),
                ..Default::default()
            },
            "maxZones requires the empty glyf outline tables",
        ),
        (
            Metadata {
                outline_tables: Outlines::EmptyGlyf,
                max_zones: Some(3),
                ..Default::default()
            },
            "maxZones must be 1 or 2",
        ),
        (
            Metadata {
                caret_slope: Some([0, 0]),
                ..Default::default()
            },
            "caret slope numerator and denominator cannot both be 0",
        ),
    ] {
        let error = embedded::write(
            &set,
            Kind::OpenType,
            &EmbeddedWriteOptions {
                metadata,
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(message), "{error}");
    }
    let error = embedded::write(
        &set,
        Kind::Color,
        &EmbeddedWriteOptions {
            metadata: Metadata {
                notdef: Notdef::Empty { advance_width: 1 },
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(error.is_err());
}

fn minimum_limit(set: &FontSet, metadata: &Metadata) -> usize {
    let fits = |limit: usize| {
        let mut options = EmbeddedWriteOptions {
            metadata: metadata.clone(),
            ..Default::default()
        };
        options.limits.sfnt.resources.max_total_bytes = limit;
        embedded::write(set, Kind::OpenType, &options).is_ok()
    };
    let (mut low, mut high) = (0, 64 << 20);
    assert!(fits(high));
    while low < high {
        let mid = low + (high - low) / 2;
        if fits(mid) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    assert!(!fits(low - 1));
    low
}

#[test]
fn outline_tables_and_reordering_are_reserved_on_the_writer_budget() {
    let set = sample(false);
    let base = minimum_limit(&set, &Metadata::default());
    // Bound: every source glyph plus .notdef (7), one glyf byte, a short loca
    // and a 32-byte maxp; reordering adds a key, a new ID and a name slot.
    let glyphs = 7;
    let outlines = Metadata {
        outline_tables: Outlines::EmptyGlyf,
        ..Default::default()
    };
    assert_eq!(
        minimum_limit(&set, &outlines),
        base + 2 * (glyphs + 1) + 1 + 32
    );
    let order = Metadata {
        glyph_order: GlyphOrder::Codepoint,
        ..Default::default()
    };
    let per_glyph = size_of::<(u32, u16)>() + size_of::<u16>() + size_of::<String>();
    assert_eq!(minimum_limit(&set, &order), base + glyphs * per_glyph);
}
