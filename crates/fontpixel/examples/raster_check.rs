//! Reproducible native projection and collection interoperability driver.
use fontpixel::{
    formats::vector_raster as vr,
    model::{Codepoint, PropertyValue},
    sfnt::{self, collection},
    woff,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().collect::<Vec<_>>();
    if a.len() < 4 {
        return Err("read INPUT JSON [PPEM|auto] [outline] | collect OUTPUT INPUT... | extract TTC SFNT [INDEX] | web TTC WOFF2 | unweb WOFF2 TTC".into());
    }
    if a[1] == "collect" {
        let files = a[3..]
            .iter()
            .map(std::fs::read)
            .collect::<std::io::Result<Vec<_>>>()?;
        let refs = files.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let (d, r) = collection::encode(&refs, 0x20000, &Default::default())?;
        std::fs::write(&a[2], d)?;
        eprint!("{r}");
        return Ok(());
    }
    let d = std::fs::read(&a[2])?;
    if a[1] == "extract" {
        let c = collection::Collection::parse(&d, &Default::default())?;
        c.validate_checksums()?;
        let (d, r) = c.extract(
            a.get(4).map(|v| v.parse()).transpose()?.unwrap_or(0),
            &Default::default(),
        )?;
        std::fs::write(&a[3], d)?;
        eprint!("{r}");
        return Ok(());
    }
    if a[1] == "web" {
        let e = woff::woff2::encode(
            &d,
            &woff::woff2::Options {
                quality: 3,
                ..Default::default()
            },
            &Default::default(),
        )?;
        std::fs::write(&a[3], e.bytes)?;
        return Ok(());
    }
    if a[1] == "unweb" {
        let e = woff::woff2::decode(&d, &Default::default())?;
        std::fs::write(&a[3], e.sfnt)?;
        return Ok(());
    }
    let mut o = vr::Options::default();
    if let Some(p) = a.get(4).filter(|p| p.as_str() != "auto") {
        o.ppem = Some(p.parse()?);
    }
    o.prefer_embedded = a.get(5).is_none_or(|s| s != "outline");
    let start = std::time::Instant::now();
    let (s, r) = vr::read(&d, &o)?;
    let elapsed = start.elapsed().as_secs_f64();
    let fonts=s.fonts.iter().map(|f|serde_json::json!({"ppem":f.metrics.pixel_size,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"family":f.names.family,"glyphs":f.glyphs().map(|(_,g)|serde_json::json!({"gid":match g.properties.get("sfnt.glyph_id"){Some(PropertyValue::Integer(gid))=>*gid,_=>-1},"cps":g.codepoints.iter().filter_map(|c|match c{Codepoint::Unicode(cp)=>Some(cp),_=>None}).collect::<Vec<_>>(),"name":g.name,"bbox":[g.metrics.bounds.width as i64,g.metrics.bounds.height as i64,g.metrics.bounds.x,g.metrics.bounds.y],"advance":g.metrics.advance.map(|v|[v.x,v.y]),"vertical_advance":g.metrics.vertical_advance.map(|v|[v.x,v.y]),"vertical_origin":g.metrics.vertical_origin.map(|v|[v.x,v.y]),"pixels":g.bitmap.as_bytes()})).collect::<Vec<_>>()})).collect::<Vec<_>>();
    std::fs::write(
        &a[3],
        serde_json::to_vec(
            &serde_json::json!({"seconds":elapsed,"fonts":fonts,"losses":r.entries.iter().map(|e|&e.code).collect::<Vec<_>>()}),
        )?,
    )?;
    let _ = sfnt::Limits::default();
    Ok(())
}
