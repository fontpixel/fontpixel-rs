//! Random lossless YAFF golden roundtrips, including source metadata and aliases.
#![cfg(feature = "yaff")]
use fontpixel::bitmap::{BitmapLimits, Gray8, Mono};
use fontpixel::formats::yaff::{self, YaffWriteOptions};
use fontpixel::model::*;
use fontpixel::{Bitmap, Font, FontSet, Glyph, ResourceLimits};
use proptest::prelude::*;
proptest! {
 #![proptest_config(ProptestConfig::with_cases(200))]
 #[test]
 fn exact_native_roundtrip(w in 0_u32..17,h in 0_u32..17,gray in any::<bool>(),x in -20_i64..20,y in -20_i64..20,advance in -20_i64..20,code in any::<u32>(),data in prop::collection::vec(any::<u8>(),289),name in ".{0,50}") {
  let raw=&data[..(w*h) as usize];
  let mut glyph=if gray {Glyph::new(Bitmap::<Gray8>::from_raw(w,h,raw,BitmapLimits::default()).unwrap())}else{
   let mut b=Bitmap::<Mono>::new(w,h).unwrap();for j in 0..h {for i in 0..w {b.set_pixel(i,j,raw[(j*w+i) as usize]&1).unwrap();}}Glyph::new(b)
  };
  glyph.codepoints=vec![Codepoint::Unicode(code),Codepoint::Legacy(code),Codepoint::Unicode(code)];glyph.name=Some(name.clone());glyph.metrics.bounds.x=x;glyph.metrics.bounds.y=y;glyph.metrics.advance=Some(Vector::new(advance,y));glyph.metrics.scalable_advance=Some(Vector::new(x,y));glyph.unicode_sequences=vec![vec![65,66],vec![65]];
  glyph.properties.push(Property::new("random",PropertyValue::Bytes(data)));glyph.properties.push(Property::new("text",name.clone()));
  let mut font=Font::from_glyphs([glyph]).unwrap();font.names.family=Some(name);font.metrics.point_size=Some(Rational::new(x,3).unwrap());
  let set=FontSet::from(font);let (bytes,report)=yaff::write(&set,&YaffWriteOptions::default()).unwrap();prop_assert!(!report.has_loss());let (back,_)=yaff::read(&bytes,&ResourceLimits::default()).unwrap();prop_assert_eq!(&set,&back);
  prop_assert_eq!(bytes,yaff::write(&back,&YaffWriteOptions::default()).unwrap().0);
 }
}
