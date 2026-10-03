//! TTC container conformance and adversarial bounds.
#![cfg(feature = "ttc")]
use fontpixel::{
    Report,
    sfnt::{
        Flavor, Sfnt, SfntBuilder,
        collection::{self, Collection, Limits},
    },
};
fn font(kind: Flavor) -> Vec<u8> {
    let mut b = SfntBuilder::new(kind);
    let mut head = vec![0; 54];
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    b.insert(*b"head", head).unwrap();
    b.insert(*b"TEST", vec![1, 2, 3]).unwrap();
    b.build(&Default::default()).unwrap()
}
fn encoded(version: u32) -> (Vec<u8>, Report) {
    let a = font(Flavor::TrueType);
    let b = font(Flavor::Cff);
    collection::encode(&[&a, &b], version, &Limits::default()).unwrap()
}
#[test]
fn versions_mixed_flavors_shared_tables() {
    for v in [0x10000, 0x20000] {
        let (d, _) = encoded(v);
        let c = Collection::parse(&d, &Default::default()).unwrap();
        c.validate_checksums().unwrap();
        assert_eq!(c.faces.len(), 2);
        assert_eq!(c.faces[1].flavor, Flavor::Cff);
        assert_eq!(c.faces[0].tables[0].offset, c.faces[1].tables[0].offset);
        for (i, flavor) in [Flavor::TrueType, Flavor::Cff].into_iter().enumerate() {
            let (b, r) = c.extract(i, &Default::default()).unwrap();
            assert!(!r.has_loss());
            let f = Sfnt::parse(&b, &Default::default()).unwrap();
            f.validate_checksums().unwrap();
            assert_eq!(f.flavor(), flavor);
            assert_eq!(f.table(*b"TEST"), Some(&[1, 2, 3][..]));
        }
        assert!(c.extract(2, &Default::default()).is_err());
    }
}
#[test]
fn deterministic_and_smaller() {
    let a = font(Flavor::TrueType);
    let d = collection::encode(&[&a, &a, &a], 0x10000, &Default::default())
        .unwrap()
        .0;
    assert_eq!(
        d,
        collection::encode(&[&a, &a, &a], 0x10000, &Default::default())
            .unwrap()
            .0
    );
    assert!(d.len() < 3 * a.len());
}
#[test]
fn truncated_every_byte() {
    let (d, _) = encoded(0x20000);
    for n in 0..d.len() - 1 {
        assert!(
            Collection::parse(&d[..n], &Default::default()).is_err(),
            "{n}"
        );
    }
}
#[test]
fn malformed_directories_offsets_and_versions() {
    let (d, _) = encoded(0x20000);
    for (at, value) in [
        (4, 0x30000u32),
        (8, u32::MAX),
        (12, 1),
        (12, u32::MAX),
        (20, 0x44534947),
        (24, 1),
        (28, 32),
    ] {
        let mut bad = d.clone();
        bad[at..at + 4].copy_from_slice(&value.to_be_bytes());
        assert!(
            Collection::parse(&bad, &Default::default()).is_err(),
            "{at}"
        );
    }
}
#[test]
fn rejects_partial_alias_and_directory_overlap() {
    let (d, _) = encoded(0x10000);
    let c = Collection::parse(&d, &Default::default()).unwrap();
    let off = c.faces[0].offset as usize;
    for value in [0, c.faces[1].offset, c.faces[0].tables[0].offset + 4] {
        let mut bad = d.clone();
        bad[off + 20..off + 24].copy_from_slice(&value.to_be_bytes());
        assert!(Collection::parse(&bad, &Default::default()).is_err());
    }
}
#[test]
fn duplicate_tags_and_limits() {
    let (d, _) = encoded(0x10000);
    let c = Collection::parse(&d, &Default::default()).unwrap();
    let off = c.faces[0].offset as usize;
    let mut bad = d.clone();
    bad.copy_within(off + 12..off + 16, off + 28);
    assert!(Collection::parse(&bad, &Default::default()).is_err());
    for i in 0..4 {
        let mut l = Limits::default();
        match i {
            0 => l.max_faces = 1,
            1 => l.max_table_references = 1,
            2 => l.max_expanded_bytes = 2,
            _ => l.sfnt.resources.max_input_bytes = 2,
        };
        assert!(Collection::parse(&d, &l).is_err());
    }
}
#[test]
fn checksum_validation_is_explicit() {
    let (mut d, _) = encoded(0x10000);
    let c = Collection::parse(&d, &Default::default()).unwrap();
    let off = c.faces[0].tables[0].offset as usize;
    d[off] ^= 1;
    let c = Collection::parse(&d, &Default::default()).unwrap();
    assert!(c.validate_checksums().is_err());
}
#[test]
fn signatures_removed_not_forged() {
    let a = font(Flavor::TrueType);
    let f = Sfnt::parse(&a, &Default::default()).unwrap();
    let mut b = SfntBuilder::new(f.flavor());
    for t in f.tables() {
        b.insert(t.tag, t.data.to_vec()).unwrap();
    }
    b.insert(*b"DSIG", vec![0; 8]).unwrap();
    let a = b.build(&Default::default()).unwrap();
    let (d, r) = collection::encode(&[&a], 0x20000, &Default::default()).unwrap();
    assert!(r.has_loss());
    let c = Collection::parse(&d, &Default::default()).unwrap();
    assert!(c.signature.is_none());
    assert!(!c.faces[0].tables.iter().any(|t| t.tag == *b"DSIG"));
}
#[test]
fn v2_signature_bounded_and_lossy_on_extract() {
    let (mut d, _) = encoded(0x20000);
    let offset = d.len() as u32;
    d[20..24].copy_from_slice(b"DSIG");
    d[24..28].copy_from_slice(&8u32.to_be_bytes());
    d[28..32].copy_from_slice(&offset.to_be_bytes());
    d.extend([0; 8]);
    let c = Collection::parse(&d, &Default::default()).unwrap();
    assert_eq!(c.signature.unwrap().len(), 8);
    assert!(c.extract(0, &Default::default()).unwrap().1.has_loss());
}
#[test]
fn invalid_writer_limits() {
    let a = font(Flavor::TrueType);
    assert!(collection::encode(&[], 0x10000, &Default::default()).is_err());
    assert!(collection::encode(&[&a], 0x40000, &Default::default()).is_err());
    let l = Limits {
        max_expanded_bytes: 1,
        ..Default::default()
    };
    assert!(collection::encode(&[&a], 0x10000, &l).is_err());
}
