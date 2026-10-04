use super::*;
use crate::Error;
use crate::model::GlyphMetrics;
/// Native, deterministic uncompressed PF2 output choices.
#[derive(Debug, Clone)]
pub struct Pf2WriteOptions {
    /// Explicit threshold required for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Input, table, glyph and aggregate limits, also applied to output readback.
    pub limits: ResourceLimits,
    /// Maximum emitted bytes.
    pub max_output_bytes: usize,
}
impl Default for Pf2WriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
fn section(out: &mut Vec<u8>, tag: &[u8], body: &[u8], o: &Pf2WriteOptions) -> Result<()> {
    check(body.len(), o.limits.max_table_bytes, "PF2 output section")?;
    check(
        out.len().saturating_add(8).saturating_add(body.len()),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "PF2 output bytes",
    )?;
    let length =
        u32::try_from(body.len()).map_err(|_| invalid("PF2 section length exceeds u32"))?;
    out.extend(tag);
    out.extend(length.to_be_bytes());
    out.extend(body);
    Ok(())
}
/// Encode Unicode mappings and per-glyph signed metrics without external tools.
pub fn write(f: &Font, o: &Pf2WriteOptions) -> Result<(Vec<u8>, Report)> {
    f.validate(&o.limits)?;
    let mut d = Diagnostics::for_font("pf2", f, &o.limits)?;
    let map = mappings(
        f,
        "unicode",
        |c| char::from_u32(c).is_some() || joined_code(c),
        &mut d,
    )?;
    if map.is_empty() {
        return Err(d.fatal("PF2 output has no representable Unicode mappings"));
    }
    check(map.len(), o.limits.max_glyphs, "PF2 mappings")?;
    check(
        map.len().saturating_mul(9),
        o.limits.max_table_bytes,
        "PF2 CHIX",
    )?;
    d.reserve(
        source_cost(f, &o.limits)?
            .saturating_mul(2)
            .saturating_add(map.len().saturating_mul(64)),
    )?;
    let mut grouped: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for (&cp, id) in &map {
        grouped.entry(id.index()).or_default().push(cp);
    }
    let mut expected = Font::default();
    expected.source_format = Some("pf2".into());
    expected.charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    let mut out = b"FILE\0\0\0\x04PFF2".to_vec();
    let full = f.names.full.clone().or_else(|| Some("Unknown".into()));
    for (tag, value) in [(b"NAME", &full), (b"FAMI", &f.names.family)] {
        if let Some(value) = value {
            if !value.is_ascii() || value.as_bytes().contains(&0) {
                d.loss(
                    "write.name",
                    None,
                    None,
                    "Non-ASCII or NUL-containing PF2 name was omitted",
                )?;
                continue;
            }
            check(
                value.len().saturating_add(1),
                o.limits.max_table_bytes,
                "PF2 name",
            )?;
            let mut b = value.as_bytes().to_vec();
            b.push(0);
            section(&mut out, tag, &b, o)?;
            if tag == b"NAME" {
                expected.names.full = Some(value.clone());
            } else {
                expected.names.family = Some(value.clone());
            }
        }
    }
    if expected.names.full.is_none() {
        section(&mut out, b"NAME", b"Unknown\0", o)?;
        expected.names.full = Some("Unknown".into());
    }
    let fallback_height = f.metrics.pixel_size.unwrap_or_else(|| {
        f.glyphs()
            .map(|(_, g)| g.bitmap.height())
            .max()
            .unwrap_or(0)
    });
    if let Some(point) = f
        .metrics
        .point_size
        .or(Some(Rational::integer(i64::from(fallback_height))))
    {
        let value = point.numerator() / i64::from(point.denominator());
        let value = value.clamp(0, i64::from(u16::MAX)) as u16;
        section(&mut out, b"PTSZ", &value.to_be_bytes(), o)?;
        expected.metrics.point_size = Some(Rational::integer(i64::from(value)));
    }
    let ascent = f.metrics.ascent.or_else(|| {
        Some(
            f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                .max()
                .unwrap_or(0)
                .max(0),
        )
    });
    let descent = f.metrics.descent.or_else(|| {
        Some(
            f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y.saturating_neg())
                .max()
                .unwrap_or(0)
                .max(0),
        )
    });
    for (tag, value) in [(b"ASCE", ascent), (b"DESC", descent)] {
        if let Some(value) = value {
            let n = value.clamp(0, i64::from(u16::MAX)) as u16;
            section(&mut out, tag, &n.to_be_bytes(), o)?;
            if tag == b"ASCE" {
                expected.metrics.ascent = Some(i64::from(n));
            } else {
                expected.metrics.descent = Some(i64::from(n));
            }
        }
    }
    let mut maximum_seen = std::collections::BTreeSet::new();
    for p in f.headers.iter() {
        if let Some(hex) = p.name.strip_prefix("pf2.section.")
            && hex.len() == 8
            && hex.is_ascii()
        {
            let tag = (0..4)
                .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
                .collect::<std::result::Result<Vec<_>, _>>();
            if let (Ok(tag), PropertyValue::Bytes(body)) = (tag, &p.value)
                && !matches!(
                    tag.as_slice(),
                    b"FILE" | b"NAME" | b"FAMI" | b"PTSZ" | b"ASCE" | b"DESC" | b"CHIX" | b"DATA"
                )
                && (!matches!(tag.as_slice(), b"MAXW" | b"MAXH") || body.len() == 2)
            {
                if matches!(tag.as_slice(), b"MAXW" | b"MAXH") && !maximum_seen.insert(tag.clone())
                {
                    continue;
                }
                let maximum = if tag == b"MAXW" {
                    f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0)
                } else if tag == b"MAXH" {
                    f.glyphs()
                        .map(|(_, g)| g.bitmap.height())
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };
                let adjusted;
                let body = if matches!(tag.as_slice(), b"MAXW" | b"MAXH")
                    && maximum > u32::from(u16::from_be_bytes([body[0], body[1]]))
                {
                    adjusted = u16::try_from(maximum)
                        .unwrap_or(u16::MAX)
                        .to_be_bytes()
                        .to_vec();
                    &adjusted
                } else {
                    body
                };
                section(&mut out, &tag, body, o)?;
                expected.headers.push(Property::new(
                    section_key(&tag),
                    PropertyValue::Bytes(body.clone()),
                ));
            }
        }
    }
    for (tag, n) in [
        (
            b"MAXW",
            f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0),
        ),
        (
            b"MAXH",
            f.glyphs()
                .map(|(_, g)| g.bitmap.height())
                .max()
                .unwrap_or(0),
        ),
    ] {
        let key = section_key(tag);
        if expected.headers.get(&key).is_none() {
            let data = u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes();
            section(&mut out, tag, &data, o)?;
            expected
                .headers
                .push(Property::new(key, PropertyValue::Bytes(data.to_vec())));
        }
    }
    let index_at = out.len();
    let index_bytes = vec![0; map.len() * 9];
    section(&mut out, b"CHIX", &index_bytes, o)?;
    check(
        out.len().saturating_add(8),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "PF2 output bytes",
    )?;
    out.extend(b"DATA\xff\xff\xff\xff");
    let mut offsets = BTreeMap::new();

    for (id, g) in f.glyphs() {
        let Some(cps) = grouped.get(&id.index()) else {
            continue;
        };
        let bounds = g.metrics.bounds;
        let advance = g
            .metrics
            .advance
            .unwrap_or(Vector::new(i64::from(bounds.width), 0));
        let fields = (
            u16::try_from(bounds.width),
            u16::try_from(bounds.height),
            i16::try_from(bounds.x),
            i16::try_from(bounds.y),
            i16::try_from(advance.x),
        );
        let (Ok(w), Ok(h), Ok(x), Ok(y), Ok(advance)) = fields else {
            for cp in cps {
                d.loss(
                    "write.omitted",
                    Some(id.index()),
                    Some(*cp),
                    "PF2 glyph dimensions or metrics exceed 16-bit fields",
                )?;
            }
            continue;
        };
        let bitmap = match mono(g, None, o.gray_threshold, id.index(), &mut d) {
            Ok(bitmap) => bitmap,
            Err(error @ Error::ResourceLimit { .. }) => return Err(error),
            Err(error) => {
                for cp in cps {
                    d.loss(
                        "write.omitted",
                        Some(id.index()),
                        Some(*cp),
                        error.to_string(),
                    )?;
                }
                continue;
            }
        };
        let packed = pack(&bitmap);
        check(
            out.len().saturating_add(10).saturating_add(packed.len()),
            o.max_output_bytes.min(o.limits.max_input_bytes),
            "PF2 output bytes",
        )?;

        offsets.insert(
            id.index(),
            u32::try_from(out.len()).map_err(|_| invalid("PF2 offset exceeds u32"))?,
        );
        out.extend(w.to_be_bytes());
        out.extend(h.to_be_bytes());
        out.extend(x.to_be_bytes());
        out.extend(y.to_be_bytes());
        out.extend(advance.to_be_bytes());
        out.extend(packed);
        let metrics = GlyphMetrics {
            bounds,
            advance: Some(Vector::new(i64::from(advance), 0)),
            ..Default::default()
        };
        glyph_metadata(g, &metrics, id.index(), &mut d)?;
        if g.codepoints
            != cps
                .iter()
                .copied()
                .map(Codepoint::Unicode)
                .collect::<Vec<_>>()
        {
            d.loss(
                "write.aliases",
                Some(id.index()),
                None,
                "Aliases were reordered, converted, or omitted",
            )?;
        }
        let mut projected = Glyph::new(bitmap);
        projected.metrics = metrics;
        projected.codepoints = cps.iter().copied().map(Codepoint::Unicode).collect();
        expected.add_glyph(projected)?;
    }
    if offsets.len() != grouped.len() {
        return Err(d.fatal("PF2 glyph fields are not representable; output was not emitted"));
    }
    for (i, (&cp, id)) in map.iter().enumerate() {
        let at = index_at + 8 + i * 9;
        out[at..at + 4].copy_from_slice(&cp.to_be_bytes());
        out[at + 4] = 0;
        out[at + 5..at + 9].copy_from_slice(&offsets[&id.index()].to_be_bytes());
    }
    normalize_metrics(&mut expected)?;
    metadata(f, &expected, &mut d)?;

    // Reader metadata and temporary offset maps use the same conservative accounting.
    let retained = out.len();
    let mut read_budget = Budget::new(retained, map.len(), &o.limits)?;
    read_budget.add(1024)?;
    read_budget.add(map.keys().filter(|&&cp| joined_code(cp)).count() * 768)?;
    for (_, g) in expected.glyphs() {
        read_budget.add(g.bitmap.as_bytes().len() * 3 + g.codepoints.len() * 16)?;
    }
    for p in expected.headers.iter() {
        if let PropertyValue::Bytes(b) = &p.value {
            read_budget.add(b.len() * 2 + 256)?;
        }
    }
    read_budget.add(
        expected.names.full.as_ref().map_or(0, |s| s.len() * 2 + 2)
            + expected
                .names
                .family
                .as_ref()
                .map_or(0, |s| s.len() * 2 + 2),
    )?;
    Ok((out, d.report))
}
