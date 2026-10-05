//! Public-API working-ledger boundaries for WOFF1/WOFF2: every failure caused
//! by a smaller `max_total_bytes` stays `Error::ResourceLimit`, including
//! failures raised inside the Brotli writer and the zlib/Brotli decoders.
#![cfg(all(feature = "woff", feature = "woff2"))]
use fontpixel::{
    Error,
    woff::{Auxiliary, Limits, woff1, woff2},
};

const FONT: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-1.ttf");

fn limits(total: usize) -> Limits {
    let mut l = Limits::default();
    l.sfnt.resources.max_total_bytes = total;
    l
}

fn auxiliary() -> Auxiliary {
    Auxiliary {
        major_version: 1,
        minor_version: 0,
        metadata: Some(b"<metadata version=\"1.0\"/>".to_vec()),
        private_data: Some(vec![7; 9]),
    }
}

/// Smallest `max_total_bytes` that succeeds; checks every smaller value in a
/// window below it, and a geometric sample further down, fails only with
/// `ResourceLimit`.
fn boundary<T: std::fmt::Debug>(
    name: &str,
    run: impl Fn(&Limits) -> fontpixel::Result<T>,
) -> usize {
    let (mut low, mut high) = (0usize, Limits::default().sfnt.resources.max_total_bytes);
    run(&limits(high)).unwrap_or_else(|e| panic!("{name}: default limits fail: {e}"));
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if run(&limits(mid)).is_ok() {
            high = mid;
        } else {
            low = mid;
        }
    }
    let mut checked = (high.saturating_sub(512)..high).collect::<Vec<_>>();
    let mut n = high;
    while n > 1 {
        n /= 2;
        checked.push(n);
    }
    checked.push(0);
    for total in checked {
        match run(&limits(total)) {
            Err(Error::ResourceLimit { .. }) => {}
            other => panic!("{name} at max_total_bytes={total}: {other:?}"),
        }
    }
    high
}

#[test]
fn woff1_resource_failures_stay_resource_limits() {
    let o = woff1::Options {
        compression_level: 9,
        auxiliary: auxiliary(),
    };
    boundary("woff1 encode", |l| woff1::encode(FONT, &o, l));
    let encoded = woff1::encode(FONT, &o, &Limits::default()).unwrap();
    boundary("woff1 decode", |l| woff1::decode(&encoded.bytes, l));
}

#[test]
fn woff2_resource_failures_stay_resource_limits() {
    for transform in [true, false] {
        let o = woff2::Options {
            quality: 5,
            transform_glyf: transform,
            transform_hmtx: transform,
            auxiliary: auxiliary(),
            ..Default::default()
        };
        boundary("woff2 encode", |l| woff2::encode(FONT, &o, l));
        let encoded = woff2::encode(FONT, &o, &Limits::default()).unwrap();
        boundary("woff2 decode", |l| woff2::decode(&encoded.bytes, l));
    }
}

#[cfg(feature = "ttc")]
#[test]
fn woff2_collection_resource_failures_stay_resource_limits() {
    let (ttc, _) =
        fontpixel::sfnt::collection::encode(&[FONT, FONT], 0x10000, &Default::default()).unwrap();
    let o = woff2::Options {
        quality: 5,
        ..Default::default()
    };
    boundary("woff2 collection encode", |l| woff2::encode(&ttc, &o, l));
    let encoded = woff2::encode(&ttc, &o, &Limits::default()).unwrap();
    boundary("woff2 collection decode", |l| {
        woff2::decode(&encoded.bytes, l)
    });
}
