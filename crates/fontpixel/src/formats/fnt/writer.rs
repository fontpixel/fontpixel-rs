use super::{FntReadOptions, FntWriteOptions, Version, charset_name, reader, support::*};
use crate::bitmap::{Bitmap, Bounds};
use crate::charset::{CharsetCodec, canonical_charset};
use crate::model::{Codepoint, Font, Glyph, GlyphBitmap, GlyphId, PropertyValue, Spacing};
use crate::{Report, Result};
use std::collections::BTreeSet;

fn codec(name: Option<&str>) -> Result<Option<CharsetCodec>> {
    name.map(|name| {
        let c = CharsetCodec::new(name, None).map_err(|e| invalid(e.to_string()))?;
        if !c.known() {
            return Err(invalid(format!("unknown FNT code page {name}")));
        }
        Ok(c)
    })
    .transpose()
}
fn map_code(cp: Codepoint, source: &str, target: Option<&CharsetCodec>, raw: bool) -> Option<u8> {
    let n = match cp {
        Codepoint::Legacy(n) if raw => Some(n),
        Codepoint::Legacy(n) => CharsetCodec::new(source, None)
            .ok()
            .filter(|c| c.known())
            .and_then(|c| c.cp2char(n))
            .and_then(|c| target.and_then(|t| t.char2cp(c))),
        Codepoint::Unicode(n) => char::from_u32(n).and_then(|c| target.and_then(|t| t.char2cp(c))),
    }?;
    u8::try_from(n).ok()
}
fn string(s: &str, max: usize, label: &str, report: &mut Report) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(s.len().min(max));
    let mut altered = false;
    for c in s.chars() {
        if bytes.len() == max {
            altered = true;
            break;
        }
        if c == '\0' || u32::from(c) > 255 {
            bytes.push(b'?');
            altered = true;
        } else {
            bytes.push(c as u8);
        }
    }
    if altered {
        loss(
            report,
            "string",
            None,
            None,
            format!("{label} is not representable as at most {max} non-NUL Latin-1 bytes"),
        );
    }
    bytes
}
fn prop_bytes(f: &Font, key: &str, n: usize) -> Result<Option<Vec<u8>>> {
    match f.properties.get(key) {
        None => Ok(None),
        Some(PropertyValue::Bytes(b)) if b.len() == n => Ok(Some(b.clone())),
        _ => Err(invalid(format!("{key} requires {n} bytes"))),
    }
}
fn geometry(g: &Glyph, top: i64, bottom: i64) -> Result<Bounds> {
    let width = g
        .metrics
        .advance
        .map_or(i64::from(g.metrics.bounds.width), |v| v.x);
    let width = word(width, "glyph advance width")?;
    if width == 0 {
        return Err(invalid("FNT cannot encode a zero-width glyph"));
    }
    Ok(Bounds::new(
        u32::from(width),
        u32::from(word(top - bottom, "FNT raster height")?),
        0,
        bottom,
    ))
}
fn raster(g: &Glyph, cell: Bounds, o: &FntWriteOptions) -> Result<Bitmap> {
    let source = match &g.bitmap {
        GlyphBitmap::Mono(b) => Bitmap::from_raw(
            b.width(),
            b.height(),
            b.as_bytes(),
            bitmap_limits(&o.limits),
        )?,
        GlyphBitmap::Gray8(b) => {
            let t = o
                .gray_threshold
                .ok_or_else(|| invalid("Gray8 FNT output requires an explicit gray_threshold"))?;
            crate::bitmap::Bitmap::<crate::bitmap::Gray8>::from_raw(
                b.width(),
                b.height(),
                b.as_bytes(),
                bitmap_limits(&o.limits),
            )?
            .to_mono(t)?
        }
    };
    source.crop_bounds(Bounds::new(
        cell.width,
        cell.height,
        cell.x
            .checked_sub(g.metrics.bounds.x)
            .ok_or_else(|| invalid("FNT x offset overflow"))?,
        cell.y
            .checked_sub(g.metrics.bounds.y)
            .ok_or_else(|| invalid("FNT y offset overflow"))?,
    ))
}
fn glyph_losses(g: &Glyph, cell: Bounds, index: usize, r: &mut Report) {
    if g.name.is_some() {
        loss(
            r,
            "glyph_name",
            Some(index),
            None,
            "Glyph name has no FNT field",
        );
    }
    for p in g.properties.iter() {
        loss(
            r,
            "glyph_property",
            Some(index),
            None,
            format!("Glyph property {:?} has no FNT field", p.name),
        );
    }
    if !g.unicode_sequences.is_empty() {
        loss(
            r,
            "sequence",
            Some(index),
            None,
            "Unicode sequences have no FNT field",
        );
    }
    if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
        loss(
            r,
            "gray",
            Some(index),
            None,
            "Gray8 coverage is thresholded to monochrome",
        );
    }
    if g.metrics.bounds != cell {
        loss(
            r,
            "bounds",
            Some(index),
            None,
            "Glyph bounds are normalized to the advance-width cell and common baseline; out-of-cell pixels are clipped",
        );
    }
    let expected = crate::model::GlyphMetrics {
        bounds: cell,
        advance: Some(crate::model::Vector::new(i64::from(cell.width), 0)),
        ..Default::default()
    };
    macro_rules! fields {($($field:ident),*)=>{$(if g.metrics.$field.is_some() && g.metrics.$field!=expected.$field{loss(r,"glyph_metric",Some(index),None,concat!("Glyph metric ",stringify!($field)," cannot be retained"));})*};}
    fields!(
        advance,
        scalable_advance,
        vertical_advance,
        vertical_scalable_advance,
        vertical_origin,
        abc,
        left_bearing,
        right_bearing,
        image_offset
    );
}
fn metadata_losses(f: &Font, back: &Font, r: &mut Report) {
    macro_rules! names {($($field:ident),*)=>{$(if f.names.$field.is_some() && f.names.$field!=back.names.$field{loss(r,"name",None,None,concat!("Font name ",stringify!($field)," cannot be retained"));})*};}
    names!(
        family,
        style,
        full,
        postscript,
        version,
        copyright,
        license,
        license_url,
        foundry,
        xlfd
    );
    if !f.names.xlfd_fields.is_empty() || !f.names.localized.is_empty() {
        loss(
            r,
            "localized_names",
            None,
            None,
            "XLFD fields and localized names have no FNT fields",
        );
    }
    macro_rules! metrics {($($field:ident),*)=>{$(if f.metrics.$field.is_some() && f.metrics.$field!=back.metrics.$field{loss(r,"font_metric",None,None,concat!("Font metric ",stringify!($field)," cannot be retained unchanged"));})*};}
    metrics!(
        pixel_size,
        ppem_x,
        ppem_y,
        point_size,
        resolution,
        ascent,
        descent,
        bounds,
        line_gap,
        line_height,
        underline_position,
        underline_thickness,
        space_width,
        spacing,
        metrics_set
    );
    if f.metrics.glyph_defaults != Default::default() {
        loss(
            r,
            "glyph_defaults",
            None,
            None,
            "Font-level glyph defaults have no FNT field",
        );
    }
    for p in f.headers.iter() {
        loss(
            r,
            "header",
            None,
            None,
            format!("Source header {:?} has no FNT field", p.name),
        );
    }
    let mut property_names = BTreeSet::new();
    for p in f.properties.iter() {
        if !property_names.insert(&p.name)
            || back.properties.get(&p.name) != Some(&p.value)
            || p.raw.is_some()
        {
            loss(
                r,
                "property",
                None,
                None,
                format!("Source property {:?} cannot be retained unchanged", p.name),
            );
        }
    }
    if f.charset.registry.is_some()
        || f.charset.encoding.is_some()
        || f.charset.gl.is_some()
        || f.charset.code_page.is_some()
        || !f.charset.code_ranges.is_empty()
    {
        loss(
            r,
            "charset_metadata",
            None,
            None,
            "Source charset registry/encoding/GL/page/range metadata have no equivalent FNT fields",
        );
    }
}
/// Write one raster FNT. Every unsupported source mapping and metric is reported.
pub fn write(font: &Font, o: &FntWriteOptions) -> Result<(Vec<u8>, Report)> {
    font.validate(&o.limits)?;
    if font
        .metrics
        .point_size
        .is_some_and(|p| p.denominator() == 0)
    {
        return Err(invalid("FNT point-size denominator is zero"));
    }
    let source_cost = model_cost(font, &o.limits)?;
    if font.is_empty() {
        return Err(invalid("FNT requires a nonempty font"));
    }
    let source_name = font.charset.canonical_name();
    let charset = o
        .charset
        .or(font.charset.windows_charset)
        .unwrap_or_else(|| {
            (0..=255)
                .find(|&id| charset_name(id).is_some_and(|n| canonical_charset(n) == source_name))
                .unwrap_or(0)
        });
    let target_name = o.code_page.as_deref().or_else(|| charset_name(charset));
    let target_key = target_name.map(canonical_charset);
    let raw = target_key.as_deref() == Some(source_name.as_str())
        || (source_name == "unknown"
            && (o.charset.is_some() || font.charset.windows_charset == Some(charset)));
    // Already encoded single-byte CJK resources do not need a CJK table merely
    // to keep their bytes. Unicode mapping still requires the exact table.
    let needs_codec =
        !raw || font.glyphs().any(|(_, g)| {
            g.codepoints
                .iter()
                .any(|c| matches!(c, Codepoint::Unicode(_)))
        }) || matches!(font.metrics.default_character, Some(Codepoint::Unicode(_)))
            || o.code_page.is_some();
    let target = if needs_codec {
        codec(target_name)?
    } else {
        None
    };
    let version = o.version.unwrap_or(Version::from_word(word(
        integer(font, "fnt.version", 0x200)?,
        "fnt.version",
    )?)?);
    let mut report = Report::default();
    let mut slots: [Option<GlyphId>; 256] = [None; 256];
    let mut seen = BTreeSet::new();
    for (id, g) in font.glyphs() {
        let mut mapped = 0;
        for &cp in &g.codepoints {
            if let Some(n) = map_code(cp, &source_name, target.as_ref(), raw) {
                if slots[usize::from(n)].is_some_and(|prev| prev != id) {
                    loss(
                        &mut report,
                        "duplicate_code",
                        Some(id.index()),
                        Some(cp.value()),
                        format!("Target byte {n} already has a glyph; keeping its first glyph"),
                    );
                } else {
                    slots[usize::from(n)] = Some(id);
                    mapped += 1;
                    seen.insert(id);
                }
            } else {
                loss(
                    &mut report,
                    "unmapped_code",
                    Some(id.index()),
                    Some(cp.value()),
                    format!(
                        "Code cannot be represented in Windows charset {charset}'s single-byte range"
                    ),
                );
            }
        }
        if mapped == 0 {
            loss(
                &mut report,
                "omitted_glyph",
                Some(id.index()),
                None,
                "Glyph has no representable single-byte encoding",
            );
        }
        if mapped > 1 {
            loss(
                &mut report,
                "alias_identity",
                Some(id.index()),
                None,
                "FNT stores aliases as independent glyph records",
            );
        }
    }
    let mut first = slots
        .iter()
        .position(Option::is_some)
        .ok_or_else(|| invalid("No glyph fits selected FNT charset"))?;
    let mut last = slots
        .iter()
        .rposition(Option::is_some)
        .ok_or_else(|| invalid("No glyph fits selected FNT charset"))?;
    let source_first = usize::from(
        u8::try_from(integer(font, "fnt.dfFirstChar", first as i64)?)
            .map_err(|_| invalid("FNT source first character exceeds byte"))?,
    );
    let source_last = usize::from(
        u8::try_from(integer(font, "fnt.dfLastChar", last as i64)?)
            .map_err(|_| invalid("FNT source last character exceeds byte"))?,
    );
    if source_first > source_last {
        return Err(invalid("reversed source FNT range"));
    }
    first = first.min(source_first);
    last = last.max(source_last);
    let count = last - first + 1;
    let mut top = font.metrics.ascent.unwrap_or(0).max(0);
    let mut bottom = font
        .metrics
        .descent
        .unwrap_or(0)
        .checked_neg()
        .ok_or_else(|| invalid("FNT descent overflow"))?
        .min(0);
    for &id in &seen {
        let g = font.glyph(id).ok_or_else(|| invalid("missing FNT glyph"))?;
        top = top.max(
            g.metrics
                .bounds
                .y
                .checked_add(i64::from(g.metrics.bounds.height))
                .ok_or_else(|| invalid("FNT top overflow"))?,
        );
        bottom = bottom.min(g.metrics.bounds.y);
    }
    let height = word(
        top.checked_sub(bottom)
            .ok_or_else(|| invalid("FNT height overflow"))?,
        "FNT pixel height",
    )?;
    if height == 0 {
        return Err(invalid("FNT cannot represent zero-height fonts"));
    }
    let ascent = word(top, "FNT ascent")?;
    let mut widths = [0u16; 256];
    let mut raster_bytes = 0usize;
    for &id in &seen {
        let g = font.glyph(id).ok_or_else(|| invalid("missing FNT glyph"))?;
        let cell = geometry(g, top, bottom)?;
        glyph_losses(g, cell, id.index(), &mut report);
    }
    for i in first..=last {
        if let Some(id) = slots[i] {
            let cell = geometry(
                font.glyph(id).ok_or_else(|| invalid("missing FNT glyph"))?,
                top,
                bottom,
            )?;
            widths[i] = cell.width as u16;
            raster_bytes = raster_bytes
                .checked_add(bitmap_size(cell.width, cell.height, &o.limits)?)
                .ok_or_else(|| invalid("FNT size overflow"))?;
        }
    }
    let max_width = *widths[first..=last].iter().max().unwrap_or(&0);
    let fixed = if widths[first..=last].iter().all(|&w| w == max_width)
        && font.metrics.spacing != Some(Spacing::Proportional)
    {
        max_width
    } else {
        0
    };
    let sentinel = max_width;
    let table_entry = match version {
        Version::One => {
            if fixed != 0 {
                0
            } else {
                2
            }
        }
        Version::Two => 4,
        Version::Three => 6,
    };
    let table_size = (count + 1) * table_entry;
    check(table_size, o.limits.max_table_bytes, "FNT character table")?;
    let bits = version.header() + table_size;
    let strike_width: usize = widths[first..=last]
        .iter()
        .map(|&w| usize::from(w))
        .sum::<usize>()
        + usize::from(sentinel);
    let stride = if version == Version::One {
        strike_width.div_ceil(16) * 2
    } else {
        usize::from(max_width).div_ceil(16) * 2
    };
    let bitmap_bytes = if version == Version::One {
        stride * usize::from(height)
    } else {
        raster_bytes + usize::from(sentinel).div_ceil(8) * usize::from(height)
    };
    check(bitmap_bytes, o.limits.max_table_bytes, "FNT bitmap table")?;
    let face = string(
        font.names.family.as_deref().unwrap_or(""),
        o.limits.max_table_bytes,
        "Family name",
        &mut report,
    );
    let copyright = string(
        font.names.copyright.as_deref().unwrap_or(""),
        60,
        "Copyright",
        &mut report,
    );
    let device = string(
        text_property(font, "fnt.device")?.unwrap_or(""),
        o.limits.max_table_bytes,
        "Device name",
        &mut report,
    );
    let total = bits
        .checked_add(bitmap_bytes)
        .and_then(|v| v.checked_add(face.len() + device.len() + 2))
        .ok_or_else(|| invalid("FNT output overflow"))?;
    check(total, o.max_output_bytes, "FNT output bytes")?;
    if version != Version::Three {
        check(total, 65535, "FNT v1/v2 file bytes")?;
    }
    // Output, readback validation model, and one normalized source raster coexist.
    let source_bytes = font.glyphs().try_fold(0usize, |n, (_, g)| {
        n.checked_add(g.bitmap.as_bytes().len())
            .ok_or_else(|| invalid("FNT source size overflow"))
    })?;
    check(
        total
            .saturating_mul(3)
            .saturating_add(source_bytes)
            .saturating_add(source_cost)
            .saturating_add(
                font.len()
                    .saturating_mul(std::mem::size_of::<Glyph>() + 160),
            )
            .saturating_add(8192),
        o.limits.max_total_bytes,
        "FNT writer working bytes",
    )?;
    let mut out = vec![0; total];
    put16(&mut out, 0, version.word());
    put32(&mut out, 2, size32(total)?);
    out[6..6 + copyright.len()].copy_from_slice(&copyright);
    for &(key, at, n) in SCALARS {
        let default = match key {
            "fnt.dfWeight" => 400,
            "fnt.dfAvgWidth" => i64::from(max_width),
            "fnt.dfMaxWidth" => i64::from(max_width),
            _ => 0,
        };
        let value = integer(font, key, default)?;
        match n {
            1 => {
                out[at] =
                    u8::try_from(value).map_err(|_| invalid(format!("{key} exceeds byte range")))?
            }
            2 => put16(&mut out, at, word(value, key)?),
            _ => put32(
                &mut out,
                at,
                u32::try_from(value).map_err(|_| invalid(format!("{key} exceeds u32")))?,
            ),
        };
    }
    if get16(&out, 66)? & 5 != 0 {
        return Err(invalid("Cannot write vector/memory-only FNT type"));
    }
    let points = font.metrics.point_size.map_or(i64::from(height), |p| {
        p.numerator() / i64::from(p.denominator())
    });
    put16(&mut out, 68, word(points, "FNT point size")?);
    let dpi = font
        .metrics
        .resolution
        .unwrap_or(crate::model::Vector::new(72, 72));
    put16(&mut out, 70, word(dpi.y, "vertical DPI")?);
    put16(&mut out, 72, word(dpi.x, "horizontal DPI")?);
    put16(&mut out, 74, ascent);
    let gap = font
        .metrics
        .line_gap
        .or_else(|| {
            font.metrics
                .line_height
                .and_then(|h| h.checked_sub(i64::from(height)))
        })
        .unwrap_or(0);
    put16(&mut out, 78, word(gap, "external leading")?);
    out[85] = charset;
    put16(&mut out, 86, fixed);
    put16(&mut out, 88, height);
    out[90] = (out[90] & 0xfe) | u8::from(fixed == 0);
    out[95] = first as u8;
    out[96] = last as u8;
    let default = font
        .metrics
        .default_character
        .and_then(|cp| map_code(cp, &source_name, target.as_ref(), raw))
        .or_else(|| {
            font.metrics
                .default_glyph
                .and_then(|id| slots.iter().position(|&g| g == Some(id)).map(|i| i as u8))
        });
    let default = default
        .map(usize::from)
        .filter(|&n| n >= first && n - first <= 255)
        .unwrap_or(first);
    if font.metrics.default_character.is_some_and(|cp| {
        map_code(cp, &source_name, target.as_ref(), raw).map(usize::from) != Some(default)
    }) {
        loss(
            &mut report,
            "default_character",
            None,
            None,
            "Default character cannot be retained in selected range",
        );
    }
    if font
        .metrics
        .default_glyph
        .is_some_and(|id| slots[default] != Some(id))
    {
        loss(
            &mut report,
            "default_glyph",
            None,
            None,
            "Default glyph cannot be retained",
        );
    }
    out[97] = (default - first) as u8;
    if font.properties.get("fnt.dfBreakChar").is_some() && !(raw && first == source_first) {
        let original = source_first as u32 + u32::from(out[98]);
        if let Some(code) = map_code(
            Codepoint::Legacy(original),
            &source_name,
            target.as_ref(),
            raw,
        )
        .map(usize::from)
        .filter(|&n| n >= first)
        {
            out[98] = (code - first) as u8;
        } else {
            loss(
                &mut report,
                "break_character",
                None,
                Some(original),
                "Break character cannot be retained in target encoding",
            );
            out[98] = 0;
        }
    } else if font.properties.get("fnt.dfBreakChar").is_none() {
        out[98] = if (first..=last).contains(&32) {
            (32 - first) as u8
        } else {
            0
        };
    }

    put16(
        &mut out,
        99,
        u16::try_from(stride).map_err(|_| invalid("FNT row stride exceeds u16"))?,
    );
    put32(&mut out, 113, size32(bits)?);
    if version != Version::One {
        out[117] = u8::try_from(integer(font, "fnt.dfReserved", 0)?)
            .map_err(|_| invalid("FNT reserved byte out of range"))?;
    }
    if version == Version::Three {
        put32(
            &mut out,
            118,
            u32::try_from(integer(
                font,
                "fnt.dfFlags",
                if fixed == 0 { 2 } else { 1 },
            )?)
            .map_err(|_| invalid("FNT flags out of range"))?,
        );
        if get32(&out, 118)? & !0x13 != 0 {
            return Err(invalid("FNT color/ABC layout output unsupported"));
        }
        for (key, at, n) in [
            ("fnt.dfAspace", 122, 2),
            ("fnt.dfBspace", 124, 2),
            ("fnt.dfCspace", 126, 2),
            ("fnt.dfColorPointer", 128, 4),
        ] {
            let v = integer(font, key, 0)?;
            if n == 2 {
                put16(&mut out, at, word(v, key)?);
            } else {
                put32(
                    &mut out,
                    at,
                    u32::try_from(v).map_err(|_| invalid("FNT color pointer exceeds u32"))?,
                );
            }
        }
        if let Some(b) = prop_bytes(font, "fnt.dfReserved1", 16)? {
            out[132..148].copy_from_slice(&b);
        }
    }
    let mut offset = bits;
    let mut x = 0usize;
    for i in 0..=count {
        let width = if i == count {
            sentinel
        } else {
            widths[first + i]
        };
        let at = version.header() + i * table_entry;
        if version == Version::One {
            if fixed == 0 {
                put16(
                    &mut out,
                    at,
                    u16::try_from(x).map_err(|_| invalid("FNT v1 bit offset exceeds u16"))?,
                );
            }
        } else {
            put16(&mut out, at, width);
            if version == Version::Two {
                put16(
                    &mut out,
                    at + 2,
                    u16::try_from(offset).map_err(|_| invalid("FNT v2 offset exceeds u16"))?,
                );
            } else {
                put32(&mut out, at + 2, size32(offset)?);
            }
        }
        if i < count
            && let Some(id) = slots[first + i]
        {
            let g = font.glyph(id).ok_or_else(|| invalid("missing FNT glyph"))?;
            let cell = geometry(g, top, bottom)?;
            let bm = raster(g, cell, o)?;
            let row = usize::from(width).div_ceil(8);
            if version == Version::One {
                for y in 0..usize::from(height) {
                    for col in 0..row {
                        let value = bm.as_bytes()[y * row + col];
                        let pos = x + col * 8;
                        out[bits + y * stride + pos / 8] |= value >> (pos % 8);
                        if !pos.is_multiple_of(8) && pos / 8 + 1 < stride {
                            out[bits + y * stride + pos / 8 + 1] |= value << (8 - pos % 8);
                        }
                    }
                }
            } else {
                for col in 0..row {
                    for y in 0..usize::from(height) {
                        out[offset + col * usize::from(height) + y] = bm.as_bytes()[y * row + col];
                    }
                }
            }
        }
        x += usize::from(width);
        offset += usize::from(width).div_ceil(8) * usize::from(height);
    }
    let name_at = bits + bitmap_bytes;
    put32(&mut out, 105, size32(name_at)?);
    out[name_at..name_at + face.len()].copy_from_slice(&face);
    if !device.is_empty() {
        put32(&mut out, 101, size32(name_at + face.len() + 1)?);
    }
    out[name_at + face.len() + 1..total - 1].copy_from_slice(&device);
    let mut read_limits = o.limits.clone();
    read_limits.max_input_bytes = read_limits.max_input_bytes.max(total);
    let (back, _) = reader::read(
        &out,
        &FntReadOptions {
            code_page: o.code_page.clone(),
        },
        &read_limits,
    )?;
    metadata_losses(font, &back, &mut report);
    if o.code_page.is_some() && target_name != charset_name(charset) {
        loss(
            &mut report,
            "code_page_external",
            None,
            None,
            "Explicit code page is not encoded in dfCharSet; readers need the same override",
        );
    }
    Ok((out, report))
}
