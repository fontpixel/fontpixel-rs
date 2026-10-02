//! Authored-model properties ensure round trips use a genuine writer.
#![cfg(feature = "pcf")]
use fontpixel::bitmap::{BitmapLimits, Bounds, Mono};
use fontpixel::formats::pcf::{self, GlyphPad, Order, PcfWriteOptions, ScanUnit};
use fontpixel::model::{Codepoint, Rational, Vector};
use fontpixel::{Bitmap, Font, Glyph, ResourceLimits};
use proptest::prelude::*;
proptest! {
    #![proptest_config(ProptestConfig {cases:512, rng_seed:proptest::test_runner::RngSeed::Fixed(0x504346), ..ProptestConfig::default()})]
    #[test]
    fn fresh_fonts_roundtrip(w in 0u32..33,h in 0u32..20,x in -12i64..13,y in -12i64..13,raw in prop::collection::vec(any::<u8>(),100),lsb in any::<bool>(),pad in 0u8..4,unit in 0u8..3) {
        let mut f=Font::default();f.metrics.point_size=Some(Rational::integer(10));f.metrics.resolution=Some(Vector::new(75,75));f.charset.registry=Some("ISO10646".into());f.charset.encoding=Some("1".into());
        let len=w.div_ceil(8) as usize*h as usize;let bytes:Vec<_>=(0..len).map(|i|raw[i%raw.len()]).collect();
        for (i,advance) in [4,7].into_iter().enumerate(){
            let mut g=Glyph::new(Bitmap::<Mono>::from_raw(w,h,&bytes,BitmapLimits::default()).unwrap());g.metrics.bounds=Bounds::new(w,h,x,y);g.metrics.advance=Some(Vector::new(advance,0));g.metrics.scalable_advance=Some(Vector::new(advance*100,0));g.name=Some(format!("test{i}"));g.codepoints=vec![Codepoint::Unicode(65+i as u32)];f.add_glyph(g).unwrap();
        }
        let order=if lsb {Order::Lsb}else{Order::Msb};let options=PcfWriteOptions{byte_order:order,bit_order:order,glyph_pad:[GlyphPad::One,GlyphPad::Two,GlyphPad::Four,GlyphPad::Eight][pad as usize],scan_unit:[ScanUnit::One,ScanUnit::Two,ScanUnit::Four][unit as usize],..PcfWriteOptions::default()};
        let bytes=pcf::write(&f,&options).unwrap().0;let (back,_)=pcf::read(&bytes,&ResourceLimits::default()).unwrap();
        for ((_,a),(_,b)) in f.glyphs().zip(back.glyphs()) {prop_assert_eq!(&a.bitmap,&b.bitmap);prop_assert_eq!(&a.metrics,&b.metrics);prop_assert_eq!(&a.name,&b.name);prop_assert_eq!(&a.codepoints,&b.codepoints);}
        prop_assert_eq!(pcf::write(&back,&options).unwrap().0,bytes);
    }
}
