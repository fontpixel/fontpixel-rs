//! Subset admission and validation regressions through the public library API.
#![cfg(feature = "sfnt")]

use fontpixel::{
    Error,
    sfnt::{self, Sfnt, SfntBuilder, subset},
};
use std::collections::BTreeMap;

const GOLDEN: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-0.ttf");

fn constant_cmap(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut data = Vec::new();
    for n in [0u16, 1, 3, 10] {
        data.extend(n.to_be_bytes());
    }
    data.extend(12u32.to_be_bytes());
    data.extend(13u16.to_be_bytes());
    data.extend(0u16.to_be_bytes());
    for n in [16 + 12 * groups.len() as u32, 0, groups.len() as u32] {
        data.extend(n.to_be_bytes());
    }
    for &(start, end, glyph) in groups {
        for n in [start, end, glyph] {
            data.extend(n.to_be_bytes());
        }
    }
    data
}

fn source_with_cmap(cmap: Vec<u8>) -> Vec<u8> {
    let limits = sfnt::Limits::default();
    let source = Sfnt::parse(GOLDEN, &limits).unwrap();
    let mut builder = SfntBuilder::new(source.flavor());
    for table in source.tables().iter().filter(|table| table.tag != *b"cmap") {
        builder.insert(table.tag, table.data.to_vec()).unwrap();
    }
    builder.insert(*b"cmap", cmap).unwrap();
    builder.build(&limits).unwrap()
}

#[test]
fn small_subset_does_not_allocate_the_complete_source_cmap() {
    let raw_cmap = constant_cmap(&[(0x20, 0x2000, 1)]);
    let source = source_with_cmap(raw_cmap.clone());
    let original = source.clone();
    let mut options = subset::Options::text("A");
    options.limits.sfnt.resources.max_total_bytes = 64 * 1024;
    // Materializing every mapping cannot fit this allowance. Selecting one
    // scalar must still validate the entire compressed source map.
    assert!(matches!(
        sfnt::cmap::decode(&raw_cmap, 4, &options.limits.sfnt),
        Err(Error::ResourceLimit { .. })
    ));
    let result = subset::subset(&source, &options).unwrap();
    assert_eq!(result.glyph_mapping, BTreeMap::from([(0, 0), (1, 1)]));
    assert_eq!(source, original);
    let font = Sfnt::parse(&result.bytes, &options.limits.sfnt).unwrap();
    font.validate_checksums().unwrap();
    assert_eq!(
        sfnt::cmap::decode(font.table(*b"cmap").unwrap(), 2, &options.limits.sfnt).unwrap(),
        BTreeMap::from([(65, 1)])
    );
}

#[test]
fn subset_rejects_an_invalid_mapping_outside_the_selection() {
    let source = source_with_cmap(constant_cmap(&[(65, 65, 1), (66, 66, 4)]));
    assert!(matches!(
        subset::subset(&source, &subset::Options::text("A")),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn subset_keeps_the_full_source_cmap_work_limit() {
    let source = source_with_cmap(constant_cmap(&[(0x20, 0x2000, 1)]));
    let mut options = subset::Options::text("A");
    options.limits.sfnt.max_cmap_mappings = 100;
    assert!(matches!(
        subset::subset(&source, &options),
        Err(Error::ResourceLimit {
            resource: "sfnt cmap mappings",
            limit: 100
        })
    ));
}
