//! Native table offsets and fixed-point projection with explicit diagnostics.
use super::*;
struct Projection<'a> {
    allow: bool,
    font: usize,
    report: &'a mut Report,
    budget: &'a mut Budget,
}
impl Projection<'_> {
    fn value(&mut self, q: i64, glyph: Option<usize>, field: &str) -> Result<i64> {
        if q % 256 != 0 {
            if !self.allow {
                return Err(invalid(format!(
                    "fractional PC/GEOS {field} requires round_fractional_metrics"
                )));
            }
            self.budget.diagnostic(self.report,"pcgeos","read.fractional_metric",Impact::Repair,Some(self.font),glyph,format!("native {field} {q}/256 projected to nearest integer, ties even; original retained"))?;
        }
        Ok(rounded(q))
    }
}
pub(super) fn parse(
    b: &[u8],
    o: &PcgeosReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 8)?;
    if &b[..4] != b"BSWF" {
        return Err(invalid("not a PC/GEOS BSWF font"));
    }
    check(b.len(), l.max_table_bytes, "PC/GEOS file bytes")?;
    let variant = o.variant.unwrap_or(if b[5] == 2 {
        PcgeosVariant::Dbcs
    } else {
        PcgeosVariant::Sbcs
    });
    if b[5] > 2 {
        return Err(invalid("unknown PC/GEOS BSWF major version"));
    }
    let dbcs = variant == PcgeosVariant::Dbcs;
    let info_len = if dbcs { 57 } else { 33 };
    let info = slice(b, 8, info_len)?;
    let namesize = if dbcs { 40 } else { 20 };
    let family = if dbcs {
        let mut name = String::new();
        for p in (5..45).step_by(2) {
            let cp = word(info, p)?;
            if cp == 0 {
                break;
            }
            name.push(
                char::from_u32(u32::from(cp))
                    .ok_or_else(|| invalid("PC/GEOS family contains a surrogate"))?,
            );
        }
        name
    } else {
        let bytes = &info[5..25];
        latin(&bytes[..bytes.iter().position(|&v| v == 0).unwrap_or(20)])
    };
    let tab = usize::from(word(info, 5 + namesize)?);
    let end = usize::from(word(info, 7 + namesize)?);
    let stride = if dbcs { 9 } else { 10 };
    if tab < info_len + 2 || end < tab || (end - tab) % stride != 0 {
        return Err(invalid("PC/GEOS point table offsets invalid"));
    }
    let header = usize::from(word(b, 6)?);
    if header < info_len || end.saturating_sub(2) > header {
        return Err(invalid("PC/GEOS point table exceeds info block"));
    }
    slice(b, 8, header)?;
    let table = slice(b, tab + 6, end - tab)?;
    let count = table.len() / stride;
    if count == 0 {
        return Err(invalid("PC/GEOS has no bitmap strikes"));
    }
    check(count, l.max_resources, "PC/GEOS strikes")?;
    budget.items::<Font>(count)?;
    budget.add(
        b.len()
            .saturating_mul(2)
            .saturating_add(count.saturating_mul(4096)),
    )?;
    let mut set = FontSet {
        source_format: Some("pcgeos".into()),
        ..Default::default()
    };
    let mut report = Report::default();
    let mut total = 0;
    set.properties.push(Property::new(
        "pcgeos.native",
        PropertyValue::Bytes(b.to_vec()),
    ));
    set.properties.push(Property::new(
        "pcgeos.variant",
        if dbcs { "dbcs" } else { "sbcs" },
    ));
    set.properties.push(Property::new(
        "pcgeos.round_fractional_metrics",
        i64::from(o.round_fractional_metrics),
    ));
    set.properties.push(Property::new(
        "pcgeos.info",
        PropertyValue::Bytes(info.to_vec()),
    ));
    for index in 0..count {
        let entry = &table[index * stride..(index + 1) * stride];
        let size = usize::from(word(entry, if dbcs { 3 } else { 4 })?);
        let pos = dword(entry, if dbcs { 5 } else { 6 })? as usize;
        if pos < 8 + header {
            return Err(invalid("PC/GEOS strike overlaps file metadata"));
        }
        let data = slice(b, pos, size)?;
        let font_header = if dbcs { 70 } else { 71 };
        slice(data, 0, font_header)?;
        if usize::from(word(data, 0)?) != size {
            return Err(invalid("PC/GEOS FontBuf size differs from point entry"));
        }
        let chars = if dbcs { 6 } else { 3 };
        let under = 40 + chars;
        let flags_at = font_header - 3;
        if data[flags_at] & 0x18 != 0 {
            return Err(invalid(
                "PC/GEOS region or generated-outline FontBuf is not a stored bitmap strike",
            ));
        }
        let first = if dbcs {
            u32::from(word(data, 40)?)
        } else {
            u32::from(data[40])
        };
        let last = if dbcs {
            u32::from(word(data, 42)?)
        } else {
            u32::from(data[41])
        };
        if last < first {
            return Err(invalid("PC/GEOS character range reversed"));
        }
        let n = (last - first + 1) as usize;
        let cs = if dbcs { 6 } else { 8 };
        slice(data, font_header, n * cs)?;
        total += n;
        check(total, l.max_glyphs, "PC/GEOS cumulative slots")?;
        budget.items::<Glyph>(n)?;
        budget.add(n.saturating_mul(256))?;
        let mut project = Projection {
            allow: o.round_fractional_metrics,
            font: index,
            report: &mut report,
            budget,
        };
        let mut f = Font::default();
        f.source_format = Some("pcgeos".into());
        f.names.family = Some(family.clone());
        f.names.style = Some(
            match entry[0] & 0x30 {
                0x30 => "Bold Italic",
                0x20 => "Bold",
                0x10 => "Italic",
                _ => "Regular",
            }
            .into(),
        );
        f.charset.name = Some(if dbcs { "unicode" } else { "pc-geos" }.into());
        f.metrics.spacing = Some(if info[4] & 0x40 != 0 {
            Spacing::Monospaced
        } else {
            Spacing::Proportional
        });
        f.metrics.point_size = Some(if dbcs {
            Rational::integer(i64::from(entry[1] as i8))
        } else {
            Rational::new(fixed(entry, 1)?, 256)?
        });
        f.metrics.pixel_size = Some(u32::from(word(data, font_header - 5)?));
        let baseline = fixed(data, 25)?;
        let height = fixed(data, 13)?;
        let descent = fixed(data, 28)?;
        f.metrics.ascent =
            Some(project.value(height - fixed(data, 16)? - descent, None, "ascent")?);
        f.metrics.descent = Some(project.value(descent, None, "descent")?);
        f.metrics.line_gap = Some(project.value(fixed(data, 31)?, None, "line gap")?);
        f.metrics.line_height = Some(project.value(
            height + fixed(data, 31)? + fixed(data, 10)?,
            None,
            "line height",
        )?);
        f.metrics.underline_position =
            Some(project.value(baseline - fixed(data, under)?, None, "underline position")?);
        f.metrics.underline_thickness =
            Some(project.value(fixed(data, under + 3)?, None, "underline thickness")?);
        let default = if dbcs {
            u32::from(word(data, 44)?)
        } else {
            u32::from(data[42])
        };
        f.metrics.default_character = Some(if dbcs {
            Codepoint::Unicode(default)
        } else {
            Codepoint::Legacy(default)
        });
        f.properties
            .push(Property::new("pcgeos.font_id", i64::from(word(info, 0)?)));
        f.properties
            .push(Property::new("pcgeos.style", i64::from(entry[0])));
        if dbcs {
            f.properties
                .push(Property::new("pcgeos.charset", i64::from(entry[2])));
        }
        f.properties.push(Property::new(
            "pcgeos.header",
            PropertyValue::Bytes(data[..font_header].to_vec()),
        ));
        let pairs = usize::from(word(data, 34)?);
        let kp = usize::from(word(data, 36)?);
        let kv = usize::from(word(data, 38)?);
        if pairs > 0 {
            if kp < font_header + n * cs || kv < font_header + n * cs {
                return Err(invalid("PC/GEOS kerning table overlaps descriptors"));
            }
            project.budget.add(pairs.saturating_mul(8))?;
            f.properties.push(Property::new(
                "pcgeos.kern_pairs",
                PropertyValue::Bytes(slice(data, kp, pairs * 2)?.to_vec()),
            ));
            f.properties.push(Property::new(
                "pcgeos.kern_values",
                PropertyValue::Bytes(slice(data, kv, pairs * 2)?.to_vec()),
            ));
        }
        for i in 0..n {
            let p = font_header + i * cs;
            let offset = usize::from(word(data, p)?);
            if offset == 0 {
                continue;
            }
            if offset < font_header + n * cs {
                return Err(invalid("PC/GEOS bitmap overlaps descriptor table"));
            }
            let cd = slice(data, offset, 4)?;
            let width = u32::from(cd[0]);
            let h = u32::from(cd[1]);
            let bytes = width.div_ceil(8) as usize * h as usize;
            check(
                width as usize * h as usize,
                l.max_pixels_per_glyph,
                "PC/GEOS glyph pixels",
            )?;
            project.budget.add(bytes.saturating_mul(2))?;
            let no_data = data[p + 5] & 8 != 0;
            let raw = if no_data {
                vec![0; bytes]
            } else {
                slice(data, offset + 4, bytes.max(1))?[..bytes].to_vec()
            };
            let mut g = Glyph::new(Bitmap::<Mono>::from_raw(width, h, &raw, bitmap_limits(l))?);
            let cp = first + i as u32;
            if dbcs && char::from_u32(cp).is_none() {
                return Err(invalid("PC/GEOS DBCS character is a surrogate"));
            }
            g.codepoints.push(if dbcs {
                Codepoint::Unicode(cp)
            } else {
                Codepoint::Legacy(cp)
            });
            g.metrics.bounds.x = i64::from(cd[3] as i8);
            g.metrics.bounds.y = project.value(
                baseline - (i64::from(cd[2] as i8) + i64::from(h)) * 256,
                Some(i),
                "glyph baseline",
            )?;
            g.metrics.advance = Some(Vector::new(
                project.value(fixed(data, p + 2)?, Some(i), "glyph advance")?,
                0,
            ));
            g.properties
                .push(Property::new("pcgeos.advance_q8", fixed(data, p + 2)?));
            g.properties
                .push(Property::new("pcgeos.flags", i64::from(data[p + 5])));
            if !dbcs {
                g.properties
                    .push(Property::new("pcgeos.usage", i64::from(word(data, p + 6)?)));
            }
            f.add_glyph(g)?;
        }
        f.metrics.default_glyph = f.metrics.default_character.and_then(|cp| f.glyph_id(cp));
        set.fonts.push(f);
    }
    Ok((set, report))
}
