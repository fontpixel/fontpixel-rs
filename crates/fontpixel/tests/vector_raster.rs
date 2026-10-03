//! Native raster, grid, bitmap-priority and registry regression tests.
#![cfg(feature = "vector-raster")]
use fontpixel::{
    Bitmap, Font, FontSet, Glyph,
    formats::vector_raster::{self as vr, Limits, Options, WriteOptions},
    model::{Codepoint, Vector},
    outline::{self, path::Edge},
    sfnt::{self, Sfnt, SfntBuilder},
    variants::OutlineFlavor,
};
fn font() -> Font {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(8);
    f.metrics.ascent = Some(7);
    f.metrics.descent = Some(1);
    f.names.family = Some("Grid Test".into());
    f.names.style = Some("Regular".into());
    let mut nd = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::default());
    nd.name = Some(".notdef".into());
    nd.metrics.advance = Some(Vector::new(3, 0));
    f.add_glyph(nd).unwrap();
    for (i, rows) in [
        ["101", "111", "101"],
        ["111", "101", "111"],
        ["100", "110", "111"],
    ]
    .iter()
    .enumerate()
    {
        let mut b: Bitmap = Bitmap::new(3, 3).unwrap();
        for (y, row) in rows.iter().enumerate() {
            for (x, p) in row.bytes().enumerate() {
                b.set_pixel(x as u32, y as u32, u8::from(p == b'1'))
                    .unwrap();
            }
        }
        let mut g = Glyph::new(b);
        g.name = Some(format!("g{i}"));
        g.metrics.bounds.x = if i == 2 { -1 } else { 1 };
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector::new(5, 0));
        if i != 2 {
            g.codepoints.push(Codepoint::Unicode(65 + i as u32));
        }
        if i == 0 {
            g.codepoints.push(Codepoint::Unicode(0x10000));
        }
        f.add_glyph(g).unwrap();
    }
    f
}
fn encoded(flavor: OutlineFlavor, embedded: bool) -> Vec<u8> {
    let mut o = WriteOptions::default();
    o.outline.flavor = flavor;
    o.outline.embed_bitmaps = embedded;
    vr::write(
        &FontSet::from(font()),
        if flavor == OutlineFlavor::Cff {
            "otf"
        } else {
            "ttf"
        },
        &o,
    )
    .unwrap()
    .0
}
fn equal(a: &Font, b: &Font) {
    assert_eq!(a.len(), b.len());
    for ((_, a), (_, b)) in a.glyphs().zip(b.glyphs()) {
        assert_eq!(a.codepoints, b.codepoints);
        assert_eq!(a.bitmap, b.bitmap);
        assert_eq!(a.metrics.bounds, b.metrics.bounds);
        assert_eq!(a.metrics.advance, b.metrics.advance);
    }
}
fn patch(data: &[u8], tag: [u8; 4], value: Vec<u8>) -> Vec<u8> {
    let l = sfnt::Limits::default();
    let f = Sfnt::parse(data, &l).unwrap();
    let mut b = SfntBuilder::new(f.flavor());
    for t in f.tables() {
        if t.tag != tag {
            b.insert(t.tag, t.data.to_vec()).unwrap();
        }
    }
    b.insert(tag, value).unwrap();
    b.build(&l).unwrap()
}
#[test]
fn grid_and_roundtrip_quadratic() {
    let d = encoded(OutlineFlavor::TrueType, false);
    assert_eq!(
        vr::detect_native_ppem(&d, &Limits::default()).unwrap(),
        Some(8)
    );
    equal(
        &font(),
        &vr::read(&d, &Options::default()).unwrap().0.fonts[0],
    );
}
#[test]
fn grid_and_roundtrip_cubic() {
    let d = encoded(OutlineFlavor::Cff, false);
    assert_eq!(
        vr::detect_native_ppem(&d, &Limits::default()).unwrap(),
        Some(8)
    );
    equal(
        &font(),
        &vr::read(&d, &Options::default()).unwrap().0.fonts[0],
    );
}
#[test]
fn preserves_aliases_unencoded_notdef_names() {
    for flavor in [OutlineFlavor::TrueType, OutlineFlavor::Cff] {
        let d = encoded(flavor, false);
        let (set, _) = vr::read(&d, &Options::default()).unwrap();
        assert_eq!(set.fonts[0].len(), 4);
        assert_eq!(
            set.fonts[0].glyphs().nth(3).unwrap().1.name.as_deref(),
            Some("g2")
        );
        assert_eq!(
            set.fonts[0]
                .glyph_for_codepoint(Codepoint::Unicode(0x10000))
                .unwrap()
                .name
                .as_deref(),
            Some("g0")
        );
    }
}
#[test]
fn nongrid_requires_explicit_ppem() {
    let d = encoded(OutlineFlavor::TrueType, false);
    let f = Sfnt::parse(&d, &Default::default()).unwrap();
    let mut t = sfnt::glyf::GlyfTable::from_sfnt(&f, &Default::default()).unwrap();
    if let sfnt::glyf::Glyph::Simple(g) = &mut t.glyphs[3] {
        g.contours[0][0].0 += 1.;
    }
    let e = t
        .encode(sfnt::glyf::LocaFormat::Long, &Default::default())
        .unwrap();
    let d = patch(&patch(&d, *b"glyf", e.glyf), *b"loca", e.loca);
    let mut head = Sfnt::parse(&d, &Default::default())
        .unwrap()
        .table(*b"head")
        .unwrap()
        .to_vec();
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    let d = patch(&d, *b"head", head);
    assert_eq!(
        vr::detect_native_ppem(&d, &Limits::default()).unwrap(),
        None
    );
    assert!(vr::read(&d, &Options::default()).is_err());
    assert!(
        vr::read(
            &d,
            &Options {
                ppem: Some(8),
                ..Default::default()
            }
        )
        .is_ok()
    );
}
#[test]
fn embedded_priority_skips_bad_outlines() {
    let d = encoded(OutlineFlavor::TrueType, true);
    let d = patch(&d, *b"glyf", vec![255; 3]);
    let (set, _) = vr::read(&d, &Options::default()).unwrap();
    equal(&font(), &set.fonts[0]);
    assert!(
        vr::read(
            &d,
            &Options {
                prefer_embedded: false,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn wrong_embedded_size_falls_back() {
    let d = encoded(OutlineFlavor::TrueType, true);
    let (set, _) = vr::read(
        &d,
        &Options {
            ppem: Some(16),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(set.fonts[0].metrics.ppem_y, Some(16));
    assert_eq!(
        set.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .width(),
        6
    );
}
#[test]
fn sparse_embedded_fills_missing_outline() {
    let d = encoded(OutlineFlavor::TrueType, true);
    let f = Sfnt::parse(&d, &Default::default()).unwrap();
    let mut s = sfnt::bitmap::decode(
        f.table(*b"EBLC").unwrap(),
        f.table(*b"EBDT").unwrap(),
        sfnt::bitmap::Kind::OpenType,
        4,
        &Default::default(),
    )
    .unwrap();
    s[0].glyphs.remove(&2);
    let e = sfnt::bitmap::encode(&s, sfnt::bitmap::Kind::OpenType, 4, &Default::default()).unwrap();
    let d = patch(&patch(&d, *b"EBDT", e.data), *b"EBLC", e.location);
    let (set, report) = vr::read(&d, &Options::default()).unwrap();
    equal(&font(), &set.fonts[0]);
    assert!(
        !report
            .entries
            .iter()
            .any(|e| e.code.ends_with("missing_image"))
    );
}
#[test]
fn dot_requires_override_then_recovers_pixels() {
    for flavor in [OutlineFlavor::TrueType, OutlineFlavor::Cff] {
        let mut o = WriteOptions::default();
        o.outline.flavor = flavor;
        o.outline.shape = outline::Shape::Dot;
        o.outline.dot_size = 0.9;
        let d = vr::write(
            &FontSet::from(font()),
            if flavor == OutlineFlavor::Cff {
                "otf"
            } else {
                "ttf"
            },
            &o,
        )
        .unwrap()
        .0;
        assert_eq!(
            vr::detect_native_ppem(&d, &Default::default()).unwrap(),
            None
        );
        let (set, _) = vr::read(
            &d,
            &Options {
                ppem: Some(8),
                ..Default::default()
            },
        )
        .unwrap();
        for cp in [65, 66] {
            assert_eq!(
                font()
                    .glyph_for_codepoint(Codepoint::Unicode(cp))
                    .unwrap()
                    .bitmap,
                set.fonts[0]
                    .glyph_for_codepoint(Codepoint::Unicode(cp))
                    .unwrap()
                    .bitmap
            );
        }
    }
}
#[test]
fn bounded_pixels_points_and_model() {
    let d = encoded(OutlineFlavor::TrueType, false);
    for setting in 0..4 {
        let mut o = Options {
            ppem: Some(8),
            ..Default::default()
        };
        match setting {
            0 => o.limits.sfnt.resources.max_glyphs = 2,
            1 => o.limits.sfnt.resources.max_pixels_per_glyph = 1,
            2 => o.limits.max_total_edges = 1,
            _ => o.limits.sfnt.resources.max_total_bytes = 2,
        };
        assert!(vr::read(&d, &o).is_err());
    }
}
#[test]
fn explicit_grid_override() {
    let d = encoded(OutlineFlavor::TrueType, false);
    let f = Sfnt::parse(&d, &Default::default()).unwrap();
    let mut head = f.table(*b"head").unwrap().to_vec();
    head[18..20].copy_from_slice(&850u16.to_be_bytes());
    let d = patch(&d, *b"head", head);
    assert_eq!(
        vr::detect_native_ppem(&d, &Default::default()).unwrap(),
        None
    );
    equal(
        &font(),
        &vr::read(
            &d,
            &Options {
                ppem: Some(8),
                grid_units: Some(100),
                ..Default::default()
            },
        )
        .unwrap()
        .0
        .fonts[0],
    );
}
#[test]
fn explicit_offset_translation() {
    let d = encoded(OutlineFlavor::TrueType, false);
    let (set, _) = vr::read(
        &d,
        &Options {
            ppem: Some(8),
            pixel_offset: (2., -3.),
            ..Default::default()
        },
    )
    .unwrap();
    let g = set.fonts[0]
        .glyph_for_codepoint(Codepoint::Unicode(65))
        .unwrap();
    assert_eq!((g.metrics.bounds.x, g.metrics.bounds.y), (3, -4));
    assert_eq!(
        g.bitmap,
        font()
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
    );
}
#[test]
fn invalid_options_and_truncations() {
    let d = encoded(OutlineFlavor::TrueType, false);
    let end = Sfnt::parse(&d, &Default::default())
        .unwrap()
        .tables()
        .iter()
        .map(|t| t.offset as usize + t.data.len())
        .max()
        .unwrap();
    for n in 0..end {
        assert!(vr::read(&d[..n], &Options::default()).is_err());
    }
    for o in [
        Options {
            ppem: Some(0),
            ..Default::default()
        },
        Options {
            grid_units: Some(100),
            ..Default::default()
        },
        Options {
            pixel_offset: (f64::NAN, 0.),
            ..Default::default()
        },
        Options {
            face_index: Some(1),
            ..Default::default()
        },
    ] {
        assert!(vr::read(&d, &o).is_err());
    }
}
#[test]
fn rejects_variations_instead_of_ignoring() {
    let d = patch(
        &encoded(OutlineFlavor::TrueType, false),
        *b"fvar",
        vec![0; 16],
    );
    assert!(
        vr::read(
            &d,
            &Options {
                ppem: Some(8),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn exact_cubic_nonzero_holes_and_monotonic_extrema() {
    let outer = [
        Edge::Line((0., 0.), (0., 3.)),
        Edge::Line((0., 3.), (3., 3.)),
        Edge::Line((3., 3.), (3., 0.)),
        Edge::Line((3., 0.), (0., 0.)),
    ];
    let mut edges = outer.to_vec();
    edges.extend([
        Edge::Line((1., 1.), (2., 1.)),
        Edge::Line((2., 1.), (2., 2.)),
        Edge::Line((2., 2.), (1., 2.)),
        Edge::Line((1., 2.), (1., 1.)),
    ]);
    let b = outline::path::rasterize(&edges, 1., (0., 0.), 3, 3, &Default::default()).unwrap();
    assert_eq!(b.get_pixel(1, 1).unwrap(), 0);
    assert_eq!(b.get_pixel(0, 0).unwrap(), 1);
    let curve = [
        Edge::Cubic((0., 0.), (0., 4.), (4., 4.), (4., 0.)),
        Edge::Line((4., 0.), (0., 0.)),
    ];
    let b = outline::path::rasterize(&curve, 1., (0., 0.), 4, 4, &Default::default()).unwrap();
    assert_eq!(b.get_pixel(1, 1).unwrap(), 1);
    assert_eq!(b.get_pixel(0, 0).unwrap(), 0);
}
#[cfg(feature = "ttc")]
#[test]
fn collection_native_faces_selected_and_all() {
    let mut b = font();
    b.names.family = Some("Second Face".into());
    let set = FontSet {
        fonts: vec![font(), b],
        ..Default::default()
    };
    for version in [false, true] {
        let (bytes, _) = vr::write(
            &set,
            "ttc",
            &WriteOptions {
                collection_v2: version,
                ..Default::default()
            },
        )
        .unwrap();
        let (all, _) = vr::read(&bytes, &Options::default()).unwrap();
        assert_eq!(all.fonts.len(), 2);
        assert_eq!(all.fonts[1].names.family.as_deref(), Some("Second Face"));
        assert_eq!(
            all.fonts[1].properties.get("sfnt.face_index"),
            Some(&fontpixel::model::PropertyValue::Integer(1))
        );
        let (one, _) = vr::read(
            &bytes,
            &Options {
                face_index: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(one.fonts.len(), 1);
        assert!(
            vr::read(
                &bytes,
                &Options {
                    face_index: Some(2),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
#[cfg(all(feature = "woff", feature = "woff2"))]
#[test]
fn all_native_registry_wrappers() {
    let set = FontSet::from(font());
    for id in ["ttf", "otf", "woff", "woff2", "ttc"] {
        let bytes = fontpixel::write(
            &set,
            id,
            &fontpixel::WriteOptions::new(
                fontpixel::ResourcePath::new(format!("test.{id}")).unwrap(),
            ),
        )
        .unwrap()
        .output
        .into_single()
        .unwrap();
        let loaded = fontpixel::read(&bytes, &Default::default()).unwrap();
        assert_eq!(loaded.format, id);
        equal(&font(), &loaded.fonts.fonts[0]);
    }
}
#[cfg(all(feature = "ttf", feature = "otb"))]
#[test]
fn embedded_outline_detection_not_ambiguous() {
    let d = encoded(OutlineFlavor::TrueType, true);
    assert_eq!(
        fontpixel::read(&d, &Default::default()).unwrap().format,
        "ttf"
    );
}

#[cfg(all(feature = "ttc", feature = "woff2"))]
#[test]
fn woff2_shared_mixed_collection_and_native_projection() {
    let a = encoded(OutlineFlavor::TrueType, false);
    let b = encoded(OutlineFlavor::Cff, false);
    for version in [0x10000, 0x20000] {
        let ttc = sfnt::collection::encode(&[&a, &a, &b], version, &Default::default())
            .unwrap()
            .0;
        let o = fontpixel::woff::woff2::Options {
            quality: 1,
            ..Default::default()
        };
        let web = fontpixel::woff::woff2::encode(&ttc, &o, &Default::default())
            .unwrap()
            .bytes;
        assert_eq!(&web[4..8], b"ttcf");
        let decoded = fontpixel::woff::woff2::decode(&web, &Default::default()).unwrap();
        let c = sfnt::collection::Collection::parse(&decoded.sfnt, &Default::default()).unwrap();
        assert_eq!(c.faces.len(), 3);
        c.validate_checksums().unwrap();
        let (set, _) = vr::read(&web, &Options::default()).unwrap();
        assert_eq!(set.fonts.len(), 3);
        for f in set.fonts {
            equal(&font(), &f);
        }
        for n in 0..web.len() {
            assert!(fontpixel::woff::woff2::decode(&web[..n], &Default::default()).is_err());
        }
    }
}
#[test]
fn writer_invalid_geometry_never_succeeds() {
    let mut o = WriteOptions::default();
    o.outline.dot_size = f64::NAN;
    assert!(vr::write(&FontSet::from(font()), "ttf", &o).is_err());
}

#[test]
fn true_type_phantom_origin_and_use_my_metrics() {
    let d = encoded(OutlineFlavor::TrueType, false);
    let f = Sfnt::parse(&d, &Default::default()).unwrap();
    let mut t = sfnt::glyf::GlyfTable::from_sfnt(&f, &Default::default()).unwrap();
    let mut component = sfnt::glyf::Component::translated(1, 100, 0);
    component.use_my_metrics = true;
    component.transform = [8192, 0, 0, 8192];
    t.glyphs[3] = sfnt::glyf::Glyph::Composite(sfnt::glyf::CompositeGlyph {
        components: vec![component],
        instructions: Vec::new(),
    });
    let e = t
        .encode(sfnt::glyf::LocaFormat::Long, &Default::default())
        .unwrap();
    let (h, mut m) = sfnt::metrics::decode(
        sfnt::metrics::Axis::Horizontal,
        f.table(*b"hhea").unwrap(),
        f.table(*b"hmtx").unwrap(),
        4,
        &Default::default(),
    )
    .unwrap();
    m[1].advance = 300;
    m[1].bearing = 0;
    m[3].advance = 500;
    let hm = sfnt::metrics::encode(
        sfnt::metrics::Axis::Horizontal,
        &h,
        &m,
        false,
        &Default::default(),
    )
    .unwrap();
    let mut head = f.table(*b"head").unwrap().to_vec();
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    let d = patch(
        &patch(
            &patch(
                &patch(&patch(&d, *b"glyf", e.glyf), *b"loca", e.loca),
                *b"head",
                head,
            ),
            *b"hhea",
            hm.header,
        ),
        *b"hmtx",
        hm.metrics,
    );
    let (set, _) = vr::read(
        &d,
        &Options {
            ppem: Some(16),
            ..Default::default()
        },
    )
    .unwrap();
    let a = set.fonts[0].glyphs().nth(1).unwrap().1;
    let b = set.fonts[0].glyphs().nth(3).unwrap().1;
    assert_eq!(a.metrics.bounds.x, 0);
    assert_eq!(a.metrics.advance, Some(Vector::new(6, 0)));
    assert_eq!(b.metrics.bounds.x, 1);
    assert_eq!(b.metrics.advance, Some(Vector::new(6, 0)));
}
#[test]
fn cff_vertical_origin_vorg_overrides_top_bearing() {
    let mut f = font();
    f.metrics.glyph_defaults.vertical_advance = Some(Vector::new(0, -8));
    let d = vr::write(&FontSet::from(f), "otf", &WriteOptions::default())
        .unwrap()
        .0;
    let mut vorg = Vec::new();
    for n in [1u16, 0, 700, 1, 1, 600] {
        vorg.extend(n.to_be_bytes());
    }
    let d = patch(&d, *b"VORG", vorg);
    let (set, _) = vr::read(&d, &Options::default()).unwrap();
    assert_eq!(
        set.fonts[0]
            .glyphs()
            .nth(1)
            .unwrap()
            .1
            .metrics
            .vertical_origin
            .unwrap()
            .y,
        6
    );
    assert_eq!(
        set.fonts[0]
            .glyphs()
            .nth(2)
            .unwrap()
            .1
            .metrics
            .vertical_origin
            .unwrap()
            .y,
        7
    );
}
