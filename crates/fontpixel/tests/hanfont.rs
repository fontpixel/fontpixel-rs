//! Original synthetic component data; no third-party font artwork.
#![cfg(feature = "hanfont")]
use fontpixel::formats::hanfont::{self, HanfontReadOptions, HanfontWriteOptions, ReadMode};
use fontpixel::model::{Codepoint, Property, Vector};
use fontpixel::{Error, ResourceLimits};
fn wire() -> Vec<u8> {
    let mut bytes = vec![0; 360 * 32];
    for slot in 0..360 {
        for p in 0..32 {
            bytes[slot * 32 + p] = ((slot * 29 + p * 47) ^ (slot >> 3)) as u8;
        }
    }
    bytes
}
fn read(mode: ReadMode) -> fontpixel::FontSet {
    hanfont::read(
        &wire(),
        &HanfontReadOptions {
            mode,
            ..Default::default()
        },
        &ResourceLimits::default(),
    )
    .unwrap()
    .0
}
#[test]
fn components_are_all_360_ordered_slots_and_roundtrip_exact() {
    let s = read(ReadMode::Components);
    let f = s.single().unwrap();
    assert_eq!(f.len(), 360);
    for (id, g) in f.glyphs() {
        assert_eq!(g.codepoints, [Codepoint::Legacy(id.index() as u32)]);
        assert_eq!(
            g.bitmap.as_bytes(),
            &wire()[id.index() * 32..(id.index() + 1) * 32]
        );
    }
    let (out, r) = hanfont::write(&s, &Default::default()).unwrap();
    assert_eq!(out, wire());
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn full_syllable_roundtrip_preserves_unused_nonblank_fillers() {
    let s = read(ReadMode::Syllables);
    assert_eq!(s.single().unwrap().len(), 11172);
    let (out, r) = hanfont::write(&s, &Default::default()).unwrap();
    assert_eq!(out, wire());
    assert!(!r.has_loss(), "{r}");
    assert_eq!(
        s,
        hanfont::read(&out, &Default::default(), &ResourceLimits::default())
            .unwrap()
            .0
    );
}
#[test]
fn registry_strict_mode_rejects_complete_model_loss() {
    let mut s = read(ReadMode::Components);
    let mut options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("test.han").unwrap());
    options.strict = true;
    assert!(fontpixel::write(&s, "hanfont", &options).is_ok());
    s.fonts[0].names.license = Some("source license".into());
    match fontpixel::write(&s, "hanfont", &options) {
        Err(fontpixel::Error::StrictLoss(report)) => assert!(report.has_loss()),
        other => panic!("expected StrictLoss, got {other:?}"),
    }
}
#[test]
fn independently_worked_component_examples_cover_all_shapes() {
    // (initial, vowel, final, expected initial slot, vowel slot, final slot)
    let cases = [
        (0, 0, 0, 1, 161, None),
        (0, 0, 1, 101, 205, Some(249)),
        (2, 0, 4, 103, 227, Some(252)),
        (15, 0, 4, 116, 205, Some(252)),
        (0, 8, 0, 21, 169, None),
        (0, 13, 0, 41, 174, None),
        (0, 9, 0, 61, 170, None),
        (0, 14, 0, 81, 175, None),
        (0, 8, 1, 121, 213, Some(333)),
        (0, 13, 1, 121, 218, Some(333)),
        (0, 9, 1, 141, 214, Some(249)),
        (0, 14, 1, 141, 219, Some(277)),
        (0, 1, 1, 101, 206, Some(305)),
        (0, 11, 1, 141, 216, Some(277)),
        (0, 10, 1, 141, 215, Some(305)),
        (0, 19, 1, 141, 224, Some(277)),
        (18, 20, 27, 119, 247, Some(303)),
    ];
    let s = read(ReadMode::Syllables);
    let bytes = wire();
    for (l, v, t, a, m, z) in cases {
        let cp = 0xac00 + (l * 21 + v) * 28 + t;
        assert_eq!(hanfont::component_slots(cp).unwrap(), (a, m, z));
        let g = s
            .single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(cp))
            .unwrap();
        for k in 0..32 {
            assert_eq!(
                g.bitmap.as_bytes()[k],
                bytes[a * 32 + k] | bytes[m * 32 + k] | z.map_or(0, |z| bytes[z * 32 + k])
            );
        }
    }
    assert!(hanfont::component_slots(0xabff).is_err());
    assert!(hanfont::component_slots(0xd7a4).is_err());
}
#[test]
fn edited_syllables_do_not_silently_replay_components() {
    let mut s = read(ReadMode::Syllables);
    let id = s.fonts[0].glyph_id(Codepoint::Unicode(0xac00)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = fontpixel::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
                16,
                16,
                &[0; 32],
                Default::default(),
            )?
            .into();
            Ok(())
        })
        .unwrap();
    assert!(hanfont::write(&s, &Default::default()).is_err());
    let mut s = read(ReadMode::Syllables);
    s.fonts[0].properties.remove("hanfont.components");
    assert!(hanfont::write(&s, &Default::default()).is_err());
}
#[test]
fn incomplete_or_duplicate_components_are_rejected() {
    let mut s = read(ReadMode::Components);
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(1)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.codepoints[0] = Codepoint::Legacy(0);
            Ok(())
        })
        .unwrap();
    assert!(hanfont::write(&s, &Default::default()).is_err());
    assert!(
        hanfont::read(
            &wire()[..11519],
            &Default::default(),
            &ResourceLimits::default()
        )
        .is_err()
    );
}
#[test]
fn loss_covers_metrics_names_and_properties() {
    let mut s = read(ReadMode::Components);
    s.properties.push(Property::new("collection", "x"));
    s.fonts[0].names.license = Some("MIT".into());
    s.fonts[0].metrics.underline_thickness = Some(2);
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(1)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.vertical_advance = Some(Vector::new(0, 16));
            g.unicode_sequences.push(vec![65, 66]);
            Ok(())
        })
        .unwrap();
    let (_, r) = hanfont::write(&s, &Default::default()).unwrap();
    for code in [
        "hanfont.collection_metadata",
        "hanfont.names",
        "hanfont.font_metrics",
        "hanfont.glyphs",
    ] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}: {r}");
    }
}
#[test]
fn budgets_cover_full_expansion_and_default_readback() {
    let small = ResourceLimits {
        max_glyphs: 360,
        ..Default::default()
    };
    assert!(hanfont::read(&wire(), &Default::default(), &small).is_err());
    assert!(
        hanfont::read(
            &wire(),
            &HanfontReadOptions {
                mode: ReadMode::Components,
                ..Default::default()
            },
            &small
        )
        .is_ok()
    );
    let s = read(ReadMode::Components);
    assert!(
        hanfont::write(
            &s,
            &HanfontWriteOptions {
                limits: small,
                ..Default::default()
            }
        )
        .is_err()
    );
    for l in [
        ResourceLimits {
            max_total_bytes: 100_000,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 255,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 11519,
            ..Default::default()
        },
        ResourceLimits {
            max_input_bytes: 11519,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            hanfont::read(&wire(), &Default::default(), &l),
            Err(Error::ResourceLimit { .. })
        ));
        assert!(
            hanfont::write(
                &s,
                &HanfontWriteOptions {
                    limits: l,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
#[test]
fn gray_components_need_explicit_threshold() {
    let mut s = read(ReadMode::Components);
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(1)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = fontpixel::Bitmap::<fontpixel::bitmap::Gray8>::from_raw(
                16,
                16,
                &[127; 256],
                Default::default(),
            )?
            .into();
            Ok(())
        })
        .unwrap();
    assert!(hanfont::write(&s, &Default::default()).is_err());
    let (out, r) = hanfont::write(
        &s,
        &HanfontWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
    assert_eq!(&out[32..64], &[0; 32]);
}

#[test]
fn removed_trailing_slot_is_reported_even_when_live_ids_are_unchanged() {
    let mut set = read(ReadMode::Components);
    let removed = set.fonts[0]
        .add_glyph(fontpixel::model::Glyph::default())
        .unwrap();
    set.fonts[0].remove_glyph(removed);
    assert_eq!(set.fonts[0].slot_count(), 361);
    let (_, report) = hanfont::write(&set, &Default::default()).unwrap();
    assert!(report.has_loss(), "{report}");
}
