use super::*;
use crate::model::{FontNames, GlyphBitmap, GlyphMetrics};
use std::collections::BTreeSet;

fn loss(r: &mut Report, code: &str, id: Option<usize>, text: impl Into<String>) {
    diag(r, "pbf", code, Impact::Loss, id, None, text);
}
fn extension(font: &Font) -> Result<&[u8]> {
    let mut extra = None;
    for p in font.headers.iter().filter(|p| p.name == "pbf.header_extra") {
        if extra.is_some() {
            return Err(invalid("duplicate pbf.header_extra property"));
        }
        match &p.value {
            PropertyValue::Bytes(v) if v.len() <= 245 => extra = Some(v.as_slice()),
            _ => return Err(invalid("pbf.header_extra must be at most 245 bytes")),
        }
    }
    Ok(extra.unwrap_or_default())
}
fn padding(g: &Glyph) -> Result<[u8; 3]> {
    let mut bytes = None;
    for p in g.properties.iter().filter(|p| p.name == "pbf.v1_padding") {
        if bytes.is_some() {
            return Err(invalid("duplicate pbf.v1_padding property"));
        }
        match &p.value {
            PropertyValue::Bytes(v) if v.len() == 3 => bytes = Some([v[0], v[1], v[2]]),
            _ => return Err(invalid("pbf.v1_padding must contain three bytes")),
        }
    }
    Ok(bytes.unwrap_or([0; 3]))
}
fn font_losses(f: &Font, expected: &FontMetrics, version: Version, r: &mut Report) {
    if f.names != FontNames::default() {
        loss(
            r,
            "write.names",
            None,
            "PBF has no font names, copyright or license fields",
        );
    }
    let charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    if f.charset != FontCharset::default() && f.charset != charset {
        loss(
            r,
            "write.charset",
            None,
            "Charset metadata has no PBF field; only explicit Unicode mappings are written",
        );
    }
    for (is_header, p) in f
        .headers
        .iter()
        .map(|p| (true, p))
        .chain(f.properties.iter().map(|p| (false, p)))
    {
        if version == Version::Three
            && p.name == "pbf.header_extra"
            && is_header
            && p.raw.is_none()
            && matches!(&p.value, PropertyValue::Bytes(bytes) if !bytes.is_empty())
        {
            continue;
        }
        loss(
            r,
            "write.property",
            None,
            format!("Property {:?} has no PBF field", p.name),
        );
    }
    macro_rules! fields{($($f:ident),*)=>{$(if f.metrics.$f.is_some()&&f.metrics.$f!=expected.$f{loss(r,"write.font_metric",None,concat!("Font metric ",stringify!($f)," cannot be preserved"));})*}}
    fields!(
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
        default_glyph,
        default_character,
        space_width,
        spacing,
        metrics_set
    );
    if f.metrics.glyph_defaults != GlyphMetrics::default() {
        loss(
            r,
            "write.glyph_defaults",
            None,
            "Font-level glyph defaults have no PBF field",
        );
    }
    if f.slot_count() != f.len() {
        loss(
            r,
            "write.slots",
            None,
            "Removed glyph slots are compacted in PBF",
        );
    }
}
fn metadata_losses(g: &Glyph, id: usize, version: Version, r: &mut Report) {
    if g.name.is_some() {
        loss(
            r,
            "write.glyph_name",
            Some(id),
            "PBF has no glyph name field",
        );
    }
    for p in g.properties.iter() {
        if version == Version::One
            && p.name == "pbf.v1_padding"
            && p.raw.is_none()
            && matches!(&p.value, PropertyValue::Bytes(bytes) if bytes.iter().any(|&v| v != 0))
        {
            continue;
        }
        loss(
            r,
            "write.glyph_property",
            Some(id),
            format!("Glyph property {:?} has no PBF field", p.name),
        );
    }
    macro_rules! fields{($($f:ident),*)=>{$(if g.metrics.$f.is_some(){loss(r,"write.glyph_metric",Some(id),concat!("Glyph metric ",stringify!($f)," has no PBF field"));})*}}
    fields!(
        scalable_advance,
        vertical_advance,
        vertical_scalable_advance,
        vertical_origin,
        abc,
        left_bearing,
        right_bearing,
        image_offset
    );
    if g.metrics.advance.is_some_and(|v| v.y != 0) {
        loss(
            r,
            "write.advance_y",
            Some(id),
            "PBF horizontal advance has no vertical component",
        );
    }
    if !g.unicode_sequences.is_empty() {
        loss(
            r,
            "write.sequence",
            Some(id),
            "PBF cannot encode Unicode sequences",
        );
    }
}
fn value(g: &Glyph, x: usize, y: usize, threshold: Option<u8>) -> bool {
    match &g.bitmap {
        GlyphBitmap::Mono(b) => {
            b.as_bytes()[y * (b.width() as usize).div_ceil(8) + x / 8] & (128 >> (x % 8)) != 0
        }
        GlyphBitmap::Gray8(b) => {
            b.as_bytes()[y * b.width() as usize + x] >= threshold.unwrap_or(128)
        }
    }
}
struct Encoded {
    bytes: Vec<u8>,
    max: usize,
}
impl Encoded {
    fn add(&mut self, b: &[u8]) -> Result<()> {
        let n = self
            .bytes
            .len()
            .checked_add(b.len())
            .ok_or_else(|| limit("PBF output bytes", self.max))?;
        check(n, self.max, "PBF output bytes")?;
        self.bytes
            .try_reserve(b.len())
            .map_err(|_| limit("PBF output allocation", self.max))?;
        self.bytes.extend_from_slice(b);
        Ok(())
    }
}
/// Encode deterministic PBF data. Unsupported source information is reported;
/// out-of-range wire values and conflicting mappings are rejected explicitly.
pub fn write(font: &Font, o: &PbfWriteOptions) -> Result<(Vec<u8>, Report)> {
    let l = &o.limits;
    font.validate(l)?;
    if font.is_empty() {
        return Err(invalid("PBF requires a glyph"));
    }
    if o.version != Version::Three && o.compression != Compression::Raw {
        return Err(invalid("PBF RLE4 requires version 3"));
    }
    if (o.version == Version::One && o.offset_width == OffsetWidth::U32)
        || (o.version == Version::Two && o.offset_width == OffsetWidth::U16)
    {
        return Err(invalid("offset width does not match PBF version"));
    }
    if o.version != Version::One && o.hash_buckets == 0 {
        return Err(invalid("PBF hash_buckets must be nonzero"));
    }
    if o.codepoint_bytes.is_some_and(|v| !matches!(v, 2 | 4))
        || (o.version == Version::One && o.codepoint_bytes == Some(4))
    {
        return Err(invalid("invalid PBF codepoint width"));
    }
    let height = match o.max_height {
        Some(h) => h,
        None => u8::try_from(
            font.metrics
                .ascent
                .or(font.metrics.pixel_size.map(i64::from))
                .ok_or_else(|| invalid("PBF needs max_height, ascent or pixel_size"))?,
        )
        .map_err(|_| invalid("PBF max_height is outside u8"))?,
    };
    let default = font
        .metrics
        .default_character
        .filter(|cp| matches!(cp,Codepoint::Unicode(v) if *v<=65535&&cp.scalar().is_some()))
        .map(Codepoint::value)
        .or_else(|| {
            font.metrics
                .default_glyph
                .and_then(|id| font.glyph(id))
                .and_then(|g| {
                    g.codepoints.iter().find(
                        |cp| matches!(cp,Codepoint::Unicode(v) if *v<=65535&&cp.scalar().is_some()),
                    )
                })
                .map(|cp| cp.value())
        });
    let wildcard = o.wildcard.unwrap_or(default.unwrap_or(0x25af) as u16);
    if char::from_u32(u32::from(wildcard)).is_none() {
        return Err(invalid("PBF wildcard must be a Unicode scalar"));
    }
    let extra = extension(font)?;
    let extra = if o.version == Version::Three {
        extra
    } else {
        &[]
    };
    let mut budget = Budget::new(10240, l)?;
    let mut mapping_count = 0usize;
    for (_, g) in font.glyphs() {
        mapping_count = mapping_count
            .checked_add(g.codepoints.len())
            .ok_or_else(|| limit("PBF mapping count", 65535))?;
        check(mapping_count, 65535.min(l.max_glyphs), "PBF mapping count")?;
        budget.add(8192)?;
        for p in g.properties.iter() {
            budget.add(p.name.len().saturating_mul(8).saturating_add(512))?;
        }
    }
    for p in font.headers.iter().chain(font.properties.iter()) {
        budget.add(p.name.len().saturating_mul(8).saturating_add(512))?;
    }
    budget.items::<(u32, usize)>(mapping_count)?;
    budget.add(mapping_count * 640)?;
    budget.items::<usize>(font.len())?;
    let max = o
        .max_output_bytes
        .min(l.max_input_bytes)
        .min(l.max_total_bytes.saturating_sub(budget.used) / 3);
    let mut readback = read_base_budget(0, mapping_count, 0, l)?;
    if !extra.is_empty() {
        readback.add(extra.len() + 128)?;
    }
    let mut data = Encoded {
        bytes: Vec::new(),
        max,
    };
    data.add(&[0; 4])?;
    let mut mappings = Vec::with_capacity(mapping_count);
    let mut seen = BTreeSet::new();
    let mut report = Report::default();
    let mut source_ids = Vec::with_capacity(font.len());
    let mut descent = 0;
    let mut bounds: Option<Bounds> = None;
    for (id, g) in font.glyphs() {
        let old_count = mappings.len();
        let offset = data.bytes.len();
        let mut last = None;
        let mut reordered = false;
        for &cp in &g.codepoints {
            let Codepoint::Unicode(cp) = cp else {
                loss(
                    &mut report,
                    "write.legacy_mapping",
                    Some(id.index()),
                    "Legacy code cannot be silently recast as Unicode",
                );
                continue;
            };
            if char::from_u32(cp).is_none() {
                loss(
                    &mut report,
                    "write.invalid_unicode",
                    Some(id.index()),
                    "Non-scalar code has no PBF Unicode mapping",
                );
                continue;
            }
            if (o.version == Version::One || o.codepoint_bytes == Some(2)) && cp > 65535 {
                return Err(invalid("PBF codepoint does not fit selected width"));
            }
            if !seen.insert(cp) {
                return Err(invalid(
                    "duplicate Unicode mapping cannot be encoded in PBF",
                ));
            }
            if last.is_some_and(|v| v >= cp) {
                reordered = true;
            }
            last = Some(cp);
            mappings.push((cp, offset));
        }
        if old_count == mappings.len() {
            loss(
                &mut report,
                "write.unencoded_glyph",
                Some(id.index()),
                "Glyph without an encodable Unicode alias is omitted",
            );
            continue;
        }
        if reordered {
            loss(
                &mut report,
                "write.alias_order",
                Some(id.index()),
                "PBF orders glyph aliases by codepoint",
            );
        }
        metadata_losses(g, id.index(), o.version, &mut report);
        let pad = padding(g)?;
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
            if o.gray_threshold.is_none() {
                return Err(invalid(
                    "Gray8 PBF output requires an explicit gray_threshold",
                ));
            }
            loss(
                &mut report,
                "write.gray_threshold",
                Some(id.index()),
                "Gray8 pixels were thresholded to monochrome",
            );
        }
        let b = g.metrics.bounds;
        let w = u8::try_from(b.width).map_err(|_| invalid("PBF glyph width exceeds u8"))?;
        let h = u8::try_from(b.height).map_err(|_| invalid("PBF glyph height exceeds u8"))?;
        let left = i8::try_from(b.x).map_err(|_| invalid("PBF glyph left bearing exceeds i8"))?;
        let top = i8::try_from(i128::from(height) - i128::from(b.y) - i128::from(b.height))
            .map_err(|_| invalid("PBF glyph top bearing exceeds i8"))?;
        let advance = i8::try_from(
            g.metrics
                .advance
                .or(font.metrics.glyph_defaults.advance)
                .map_or(i64::from(w), |v| v.x),
        )
        .map_err(|_| invalid("PBF glyph advance exceeds i8"))?;
        let mut packed = [0u8; 8192];
        let mut count = 0usize;
        let mut has_ink = false;
        let byte_count = if o.compression == Compression::Rle4 {
            if w == 0 && h != 0 {
                return Err(invalid(
                    "RLE4 cannot preserve nonzero height with zero width",
                ));
            }
            let mut pos = 0;
            let pixels = usize::from(w) * usize::from(h);
            while pos < pixels {
                let ink = value(
                    g,
                    pos % usize::from(w),
                    pos / usize::from(w),
                    o.gray_threshold,
                );
                let mut run = 1;
                while run < 8
                    && pos + run < pixels
                    && value(
                        g,
                        (pos + run) % usize::from(w),
                        (pos + run) / usize::from(w),
                        o.gray_threshold,
                    ) == ink
                {
                    run += 1;
                }
                if count == 255 {
                    return Err(invalid("PBF RLE4 exceeds 255 run units"));
                }
                has_ink |= ink;
                let unit = (u8::from(ink) << 3) | (run as u8 - 1);
                packed[count / 2] |= unit << (4 * (count % 2));
                count += 1;
                pos += run;
            }
            count.div_ceil(2)
        } else {
            for y in 0..usize::from(h) {
                for x in 0..usize::from(w) {
                    let pos = y * usize::from(w) + x;
                    if value(g, x, y, o.gray_threshold) {
                        has_ink = true;
                        packed[pos / 8] |= 1 << (pos % 8);
                    }
                }
            }
            (usize::from(w) * usize::from(h)).div_ceil(8)
        };
        let record_h = if o.compression == Compression::Rle4 {
            count as u8
        } else {
            h
        };
        if o.version == Version::One {
            data.add(&[
                w,
                record_h,
                left as u8,
                top as u8,
                pad[0],
                pad[1],
                pad[2],
                advance as u8,
            ])?;
        } else {
            data.add(&[w, record_h, left as u8, top as u8, advance as u8])?;
        }
        readback.add((usize::from(w).div_ceil(8) * usize::from(h)) * 2)?;
        readback.items::<Codepoint>(mappings.len() - old_count)?;
        if o.version == Version::One && pad != [0; 3] {
            readback.add(160)?;
        }
        // Official fontgen pads the payload to a four-byte size; v1 also needs
        // every next record word-aligned. The fixed scratch tail is zeroed.
        data.add(&packed[..byte_count.div_ceil(4) * 4])?;
        if has_ink {
            descent = descent.max(-b.y);
        }
        if b.width != 0 && b.height != 0 {
            bounds = Some(match bounds {
                None => b,
                Some(old) => {
                    let x = old.x.min(b.x);
                    let y = old.y.min(b.y);
                    let right = (old.x + i64::from(old.width)).max(b.x + i64::from(b.width));
                    let top = (old.y + i64::from(old.height)).max(b.y + i64::from(b.height));
                    Bounds::new((right - x) as u32, (top - y) as u32, x, y)
                }
            });
        }
        source_ids.push(id);
    }
    if mappings.is_empty() {
        return Err(invalid("PBF has no encodable Unicode mappings"));
    }
    let cp = o
        .codepoint_bytes
        .unwrap_or(if mappings.iter().any(|m| m.0 > 65535) {
            4
        } else {
            2
        }) as usize;
    let off = match o.version {
        Version::One => 2,
        Version::Two => 4,
        Version::Three => match o.offset_width {
            OffsetWidth::Auto => {
                if data.bytes.len() <= 65536 {
                    2
                } else {
                    4
                }
            }
            OffsetWidth::U16 => 2,
            OffsetWidth::U32 => 4,
        },
    };
    for &(_, offset) in &mappings {
        let wire = if o.version == Version::One {
            offset / 4
        } else {
            offset
        };
        if off == 2 && wire > 65535 {
            return Err(invalid("PBF glyph offset exceeds u16"));
        }
        if wire > u32::MAX as usize {
            return Err(invalid("PBF glyph offset exceeds u32"));
        }
    }
    let buckets = if o.version == Version::One {
        0
    } else {
        o.hash_buckets as usize
    };
    let size = match o.version {
        Version::One => 6,
        Version::Two => 8,
        Version::Three => 10 + extra.len(),
    };
    let table_bytes = size + buckets * 4 + mappings.len() * (cp + off);
    check(table_bytes, l.max_table_bytes, "PBF mapping table")?;
    let total = table_bytes
        .checked_add(data.bytes.len())
        .ok_or_else(|| limit("PBF output bytes", max))?;
    check(total, max, "PBF output bytes")?;
    readback.add(total)?;
    mappings.sort_unstable_by_key(|&(code, _)| {
        (
            if buckets == 0 {
                0
            } else {
                code as usize % buckets
            },
            code,
        )
    });
    let mut output = Encoded {
        bytes: Vec::with_capacity(total),
        max,
    };
    output.add(&[
        match o.version {
            Version::One => 1,
            Version::Two => 2,
            Version::Three => 3,
        },
        height,
    ])?;
    output.add(&(mappings.len() as u16).to_le_bytes())?;
    output.add(&wildcard.to_le_bytes())?;
    if o.version != Version::One {
        output.add(&[buckets as u8, cp as u8])?;
    }
    if o.version == Version::Three {
        output.add(&[
            size as u8,
            u8::from(off == 2) | (u8::from(o.compression == Compression::Rle4) << 1),
        ])?;
        output.add(extra)?;
    }
    if buckets != 0 {
        let mut index = 0;
        for bucket in 0..buckets {
            let start = index;
            while index < mappings.len() && mappings[index].0 as usize % buckets == bucket {
                index += 1;
            }
            let count = u8::try_from(index - start)
                .map_err(|_| invalid("PBF hash bucket exceeds 255 entries"))?;
            let offset = u16::try_from(start * (cp + off))
                .map_err(|_| invalid("PBF bucket table offset exceeds u16"))?;
            output.add(&[bucket as u8, count])?;
            output.add(&offset.to_le_bytes())?;
        }
    }
    for &(code, offset) in &mappings {
        output.add(&code.to_le_bytes()[..cp])?;
        let offset = if o.version == Version::One {
            offset / 4
        } else {
            offset
        };
        output.add(&(offset as u32).to_le_bytes()[..off])?;
    }
    output.add(&data.bytes)?;
    let expected = FontMetrics {
        pixel_size: Some(u32::from(height)),
        point_size: Some(Rational::integer(i64::from(height))),
        resolution: Some(Vector::new(72, 72)),
        ascent: Some(i64::from(height)),
        descent: Some(descent.max(0)),
        bounds,
        default_character: Some(Codepoint::Unicode(u32::from(wildcard))),
        default_glyph: font
            .glyph_id(Codepoint::Unicode(u32::from(wildcard)))
            .filter(|id| source_ids.contains(id)),
        ..Default::default()
    };
    font_losses(font, &expected, o.version, &mut report);
    // Stable IDs change if any source glyph is omitted, even with no removed slots.
    if source_ids
        .iter()
        .enumerate()
        .any(|(index, id)| index != id.index())
    {
        loss(
            &mut report,
            "write.glyph_order",
            None,
            "Omitted glyphs compact the PBF glyph indices",
        );
    }
    if !seen.contains(&u32::from(wildcard)) {
        diag(
            &mut report,
            "pbf",
            "write.missing_wildcard",
            Impact::None,
            None,
            Some(u32::from(wildcard)),
            "PBF wildcard has no matching glyph",
        );
    }
    if !extra.is_empty() {
        diag(
            &mut report,
            "pbf",
            "write.extended_header",
            Impact::None,
            None,
            None,
            "Preserved header extensions are unsupported by the official Pebble runtime",
        );
    }
    Ok((output.bytes, report))
}
