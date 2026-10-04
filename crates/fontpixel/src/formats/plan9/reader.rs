use super::image;
use crate::bitmap::Bounds;
use crate::formats::tex_common::*;
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, Property, PropertyValue, Rational, Vector,
};
use crate::resources::{ReadContext, ResourcePath};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Detect an image-backed subfont or a structurally plausible text font header.
pub fn detect(bytes: &[u8]) -> crate::formats::Confidence {
    if image::looks(bytes) {
        return crate::formats::Confidence::Possible;
    }
    let Ok(s) = std::str::from_utf8(&bytes[..bytes.len().min(4096)]) else {
        return crate::formats::Confidence::NoMatch;
    };
    let mut t = s.split_ascii_whitespace();
    if t.next().is_some_and(|s| numeric(s).is_ok()) && t.next().is_some_and(|s| numeric(s).is_ok())
    {
        crate::formats::Confidence::Possible
    } else {
        crate::formats::Confidence::NoMatch
    }
}
/// Read a direct subfont or resolve all referenced subfonts through the context.
pub fn read(c: &mut ReadContext<'_>) -> Result<(Font, Report)> {
    let l = c.limits().clone();
    let mut budget = Budget::new(c.retained_input_bytes(), &l)?;
    budget.add(4096)?;
    if image::looks(c.bytes()) {
        return subfont(c.bytes(), &l, &mut budget);
    }
    let mut r = Report::default();
    let text =
        std::str::from_utf8(c.bytes()).map_err(|_| invalid("Plan 9 font manifest is not UTF-8"))?;
    budget.add(text.len().saturating_mul(8))?;
    let mut lines = text.lines().filter(|s| !s.trim().is_empty());
    let first = lines
        .next()
        .ok_or_else(|| invalid("empty Plan 9 font manifest"))?;
    check(first.len(), l.max_table_bytes, "Plan 9 font record")?;
    let fields: Vec<_> = first.split_ascii_whitespace().collect();
    if fields.len() != 2 {
        return Err(invalid("Plan 9 font header requires height and ascent"));
    }
    let height = numeric(fields[0])?;
    let ascent = numeric(fields[1])?;
    let mut f = base(height, ascent, true)?;
    let mut ranges = Vec::new();
    let mut total = 0usize;
    for line in lines {
        check(line.len(), l.max_table_bytes, "Plan 9 font record")?;
        let a: Vec<_> = line.split_ascii_whitespace().collect();
        if !matches!(a.len(), 3 | 4) {
            return Err(invalid(
                "Plan 9 range needs two or three integers and a path",
            ));
        }
        let first =
            u32::try_from(numeric(a[0])?).map_err(|_| invalid("Plan 9 range start outside u32"))?;
        let last =
            u32::try_from(numeric(a[1])?).map_err(|_| invalid("Plan 9 range end outside u32"))?;
        if first > last
            || char::from_u32(first).is_none()
            || char::from_u32(last).is_none()
            || first <= 0xdfff && last >= 0xd800
        {
            return Err(invalid("Plan 9 range is not a valid Unicode scalar range"));
        }
        let offset = if a.len() == 4 {
            usize::try_from(numeric(a[2])?)
                .map_err(|_| invalid("negative Plan 9 subfont offset"))?
        } else {
            0
        };
        let path = ResourcePath::new(a[a.len() - 1])?;
        let count = (last - first) as usize + 1;
        total = total.checked_add(count).ok_or_else(limit)?;
        check(total, l.max_glyphs, "Plan 9 mapped glyphs")?;
        budget.add(
            256 + path.as_str().len()
                + count
                    .saturating_mul(128usize.saturating_add(path.as_str().len().saturating_mul(3))),
        )?;
        ranges.push((first, last, offset, path));
    }
    let mut resources = BTreeMap::new();
    let mut resource_glyphs = 0usize;
    let mut used = BTreeMap::<(ResourcePath, usize), Vec<Codepoint>>::new();
    let mut order = Vec::new();
    for (first, last, offset, path) in ranges {
        if !resources.contains_key(&path) {
            let bytes = c.resource_limited(&path, budget.remaining())?;
            budget.add(bytes.len())?;
            let (sub, mut subreport) = subfont(&bytes, &l, &mut budget)?;
            resource_glyphs = resource_glyphs.checked_add(sub.len()).ok_or_else(limit)?;
            check(
                resource_glyphs,
                l.max_glyphs,
                "cumulative Plan 9 resource glyphs",
            )?;
            for e in &mut subreport.entries {
                e.location.asset = Some(path.to_string())
            }
            r.extend(subreport);
            resources.insert(path.clone(), sub);
        }
        let sub = &resources[&path];
        let count = (last - first) as usize + 1;
        if offset.checked_add(count).is_none_or(|n| n > sub.len()) {
            return Err(invalid("Plan 9 font range exceeds referenced subfont"));
        }
        for i in 0..count {
            let key = (path.clone(), offset + i);
            if !used.contains_key(&key) {
                order.push(key.clone());
            }
            used.entry(key)
                .or_default()
                .push(Codepoint::Unicode(first + i as u32));
        }
    }
    let mut mapped = BTreeMap::new();
    for key in order {
        let sub = &resources[&key.0];
        let source = sub
            .glyph_for_codepoint(Codepoint::Legacy(key.1 as u32))
            .ok_or_else(|| invalid("Plan 9 glyph index missing"))?;
        budget.add(2048 + source.bitmap.as_bytes().len())?;
        let mut g = source.clone();
        g.codepoints = used
            .remove(&key)
            .ok_or_else(|| invalid("Plan 9 mapping state missing"))?;
        for cp in &g.codepoints {
            if mapped.insert(*cp, f.len()).is_some() {
                loss(
                    &mut r,
                    "plan9",
                    "duplicate_mapping",
                    Some(f.len()),
                    Some(cp.value()),
                    "Overlapping font ranges preserve duplicate mappings; effective lookup uses the last glyph",
                );
            }
        }
        f.add_glyph(g)?;
    }
    f.update_bounds()?;
    report_cost(&r, &mut budget)?;
    Ok((f, r))
}
fn base(height: i64, ascent: i64, unicode: bool) -> Result<Font> {
    if height < 0 || height > i64::from(i32::MAX) || ascent < 0 || ascent > height {
        return Err(invalid("Plan 9 requires 0 <= ascent <= height <= i32::MAX"));
    }
    let mut f = Font::default();
    f.source_format = Some("plan9".into());
    f.charset = FontCharset {
        name: Some(if unicode { "unicode" } else { "unknown" }.into()),
        ..Default::default()
    };
    f.metrics = FontMetrics {
        pixel_size: Some(height as u32),
        point_size: Some(Rational::integer(height)),
        resolution: Some(Vector::new(72, 72)),
        ascent: Some(ascent),
        descent: Some(height - ascent),
        line_height: Some(height),
        ..Default::default()
    };
    Ok(f)
}
pub(super) fn subfont(
    bytes: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(Font, Report)> {
    let mut c = Cursor::new(bytes);
    let image = image::read(&mut c, l, budget)?;
    let count = usize::try_from(image::number(&mut c)?)
        .map_err(|_| invalid("negative Plan 9 glyph count"))?;
    check(count, l.max_glyphs, "Plan 9 subfont glyphs")?;
    let height = image::number(&mut c)?;
    let ascent = image::number(&mut c)?;
    let mut f = base(height, ascent, false)?;
    let table = count
        .checked_add(1)
        .and_then(|n| n.checked_mul(6))
        .ok_or_else(limit)?;
    check(table, l.max_table_bytes, "Plan 9 character table bytes")?;
    budget.add(count.saturating_mul(2048) + table)?;
    let chars = c.take(table)?;
    if c.p != bytes.len() {
        return Err(invalid("trailing bytes after Plan 9 character table"));
    }
    let mut r = Report::default();
    if image.converted {
        loss(
            &mut r,
            "plan9",
            "image_channels",
            None,
            None,
            "Image color or alpha channels are reduced to glyph coverage",
        )
    }
    budget.add(image.bitmap.as_bytes().len())?;
    let mut unused = image.bitmap.as_bytes().to_vec();
    f.headers.push(Property::new(
        "plan9.sentinel",
        PropertyValue::Bytes(chars[count * 6 + 2..count * 6 + 6].to_vec()),
    ));
    for i in 0..count {
        let a = &chars[i * 6..i * 6 + 6];
        let x = i64::from(u16::from_le_bytes([a[0], a[1]]));
        let right = i64::from(u16::from_le_bytes([
            chars[(i + 1) * 6],
            chars[(i + 1) * 6 + 1],
        ]));
        let top = i64::from(a[2]);
        let bottom = i64::from(a[3]);
        if right < x
            || bottom < top
            || x < image.minx
            || right > image.minx + i64::from(image.bitmap.width())
            || top < image.miny
            || bottom > image.miny + i64::from(image.bitmap.height())
        {
            return Err(invalid("Plan 9 character rectangle is outside image"));
        }
        let w = (right - x) as u32;
        let h = (bottom - top) as u32;
        let n = bitmap_size(w, h, l)?;
        budget.add(n.saturating_mul(
            if matches!(image.bitmap, crate::model::GlyphBitmap::Mono(_)) {
                2
            } else {
                16
            },
        ))?;
        let crop = Bounds::new(
            w,
            h,
            x - image.minx,
            i64::from(image.bitmap.height()) - (bottom - image.miny),
        );
        let mut g = Glyph::new(image.bitmap.crop(crop)?);
        let iy = (top - image.miny) as usize;
        let ix = (x - image.minx) as usize;
        let mono = matches!(image.bitmap, crate::model::GlyphBitmap::Mono(_));
        let stride = if mono {
            (image.bitmap.width() as usize).div_ceil(8)
        } else {
            image.bitmap.width() as usize
        };
        for row in iy..iy + h as usize {
            let data = &mut unused[row * stride..(row + 1) * stride];
            if mono {
                let mut done = 0usize;
                while done < w as usize {
                    let bit = ix + done;
                    let n = (8 - bit % 8).min(w as usize - done);
                    data[bit / 8] &= !((255u8 << (8 - n)) >> (bit % 8));
                    done += n
                }
            } else {
                data[ix..ix + w as usize].fill(0)
            }
        }
        g.metrics.bounds = Bounds::new(w, h, i64::from(a[4] as i8), ascent - bottom);
        g.metrics.advance = Some(Vector::new(i64::from(a[5]), 0));
        g.codepoints.push(Codepoint::Legacy(i as u32));
        f.add_glyph(g)?;
    }
    if unused.iter().any(|&b| b != 0) {
        loss(
            &mut r,
            "plan9",
            "unmapped_image",
            None,
            None,
            "Image contains ink outside every character rectangle",
        );
    }
    f.update_bounds()?;
    Ok((f, r))
}
fn numeric(s: &str) -> Result<i64> {
    let (neg, s) = s.strip_prefix('-').map_or((false, s), |s| (true, s));
    let (base, digits) = if let Some(s) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, s)
    } else if s.len() > 1 && s.starts_with('0') {
        (8, &s[1..])
    } else {
        (10, s)
    };
    let n = i64::from_str_radix(digits, base)
        .map_err(|_| invalid("invalid Plan 9 C-format integer"))?;
    Ok(if neg { -n } else { n })
}
