//! Seeded proptest round trips over freshly authored models, not cached BDF bytes.
#![cfg(feature = "bdf")]
use fontpixel::bitmap::{BitmapLimits, Bounds, Mono};
use fontpixel::formats::bdf::{self, BdfWriteOptions};
use fontpixel::model::{Codepoint, Rational, Vector};
use fontpixel::{Bitmap, Font, Glyph, ResourceLimits};
use proptest::prelude::*;
proptest! {
    #![proptest_config(ProptestConfig {cases: 512, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Direct("tests/bdf_properties.proptest-regressions"))), rng_seed: proptest::test_runner::RngSeed::Fixed(0xbdf20261002), ..ProptestConfig::default()})]
    #[test]
    fn authored_model_roundtrip(width in 0_u32..65, height in 0_u32..33, x in -20_i64..21, y in -20_i64..21,
        advance in -50_i64..51, seed in any::<u64>(), codes in prop::collection::btree_set(0_u32..0x7fffffff,0..12), family in "[a-zA-Z0-9\" ]{0,40}") {
        let mut font=Font::default();font.metrics.point_size=Some(Rational::integer(16));font.metrics.resolution=Some(Vector::new(75,100));font.metrics.bounds=Some(Bounds::new(width,height,x,y));font.names.family=Some(family.clone());
        let mut rng=seed;
        for code in codes {
            let mut bytes=vec![0_u8;(width as usize).div_ceil(8)*height as usize];
            for byte in &mut bytes {rng=rng.wrapping_mul(6364136223846793005).wrapping_add(1);*byte=(rng>>32) as u8;}
            let bitmap=Bitmap::<Mono>::from_raw(width,height,&bytes,BitmapLimits::default()).unwrap();
            let mut glyph=Glyph::new(bitmap);glyph.codepoints.push(Codepoint::Unicode(code));glyph.name=Some(format!("code-{code}"));glyph.metrics.bounds.x=x;glyph.metrics.bounds.y=y;glyph.metrics.advance=Some(Vector::new(advance,0));glyph.metrics.vertical_origin=Some(Vector::new(x,y));font.add_glyph(glyph).unwrap();
        }
        let (bytes,report)=bdf::write(&font,&BdfWriteOptions::default()).unwrap();prop_assert!(!report.has_loss());
        let (again,_)=bdf::read(&bytes,&ResourceLimits::default()).unwrap();prop_assert_eq!(again.metrics.bounds,font.metrics.bounds);prop_assert_eq!(&again.names.family,&font.names.family);prop_assert_eq!(again.len(),font.len());
        for (_,glyph) in font.glyphs() {let found=again.glyph_for_codepoint(glyph.codepoints[0]).unwrap();prop_assert_eq!(&found.bitmap,&glyph.bitmap);prop_assert_eq!(&found.metrics,&glyph.metrics);prop_assert_eq!(&found.name,&glyph.name);}
        prop_assert_eq!(bdf::write(&again,&BdfWriteOptions::default()).unwrap().0,bytes);
    }
}
