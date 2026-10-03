//! W2.4 independent-tool bridge. No oracle runs inside the production codec.
use fontpixel::{ReadOptions, formats::embedded, sfnt::bitmap::Kind};
use std::{env, fs};
fn kind(s: &str) -> Kind {
    match s {
        "otb" => Kind::OpenType,
        "bdat" => Kind::Apple,
        "color" => Kind::Color,
        _ => panic!("unknown bitmap family"),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = env::args().collect();
    match a[1].as_str() {
        "write" => {
            let mut set = fontpixel::FontSet::default();
            for path in &a[4..] {
                set.fonts.extend(
                    fontpixel::read(
                        &fs::read(path)?,
                        &ReadOptions {
                            format: Some("bdf".into()),
                            ..Default::default()
                        },
                    )?
                    .fonts
                    .fonts,
                );
            }
            let (bytes, report) = embedded::write(&set, kind(&a[2]), &Default::default())?;
            fs::write(&a[3], bytes)?;
            eprintln!("{} reported losses", report.entries.len());
        }
        "read" => {
            let (set, report) =
                embedded::read(&fs::read(&a[3])?, kind(&a[2]), &Default::default())?;
            let fonts:Vec<_>=set.fonts.iter().map(|f|{
                let glyphs:Vec<_>=f.glyphs().map(|(_,g)|serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"name":g.name,"w":g.bitmap.width(),"h":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|[v.x,v.y]),"vertical":g.metrics.vertical_advance.map(|v|[v.x,v.y]),"origin":g.metrics.vertical_origin.map(|v|[v.x,v.y]),"mono":matches!(g.bitmap,fontpixel::model::GlyphBitmap::Mono(_)),"pixels":g.bitmap.as_bytes()})).collect();
                serde_json::json!({"ppem":[f.metrics.ppem_x,f.metrics.ppem_y],"ascent":f.metrics.ascent,"descent":f.metrics.descent,"glyphs":glyphs})
            }).collect();
            fs::write(
                &a[4],
                serde_json::to_vec(
                    &serde_json::json!({"fonts":fonts,"report":report.entries.iter().map(|r|&r.code).collect::<Vec<_>>()}),
                )?,
            )?;
        }
        "repack" => {
            let (set, _) = embedded::read(&fs::read(&a[3])?, kind(&a[2]), &Default::default())?;
            let (bytes, _) = embedded::write(&set, kind(&a[2]), &Default::default())?;
            fs::write(&a[4], bytes)?;
        }
        _ => panic!(
            "write family output input.bdf... | read family input output.json | repack family input output"
        ),
    }
    Ok(())
}
