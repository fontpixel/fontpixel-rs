use super::*;
use std::fmt::Write;
fn line_text(s: &str) -> Result<&str> {
    if !s.is_ascii() || s.bytes().any(|c| c == b'\n' || c == b'\r' || c == 0) {
        Err(invalid("SFD names must be single-line ASCII"))
    } else {
        Ok(s)
    }
}
fn slot(g: &Glyph, default: usize) -> Result<i64> {
    match g.properties.get("sfd.slot") {
        Some(PropertyValue::Integer(n)) if *n >= 0 => Ok(*n),
        _ => i64::try_from(default).map_err(invalid),
    }
}
pub(super) fn write(set: &FontSet, o: &SfdWriteOptions) -> Result<(Vec<u8>, Report)> {
    set.validate(&o.limits)?;
    let source_bytes = set.fonts.iter().try_fold(
        fidelity::properties_bytes(&set.properties)?
            .saturating_add(set.source_format.as_ref().map_or(0, String::capacity))
            .saturating_add(
                set.fonts
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Font>()),
            ),
        |n, f| Ok::<_, Error>(n.saturating_add(fidelity::model_bytes(f)?)),
    )?;
    let mut budget = Budget::new(source_bytes, &o.limits)?;
    budget.add(
        set.fonts
            .iter()
            .map(Font::len)
            .sum::<usize>()
            .saturating_mul(128),
    )?;
    let mut estimate = 4096usize;
    for f in &set.fonts {
        for name in [
            &f.names.postscript,
            &f.names.full,
            &f.names.family,
            &f.names.style,
            &f.names.version,
            &f.names.copyright,
        ] {
            estimate = estimate
                .saturating_add(name.as_ref().map_or(0, String::len))
                .saturating_add(64);
        }
        for p in f
            .properties
            .iter()
            .filter(|p| p.name.starts_with("sfd.bdf."))
        {
            if let PropertyValue::Text(value) = &p.value {
                estimate = estimate
                    .saturating_add(value.len())
                    .saturating_add(p.name.len())
                    .saturating_add(64);
            }
        }
        let gray = f
            .glyphs()
            .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_)));
        for (_, g) in f.glyphs() {
            let raster = if gray {
                u64::from(g.bitmap.width()) * u64::from(g.bitmap.height())
            } else {
                g.bitmap.as_bytes().len() as u64
            };
            estimate = estimate
                .saturating_add(512)
                .saturating_add(g.name.as_ref().map_or(0, String::len))
                .saturating_add(g.codepoints.len().saturating_mul(32))
                .saturating_add(usize::try_from(raster).map_err(invalid)?.saturating_mul(2));
        }
    }
    budget.add(estimate)?;
    budget.temporary(4096)?;
    let first = set.fonts.first().ok_or_else(|| invalid("empty font set"))?;
    let mut s = String::with_capacity(estimate);
    s.push_str("SplineFontDB: 3.0\n");
    for (key, value) in [
        ("FontName", &first.names.postscript),
        ("FullName", &first.names.full),
        ("FamilyName", &first.names.family),
        ("Weight", &first.names.style),
        ("Version", &first.names.version),
        ("Copyright", &first.names.copyright),
    ] {
        if let Some(v) = value {
            writeln!(s, "{key}: {}", line_text(v)?).map_err(invalid)?;
        }
    }
    let mut identities = BTreeMap::<i64, (&Glyph, usize)>::new();
    for f in &set.fonts {
        for (id, g) in f.glyphs() {
            let n = slot(g, id.index())?;
            if let Some((old, _)) = identities.get(&n) {
                if old.codepoints != g.codepoints || old.name != g.name {
                    return Err(invalid("strike glyph identities disagree at a shared slot"));
                }
            } else {
                identities.insert(n, (g, id.index()));
            }
        }
    }
    let slots = identities.last_key_value().map_or(Ok(0), |(n, _)| {
        n.checked_add(1).ok_or_else(|| invalid("slot overflow"))
    })?;
    bound(
        usize::try_from(slots).map_err(invalid)?,
        o.limits.max_glyphs,
        "SFD potential slots",
    )?;
    let encoding_slots = identities.values().try_fold(0i64, |maximum, (g, _)| {
        let local = match g.properties.get("sfd.encoding") {
            Some(PropertyValue::Integer(n)) => *n,
            _ => g.codepoints.first().map_or(-1, |v| i64::from(v.value())),
        };
        Ok::<_, Error>(
            maximum.max(
                local
                    .checked_add(1)
                    .ok_or_else(|| invalid("encoding slot overflow"))?,
            ),
        )
    })?;
    let header_number = |key: &str, default: i64| -> Result<i64> {
        match first.headers.get(key) {
            Some(PropertyValue::Text(s)) => s.parse().map_err(invalid),
            Some(PropertyValue::Integer(n)) => Ok(*n),
            None => Ok(default),
            _ => Err(invalid("invalid outline metric property")),
        }
    };
    writeln!(
        s,
        "Ascent: {}\nDescent: {}\nEncoding: UnicodeFull\nOnlyBitmaps: 1\nBeginChars: {} {}",
        header_number(
            "Ascent",
            first
                .metrics
                .ascent
                .unwrap_or_else(|| i64::from(first.metrics.pixel_size.unwrap_or(0)))
        )?,
        header_number("Descent", first.metrics.descent.unwrap_or(0))?,
        encoding_slots.max(slots).max(0x110000),
        identities.len()
    )
    .map_err(invalid)?;
    for (slot, (g, index)) in &identities {
        let generated = format!("glyph{index}");
        let name = line_text(g.name.as_deref().unwrap_or(&generated))?;
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return Err(invalid("glyph name contains whitespace"));
        }
        let uni = g
            .codepoints
            .iter()
            .find_map(|v| {
                if let Codepoint::Unicode(n) = v {
                    Some(i64::from(*n))
                } else {
                    None
                }
            })
            .unwrap_or(-1);
        let local = match g.properties.get("sfd.encoding") {
            Some(PropertyValue::Integer(n)) => *n,
            _ => g.codepoints.first().map_or(-1, |v| i64::from(v.value())),
        };
        writeln!(
            s,
            "StartChar: {name}\nEncoding: {local} {uni} {slot}\nWidth: {}",
            match g.properties.get("sfd.outline_width") {
                Some(PropertyValue::Integer(n)) => *n,
                _ => g.metrics.advance.unwrap_or_default().x,
            }
        )
        .map_err(invalid)?;
        if let Some(PropertyValue::Integer(n)) = g.properties.get("sfd.outline_vwidth") {
            writeln!(s, "VWidth: {n}").map_err(invalid)?;
        }
        let aliases = g.codepoints.iter().filter_map(|v| match v {
            Codepoint::Unicode(n) if i64::from(*n) != uni => Some(*n),
            _ => None,
        });
        if aliases.clone().next().is_some() {
            s.push_str("AltUni2:");
            for cp in aliases {
                write!(s, " {cp:x}.ffffffff.0").map_err(invalid)?;
            }
            s.push('\n');
        }
        s.push_str("EndChar\n");
        bound(s.len(), o.limits.max_input_bytes, "SFD output")?;
    }
    s.push_str("EndChars\n");
    for f in &set.fonts {
        let gray = f
            .glyphs()
            .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_)));
        let native_depth = match f.properties.get("sfd.depth") {
            Some(PropertyValue::Integer(n)) if [2, 4, 8].contains(n) => *n as usize,
            _ => 8,
        };
        let quantum = 255 / ((1usize << native_depth) - 1);
        let fits = f.glyphs().all(|(_, g)| match &g.bitmap {
            GlyphBitmap::Gray8(b) => b
                .as_bytes()
                .iter()
                .all(|&v| (v as usize).is_multiple_of(quantum)),
            _ => true,
        });
        let depth = if !gray {
            1
        } else if fits {
            native_depth
        } else {
            8
        };
        let pixel = f.metrics.pixel_size.unwrap_or_else(|| {
            f.metrics
                .ascent
                .unwrap_or(0)
                .checked_add(f.metrics.descent.unwrap_or(0))
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        });
        writeln!(
            s,
            "BitmapFont: {pixel} {slots} {} {} {depth}",
            f.metrics.ascent.unwrap_or(i64::from(pixel)),
            f.metrics.descent.unwrap_or(0)
        )
        .map_err(invalid)?;
        let properties = f
            .properties
            .iter()
            .filter_map(|p| p.name.strip_prefix("sfd.bdf.").map(|k| (k, &p.value)));
        let property_count = f
            .properties
            .iter()
            .filter(|p| p.name.starts_with("sfd.bdf."))
            .count();
        if property_count != 0 {
            writeln!(s, "BDFStartProperties: {property_count}").map_err(invalid)?;
            for (k, v) in properties {
                if k.is_empty() || k.chars().any(char::is_whitespace) {
                    return Err(invalid("invalid BDF property key"));
                }
                if let PropertyValue::Text(v) = v {
                    writeln!(s, "{} {}", line_text(k)?, line_text(v)?).map_err(invalid)?;
                } else {
                    return Err(invalid(
                        "native BDF property must retain type and value text",
                    ));
                }
            }
            s.push_str("BDFEndProperties\n");
        }
        if let Some(v) = f.metrics.resolution {
            writeln!(s, "Resolution: {}", v.x).map_err(invalid)?;
        }
        for (id, g) in f.glyphs() {
            let b = g.metrics.bounds;
            let local = match g.properties.get("sfd.encoding") {
                Some(PropertyValue::Integer(n)) => *n,
                _ => g.codepoints.first().map_or(-1, |v| i64::from(v.value())),
            };
            let xmax =
                b.x.checked_add(i64::from(b.width))
                    .and_then(|n| n.checked_sub(1))
                    .ok_or_else(|| invalid("xmax overflow"))?;
            let ymax =
                b.y.checked_add(i64::from(b.height))
                    .and_then(|n| n.checked_sub(1))
                    .ok_or_else(|| invalid("ymax overflow"))?;
            write!(
                s,
                "BDFChar: {} {local} {} {} {xmax} {} {ymax}",
                slot(g, id.index())?,
                g.metrics.advance.unwrap_or_default().x,
                b.x,
                b.y
            )
            .map_err(invalid)?;
            if let Some(v) = g.metrics.vertical_advance {
                write!(
                    s,
                    " {}",
                    v.y.checked_neg()
                        .ok_or_else(|| invalid("vertical advance overflow"))?
                )
                .map_err(invalid)?;
            }
            s.push('\n');
            let bytes = if gray {
                u64::from(b.width) * u64::from(b.height)
            } else {
                u64::from(b.width).div_ceil(8) * u64::from(b.height)
            };
            let estimate = usize::try_from(bytes).map_err(invalid)?.saturating_mul(2);
            bound(
                s.len().saturating_add(estimate),
                o.limits.max_input_bytes.min(o.limits.max_total_bytes),
                "SFD output",
            )?;
            match (&g.bitmap, depth) {
                (GlyphBitmap::Gray8(b), 8) => ascii85::encode(b.as_bytes(), &mut s),
                (GlyphBitmap::Mono(b), 1) => ascii85::encode(b.as_bytes(), &mut s),
                (bitmap, depth) => {
                    let width = bitmap.width() as usize;
                    let stride = width.saturating_mul(depth).div_ceil(8);
                    let size = stride.saturating_mul(bitmap.height() as usize);
                    budget.temporary(size)?;
                    let mut data = vec![0u8; size];
                    let maximum = (1usize << depth) - 1;
                    for y in 0..if width == 0 {
                        0
                    } else {
                        bitmap.height() as usize
                    } {
                        for x in 0..width {
                            let sample = match bitmap {
                                GlyphBitmap::Gray8(b) => {
                                    b.as_bytes()[y * width + x] as usize * maximum / 255
                                }
                                GlyphBitmap::Mono(b) => {
                                    if b.as_bytes()[y * width.div_ceil(8) + x / 8]
                                        & (0x80 >> (x % 8))
                                        != 0
                                    {
                                        maximum
                                    } else {
                                        0
                                    }
                                }
                            };
                            data[y * stride + x * depth / 8] |=
                                (sample as u8) << (8 - depth - x * depth % 8);
                        }
                    }
                    ascii85::encode(&data, &mut s);
                }
            }
        }
        s.push_str("EndBitmapFont\n");
    }
    s.push_str("EndSplineFont\n");
    let bytes = s.into_bytes();
    drop(identities);
    let held = source_bytes.saturating_add(bytes.capacity());
    let mut read_limits = o.limits.clone();
    read_limits.max_total_bytes = Budget::new(held, &o.limits)?.remaining();
    let (out, mut report) = read(&bytes, &read_limits)?;
    let model = out.fonts.iter().try_fold(0usize, |n, f| {
        Ok::<_, Error>(n.saturating_add(fidelity::model_bytes(f)?))
    })?;
    let remaining = Budget::new(held.saturating_add(model), &o.limits)?.remaining();
    for (i, (a, b)) in set.fonts.iter().zip(&out.fonts).enumerate() {
        audit(a, b, "sfd", i, &mut report, remaining)?;
    }
    if !set.properties.is_empty() || set.source_format != out.source_format {
        bound(
            report.entries.len().saturating_add(1).saturating_mul(512),
            remaining,
            "SFD report",
        )?;
        loss(
            &mut report,
            "sfd",
            "container_metadata",
            0,
            None,
            "Container metadata or source format changed",
        );
    }
    Ok((bytes, report))
}
