//! Independent outline integration driver: replace a reference font's outlines
//! using source bitmap records, or re-encode arbitrary TrueType glyf tables.
use fontpixel::{
    Bitmap,
    outline::{self, Options, Shape},
    sfnt::{
        Sfnt, SfntBuilder,
        glyf::{GlyfTable, Glyph, Limits, LocaFormat},
    },
};
use serde_json::{Value, json};
use std::path::Path;
fn rewrite(source: &[u8], table: GlyfTable, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let l = Limits::default();
    let font = Sfnt::parse(source, &l.sfnt)?;
    let e = table.encode(LocaFormat::Auto, &l)?;
    let mut head = font.table(*b"head").ok_or("missing head")?.to_vec();
    head[50..52].copy_from_slice(&e.index_to_loc_format.to_be_bytes());
    for (i, v) in e.bounds.iter().enumerate() {
        head[36 + i * 2..38 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    let mut maxp = font.table(*b"maxp").ok_or("missing maxp")?.to_vec();
    let s = &e.statistics;
    for (off, value) in [
        (4, table.glyphs.len() as u16),
        (6, s.max_points),
        (8, s.max_contours),
        (10, s.max_composite_points),
        (12, s.max_composite_contours),
        (26, s.max_size_of_instructions),
        (28, s.max_component_elements),
        (30, s.max_component_depth),
    ] {
        maxp[off..off + 2].copy_from_slice(&value.to_be_bytes());
    }
    let mut builder = SfntBuilder::new(font.flavor());
    for t in font.tables() {
        if ![*b"head", *b"maxp", *b"glyf", *b"loca"].contains(&t.tag) {
            builder.insert(t.tag, t.data.to_vec())?;
        }
    }
    for (tag, data) in [
        (*b"head", head),
        (*b"maxp", maxp),
        (*b"glyf", e.glyf),
        (*b"loca", e.loca),
    ] {
        builder.insert(tag, data)?;
    }
    let output = builder.build(&l.sfnt)?;
    let f = Sfnt::parse(&output, &l.sfnt)?;
    f.validate_checksums()?;
    assert_eq!(GlyfTable::from_sfnt(&f, &l)?, table);
    std::fs::write(path, output)?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().collect::<Vec<_>>();
    let l = Limits::default();
    if a.len() < 3 {
        return Err("usage: glyf_check --inspect FONT | --repack FONT OUTPUT | --generate FONT RECORDS SHAPE OUTPUT".into());
    }
    let source = std::fs::read(&a[2])?;
    let f = Sfnt::parse(&source, &l.sfnt)?;
    let mut table = GlyfTable::from_sfnt(&f, &l)?;
    if a[1] == "--inspect-stream" {
        use std::io::Write;
        let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
        for id in 0..table.glyphs.len() {
            let components = if let Glyph::Composite(g) = &table.glyphs[id] {
                g.components.len()
            } else {
                0
            };
            serde_json::to_writer(
                &mut stdout,
                &json!({"gid": id, "contours": table.contours(id as u16, &l)?, "components": components}),
            )?;
            stdout.write_all(b"\n")?;
        }
        stdout.flush()?;
        return Ok(());
    }
    if a[1] == "--inspect" {
        let outlines = (0..table.glyphs.len())
            .map(|i| table.contours(i as u16, &l))
            .collect::<fontpixel::Result<Vec<_>>>()?;
        println!(
            "{}",
            json!({"contours":outlines,"components":table.glyphs.iter().map(|g|if let Glyph::Composite(g)=g{g.components.len()}else{0}).collect::<Vec<_>>() })
        );
        return Ok(());
    }
    if a[1] == "--repack" {
        return rewrite(&source, table, Path::new(a.get(3).ok_or("missing output")?));
    }
    if a[1] != "--generate" || a.len() != 7 {
        return Err("invalid arguments".into());
    }
    let records: Value = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let shape = match a[4].as_str() {
        "square" => Shape::Square,
        "separate" => Shape::SquareSeparate,
        "dot" => Shape::Dot,
        _ => return Err("invalid shape".into()),
    };
    let dot_id = records["dot_id"].as_u64().map(|n| n as u16);
    let rows = records["glyphs"].as_array().ok_or("missing glyphs")?;
    assert_eq!(rows.len(), table.glyphs.len());
    for (i, g) in rows.iter().enumerate() {
        if g.is_null() {
            continue;
        }
        let row_strings = g["rows"]
            .as_array()
            .ok_or("missing rows")?
            .iter()
            .map(|s| s.as_str().ok_or("invalid row"))
            .collect::<Result<Vec<_>, _>>()?;
        let h = row_strings.len() as u32;
        let w = row_strings.first().map_or(0, |s| s.len()) as u32;
        let mut bitmap = Bitmap::new(w, h)?;
        for (y, row) in row_strings.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c == b'#' {
                    bitmap.set_pixel(x as u32, y as u32, 1)?;
                }
            }
        }
        let o = Options {
            shape,
            units_per_pixel: 100,
            dot_size: if shape == Shape::Square { 1.0 } else { 0.9 },
            origin: (
                g["x"].as_i64().ok_or("x")? as i32,
                g["y"].as_i64().ok_or("y")? as i32,
            ),
        };
        table.glyphs[i] = if shape == Shape::Dot {
            Glyph::pixels(&bitmap, &o, dot_id.ok_or("missing dot")?, &l)?
        } else {
            Glyph::simple(outline::contours(&bitmap, &o, &l.geometry)?)
        };
        let cs = table.contours(i as u16, &l)?;
        assert_eq!(
            outline::rasterize(&cs, 100., o.origin, w, h, &l.geometry)?,
            bitmap
        );
    }
    if let Some(id) = dot_id {
        let o = Options {
            shape: Shape::Dot,
            dot_size: 0.9,
            ..Default::default()
        };
        table.glyphs[id as usize] = Glyph::simple(vec![outline::pixel_contour(&o)?]);
    }
    rewrite(&source, table, Path::new(&a[5]))?;
    // Last argument is a comparison JSON artifact, kept separate from font bytes.
    let encoded = GlyfTable::from_sfnt(&Sfnt::parse(&std::fs::read(&a[5])?, &l.sfnt)?, &l)?;
    std::fs::write(
        &a[6],
        serde_json::to_vec(&json!({"glyphs":encoded.glyphs.len(),"shape":a[4]}))?,
    )?;
    Ok(())
}
