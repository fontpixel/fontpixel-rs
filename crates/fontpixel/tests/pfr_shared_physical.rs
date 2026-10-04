//! Deterministic repeated-physical-record PFR allocation and identity contracts.
#![cfg(feature = "pfr")]
use fontpixel::formats::pfr::{self, PfrDialect, PfrReadOptions};
use fontpixel::resources::ReadContext;
use fontpixel::{Error, FontSet, Input, Report, ResourceLimits};

const SOURCE: &[u8] = include_bytes!("pfr/pfr-ft-0-0-1.pfr");
fn number(bytes: &[u8], at: usize, size: usize) -> usize {
    bytes[at..at + size]
        .iter()
        .fold(0usize, |value, &b| value * 256 + usize::from(b))
}
fn put(bytes: &mut [u8], at: usize, size: usize, value: usize) {
    assert!(value < 1usize << (size * 8));
    for (i, b) in bytes[at..at + size].iter_mut().enumerate() {
        *b = (value >> (8 * (size - i - 1))) as u8;
    }
}
fn append(out: &mut Vec<u8>, size: usize, value: usize) {
    let start = out.len();
    out.resize(start + size, 0);
    put(out, start, size, value);
}

// Reframe the project's original MIT fixture into multiple logical records.
// Character entries are independently assembled: 16-bit code/advance, followed
// by 8-bit outline size and 16-bit GPS offset. Each bitmap table still names
// only code 65, so physical-table work is separable from real output work.
fn fixture(
    references: &[usize],
    characters: usize,
    outlines: bool,
    dialect: PfrDialect,
) -> Vec<u8> {
    assert!(!references.is_empty() && (66..=4096).contains(&characters));
    let original_off = number(SOURCE, 27, 3);
    let original_size = number(SOURCE, 22, 2);
    let original_gps = number(SOURCE, 35, 3);
    let mut physical = SOURCE[original_off..original_off + original_size - 8].to_vec();
    physical[14] |= 2;
    append(&mut physical, 2, characters);
    for code in 0..characters {
        append(&mut physical, 2, code);
        append(&mut physical, 2, 11);
        physical.push(u8::from(outlines && code == 65));
        append(&mut physical, 2, 0);
    }
    // The original bitmap-info extension starts after the native header.
    // Its single BCT offset is relative to physical start for DAVIC and end
    // for FreeType; the immutable cache must honor the selected dialect.
    let physical_len = physical.len();
    put(
        &mut physical,
        28,
        2,
        if dialect == PfrDialect::Davic {
            physical_len
        } else {
            0
        },
    );
    let bct = &SOURCE[original_off + original_size..original_gps];
    let gps = &SOURCE[original_gps..SOURCE.len() - 8];
    let records = references.iter().copied().max().unwrap() + 1;
    let directory_len = 2 + references.len() * 5;
    let logical_start = 58 + directory_len;
    let logical_size = 18;
    let physical_start = logical_start + references.len() * logical_size;
    let physical_size = records * (physical.len() + bct.len());
    let gps_start = physical_start + physical_size;
    let original_logical = number(SOURCE, 19, 3);
    let mut out = SOURCE[..58].to_vec();
    for (at, size, value) in [
        (10, 2, directory_len),
        (12, 2, 58),
        (14, 2, logical_size),
        (16, 3, references.len() * logical_size),
        (19, 3, logical_start),
        (22, 2, physical.len()),
        (24, 3, physical_size),
        (27, 3, physical_start),
        (30, 2, gps.len()),
        (32, 3, gps.len()),
        (35, 3, gps_start),
        (43, 3, bct.len() * records),
        (46, 3, bct.len()),
        (49, 3, bct.len()),
        (52, 2, references.len()),
        (56, 2, records),
    ] {
        put(&mut out, at, size, value);
    }
    append(&mut out, 2, references.len());
    for i in 0..references.len() {
        append(&mut out, 2, logical_size);
        append(&mut out, 3, logical_start + i * logical_size);
    }
    for &reference in references {
        out.extend_from_slice(&SOURCE[original_logical..original_logical + 13]);
        append(&mut out, 2, physical.len());
        append(
            &mut out,
            3,
            physical_start + reference * (physical.len() + bct.len()),
        );
    }
    for _ in 0..records {
        out.extend_from_slice(&physical);
        out.extend_from_slice(bct);
    }
    out.extend_from_slice(gps);
    let total = out.len() + 8;
    append(&mut out, 3, total);
    out.extend_from_slice(b"$PFR$");
    assert_eq!(out.len(), total);
    out
}
fn load(
    bytes: &[u8],
    limits: &ResourceLimits,
    dialect: Option<PfrDialect>,
) -> fontpixel::Result<(FontSet, Report)> {
    let input = Input::new(bytes);
    let mut context = ReadContext::new(&input, limits)?;
    pfr::read(&mut context, &PfrReadOptions { dialect })
}

#[test]
fn shared_and_distinct_physical_records_have_identical_models() {
    for dialect in [PfrDialect::FreeType, PfrDialect::Davic] {
        for count in [1, 2, 7, 16] {
            let shared = fixture(&vec![0; count], 512, false, dialect);
            let separate = fixture(&(0..count).collect::<Vec<_>>(), 512, false, dialect);
            for choice in [None, Some(dialect)] {
                let (actual, report) = load(&shared, &ResourceLimits::default(), choice).unwrap();
                let (expected, other) =
                    load(&separate, &ResourceLimits::default(), choice).unwrap();
                assert_eq!(actual, expected);
                assert_eq!(report, other);
                assert!(!report.has_loss());
                assert_eq!(actual.fonts.len(), count);
                assert!(actual.fonts.iter().all(|f| f.len() == 1));
            }
        }
    }
}

