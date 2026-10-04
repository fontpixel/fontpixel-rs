use super::{Container, IigsReadOptions, IigsWriteOptions, Version, reader, support::*};
use crate::model::{Codepoint, Font, Glyph, GlyphBitmap, PropertyValue};
use crate::{FontSet, Report, Result};
fn property(f: &Font, name: &str) -> Result<Option<u16>> {
    match f.properties.get(name) {
        None => Ok(None),
        Some(PropertyValue::Integer(n)) => {
            Ok(Some(u16::try_from(*n).map_err(|_| {
                invalid(format!("IIgs {name} is not an unsigned word"))
            })?))
        }
        _ => Err(invalid(format!("IIgs {name} requires an integer"))),
    }
}
fn put(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn unsigned(n: i64) -> Result<u16> {
    u16::try_from(n).map_err(|_| invalid("IIgs metric outside unsigned word range"))
}
fn signed(n: i64) -> Result<u16> {
    i16::try_from(n)
        .map(|v| v as u16)
        .map_err(|_| invalid("IIgs metric outside signed word range"))
}
/// Write a deterministic standalone data fork and verify the default reader can
/// consume it under exactly the same limits. Report all changed model fields.
pub fn write(s: &FontSet, o: &IigsWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut b = Budget::new(&o.limits);
    b.model(s)?;
    let f = s.single()?;
    let mut report = Report::default();
    let mut replay = None;
    if let Some(raw) = bytes(&f.properties, "iigs.source") {
        let ro = IigsReadOptions {
            charset: f.charset.name.clone(),
        };
        let original = reader::decode(raw, &ro, &mut b)?;
        let version = reader::header(raw)?.version;
        if original.single()? == f
            && (o.version == Version::Auto
                || o.version == Version::V1_1 && version == 0x101
                || o.version == Version::V1_5 && version == 0x105)
        {
            b.table(raw.len())?;
            replay = Some(raw.to_vec());
        }
    }
    let data = if let Some(raw) = replay {
        raw
    } else {
        encode(f, o, &mut b)?
    };
    check(
        data.len(),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "IIgs output bytes",
    )?;
    let data = if o.container == Container::BinScii {
        if let Some(raw) = bytes(&s.properties, "iigs.binscii-source") {
            if super::binscii::decode(raw, &mut b)? == data {
                check(
                    raw.len(),
                    o.max_output_bytes.min(o.limits.max_input_bytes),
                    "BinSCII output bytes",
                )?;
                b.table(raw.len())?;
                raw.to_vec()
            } else {
                super::binscii::encode(&data, &mut b, o.max_output_bytes)?
            }
        } else {
            super::binscii::encode(&data, &mut b, o.max_output_bytes)?
        }
    } else {
        data
    };
    let decoded = reader::decode_input(&data, &IigsReadOptions::default(), &mut b)?;
    compare(s, &decoded, "iigs", &mut b, &mut report)?;
    Ok((data, report))
}
fn encode(f: &Font, o: &IigsWriteOptions, b: &mut Budget<'_>) -> Result<Vec<u8>> {
    let mut slots: [Option<&Glyph>; 256] = [None; 256];
    for (_, g) in f.glyphs() {
        for &cp in &g.codepoints {
            if let Codepoint::Legacy(v) = cp
                && v < 256
            {
                slots[v as usize] = Some(g);
            }
        }
    }
    let first = slots
        .iter()
        .position(Option::is_some)
        .ok_or_else(|| invalid("IIgs output requires at least one legacy byte mapping"))?;
    let last = slots
        .iter()
        .rposition(Option::is_some)
        .expect("first exists");
    let default = f.metrics.default_glyph.and_then(|id| f.glyph(id));
    let selected = slots[first..=last]
        .iter()
        .copied()
        .chain(std::iter::once(default));
    let count = last - first + 2;
    check(count, o.limits.max_glyphs, "IIgs output glyph slots")?;
    // At least one selected byte glyph exists. Do not artificially include the
    // origin: valid Apple rectangles may lie wholly on either side of it.
    let mut left = i64::MAX;
    let mut right = i64::MIN;
    let mut top = i64::MIN;
    let mut bottom = i64::MAX;
    let mut total_width = 0usize;
    let mut max_advance = 0i64;
    let mut extent = 0i64;
    for g in selected.clone().flatten() {
        left = left.min(g.metrics.bounds.x);
        right = right.max(
            g.metrics
                .bounds
                .x
                .checked_add(i64::from(g.bitmap.width()))
                .ok_or_else(|| invalid("IIgs glyph right overflow"))?,
        );
        bottom = bottom.min(g.metrics.bounds.y);
        top = top.max(
            g.metrics
                .bounds
                .y
                .checked_add(i64::from(g.bitmap.height()))
                .ok_or_else(|| invalid("IIgs glyph top overflow"))?,
        );
        total_width = total_width
            .checked_add(g.bitmap.width() as usize)
            .ok_or_else(|| invalid("IIgs strike width overflow"))?;
        let advance = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .map_or(i64::from(g.bitmap.width()), |a| a.x);
        if !(0..=254).contains(&advance) {
            return Err(invalid("IIgs advance must be in 0..=254"));
        }
        max_advance = max_advance.max(advance);
        extent = extent
            .max(advance)
            .max(g.metrics.bounds.x.saturating_abs())
            .max(g.metrics.bounds.x + i64::from(g.bitmap.width()));
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) && o.gray_threshold.is_none() {
            return Err(invalid("IIgs Gray8 output requires explicit threshold"));
        }
    }
    if let Some(bounds) = f.metrics.bounds {
        left = left.min(bounds.x);
        right = right.max(
            bounds
                .x
                .checked_add(i64::from(bounds.width))
                .ok_or_else(|| invalid("IIgs font bounds overflow"))?,
        );
        bottom = bottom.min(bounds.y);
        top = top.max(
            bounds
                .y
                .checked_add(i64::from(bounds.height))
                .ok_or_else(|| invalid("IIgs font bounds overflow"))?,
        );
    }
    if let Some(ascent) = f.metrics.ascent {
        if ascent < top {
            return Err(invalid("IIgs ascent clips glyph bounds"));
        }
        top = ascent;
    }
    if let Some(descent) = f.metrics.descent {
        let low = descent
            .checked_neg()
            .ok_or_else(|| invalid("IIgs descent overflow"))?;
        if low > bottom {
            return Err(invalid("IIgs descent clips glyph bounds"));
        }
        bottom = low;
    }
    let height = usize::from(unsigned(
        top.checked_sub(bottom)
            .ok_or_else(|| invalid("IIgs font height overflow"))?,
    )?);
    let kern =
        i16::try_from(left).map_err(|_| invalid("IIgs kernMax outside signed word range"))?;
    let rectwidth = unsigned(
        right
            .checked_sub(left)
            .ok_or_else(|| invalid("IIgs font width overflow"))?,
    )?;
    signed(top)?;
    signed(
        bottom
            .checked_neg()
            .ok_or_else(|| invalid("IIgs negative descent overflow"))?,
    )?;
    if total_width > usize::from(u16::MAX) {
        return Err(invalid("IIgs bitmap locations exceed 16 bits"));
    }
    let row = total_width.div_ceil(16) * 2;
    let strike_size = row
        .checked_mul(height)
        .ok_or_else(|| invalid("IIgs strike size overflow"))?;
    check(
        strike_size,
        o.limits.max_table_bytes,
        "IIgs output strike table",
    )?;
    let ow_relative = 26usize
        .checked_add(strike_size)
        .and_then(|v| v.checked_add((count + 1) * 2))
        .ok_or_else(|| invalid("IIgs output table offset overflow"))?;
    let offset = (ow_relative - 16) / 2;
    let native_version = property(f, "iigs.version")?;
    let version = match o.version {
        Version::Auto => {
            if offset > 65535 || native_version == Some(0x105) {
                0x105
            } else {
                0x101
            }
        }
        Version::V1_1 => 0x101,
        Version::V1_5 => 0x105,
    };
    if version == 0x101 && offset > 65535 {
        return Err(invalid(
            "IIgs version 1.1 width offset exceeds 16 bits; select 1.5",
        ));
    }
    let codec =
        crate::charset::CharsetCodec::new("macroman", None).map_err(|e| invalid(e.to_string()))?;
    let name: Vec<u8> = f
        .names
        .family
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(255)
        .map(|c| {
            codec
                .unicode2cp(u32::from(c))
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(b'?')
        })
        .collect();
    let anchor = 1 + name.len();
    let words = if version == 0x105 { 7 } else { 6 };
    let mf = anchor + words * 2;
    let size = mf
        .checked_add(ow_relative)
        .and_then(|v| v.checked_add((count + 1) * 2))
        .ok_or_else(|| invalid("IIgs output size overflow"))?;
    check(
        size,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "IIgs output bytes",
    )?;
    b.table(size)?;
    let mut out = vec![0; size];
    out[0] = name.len() as u8;
    out[1..anchor].copy_from_slice(&name);
    put(&mut out, anchor, words as u16);
    put(
        &mut out,
        anchor + 2,
        property(f, "iigs.family-id")?.unwrap_or(0),
    );
    let mut style = property(f, "iigs.style")?.unwrap_or(0);
    if let Some(s) = &f.names.style
        && s != &reader::style_name(style)
    {
        style &= !31;
        for (i, name) in ["bold", "italic", "underline", "outline", "shadow"]
            .iter()
            .enumerate()
        {
            if s.split_whitespace()
                .any(|part| part.eq_ignore_ascii_case(name))
            {
                style |= 1 << i;
            }
        }
    }
    put(&mut out, anchor + 4, style);
    let point = if let Some(p) = f.metrics.point_size {
        if p.numerator() % i64::from(p.denominator()) != 0 {
            return Err(invalid("IIgs point size must be integral"));
        }
        unsigned(p.numerator() / i64::from(p.denominator()))?
    } else {
        unsigned(i64::from(f.metrics.pixel_size.unwrap_or(height as u32)))?
    };
    put(&mut out, anchor + 6, point);
    put(&mut out, anchor + 8, version);
    put(
        &mut out,
        anchor + 10,
        // A stale native extent must never undersize QuickDraw II's text buffer
        // after a glyph edit. The reread comparison reports this normalization.
        property(f, "iigs.fbr-extent")?
            .unwrap_or(0)
            .max(unsigned(extent)?),
    );
    if words == 7 {
        put(&mut out, anchor + 12, (offset >> 16) as u16);
    }
    put(&mut out, mf, 0);
    put(&mut out, mf + 2, first as u16);
    put(&mut out, mf + 4, last as u16);
    put(&mut out, mf + 6, max_advance as u16);
    put(&mut out, mf + 8, kern as u16);
    put(
        &mut out,
        mf + 10,
        i16::try_from(bottom).map_err(|_| invalid("IIgs negative descent outside signed word"))?
            as u16,
    );
    put(&mut out, mf + 12, rectwidth);
    put(&mut out, mf + 14, height as u16);
    put(&mut out, mf + 16, offset as u16);
    put(&mut out, mf + 18, signed(top)?);
    put(&mut out, mf + 20, signed(-bottom)?);
    put(&mut out, mf + 22, signed(f.metrics.line_gap.unwrap_or(0))?);
    put(&mut out, mf + 24, (row / 2) as u16);
    let loc = mf + 26 + strike_size;
    let ow = mf + ow_relative;
    let mut x = 0usize;
    for (i, g) in selected.enumerate() {
        put(&mut out, loc + i * 2, x as u16);
        if let Some(g) = g {
            let advance = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(i64::from(g.bitmap.width()), |a| a.x);
            let bearing = g.metrics.bounds.x - left;
            if !(0..=254).contains(&bearing) {
                return Err(invalid("IIgs offset from kernMax must be in 0..=254"));
            }
            put(&mut out, ow + i * 2, (bearing as u16) << 8 | advance as u16);
            let glyph_pixels = (g.bitmap.width() as usize)
                .checked_mul(height)
                .ok_or_else(|| invalid("IIgs output glyph area overflow"))?;
            check(
                glyph_pixels,
                o.limits.max_pixels_per_glyph,
                "IIgs output glyph pixels",
            )?;
            let y = (top - g.metrics.bounds.y - i64::from(g.bitmap.height())) as usize;
            for gy in 0..g.bitmap.height() as usize {
                let start = mf + 26 + (y + gy) * row;
                match &g.bitmap {
                    GlyphBitmap::Mono(m) => {
                        let shift = x % 8;
                        for (ix, &byte) in m.as_bytes()[gy * m.stride()..(gy + 1) * m.stride()]
                            .iter()
                            .enumerate()
                        {
                            let at = start + x / 8 + ix;
                            out[at] |= byte >> shift;
                            if shift != 0 && at + 1 < start + row {
                                out[at + 1] |= byte << (8 - shift);
                            }
                        }
                    }
                    GlyphBitmap::Gray8(m) => {
                        for gx in 0..m.width() as usize {
                            if m.as_bytes()[gy * m.stride() + gx]
                                >= o.gray_threshold.expect("validated threshold")
                            {
                                out[start + (x + gx) / 8] |= 0x80 >> ((x + gx) % 8);
                            }
                        }
                    }
                }
            }
            x += g.bitmap.width() as usize;
        } else if i == count - 1 {
            put(&mut out, ow + i * 2, 0);
        }
        // required empty missing symbol
        else {
            put(&mut out, ow + i * 2, 0xffff);
        }
    }
    put(&mut out, loc + count * 2, x as u16);
    put(&mut out, ow + count * 2, 0xffff);
    Ok(out)
}
