//! Reproducible SVG font projection driver for independent raster comparisons.
use fontpixel::{formats::svg, model::Codepoint};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().collect::<Vec<_>>();
    if a.len() < 3 {
        return Err("INPUT OUTPUT.json [PPEM]".into());
    }
    let mut o = svg::ReadOptions {
        ppem: a.get(3).map(|s| s.parse()).transpose()?,
        ..Default::default()
    };
    if let Some(max) = a.get(4) {
        o.limits.resources.max_input_bytes = max.parse()?;
    }
    let (set, report) = svg::read(&std::fs::read(&a[1])?, &o)?;
    let fonts=set.fonts.iter().map(|f|serde_json::json!({"ppem":f.metrics.pixel_size,"family":f.names.family,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"glyphs":f.glyphs().map(|(_,g)|serde_json::json!({"cps":g.codepoints.iter().filter_map(|c|match c{Codepoint::Unicode(cp)=>Some(cp),_=>None}).collect::<Vec<_>>(),"sequences":g.unicode_sequences,"name":g.name,"bbox":[g.metrics.bounds.width as i64,g.metrics.bounds.height as i64,g.metrics.bounds.x,g.metrics.bounds.y],"advance":g.metrics.advance.map(|v|[v.x,v.y]),"vertical_advance":g.metrics.vertical_advance.map(|v|[v.x,v.y]),"vertical_origin":g.metrics.vertical_origin.map(|v|[v.x,v.y]),"pixels":g.bitmap.as_bytes()})).collect::<Vec<_>>()})).collect::<Vec<_>>();
    std::fs::write(
        &a[2],
        serde_json::to_vec(
            &serde_json::json!({"fonts":fonts,"losses":report.entries.iter().map(|e|&e.code).collect::<Vec<_>>()}),
        )?,
    )?;
    Ok(())
}
