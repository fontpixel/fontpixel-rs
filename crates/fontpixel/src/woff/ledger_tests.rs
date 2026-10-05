//! Exact-capacity regressions for the shared WOFF working ledger.
//!
//! Each case finds the smallest ledger that lets the complete operation
//! succeed, then checks success at exactly that capacity, `ResourceLimit` one
//! byte below it, byte-identical output to an unconstrained run, and full
//! release of every charge afterwards.
use super::*;
use crate::working::Budget;

const FONT: &[u8] = include_bytes!("../../../../tests/data/compat/mini-square-1.0-1.ttf");
const OTHER: &[u8] = include_bytes!("../../../../tests/data/compat/mini-circle-1.0-1.ttf");

fn boundary(name: &str, run: impl Fn(&Budget) -> Result<Vec<u8>>) -> usize {
    let unlimited = Budget::new(usize::MAX, "unlimited test ledger");
    let expected = run(&unlimited).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(unlimited.remaining(), usize::MAX, "{name}: leaked charge");
    let high_water = unlimited.peak();
    assert!(high_water > 0, "{name}: nothing was admitted");
    // Growth may fall back to exact capacity under a tighter ledger, so the
    // smallest sufficient capacity can be below the unconstrained high water.
    let (mut low, mut high) = (0usize, high_water);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if run(&Budget::new(mid, "probe ledger")).is_ok() {
            high = mid;
        } else {
            low = mid;
        }
    }
    let exact = Budget::new(high, "exact ledger");
    assert_eq!(
        run(&exact).unwrap_or_else(|e| panic!("{name} at {high}: {e}")),
        expected,
        "{name}: output changed under the exact ledger"
    );
    assert_eq!(exact.remaining(), high, "{name}: exact ledger leaked");
    let short = Budget::new(high - 1, "short ledger");
    match run(&short) {
        Err(Error::ResourceLimit {
            resource: "short ledger",
            limit,
        }) => assert_eq!(limit, high - 1),
        other => panic!("{name}: one byte short gave {other:?}"),
    }
    assert_eq!(short.remaining(), high - 1, "{name}: failed run leaked");
    high
}

fn woff2_options(quality: u32, transform: bool) -> woff2::Options {
    woff2::Options {
        quality,
        transform_glyf: transform,
        transform_hmtx: transform,
        ..Default::default()
    }
}

fn auxiliary() -> Auxiliary {
    Auxiliary {
        major_version: 3,
        minor_version: 4,
        metadata: Some(b"<metadata version=\"1.0\"><uniqueid id=\"x\"/></metadata>".to_vec()),
        private_data: Some(vec![1, 2, 3, 4, 5]),
    }
}

#[test]
fn woff1_encode_and_decode_have_exact_ledger_boundaries() {
    let l = Limits::default();
    let o = woff1::Options {
        compression_level: 9,
        auxiliary: auxiliary(),
    };
    let encoded = woff1::encode(FONT, &o, &l).unwrap();
    boundary("woff1 encode", |b| {
        woff1::encode_with_budget(FONT, &o, &l, b).map(|e| e.bytes)
    });
    let decode = boundary("woff1 decode", |b| {
        woff1::decode_with_budget(&encoded.bytes, &l, b).map(|d| d.sfnt)
    });
    // The decoded tables, assembly and both optional blocks are charged.
    assert!(decode > FONT.len());
}

#[test]
fn woff2_encode_with_and_without_transforms_has_exact_ledger_boundaries() {
    let l = Limits::default();
    for transform in [true, false] {
        for quality in [2, 5] {
            let mut o = woff2_options(quality, transform);
            o.auxiliary = auxiliary();
            boundary(&format!("woff2 encode q{quality} t{transform}"), |b| {
                woff2::encode_with_budget(FONT, &o, &l, b).map(|e| e.bytes)
            });
        }
    }
}

#[test]
fn woff2_decode_has_exact_ledger_boundaries() {
    let l = Limits::default();
    for transform in [true, false] {
        let mut o = woff2_options(5, transform);
        o.auxiliary = auxiliary();
        let encoded = woff2::encode(FONT, &o, &l).unwrap();
        boundary(&format!("woff2 decode t{transform}"), |b| {
            woff2::decode_with_budget(&encoded.bytes, &l, b).map(|d| d.sfnt)
        });
    }
}

#[cfg(feature = "ttc")]
#[test]
fn woff2_collection_has_exact_ledger_boundaries() {
    let l = Limits::default();
    let (ttc, _) =
        crate::sfnt::collection::encode(&[FONT, OTHER, FONT], 0x20000, &Default::default())
            .unwrap();
    let o = woff2_options(5, true);
    let encoded = woff2::encode(&ttc, &o, &l).unwrap();
    boundary("woff2 collection encode", |b| {
        woff2::collection::encode(&ttc, &o, &l, b).map(|e| e.bytes)
    });
    boundary("woff2 collection decode", |b| {
        woff2::collection::decode(&encoded.bytes, &l, b).map(|d| d.sfnt)
    });
}

#[cfg(feature = "ttc")]
#[test]
fn shared_collection_tables_are_charged_once() {
    let l = Limits::default();
    let o = woff2_options(5, true);
    // A face dominated by one large opaque table.
    let source = crate::sfnt::Sfnt::parse(FONT, &l.sfnt).unwrap();
    let mut builder = crate::sfnt::SfntBuilder::new(source.flavor());
    for t in source.tables() {
        builder.insert(t.tag, t.data.to_vec()).unwrap();
    }
    let payload = 64 * 1024;
    let mut state = 0x2545_f491u32;
    let opaque = (0..payload)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    builder.insert(*b"ZZZZ", opaque).unwrap();
    let face = builder.build(&l.sfnt).unwrap();
    let peak = |faces: &[&[u8]]| {
        let (ttc, _) =
            crate::sfnt::collection::encode(faces, 0x10000, &Default::default()).unwrap();
        let budget = Budget::new(usize::MAX, "sharing ledger");
        woff2::collection::encode(&ttc, &o, &l, &budget).unwrap();
        (ttc.len(), budget.peak())
    };
    let (one_len, one) = peak(&[&face]);
    let (many_len, many) = peak(&[&face, &face, &face, &face]);
    // Identical faces share every table in the TTC and in the WOFF2 payloads:
    // three more references add directory/bookkeeping storage, not three more
    // retained copies of the 64 KiB table.
    assert!(many_len < one_len + payload / 2);
    assert!(many < one + payload / 2, "one={one} many={many}");
}
