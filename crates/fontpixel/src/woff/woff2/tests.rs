use super::*;
fn font() -> Vec<u8> {
    let mut b = sfnt::SfntBuilder::new(sfnt::Flavor::TrueType);
    let mut head = vec![0; 54];
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    b.insert(*b"head", head).unwrap();
    b.insert(*b"maxp", vec![0, 1, 0, 0, 0, 1]).unwrap();
    b.insert(*b"glyf", vec![]).unwrap();
    b.insert(*b"loca", vec![0; 4]).unwrap();
    b.insert(*b"ZZZZ", vec![17; 1000]).unwrap();
    b.build(&Limits::default().sfnt).unwrap()
}
#[test]
fn basic_roundtrip() {
    let l = Limits::default();
    let f = font();
    for threads in [1, 2] {
        let o = Options {
            quality: 3,
            compression_threads: threads,
            ..Default::default()
        };
        let e = encode(&f, &o, &l).unwrap();
        let d = decode(&e.bytes, &l).unwrap();
        let s = sfnt::Sfnt::parse(&d.sfnt, &l.sfnt).unwrap();
        s.validate_checksums().unwrap();
        assert_eq!(s.table(*b"ZZZZ").unwrap(), &vec![17; 1000]);
        assert_eq!(encode(&f, &o, &l).unwrap(), e);
    }
}
#[test]
fn integers() {
    let budget = Budget::new(64, "integer test bytes");
    for n in 0..=u16::MAX {
        let mut v = Buffer::new(&budget).unwrap();
        ushort255(&mut v, n).unwrap();
        assert_eq!(Cursor::new(&v).ushort255().unwrap(), n);
    }
    for n in [0, 1, 127, 128, 16383, 16384, u32::MAX] {
        let mut v = Buffer::new(&budget).unwrap();
        base128(&mut v, n).unwrap();
        assert_eq!(Cursor::new(&v).base128().unwrap(), n);
    }
    assert_eq!(budget.remaining(), 64);
    for v in [
        &[0x80, 0][..],
        &[0xff; 6][..],
        &[0x90, 0x80, 0x80, 0x80, 0][..],
    ] {
        assert!(Cursor::new(v).base128().is_err());
    }
}
#[test]
fn auxiliary_and_parallel() {
    let l = Limits::default();
    let f = font();
    let o = Options {
        quality: 2,
        auxiliary: Auxiliary {
            major_version: 1,
            minor_version: 2,
            metadata: Some(b"<metadata version=\"1.0\"/>".to_vec()),
            private_data: Some(vec![2, 3, 4]),
        },
        ..Default::default()
    };
    let e = encode(&f, &o, &l).unwrap();
    assert_eq!(decode(&e.bytes, &l).unwrap().auxiliary, o.auxiliary);
    assert_eq!(
        encode_many(&[&f, &f], &o, &l, 2).unwrap(),
        vec![e.clone(), e]
    );
    assert!(encode_many(&[&f], &o, &l, 0).is_err());
}
#[cfg(feature = "woff")]
#[test]
fn woff1_roundtrip() {
    let l = Limits::default();
    let f = font();
    for quality in [0, 1, 9] {
        let o = super::super::woff1::Options {
            compression_level: quality,
            auxiliary: Auxiliary {
                metadata: Some(b"<metadata version=\"1.0\"/>".to_vec()),
                private_data: Some(vec![2, 3, 4]),
                ..Default::default()
            },
        };
        let e = super::super::woff1::encode(&f, &o, &l).unwrap();
        let d = super::super::woff1::decode(&e.bytes, &l).unwrap();
        assert_eq!(d.sfnt, f);
        assert_eq!(d.auxiliary, o.auxiliary);
    }
}
#[test]
fn encoder_bound_is_admitted_and_writer_failures_stay_resource_limits() {
    let data = vec![7u8; 10_000];
    let l = Limits::default();
    let o = Options {
        quality: 2,
        compression_threads: 1,
        ..Default::default()
    };
    let bound = brotli_encoder_heap_bound(2, LGWIN, data.len(), 1).unwrap();
    // The bound itself is refused before the encoder runs.
    let short = Budget::new(bound - 1, "encoder test bytes");
    assert!(matches!(
        compress(&data, &o, &l, &short),
        Err(Error::ResourceLimit {
            resource: "encoder test bytes",
            ..
        })
    ));
    assert_eq!(short.remaining(), bound - 1);
    // The bound fits but the ledger output buffer inside the Brotli writer does
    // not: the io::Error carries the original ResourceLimit back out.
    let writer = Budget::new(bound + 1, "writer test bytes");
    assert!(matches!(
        compress(&data, &o, &l, &writer),
        Err(Error::ResourceLimit {
            resource: "writer test bytes",
            ..
        })
    ));
    assert_eq!(writer.remaining(), bound + 1);
    let enough = Budget::new(bound + 4096, "writer test bytes");
    let out = compress(&data, &o, &l, &enough).unwrap();
    assert!(!out.is_empty());
    drop(out);
    assert_eq!(enough.remaining(), bound + 4096);
}
