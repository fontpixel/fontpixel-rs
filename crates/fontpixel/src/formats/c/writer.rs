use super::*;
use std::io::Write;
struct Output {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("C output exceeds byte budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn symbol(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && ![
            "auto",
            "break",
            "case",
            "char",
            "const",
            "continue",
            "default",
            "do",
            "double",
            "else",
            "enum",
            "extern",
            "float",
            "for",
            "goto",
            "if",
            "inline",
            "int",
            "long",
            "register",
            "restrict",
            "return",
            "short",
            "signed",
            "sizeof",
            "static",
            "struct",
            "switch",
            "typedef",
            "union",
            "unsigned",
            "void",
            "volatile",
            "while",
            "_Alignas",
            "_Alignof",
            "_Atomic",
            "_Bool",
            "_Complex",
            "_Generic",
            "_Imaginary",
            "_Noreturn",
            "_Static_assert",
            "_Thread_local",
        ]
        .contains(&s)
}
fn pixels(g: &Glyph, cell: Bounds, threshold: Option<u8>, l: &ResourceLimits) -> Result<Bitmap> {
    if u64::from(cell.width) * u64::from(cell.height) > l.max_pixels_per_glyph as u64 {
        return Err(Error::ResourceLimit {
            resource: "C raster pixels",
            limit: l.max_pixels_per_glyph,
        });
    }
    if matches!(g.bitmap, GlyphBitmap::Gray8(_)) && threshold.is_none() {
        return Err(invalid("Gray8 C output requires explicit gray_threshold"));
    }
    let stride = (cell.width as usize).div_ceil(8);
    let len = stride
        .checked_mul(cell.height as usize)
        .ok_or_else(|| invalid("C raster size overflow"))?;
    check(len.saturating_mul(2), l.max_total_bytes, "C raster scratch")?;
    let mut bytes = vec![0u8; len];
    let sx = cell
        .x
        .checked_sub(g.metrics.bounds.x)
        .ok_or_else(|| invalid("C source x overflow"))?;
    let sy = g
        .metrics
        .bounds
        .y
        .checked_add(i64::from(g.bitmap.height()))
        .and_then(|v| {
            cell.y
                .checked_add(i64::from(cell.height))
                .and_then(|top| v.checked_sub(top))
        })
        .ok_or_else(|| invalid("C source y overflow"))?;
    for y in 0..if cell.width == 0 { 0 } else { cell.height } {
        let py = sy
            .checked_add(i64::from(y))
            .ok_or_else(|| invalid("C row overflow"))?;
        for x in 0..cell.width {
            let px = sx
                .checked_add(i64::from(x))
                .ok_or_else(|| invalid("C column overflow"))?;
            if px < 0
                || py < 0
                || px >= i64::from(g.bitmap.width())
                || py >= i64::from(g.bitmap.height())
            {
                continue;
            }
            let on = match &g.bitmap {
                GlyphBitmap::Mono(b) => b.get_pixel(px as u32, py as u32)? != 0,
                GlyphBitmap::Gray8(b) => {
                    b.get_pixel(px as u32, py as u32)?
                        >= threshold.ok_or_else(|| {
                            invalid("Gray8 C output requires explicit gray_threshold")
                        })?
                }
            };
            if on {
                bytes[y as usize * stride + x as usize / 8] |= 128 >> (x % 8);
            }
        }
    }
    Bitmap::from_raw(cell.width, cell.height, &bytes, bitmap_limits(l))
}
pub(super) fn write(set: &FontSet, o: &CWriteOptions) -> Result<(Vec<u8>, Report)> {
    let l = &o.limits;
    set.validate(l)?;
    let f = set.single()?;
    if !matches!(o.family, Family::Generic | Family::Gfx) {
        return Err(invalid("C output supports Generic or Gfx"));
    }
    if !symbol(&o.array_name) {
        return Err(invalid(
            "C output array_name must be a non-keyword identifier",
        ));
    }
    check(o.array_name.len(), l.max_table_bytes, "C symbol bytes")?;
    let mut b = Budget::new(0, l)?;
    b.add(o.array_name.len().saturating_mul(8))?;
    if let Some(g) = &o.generic {
        b.add(g.charset.len())?;
    }
    let mut report = Report::default();
    let unicode = o.family == Family::Gfx
        || o.generic.as_ref().map_or_else(
            || {
                f.glyphs().any(|(_, g)| {
                    g.codepoints
                        .iter()
                        .any(|cp| matches!(cp, Codepoint::Unicode(_)))
                })
            },
            |g| g.charset == "unicode",
        );
    let mut selected = BTreeMap::new();
    for (id, g) in f.glyphs() {
        b.glyph(g.bitmap.as_bytes().len(), id.index().saturating_add(1), l)?;
        b.add(g.codepoints.len().saturating_mul(32))?;
        let cp = g.codepoints.iter().find_map(|cp| match (unicode, cp) {
            (true, Codepoint::Unicode(v)) | (false, Codepoint::Legacy(v)) => Some(*v),
            _ => None,
        });
        if let Some(cp) = cp {
            if selected.insert(cp, (id.index(), g)).is_some() {
                loss(
                    &mut report,
                    "write.duplicate_mapping",
                    Some(id.index()),
                    "Later glyph replaced an earlier glyph at the same code",
                );
            }
        } else {
            loss(
                &mut report,
                "write.unmapped",
                Some(id.index()),
                "Glyph has no mapping in the selected output charset and was omitted",
            );
        }
    }
    let first = o
        .range
        .map(|r| r.0)
        .or_else(|| o.generic.as_ref().map(|g| g.first_code))
        .or_else(|| selected.first_key_value().map(|(&cp, _)| cp));
    let last = o
        .range
        .map(|r| r.1)
        .or_else(|| selected.last_key_value().map(|(&cp, _)| cp));
    let (Some(first), Some(last)) = (first, last) else {
        report.push(ReportEntry {
            level: Level::Error,
            impact: Impact::None,
            code: "c.write.no_representable_glyphs".into(),
            location: Location {
                font_index: Some(0),
                ..Default::default()
            },
            message: "No glyph is encodable in the selected C charset; cannot infer output range"
                .into(),
        });
        return Err(Error::ReportedErrors(report));
    };
    let count = last
        .checked_sub(first)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("C output code range is reversed or overflowing"))?
        as usize;
    check(count, l.max_glyphs, "C output glyph count")?;
    for (&cp, (id, _)) in &selected {
        if cp < first || cp > last {
            loss(
                &mut report,
                "write.outside_range",
                Some(*id),
                "Glyph outside the selected output range was omitted",
            );
        }
    }
    b.add(count.saturating_mul(512 + std::mem::size_of::<Glyph>() * 2))?;
    let mut out = Output {
        bytes: Vec::new(),
        max: l
            .max_input_bytes
            .min(l.max_total_bytes.saturating_sub(b.used) / 2),
    };
    let mut generic = o.generic.clone();
    if o.family == Family::Generic {
        let cell = if let Some(g) = &generic {
            cell_size(g, l)?;
            if g.first_code != first {
                return Err(invalid("generic first_code and output range disagree"));
            }
            Bounds::new(g.width, g.height, 0, g.baseline - i64::from(g.height))
        } else {
            let mut bounds = None;
            for (_, g) in selected.range(first..=last).map(|(_, v)| v) {
                let v = g.metrics.bounds;
                if v.width > 0 && v.height > 0 {
                    bounds = Some(match bounds {
                        None => v,
                        Some(old) => union(old, v)?,
                    });
                }
            }
            let mut cell = bounds.unwrap_or(Bounds::new(1, 1, 0, 0));
            cell.width = cell.width.max(1);
            cell.height = cell.height.max(1);
            let charset = if unicode {
                "unicode".into()
            } else {
                f.charset.name.clone().unwrap_or_else(|| "unknown".into())
            };
            generic = Some(ArrayOptions {
                width: cell.width,
                height: cell.height,
                first_code: first,
                charset,
                baseline: cell.y + i64::from(cell.height),
                lsb_first: false,
            });
            cell
        };
        let g = generic
            .as_ref()
            .ok_or_else(|| invalid("missing generic geometry"))?;
        if !g
            .charset
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
        {
            return Err(invalid("C charset name contains unsupported characters"));
        }
        let size = cell_size(g, l)?;
        b.add(size.saturating_mul(4))?;
        out.write_all(b"#include <stdint.h>\n/* fontpixel-c: ")?;
        serde_json::to_writer(&mut out, g).map_err(|e| invalid(e.to_string()))?;
        write!(
            out,
            " */\nstatic const uint8_t {}[{count}][{size}] = {{\n",
            o.array_name
        )?;
        for cp in first..=last {
            out.write_all(b"  {")?;
            if let Some((_, glyph)) = selected.get(&cp) {
                let bitmap = pixels(glyph, cell, o.gray_threshold, l)?;
                for (i, &v) in bitmap.as_bytes().iter().enumerate() {
                    if i > 0 {
                        out.write_all(b", ")?;
                    }
                    write!(
                        out,
                        "0x{:02X}",
                        if g.lsb_first { v.reverse_bits() } else { v }
                    )?;
                }
            } else {
                loss(
                    &mut report,
                    "write.added_empty",
                    None,
                    format!("Added empty glyph at code {cp}"),
                );
                for i in 0..size {
                    if i > 0 {
                        out.write_all(b", ")?;
                    }
                    out.write_all(b"0x00")?;
                }
            }
            writeln!(out, "}}, // 0x{cp:X}")?;
        }
        out.write_all(b"};\n")?;
    } else {
        if last > 65535 {
            return Err(invalid("GFX output range exceeds u16"));
        }
        let mut bytes = Vec::new();
        let mut records = Vec::new();
        b.add(count.saturating_mul(128))?;
        for cp in first..=last {
            if let Some((_, g)) = selected.get(&cp) {
                let bounds = g.metrics.bounds;
                let a = g
                    .metrics
                    .advance
                    .unwrap_or(Vector::new(i64::from(bounds.width), 0));
                let y = bounds
                    .y
                    .checked_add(i64::from(bounds.height))
                    .and_then(i64::checked_neg)
                    .ok_or_else(|| invalid("GFX yOffset overflow"))?;
                if bounds.width > 255
                    || bounds.height > 255
                    || !(0..=255).contains(&a.x)
                    || !(-128..=127).contains(&bounds.x)
                    || !(-128..=127).contains(&y)
                    || bytes.len() > 65535
                {
                    return Err(invalid("GFX output field exceeds documented range"));
                }
                let bitmap = pixels(g, bounds, o.gray_threshold, l)?;
                records.push((bytes.len(), bounds.width, bounds.height, a.x, bounds.x, y));
                let size = (bounds.width as usize * bounds.height as usize).div_ceil(8);
                b.add(size.saturating_mul(2))?;
                let offset = bytes.len();
                bytes.resize(offset + size, 0);
                for py in 0..if bounds.width == 0 { 0 } else { bounds.height } {
                    for px in 0..bounds.width {
                        if bitmap.get_pixel(px, py)? != 0 {
                            let bit = py as usize * bounds.width as usize + px as usize;
                            bytes[offset + bit / 8] |= 128 >> (bit % 8);
                        }
                    }
                }
            } else {
                loss(
                    &mut report,
                    "write.added_empty",
                    None,
                    format!("Added empty glyph at code {cp}"),
                );
                if bytes.len() > 65535 {
                    return Err(invalid("GFX bitmap offset exceeds u16"));
                }
                records.push((bytes.len(), 0, 0, 0, 0, 0));
            }
        }
        let height = f
            .metrics
            .line_height
            .or_else(|| {
                f.metrics
                    .ascent
                    .and_then(|a| f.metrics.descent.and_then(|d| a.checked_add(d)))
            })
            .unwrap_or_else(|| f.metrics.bounds.map_or(1, |b| i64::from(b.height)));
        if !(0..=255).contains(&height) {
            return Err(invalid("GFX yAdvance exceeds u8"));
        }
        writeln!(
            out,
            "#include <stdint.h>\n#include <gfxfont.h>\nstatic const uint8_t {}Bitmaps[] = {{",
            o.array_name
        )?;
        if bytes.is_empty() {
            out.write_all(b"0x00")?;
        }
        for (i, v) in bytes.iter().enumerate() {
            if i > 0 {
                out.write_all(b",")?;
            }
            if i % 16 == 0 {
                out.write_all(b"\n  ")?;
            }
            write!(out, "0x{v:02X}")?;
        }
        writeln!(
            out,
            "\n}};\nstatic const GFXglyph {}Glyphs[{count}] = {{",
            o.array_name
        )?;
        for (offset, w, h, a, x, y) in records {
            writeln!(out, "  {{{offset}, {w}, {h}, {a}, {x}, {y}}},")?;
        }
        writeln!(
            out,
            "}};\nstatic const GFXfont {} = {{ (uint8_t *){}Bitmaps, (GFXglyph *){}Glyphs, {first}, {last}, {height} }};",
            o.array_name, o.array_name, o.array_name
        )?;
    }
    // Independently parse the generated declaration structure before returning it.
    // This also derives the target model used for field-by-field loss reporting.
    let input = crate::Input::new(&out.bytes);
    let options = ReadOptions {
        limits: ResourceLimits {
            max_total_bytes: l.max_total_bytes.saturating_sub(b.used),
            ..l.clone()
        },
        ..Default::default()
    };
    let mut context = ReadContext::new(&input, &options.limits)?;
    let reader = CCodec {
        read_options: CReadOptions {
            family: o.family,
            array_name: Some(o.array_name.clone()),
            generic: None,
        },
        ..Default::default()
    };
    let (back, _) = reader.read(&mut context, &options)?;
    let target = back.single()?;
    compare(f, target, &selected, first, last, &mut report);
    if !set.properties.is_empty() {
        loss(
            &mut report,
            "write.container_properties",
            None,
            "C source has no container property fields",
        );
    }
    Ok((out.bytes, report))
}
fn union(a: Bounds, b: Bounds) -> Result<Bounds> {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + i64::from(a.width)).max(b.x + i64::from(b.width));
    let top = (a.y + i64::from(a.height)).max(b.y + i64::from(b.height));
    let w = right
        .checked_sub(x)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| invalid("C cell width overflow"))?;
    let h = top
        .checked_sub(y)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| invalid("C cell height overflow"))?;
    Ok(Bounds::new(w, h, x, y))
}
fn compare(
    source: &Font,
    target: &Font,
    selected: &BTreeMap<u32, (usize, &Glyph)>,
    first: u32,
    last: u32,
    r: &mut Report,
) {
    let reordered = selected.range(first..=last).any(|(&cp, (id, _))| {
        target
            .glyph_id(Codepoint::Unicode(cp))
            .or_else(|| target.glyph_id(Codepoint::Legacy(cp)))
            .is_some_and(|back| back.index() != *id)
    });
    if source.slot_count() != target.slot_count() || reordered {
        loss(
            r,
            "write.glyph_slots",
            None,
            "C target changed stable glyph slots or removed holes",
        );
    }
    if source.names != target.names {
        loss(
            r,
            "write.names",
            None,
            "C target does not retain font names, copyright or license metadata",
        );
    }
    if source.metrics != target.metrics {
        loss(
            r,
            "write.font_metrics",
            None,
            "C target changed font metrics or default-glyph references",
        );
    }
    if source.charset != target.charset {
        loss(
            r,
            "write.charset",
            None,
            "C target changed charset metadata",
        );
    }
    if source.headers != target.headers || source.properties != target.properties {
        loss(
            r,
            "write.font_properties",
            None,
            "C target does not retain source headers/properties",
        );
    }
    for (&cp, (id, g)) in selected.range(first..=last) {
        let back = target
            .glyph_for_codepoint(Codepoint::Unicode(cp))
            .or_else(|| target.glyph_for_codepoint(Codepoint::Legacy(cp)));
        if let Some(back) = back {
            if g.bitmap != back.bitmap {
                loss(
                    r,
                    "write.pixels",
                    Some(*id),
                    "C target normalized the cell, cropped pixels or reduced grayscale depth",
                );
            }
            if g.metrics != back.metrics {
                loss(
                    r,
                    "write.glyph_metrics",
                    Some(*id),
                    "C target changed glyph bearings/advances or additional metrics",
                );
            }
            if g.codepoints != back.codepoints || g.unicode_sequences != back.unicode_sequences {
                loss(
                    r,
                    "write.mappings",
                    Some(*id),
                    "C target retained one code; aliases/sequences were not representable",
                );
            }
            if g.name != back.name || g.properties != back.properties {
                loss(
                    r,
                    "write.glyph_metadata",
                    Some(*id),
                    "C target does not retain glyph names/properties",
                );
            }
        }
    }
}
