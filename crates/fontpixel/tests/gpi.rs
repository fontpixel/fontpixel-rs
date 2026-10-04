//! OS/2 GPI native kinds, resource modules and compressed-page regressions.
#![cfg(feature = "gpi")]
use fontpixel::formats::gpi::{self, GpiContainer, GpiKind, GpiReadOptions, GpiWriteOptions};
use fontpixel::model::{Codepoint, Glyph, Property, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn source() -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("Original".into());
    for cp in [65, 66, 67] {
        let mut g = Glyph::new("#.......#\n.###.....\n.....####".parse::<Bitmap>().unwrap());
        g.metrics.bounds.x = -2;
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector::new(8, 0));
        g.codepoints.push(Codepoint::Legacy(cp));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(gpi::read(&mut ReadContext::new(&i, l)?, &Default::default())?.0)
}
fn write(s: &FontSet, o: &GpiWriteOptions) -> Vec<u8> {
    gpi::write(s, &WriteOptions::new(ResourcePath::new("f").unwrap()), o)
        .unwrap()
        .output
        .into_single()
        .unwrap()
}
#[test]
fn all_native_kinds_and_containers() {
    for kind in [GpiKind::Fixed, GpiKind::Proportional, GpiKind::Abc] {
        for container in [GpiContainer::Bare, GpiContainer::Ne, GpiContainer::Lx] {
            let s = source();
            let b = write(
                &s,
                &GpiWriteOptions {
                    kind: Some(kind),
                    container: Some(container),
                    ..Default::default()
                },
            );
            let back = read(&b, &Default::default()).unwrap();
            assert_eq!(back.fonts.len(), 1);
            assert_eq!(back.fonts[0].len(), 3);
            for (_, g) in s.fonts[0].glyphs() {
                let got = back.fonts[0].glyph_for_codepoint(g.codepoints[0]).unwrap();
                assert_eq!(g.bitmap, got.bitmap);
                assert_eq!(g.metrics.bounds.y, got.metrics.bounds.y);
                if kind == GpiKind::Abc {
                    assert_eq!(g.metrics.advance, got.metrics.advance);
                    assert_eq!(g.metrics.bounds, got.metrics.bounds);
                }
            }
            let strict = WriteOptions {
                strict: true,
                ..WriteOptions::new(ResourcePath::new("f").unwrap())
            };
            assert_eq!(
                b,
                gpi::write(&back, &strict, &Default::default())
                    .unwrap()
                    .output
                    .into_single()
                    .unwrap()
            );
        }
    }
}
#[test]
fn complete_inclusive_range_null_and_missing() {
    let mut s = source();
    let mut null = Glyph::new("##\n.#\n#.".parse::<Bitmap>().unwrap());
    null.name = Some(".null".into());
    null.metrics.bounds.y = -1;
    s.fonts[0].add_glyph(null).unwrap();
    let b = write(&s, &Default::default());
    let back = read(&b, &Default::default()).unwrap();
    assert_eq!(back.fonts[0].len(), 4);
    assert!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(67))
            .is_some()
    );
    let input = Input::new(&b);
    let small = gpi::read(
        &mut ReadContext::new(&input, &Default::default()).unwrap(),
        &GpiReadOptions {
            include_unmapped_null: false,
        },
    )
    .unwrap()
    .0;
    assert_eq!(small.fonts[0].len(), 3);
    let mut many = source();
    let mut other = many.fonts[0].clone();
    other
        .properties
        .push(Property::new("gpi.resource_id", 23i64));
    many.fonts.push(other);
    let b = write(
        &many,
        &GpiWriteOptions {
            container: Some(GpiContainer::Lx),
            ..Default::default()
        },
    );
    assert_eq!(read(&b, &Default::default()).unwrap().fonts.len(), 2);
}
#[test]
fn lx_pack_and_hostile_bounds() {
    let raw = write(&source(), &Default::default());
    let plain = write(
        &source(),
        &GpiWriteOptions {
            container: Some(GpiContainer::Lx),
            ..Default::default()
        },
    );
    let u32at = |b: &[u8], p: usize| u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize;
    let pages = u32at(&plain, 72) + 8;
    let at = u32at(&plain, 128) + 4096;
    for kind in [1u16, 5] {
        let mut b = plain.clone();
        let compressed = if kind == 1 {
            let mut v = vec![1, 0];
            v.extend((raw.len() as u16).to_le_bytes());
            v.extend(&raw);
            v
        } else {
            let mut v = Vec::new();
            for c in raw.chunks(63) {
                v.push((c.len() as u8) << 2);
                v.extend(c);
            }
            v.extend([0, 0]);
            v
        };
        b[pages + 4..pages + 6].copy_from_slice(&(compressed.len() as u16).to_le_bytes());
        b[pages + 6..pages + 8].copy_from_slice(&kind.to_le_bytes());
        b[at..at + compressed.len()].copy_from_slice(&compressed);
        assert_eq!(read(&b, &Default::default()).unwrap().fonts[0].len(), 3);
    }
    for n in 0..raw.len() {
        assert!(read(&raw[..n], &Default::default()).is_err(), "prefix {n}");
    }
    assert!(
        read(
            &raw,
            &ResourceLimits {
                max_glyphs: 2,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        read(
            &raw,
            &ResourceLimits {
                max_pixels_per_glyph: 26,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut bad = raw;
    bad[20 + 168 + 28..20 + 168 + 32].fill(255);
    assert!(read(&bad, &Default::default()).is_err());
}

#[test]
fn extreme_font_metrics_return_errors() {
    for value in [i64::MIN, i64::MAX] {
        let mut s = source();
        s.fonts[0].metrics.ascent = Some(value);
        assert!(
            gpi::write(
                &s,
                &WriteOptions::new(ResourcePath::new("f").unwrap()),
                &Default::default()
            )
            .is_err()
        );
    }
}

#[test]
fn native_zero_segment_padding_is_preserved() {
    let mut b = write(&source(), &Default::default());
    b.extend([0; 16]);
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(b, write(&s, &Default::default()));
    *b.last_mut().unwrap() = 1;
    assert!(read(&b, &Default::default()).is_err());
}

fn sparse_descriptors(last: u32, extra: usize) -> FontSet {
    let mut f = Font::default();
    for cp in [0, last] {
        let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
        g.codepoints.push(Codepoint::Legacy(cp));
        if cp == 0 {
            g.properties.push(Property::new(
                "gpi.descriptor_extra",
                fontpixel::model::PropertyValue::Bytes(vec![0; extra]),
            ));
        }
        f.add_glyph(g).unwrap();
    }
    f.into()
}

#[test]
fn sparse_descriptor_table_checks_output_cap_before_allocation() {
    // Only 64 KiB of model data would expand into nearly 4 GiB of descriptors.
    // The table cap must win over the later aggregate-allocation check.
    let s = sparse_descriptors(65535, 65525);
    let error = gpi::write(
        &s,
        &WriteOptions::new(ResourcePath::new("f").unwrap()),
        &GpiWriteOptions {
            max_output_bytes: 1024 * 1024,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        fontpixel::Error::ResourceLimit {
            resource: "GPI output record bytes",
            limit: 1_048_576,
        }
    ));
}

#[test]
fn sparse_descriptor_table_checks_cumulative_budget_before_allocation() {
    let s = sparse_descriptors(255, 4086);
    let error = gpi::write(
        &s,
        &WriteOptions::new(ResourcePath::new("f").unwrap()),
        &GpiWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: 512 * 1024,
                ..Default::default()
            },
            max_output_bytes: 2 * 1024 * 1024,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        fontpixel::Error::ResourceLimit {
            resource: "legacy aggregate bytes",
            limit: 524_288,
        }
    ));
}