#[test]
fn repeated_physical_storage_is_charged_once_but_outputs_are_not_free() {
    let shared = fixture(&[0; 16], 2048, false, PfrDialect::FreeType);
    let distinct = fixture(
        &(0..16).collect::<Vec<_>>(),
        2048,
        false,
        PfrDialect::FreeType,
    );
    let limits = ResourceLimits {
        max_total_bytes: 256 * 1024,
        ..ResourceLimits::default()
    };
    let (set, report) = load(&shared, &limits, None).unwrap();
    assert_eq!(set.fonts.len(), 16);
    assert!(!report.has_loss());
    // Give the distinct-record input its extra wire bytes as additional budget;
    // rejection must still follow from retained decoded physical storage.
    let distinct_limits = ResourceLimits {
        max_total_bytes: limits.max_total_bytes + distinct.len() - shared.len(),
        ..limits.clone()
    };
    assert!(matches!(
        load(&distinct, &distinct_limits, None),
        Err(Error::ResourceLimit { .. })
    ));
    for limits in [
        ResourceLimits {
            max_glyphs: 15,
            ..limits.clone()
        },
        ResourceLimits {
            max_resources: 15,
            ..limits.clone()
        },
        ResourceLimits {
            max_total_bytes: shared.len() + 64,
            ..limits
        },
    ] {
        assert!(matches!(
            load(&shared, &limits, None),
            Err(Error::ResourceLimit { .. })
        ));
    }
}

#[test]
fn shared_outline_loss_keeps_every_logical_location_and_order() {
    let bytes = fixture(&[0, 0, 1, 0, 1], 128, true, PfrDialect::FreeType);
    let (set, report) = load(&bytes, &ResourceLimits::default(), None).unwrap();
    assert_eq!(set.fonts.len(), 5);
    let losses = report
        .entries
        .iter()
        .filter(|e| e.code == "pfr.outlines-omitted")
        .collect::<Vec<_>>();
    assert_eq!(losses.len(), 5);
    assert_eq!(
        losses
            .iter()
            .map(|e| e.location.font_index)
            .collect::<Vec<_>>(),
        (0..5).map(Some).collect::<Vec<_>>()
    );
    assert!(
        losses
            .iter()
            .all(|e| e.message == "1 outline glyph programs are not bitmap strikes")
    );
}

#[test]
fn cache_key_includes_record_size_and_rejects_malformed_later_reference() {
    let mut bytes = fixture(&[0, 0], 128, false, PfrDialect::FreeType);
    let logical_start = number(&bytes, 19, 3);
    let physical_size = number(&bytes, logical_start + 13, 2);
    // Same physical offset, different declared size must not hit the first
    // valid cached record; otherwise the invalid second font is accepted.
    put(&mut bytes, logical_start + 18 + 13, 2, physical_size - 1);
    assert!(load(&bytes, &ResourceLimits::default(), None).is_err());
}

#[test]
fn cache_is_call_local_and_explicit_dialect_remains_authoritative() {
    let bytes = fixture(&[0, 0], 128, false, PfrDialect::FreeType);
    let limits = ResourceLimits::default();
    let first = load(&bytes, &limits, Some(PfrDialect::FreeType)).unwrap();
    assert!(load(&bytes, &limits, Some(PfrDialect::Davic)).is_err());
    assert_eq!(
        first,
        load(&bytes, &limits, Some(PfrDialect::FreeType)).unwrap()
    );
}

#[test]
fn every_shared_bitmap_output_counts_against_the_glyph_limit() {
    let bytes = fixture(&[0; 80], 66, false, PfrDialect::FreeType);
    let limits = ResourceLimits {
        max_glyphs: 66,
        max_resources: 96,
        max_total_bytes: 4 * 1024 * 1024,
        ..ResourceLimits::default()
    };
    assert!(matches!(
        load(&bytes, &limits, None),
        Err(Error::ResourceLimit {
            resource: "PFR total glyphs",
            limit: 66
        })
    ));
}

#[test]
fn cached_absent_strikes_keep_logical_loss_locations() {
    let mut bytes = fixture(&[0, 1, 0, 1], 128, false, PfrDialect::FreeType);
    let first_physical = number(&bytes, 27, 3);
    // This first physical record has an opaque native extension in place of
    // bitmap-info, while the second physical record still has its bitmap.
    bytes[first_physical + 17] = 7;
    let (set, report) = load(&bytes, &ResourceLimits::default(), None).unwrap();
    assert_eq!(set.fonts.len(), 2);
    let locations = report
        .entries
        .iter()
        .filter(|e| e.code == "pfr.no-bitmap-strike")
        .map(|e| e.location.font_index)
        .collect::<Vec<_>>();
    assert_eq!(locations, [Some(0), Some(2)]);
}

#[test]
fn sorted_cache_preserves_logical_order_and_distinct_record_identity() {
    let references = [2usize, 0, 2, 1, 0];
    let mut bytes = fixture(&references, 128, false, PfrDialect::FreeType);
    let physical_start = number(&bytes, 27, 3);
    let logical_start = number(&bytes, 19, 3);
    let record_size = number(&bytes, logical_start + 13, 2);
    let bct_size = number(&bytes, 46, 3);
    for record in 0..3 {
        let start = physical_start + record * (record_size + bct_size);
        let name_start = start + 18 + usize::from(bytes[start + 16]) + 2;
        bytes[name_start] = b'A' + record as u8;
    }
    let (set, report) = load(&bytes, &ResourceLimits::default(), None).unwrap();
    assert!(!report.has_loss());
    assert_eq!(
        set.fonts
            .iter()
            .map(|font| font.names.family.as_deref().unwrap().as_bytes()[0])
            .collect::<Vec<_>>(),
        references.map(|record| b'A' + record as u8)
    );
}
