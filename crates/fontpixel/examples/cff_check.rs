//! Independent CFF1 integration driver for external interoperability tests.
use fontpixel::{
    Bitmap,
    outline::{Options, Shape},
    sfnt::{
        Sfnt, SfntBuilder,
        cff::{Cff, Command, DotOptions, Font, Glyph, Limits, Outline},
    },
};
use serde_json::Value;
use std::{io::Write, path::Path};
// Normalized full-SC dots expand shared subroutines to >64 MiB. The test
// driver opts into a larger table envelope; production defaults stay unchanged.
fn limits() -> Limits {
    let mut l = Limits::default();
    l.sfnt.resources.max_input_bytes = 256 * 1024 * 1024;
    l.sfnt.resources.max_table_bytes = 256 * 1024 * 1024;
    l
}
fn path_json(out: &mut impl Write, commands: &[Command]) -> std::io::Result<()> {
    out.write_all(b"[")?;
    for (i, c) in commands.iter().enumerate() {
        if i != 0 {
            out.write_all(b",")?;
        }
        match *c {
            Command::MoveTo((x, y)) => write!(out, "[\"moveTo\",[{x},{y}]]")?,
            Command::LineTo((x, y)) => write!(out, "[\"lineTo\",[{x},{y}]]")?,
            Command::CurveTo((a, b), (c, d), (x, y)) => {
                write!(out, "[\"curveTo\",[{a},{b}],[{c},{d}],[{x},{y}]]")?
            }
            Command::Close => out.write_all(b"[\"closePath\"]")?,
        }
    }
    out.write_all(b"]")
}
fn write(source: &Sfnt<'_>, font: &Font, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let l = limits();
    let encoded = font.encode(&l)?;
    let mut builder = SfntBuilder::new(source.flavor());
    let mut head = source.table(*b"head").ok_or("head")?.to_vec();
    for (i, v) in encoded.bounds.iter().enumerate() {
        head[36 + i * 2..38 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    for table in source.tables() {
        if ![*b"head", *b"maxp", *b"CFF "].contains(&table.tag) {
            builder.insert(table.tag, table.data.to_vec())?;
        }
    }
    builder.insert(*b"head", head)?;
    builder.insert(*b"maxp", encoded.maxp)?;
    builder.insert(*b"CFF ", encoded.data)?;
    let out = builder.build(&l.sfnt)?;
    Sfnt::parse(&out, &l.sfnt)?.validate_checksums()?;
    std::fs::write(output, out)?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        return Err("usage: cff_check --inspect-stream FONT | --repack FONT OUTPUT | --generate FONT RECORDS SHAPE OUTPUT".into());
    }
    let data = std::fs::read(&args[2])?;
    let l = limits();
    let source = Sfnt::parse(&data, &l.sfnt)?;
    let table = Cff::from_sfnt(&source, &l)?;
    if args[1] == "--inspect-stream" {
        let mut out = std::io::BufWriter::new(std::io::stdout().lock());
        for gid in 0..table.len() {
            let g = table.glyph(gid as u16, &l)?;
            write!(
                &mut out,
                "{{\"gid\":{gid},\"advance\":{},\"commands\":",
                g.advance
            )?;
            path_json(&mut out, &g.commands)?;
            out.write_all(b",\"id\":")?;
            serde_json::to_writer(&mut out, &table.identifier(gid as u16))?;
            out.write_all(b",\"name\":")?;
            serde_json::to_writer(
                &mut out,
                &table
                    .glyph_name(gid as u16)
                    .and_then(|v| std::str::from_utf8(v).ok()),
            )?;
            out.write_all(b"}\n")?;
        }
        out.flush()?;
        return Ok(());
    }
    if args[1] == "--extract" {
        std::fs::write(args.get(3).ok_or("output")?, table.data())?;
        return Ok(());
    }
    let units = u16::from_be_bytes(source.table(*b"head").ok_or("head")?[18..20].try_into()?);
    let glyphs = table
        .read_all(&l)?
        .into_iter()
        .enumerate()
        .map(|(id, g)| Glyph {
            name: table
                .glyph_name(id as u16)
                .and_then(|n| std::str::from_utf8(n).ok())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if id == 0 {
                        ".notdef".into()
                    } else {
                        format!("cid{id}")
                    }
                }),
            advance: g.advance,
            outline: Outline::Path(g.commands),
        })
        .collect();
    let mut font = Font {
        postscript_name: "FontPixel-Crosscheck".into(),
        full_name: "FontPixel Crosscheck".into(),
        units_per_em: units,
        glyphs,
        dots: DotOptions::default(),
    };
    if args[1] == "--repack" {
        return write(&source, &font, Path::new(args.get(3).ok_or("output")?));
    }
    if args[1] != "--generate" || args.len() != 6 {
        return Err("invalid arguments".into());
    }
    let records: Value = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let shape = match args[4].as_str() {
        "square" => Shape::Square,
        "dot" => Shape::Dot,
        "separate" => Shape::SquareSeparate,
        _ => return Err("shape".into()),
    };
    let glyphs = records["glyphs"].as_array().ok_or("glyphs")?;
    assert_eq!(glyphs.len(), font.glyphs.len());
    for (id, g) in glyphs.iter().enumerate() {
        if g.is_null() {
            continue;
        }
        let rows = g["rows"].as_array().ok_or("rows")?;
        let h = rows.len() as u32;
        let w = rows.first().and_then(Value::as_str).map_or(0, str::len) as u32;
        let mut bitmap = Bitmap::new(w, h)?;
        for (y, row) in rows.iter().enumerate() {
            for (x, b) in row.as_str().ok_or("row")?.bytes().enumerate() {
                if b == b'#' {
                    bitmap.set_pixel(x as u32, y as u32, 1)?;
                }
            }
        }
        let o = Options {
            shape,
            units_per_pixel: 100,
            dot_size: if shape == Shape::Square { 1. } else { 0.9 },
            origin: (
                g["x"].as_i64().ok_or("x")? as i32,
                g["y"].as_i64().ok_or("y")? as i32,
            ),
        };
        font.glyphs[id] = Glyph::from_bitmap(
            font.glyphs[id].name.clone(),
            g["adv"].as_f64().ok_or("advance")? * 100.,
            &bitmap,
            &o,
            &l,
        )?;
    }
    write(&source, &font, Path::new(&args[5]))?;
    Ok(())
}
