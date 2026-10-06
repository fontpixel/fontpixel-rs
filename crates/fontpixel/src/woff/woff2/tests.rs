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

// ---- parallel Brotli: thread pool versus single-threaded spawner ----

/// Deterministic font-like bytes: mutated glyph bitmaps, metric words and
/// noise, so every parallel chunk and its preceding-input dictionary matter.
fn font_like(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let glyphs: Vec<[u8; 28]> = (0..96)
        .map(|_| {
            let mut glyph = [0u8; 28];
            for byte in &mut glyph {
                *byte = (next() & next() & 0xff) as u8;
            }
            glyph
        })
        .collect();
    let mut out = Vec::with_capacity(len + 28);
    while out.len() < len {
        let r = next();
        match r % 5 {
            0..=2 => {
                let mut glyph = glyphs[(r >> 8) as usize % glyphs.len()];
                glyph[(r >> 16) as usize % 28] ^= 1 << ((r >> 24) % 8);
                out.extend_from_slice(&glyph);
            }
            3 => out.extend_from_slice(&((r >> 4) as u16 % 2048).to_be_bytes()),
            _ => out.push((r >> 24) as u8),
        }
    }
    out.truncate(len);
    out
}
/// The parallel stream through the production helper with brotli's thread
/// pool (native) and its single-threaded spawner (`wasm`), plus brotli's own
/// `compress_multi` (the call the writer made before the helper existed).
fn parallel_streams(data: &[u8], quality: u32, threads: usize) -> [Vec<u8>; 3] {
    assert!(data.len() >= brotli_encode::PARALLEL_MIN_BYTES && threads > 1);
    let l = Limits::default();
    let o = Options {
        quality,
        compression_threads: threads,
        ..Default::default()
    };
    let params = encoder_params(quality, data.len());
    let budget = ledger(&l);
    let pool = compress_parallel(
        data,
        &o,
        &l,
        &budget,
        &params,
        &mut brotli::enc::new_work_pool(threads - 1),
    )
    .unwrap()
    .to_vec();
    let single = compress_parallel(
        data,
        &o,
        &l,
        &budget,
        &params,
        &mut brotli::enc::singlethreading::SingleThreadedSpawner::default(),
    )
    .unwrap()
    .to_vec();
    let mut out =
        vec![0; brotli::enc::encode::BrotliEncoderMaxCompressedSizeMulti(data.len(), threads)];
    let mut allocators = (0..threads)
        .map(|_| {
            brotli::enc::SendAlloc::new(
                EncoderAlloc::default(),
                brotli::enc::backward_references::UnionHasher::Uninit,
            )
        })
        .collect::<Vec<_>>();
    let mut owned = brotli::enc::Owned::new(Bytes(data.to_vec()));
    let n = brotli::enc::compress_multi(&params, &mut owned, &mut out, &mut allocators).unwrap();
    out.truncate(n);
    [pool, single, out]
}
fn assert_spawners_agree(name: &str, data: &[u8], qualities: &[u32], threads: &[usize]) {
    for &quality in qualities {
        for &t in threads {
            let [pool, single, reference] = parallel_streams(data, quality, t);
            assert_eq!(
                pool, reference,
                "{name} q{quality} t{t}: helper vs compress_multi"
            );
            assert_eq!(
                pool, single,
                "{name} q{quality} t{t}: pool vs single-threaded"
            );
            eprintln!(
                "{name}: {} bytes, q{quality} t{t}: {} compressed bytes identical",
                data.len(),
                pool.len()
            );
        }
    }
}
#[test]
fn single_threaded_spawner_matches_the_thread_pool() {
    // Above PARALLEL_MIN_BYTES (128 KiB) the writer splits the stream; below it
    // both targets use one BrotliCompress stream and prove nothing here.
    let data = font_like(192 << 10);
    assert_spawners_agree("192KiB-font-like", &data, &[0, 5, 11], &[2, 3, 4]);
}
/// The WOFF2 compressed block of a single-font file and its raw stream length.
fn compressed_block(woff2: &[u8]) -> (&[u8], usize) {
    let tables = u16::from_be_bytes([woff2[12], woff2[13]]);
    let mut cursor = Cursor::new(woff2);
    cursor.pos = 48;
    let mut size = 0;
    for _ in 0..tables {
        let flags = cursor.byte().unwrap();
        let tag: [u8; 4] = if flags & 63 == 63 {
            cursor.take(4).unwrap().try_into().unwrap()
        } else {
            TAGS[(flags & 63) as usize]
        };
        let transformed = matches!((&tag, flags >> 6), (b"glyf" | b"loca", 0) | (b"hmtx", 1));
        let original = cursor.base128().unwrap() as usize;
        size += if transformed {
            cursor.base128().unwrap() as usize
        } else {
            original
        };
    }
    let len = u32::from_be_bytes(woff2[20..24].try_into().unwrap()) as usize;
    (&woff2[cursor.pos..cursor.pos + len], size)
}
/// Tong Pixel Sans SC (38,653 glyphs) as an OTB sfnt: `FONTPIXEL_TONG_SC_BDF`
/// or the sibling `samples` checkout; `None` (skipped) when neither exists.
#[cfg(all(feature = "bdf", feature = "otb"))]
fn tong_sc_otb() -> Option<Vec<u8>> {
    let path = std::env::var_os("FONTPIXEL_TONG_SC_BDF")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf")
        });
    let Ok(bdf) = std::fs::read(&path) else {
        eprintln!(
            "skipped: {} is absent (set FONTPIXEL_TONG_SC_BDF)",
            path.display()
        );
        return None;
    };
    let registry = crate::registry();
    let loaded = registry
        .read(
            &crate::Input::new(&bdf).with_name("TongPixelSans14Large-SC.bdf"),
            &crate::ReadOptions::default(),
        )
        .unwrap();
    let written = registry
        .write(
            &loaded.fonts,
            "otb",
            &crate::WriteOptions::new(crate::ResourcePath::new("tong.otb").unwrap()),
        )
        .unwrap();
    Some(written.output.assets()[0].bytes.clone())
}
/// The full matrix, run in release:
/// `cargo test --release -p fontpixel --all-features --lib -- --ignored spawner`.
/// A 1 MiB generated stream and the real Tong SC sfnt, threads 2-4, qualities
/// through 11; then the production WOFF2 file of Tong SC, whose compressed
/// block the single-threaded spawner reproduces from its raw stream.
#[test]
#[ignore = "heavy at quality 11; run in release (see doc comment)"]
fn single_threaded_spawner_matches_the_thread_pool_on_large_inputs() {
    let qualities = [0, 2, 5, 9, 10, 11];
    assert_spawners_agree(
        "1MiB-font-like",
        &font_like(1 << 20),
        &qualities,
        &[2, 3, 4],
    );
    #[cfg(all(feature = "bdf", feature = "otb"))]
    if let Some(sfnt) = tong_sc_otb() {
        assert_spawners_agree("Tong-SC-otb-sfnt", &sfnt, &qualities, &[2, 3, 4]);
        let l = Limits::default();
        for threads in [2, 3, 4] {
            let o = Options {
                compression_threads: threads,
                ..Default::default()
            };
            let woff2 = encode(&sfnt, &o, &l).unwrap();
            let (block, size) = compressed_block(&woff2.bytes);
            let budget = ledger(&l);
            let raw = decompress(block, size, &budget).unwrap();
            assert!(raw.len() >= brotli_encode::PARALLEL_MIN_BYTES);
            let single = compress_parallel(
                &raw,
                &o,
                &l,
                &budget,
                &encoder_params(o.quality, raw.len()),
                &mut brotli::enc::singlethreading::SingleThreadedSpawner::default(),
            )
            .unwrap();
            assert_eq!(&single[..], block, "Tong SC WOFF2 q11 t{threads}");
            eprintln!(
                "Tong-SC WOFF2 q11 t{threads}: raw {} bytes, block {} bytes, file {} bytes identical",
                raw.len(),
                block.len(),
                woff2.bytes.len()
            );
        }
    }
}
