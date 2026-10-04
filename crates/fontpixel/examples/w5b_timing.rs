//! W5.B independent-oracle adapter and deterministic read/write timing probe.
use fontpixel::bitmap::{BitmapLimits, Gray8};
use fontpixel::formats::{beos, geos, gpi, oberon, pcgeos, symbos, vtfont, wsfont};
use fontpixel::model::{Codepoint, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
use std::path::Path;
fn read(profile: &str, b: &[u8]) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits)?;
    Ok(match profile.split('-').next().unwrap_or(profile) {
        "beos" => {
            beos::read(
                &mut c,
                &beos::BeosReadOptions {
                    coverage: if profile == "beos-16" {
                        beos::BeosCoverage::FullNibble
                    } else {
                        beos::BeosCoverage::Native8
                    },
                    round_fractional_metrics: true,
                },
            )?
            .0
        }
        "geos" => geos::read(&mut c, &Default::default())?.0,
        "gpi" => gpi::read(&mut c, &Default::default())?.0,
        "pcgeos" => {
            pcgeos::read(
                &mut c,
                &pcgeos::PcgeosReadOptions {
                    round_fractional_metrics: true,
                    ..Default::default()
                },
            )?
            .0
        }
        "vtfont" => vtfont::read(&mut c, &Default::default())?.0,
        "oberon" => oberon::read(&mut c, &Default::default())?.0,
        "symbos" => symbos::read(&mut c, &Default::default())?.0,
        "wsfont" => wsfont::read(&mut c, &Default::default())?.0,
        _ => return Err(fontpixel::Error::InvalidInput("unknown W5.B format".into())),
    })
}
fn dump(s: &FontSet) -> serde_json::Value {
    serde_json::Value::Array(s.fonts.iter().map(|f|{let glyphs:Vec<_>=f.glyphs().map(|(_,g)|serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"gray":matches!(g.bitmap,fontpixel::model::GlyphBitmap::Gray8(_)),"bytes":g.bitmap.as_bytes()})).collect();serde_json::json!({"glyphs":glyphs,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"family":f.names.family,"style":f.names.style,"size":f.metrics.pixel_size,"line_height":f.metrics.line_height,"point":f.metrics.point_size.map(|p|[p.numerator(),i64::from(p.denominator())])})}).collect())
}
fn source(profile: &str, large: bool) -> fontpixel::Result<FontSet> {
    let mut f = Font::default();
    f.names.family = Some("Original".into());
    if profile == "pcgeos-styled" || profile == "pcgeos-dbcs" {
        f.names.style = Some("Bold Italic".into());
        f.metrics.point_size = Some(if profile == "pcgeos-styled" {
            fontpixel::model::Rational::new(25, 2)?
        } else {
            fontpixel::model::Rational::integer(12)
        });
    }
    let unicode = profile.starts_with("beos")
        || profile.starts_with("vtfont")
        || profile.contains("utf8")
        || profile == "pcgeos-dbcs";
    let sym = profile.starts_with("symbos");
    let width = if sym && !profile.contains("big") {
        8
    } else if large {
        16
    } else {
        10
    };
    let height = if large {
        if sym && !profile.contains("big") {
            8
        } else {
            16
        }
    } else {
        6
    };
    let codes: Vec<u32> = if profile.starts_with("geos") {
        let mut codes: Vec<u32> = (32..127).collect();
        if profile.contains("utf8") {
            codes.extend([0x80, 0x7ff, 0x800, 0xffff, 0x10000, 0x40000, 0x10ffff]);
        }
        codes
    } else if profile == "pcgeos-dbcs" {
        (0x3400..0x3410).collect()
    } else if sym {
        if profile.contains("extended") {
            (1..256).collect()
        } else {
            (32..128).collect()
        }
    } else if large {
        (32..256).collect()
    } else {
        vec![65, 66, 67]
    };
    for code in codes {
        let mut bm = Bitmap::<fontpixel::bitmap::Mono>::new(width, height)?;
        for y in 0..height {
            for x in 0..width {
                let v = if code == 66 {
                    0
                } else {
                    (((code * 37 + y * 11 + x * 3) >> (x % 5)) & 1) as u8
                };
                bm.set_pixel(x, y, v)?;
            }
        }
        let mut g = if profile.contains("alpha") {
            let values: Vec<_> = (0..width * height)
                .map(|i| ((i * 17 + code) % 256) as u8)
                .collect();
            Glyph::new(Bitmap::<Gray8>::from_raw(
                width,
                height,
                &values,
                BitmapLimits::default(),
            )?)
        } else if profile.starts_with("beos") {
            let max = if profile == "beos-16" { 15 } else { 7 };
            let values: Vec<_> = (0..width * height)
                .map(|i| (((i + code) % (max + 1) * 255 + max / 2) / max) as u8)
                .collect();
            Glyph::new(Bitmap::<Gray8>::from_raw(
                width,
                height,
                &values,
                BitmapLimits::default(),
            )?)
        } else {
            Glyph::new(bm)
        };
        g.metrics.advance = Some(Vector::new(i64::from(width), 0));
        if profile.starts_with("pcgeos")
            || profile == "gpi"
            || profile == "gpi-ne"
            || profile == "gpi-lx"
        {
            g.metrics.bounds.x = -2;
            g.metrics.bounds.y = -1;
        }
        g.codepoints.push(if unicode {
            Codepoint::Unicode(code)
        } else {
            Codepoint::Legacy(code)
        });
        f.add_glyph(g)?;
    }
    Ok(f.into())
}
fn write(s: &FontSet, profile: &str) -> fontpixel::Result<Vec<u8>> {
    let w = WriteOptions::new(ResourcePath::new("font")?);
    let out = match profile.split('-').next().unwrap_or(profile) {
        "beos" => beos::write(
            s,
            &w,
            &beos::BeosWriteOptions {
                coverage: Some(if profile == "beos-16" {
                    beos::BeosCoverage::FullNibble
                } else {
                    beos::BeosCoverage::Native8
                }),
                round_fractional_metrics: true,
                ..Default::default()
            },
        )?,
        "vtfont" => vtfont::write(
            s,
            &w,
            &vtfont::VtfontWriteOptions {
                preserve_native: profile != "vtfont-rebuild",
                ..Default::default()
            },
        )?,
        "geos" => geos::write(
            s,
            &w,
            &geos::GeosWriteOptions {
                container: Some(if profile == "geos-bare" {
                    geos::GeosContainer::Bare
                } else {
                    geos::GeosContainer::Cvt
                }),
                variant: Some(match profile {
                    "geos-extended" => geos::GeosVariant::Extended,
                    "geos-utf8" => geos::GeosVariant::Utf8,
                    "geos-mega" => geos::GeosVariant::Mega,
                    "geos-mega-utf8" => geos::GeosVariant::MegaUtf8,
                    _ => geos::GeosVariant::Standard,
                }),
                ..Default::default()
            },
        )?,
        "gpi" => gpi::write(
            s,
            &w,
            &gpi::GpiWriteOptions {
                container: Some(if profile.contains("-ne") {
                    gpi::GpiContainer::Ne
                } else if profile.contains("-lx") {
                    gpi::GpiContainer::Lx
                } else {
                    gpi::GpiContainer::Bare
                }),
                kind: Some(if profile.contains("fixed") {
                    gpi::GpiKind::Fixed
                } else if profile.contains("prop") {
                    gpi::GpiKind::Proportional
                } else {
                    gpi::GpiKind::Abc
                }),
                ..Default::default()
            },
        )?,
        "pcgeos" => pcgeos::write(
            s,
            &w,
            &pcgeos::PcgeosWriteOptions {
                variant: Some(if profile == "pcgeos-dbcs" {
                    pcgeos::PcgeosVariant::Dbcs
                } else {
                    pcgeos::PcgeosVariant::Sbcs
                }),
                round_fractional_metrics: true,
                ..Default::default()
            },
        )?,
        "oberon" => oberon::write(s, &w, &Default::default())?,
        "symbos" => symbos::write(
            s,
            &w,
            &symbos::SymbosWriteOptions {
                version: Some(if profile.contains("big") {
                    symbos::SymbosVersion::Big
                } else if profile.contains("small") {
                    symbos::SymbosVersion::Small
                } else {
                    symbos::SymbosVersion::Medium
                }),
                extended: if profile.contains("extended") {
                    Some(symbos::SymbosExtendedInfo::WritingSystem(2))
                } else {
                    None
                },
                ..Default::default()
            },
        )?,
        "wsfont" => wsfont::write(
            s,
            &w,
            &wsfont::WsfontWriteOptions {
                bit_order: Some(if profile.contains("rbit") {
                    wsfont::WsfontOrder::RightToLeft
                } else {
                    wsfont::WsfontOrder::LeftToRight
                }),
                byte_order: Some(if profile.contains("rbyte") {
                    wsfont::WsfontOrder::RightToLeft
                } else {
                    wsfont::WsfontOrder::LeftToRight
                }),
                ..Default::default()
            },
        )?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown W5.B format".into())),
    };
    out.output.into_single()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() < 3 {
        return Err("usage: emit DIR | dump PROFILE FILE | rewrite PROFILE INPUT OUTPUT | bench PROFILE FILE".into());
    }
    match a[1].as_str() {
        "emit" => {
            let p = Path::new(&a[2]);
            std::fs::create_dir_all(p)?;
            for profile in [
                "beos",
                "beos-16",
                "oberon",
                "symbos",
                "symbos-small",
                "symbos-big",
                "symbos-big-extended",
                "symbos-small-extended",
                "wsfont",
                "wsfont-rbit",
                "wsfont-rbyte",
                "wsfont-rbit-rbyte",
                "wsfont-alpha",
                "vtfont",
                "geos-bare",
                "geos",
                "geos-extended",
                "geos-utf8",
                "geos-mega",
                "geos-mega-utf8",
                "gpi-fixed",
                "gpi-prop",
                "gpi",
                "gpi-ne",
                "gpi-lx",
                "pcgeos",
                "pcgeos-styled",
                "pcgeos-dbcs",
            ] {
                let s = source(profile, false)?;
                let b = write(&s, profile)?;
                std::fs::write(p.join(format!("{profile}.bin")), &b)?;
                std::fs::write(
                    p.join(format!("{profile}.source.json")),
                    serde_json::to_vec(&dump(&s))?,
                )?;
            }
            for profile in [
                "beos",
                "oberon",
                "symbos-big-extended",
                "wsfont",
                "wsfont-alpha",
                "vtfont",
                "geos",
                "geos-utf8",
                "gpi",
                "gpi-lx",
                "pcgeos",
                "pcgeos-dbcs",
            ] {
                std::fs::write(
                    p.join(format!("{profile}.large.bin")),
                    write(&source(profile, true)?, profile)?,
                )?;
            }
        }
        "dump" => println!("{}", dump(&read(&a[2], &std::fs::read(&a[3])?)?)),
        "rewrite" => {
            let s = read(&a[2], &std::fs::read(&a[3])?)?;
            std::fs::write(&a[4], write(&s, &a[2])?)?;
        }
        "bench" => {
            let b = std::fs::read(&a[3])?;
            let s = read(&a[2], &b)?;
            for _ in 0..3 {
                std::hint::black_box(read(&a[2], &b)?);
                std::hint::black_box(write(&s, &a[2])?);
            }
            let t = std::time::Instant::now();
            for _ in 0..100 {
                std::hint::black_box(read(&a[2], std::hint::black_box(&b))?);
            }
            let read_s = t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            for _ in 0..20 {
                std::hint::black_box(write(std::hint::black_box(&s), &a[2])?);
            }
            println!(
                "{}",
                serde_json::json!({"profile":a[2],"input_bytes":b.len(),"read_iterations":100,"read_seconds":read_s,"write_iterations":20,"write_seconds":t.elapsed().as_secs_f64()})
            );
        }
        _ => return Err("unknown command".into()),
    }
    Ok(())
}
