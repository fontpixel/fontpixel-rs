use super::{Compression, Container, Plan9WriteOptions, image, reader};
use crate::formats::tex_common::*;
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap};
use crate::resources::{
    Input, OutputAsset, OutputBundle, ReadContext, ResourcePath, ResourceResolver, Written,
};
use crate::{Report, Result};
use std::collections::BTreeSet;
/// Encode a complete font/subfont bundle and verify its reader limits before return.
pub fn write(set: &FontSet, primary: &ResourcePath, o: &Plan9WriteOptions) -> Result<Written> {
    let mut report = Report::default();
    match write_inner(set, primary, o, &mut report) {
        Ok(output) => Ok(Written { output, report }),
        Err(error) => Err(write_error(error, report, "plan9")),
    }
}
fn write_inner(
    set: &FontSet,
    primary: &ResourcePath,
    o: &Plan9WriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let f = set.single()?;
    let mut budget = model_budget(f, &o.limits)?;
    budget.add(container_allowance(set, &o.limits)?)?;
    for p in set.properties.iter() {
        loss(
            r,
            "plan9",
            "container_property",
            None,
            None,
            format!("Container property {:?} is not represented", p.name),
        );
    }
    if o.max_glyphs_per_subfont == 0 {
        return Err(invalid("Plan 9 max_glyphs_per_subfont must be positive"));
    }
    let mut maps = Vec::new();
    let mut codes = BTreeSet::new();
    for (id, g) in f.glyphs() {
        if o.container == Container::Subfont {
            maps.push((id.index(), maps.len() as u32, g));
            continue;
        }
        if g.codepoints.is_empty() {
            loss(
                r,
                "plan9",
                "unencoded",
                Some(id.index()),
                None,
                "Unencoded glyph omitted from Unicode font",
            );
        }
        for &cp in &g.codepoints {
            if !matches!(cp, Codepoint::Unicode(_)) || cp.scalar().is_none() {
                loss(
                    r,
                    "plan9",
                    "unrepresentable_mapping",
                    Some(id.index()),
                    Some(cp.value()),
                    "Plan 9 font manifests require Unicode scalar mappings",
                );
                continue;
            }
            if !codes.insert(cp.value()) {
                loss(
                    r,
                    "plan9",
                    "duplicate_mapping",
                    Some(id.index()),
                    Some(cp.value()),
                    "Duplicate codepoint omitted from font manifest",
                );
                continue;
            }
            maps.push((id.index(), cp.value(), g));
        }
    }
    maps.retain(|&(index, cp, g)| {
        let b = g.metrics.bounds;
        let adv = g.metrics.advance.unwrap_or(crate::model::Vector::new(i64::from(b.width), 0));
        let top = i128::from(b.y) + i128::from(b.height);
        let ok = i8::try_from(b.x).is_ok() && u8::try_from(adv.x).is_ok()
            && b.width <= 65535 && b.height <= 255 && b.y >= -255
            && top <= i128::from(i32::MAX);
        if !ok {
            loss(r, "plan9", "unrepresentable_glyph", Some(index), Some(cp),
                "Glyph omitted: bearing, advance, image width or vertical extent exceeds Plan 9 Fontchar fields");
        }
        ok
    });
    if maps.is_empty() {
        return Err(fatal(
            std::mem::take(r),
            "plan9",
            "No representable Plan 9 glyphs",
        ));
    }
    check(maps.len(), o.limits.max_glyphs, "Plan 9 output mappings")?;
    budget.add(maps.len().saturating_mul(256))?;
    if o.container == Container::Font {
        maps.sort_by_key(|v| v.1)
    }
    let global = metrics(f, &maps)?;
    let cap = o.max_output_bytes.min(budget.remaining() / 3);
    let mut assets = Vec::new();
    let mut emitted = Vec::new();
    if o.container == Container::Subfont {
        let bytes = encode(f, &maps, local_metrics(f, &maps)?, o, cap)?;
        check(
            bytes.len(),
            o.limits.max_input_bytes,
            "Plan 9 primary bytes",
        )?;
        emitted.extend_from_slice(&maps);
        assets.push(OutputAsset {
            path: primary.clone(),
            bytes,
        });
    } else {
        let mut manifest = Output::new(cap.min(o.limits.max_input_bytes));
        manifest.put(format!("{} {}\n", global.0, global.1).as_bytes())?;
        let full = primary.as_str();
        let (parent, name) = full.rsplit_once('/').map_or(("", full), |(p, n)| (p, n));
        let stem = name.strip_suffix(".font").unwrap_or(name);
        let mut start = 0;
        let mut n = 0;
        while start < maps.len() {
            let mut end = start + 1;
            let mut width = u64::from(maps[start].2.bitmap.width());
            while end < maps.len()
                && end - start < o.max_glyphs_per_subfont
                && maps[end].1 == maps[end - 1].1 + 1
                && width + u64::from(maps[end].2.bitmap.width()) <= 65535
            {
                let candidate = &maps[start..=end];
                if local_metrics(f, candidate).is_err() {
                    break;
                }
                width += u64::from(maps[end].2.bitmap.width());
                end += 1;
            }
            check(n + 1, o.limits.max_resources, "Plan 9 output sidecars")?;
            let group = &maps[start..end];
            let local = local_metrics(f, group)?;
            let bytes = encode(f, group, local, o, cap.min(o.limits.max_resource_bytes))?;
            budget.add(bytes.len())?;
            let basename = format!("{stem}.{n:04}.subfont");
            let path = ResourcePath::new(if parent.is_empty() {
                basename.clone()
            } else {
                format!("{parent}/{basename}")
            })?;
            let record = format!(
                "0x{:x} 0x{:x} {}\n",
                group[0].1,
                group[group.len() - 1].1,
                basename
            );
            check(
                record.len() - 1,
                o.limits.max_table_bytes,
                "Plan 9 font record",
            )?;
            manifest.put(record.as_bytes())?;
            assets.push(OutputAsset { path, bytes });
            emitted.extend_from_slice(group);
            start = end;
            n += 1;
        }
        assets.push(OutputAsset {
            path: primary.clone(),
            bytes: manifest.b,
        });
    }
    let size = assets.iter().try_fold(0usize, |n, a| {
        n.checked_add(a.bytes.len()).ok_or_else(limit)
    })?;
    check(size, o.max_output_bytes, "Plan 9 output bytes")?;
    budget.add(size)?;
    let bundle = OutputBundle::new(primary.clone(), assets)?;
    let resolver = Assets {
        assets: bundle.assets(),
        parent: primary.as_str().rsplit_once('/').map_or("", |(p, _)| p),
    };
    let raw = &bundle
        .assets()
        .iter()
        .find(|a| a.path == *primary)
        .ok_or_else(|| invalid("missing primary asset"))?
        .bytes;
    let input = Input::new(raw).with_resources(&resolver);
    let mut limits = o.limits.clone();
    limits.max_total_bytes = budget.remaining();
    let mut context = ReadContext::new(&input, &limits)?;
    let (target, rr) = reader::read(&mut context)?;
    r.extend(rr);
    compare_font(f, &target, "plan9", r);
    compare_mapped(&target, &emitted, "plan9", r);
    report_cost(r, &mut budget)?;
    Ok(bundle)
}
struct Assets<'a> {
    assets: &'a [OutputAsset],
    parent: &'a str,
}
impl ResourceResolver for Assets<'_> {
    fn resolve(&self, path: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let full = if self.parent.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.parent, path)
        };
        let a = self
            .assets
            .iter()
            .find(|a| a.path.as_str() == full)
            .ok_or_else(|| crate::Error::MissingResource(path.to_string()))?;
        check(a.bytes.len(), max, "Plan 9 resource bytes")?;
        Ok(a.bytes.clone())
    }
}
fn metrics(f: &Font, maps: &[(usize, u32, &Glyph)]) -> Result<(i64, i64)> {
    let top = maps
        .iter()
        .map(|v| v.2.metrics.bounds.y + i64::from(v.2.bitmap.height()))
        .max()
        .unwrap_or(0)
        .max(0);
    let bottom = maps
        .iter()
        .map(|v| -v.2.metrics.bounds.y)
        .max()
        .unwrap_or(0)
        .max(0);
    let ascent = f.metrics.ascent.unwrap_or(top);
    let height = match f
        .metrics
        .line_height
        .or(f.metrics.pixel_size.map(i64::from))
    {
        Some(height) => height,
        None => ascent
            .checked_add(f.metrics.descent.unwrap_or(bottom))
            .ok_or_else(|| invalid("Plan 9 line metrics overflow"))?,
    };
    if ascent < 0 || height < ascent || height > i64::from(i32::MAX) {
        return Err(invalid(
            "Plan 9 line metrics require 0 <= ascent <= height <= i32::MAX",
        ));
    }
    Ok((height, ascent))
}
fn local_metrics(f: &Font, maps: &[(usize, u32, &Glyph)]) -> Result<(i64, i64)> {
    let global = metrics(f, maps)?;
    let top = maps
        .iter()
        .map(|v| v.2.metrics.bounds.y + i64::from(v.2.bitmap.height()))
        .max()
        .unwrap_or(0)
        .max(0);
    let min_y = maps.iter().map(|v| v.2.metrics.bounds.y).min().unwrap_or(0);
    if global.1 >= top && i128::from(global.1) - i128::from(min_y) <= 255 {
        Ok(global)
    } else if i128::from(top) - i128::from(min_y) <= 255 {
        Ok((global.0.max(top), top))
    } else {
        Err(invalid("Plan 9 subfont glyph vertical range exceeds 255"))
    }
}
fn encode(
    f: &Font,
    maps: &[(usize, u32, &Glyph)],
    metrics: (i64, i64),
    o: &Plan9WriteOptions,
    cap: usize,
) -> Result<Vec<u8>> {
    let (height, ascent) = metrics;

    let width = maps.iter().try_fold(0u32, |v, g| {
        v.checked_add(g.2.bitmap.width()).ok_or_else(limit)
    })?;
    if width > 65535 {
        return Err(invalid("Plan 9 subfont atlas width exceeds u16"));
    }
    let gray = o.gray_threshold.is_none()
        && maps
            .iter()
            .any(|v| matches!(v.2.bitmap, GlyphBitmap::Gray8(_)));
    let stride = if gray {
        width as usize
    } else {
        (width as usize).div_ceil(8)
    };
    let atlas_height = maps
        .iter()
        .map(|v| ascent - v.2.metrics.bounds.y)
        .max()
        .unwrap_or(0);
    if !(0..=255).contains(&atlas_height) {
        return Err(invalid("Plan 9 glyph image bottom outside u8"));
    }
    let size = stride
        .checked_mul(atlas_height as usize)
        .ok_or_else(limit)?;
    check(size, o.limits.max_table_bytes, "Plan 9 atlas bytes")?;
    check(
        size,
        o.limits.max_decompressed_bytes,
        "Plan 9 atlas decompressed bytes",
    )?;
    let table = (maps.len() + 1).checked_mul(6).ok_or_else(limit)?;
    check(
        table,
        o.limits.max_table_bytes,
        "Plan 9 character table bytes",
    )?;
    check(
        size.saturating_add(table).saturating_add(96),
        cap,
        "Plan 9 output subfont bytes",
    )?;
    let mut raster = vec![0u8; size];
    let mut chars = Vec::with_capacity(table);
    let mut x = 0usize;
    for &(_, _, g) in maps {
        let b = g.metrics.bounds;
        let top = ascent - b.y - i64::from(b.height);
        let bottom = ascent - b.y;
        if top < 0 || bottom < top || bottom > 255 {
            return Err(invalid("Plan 9 glyph does not fit subfont height/ascent"));
        }
        let left = i8::try_from(b.x).map_err(|_| invalid("Plan 9 left bearing outside i8"))?;
        let adv = g
            .metrics
            .advance
            .unwrap_or(crate::model::Vector::new(i64::from(b.width), 0));
        let advance = u8::try_from(adv.x).map_err(|_| invalid("Plan 9 advance outside u8"))?;
        chars.extend_from_slice(&(x as u16).to_le_bytes());
        chars.extend_from_slice(&[top as u8, bottom as u8, left as u8, advance]);
        let w = b.width as usize;
        let h = b.height as usize;
        match &g.bitmap {
            GlyphBitmap::Mono(bitmap) => {
                let source_stride = w.div_ceil(8);
                if w > 0 {
                    for y in 0..h {
                        let src = &bitmap.as_bytes()[y * source_stride..(y + 1) * source_stride];
                        let dst = &mut raster
                            [(top as usize + y) * stride..(top as usize + y + 1) * stride];
                        if gray {
                            for (i, value) in dst[x..x + w].iter_mut().enumerate() {
                                *value = if src[i / 8] & (128 >> (i % 8)) != 0 {
                                    255
                                } else {
                                    0
                                }
                            }
                        } else {
                            copy_bits(src, 0, dst, x, w)
                        }
                    }
                }
            }
            GlyphBitmap::Gray8(bitmap) => {
                for y in 0..h {
                    let src = &bitmap.as_bytes()[y * w..(y + 1) * w];
                    let dst =
                        &mut raster[(top as usize + y) * stride..(top as usize + y + 1) * stride];
                    if gray {
                        dst[x..x + w].copy_from_slice(src)
                    } else {
                        let threshold = o.gray_threshold.unwrap_or(128);
                        let mut at = 0;
                        while at < w {
                            if src[at] < threshold {
                                at += 1;
                                continue;
                            }
                            let start = at;
                            while at < w && src[at] >= threshold {
                                at += 1
                            }
                            black(dst, x + start, at - start);
                        }
                    }
                }
            }
        }
        x += w;
    }
    chars.extend_from_slice(&(x as u16).to_le_bytes());
    chars.extend_from_slice(
        bytes(&f.headers, "plan9.sentinel")
            .filter(|v| v.len() == 4)
            .unwrap_or(&[0; 4]),
    );
    let mut out = Output::new(cap);
    if o.compression == Compression::Literal {
        out.put(b"compressed\n")?;
    }
    image::header(&mut out, if gray { "k8" } else { "k1" })?;
    for v in [0, 0, i64::from(width), atlas_height] {
        image::header(&mut out, &v.to_string())?
    }
    if o.compression == Compression::Raw {
        out.put(&raster)?;
    } else {
        let encoded_row = stride.checked_add(stride.div_ceil(128)).ok_or_else(limit)?;
        check(encoded_row, 6000, "Plan 9 compressed row bytes")?;
        if atlas_height > 0 && stride == 0 {
            return Err(invalid(
                "zero-byte rows cannot use Plan 9 compressed blocks",
            ));
        }
        let rows = 6000usize.checked_div(encoded_row).unwrap_or(1);
        let mut y = 0usize;
        while y < atlas_height as usize {
            let end = (y + rows).min(atlas_height as usize);
            image::header(&mut out, &end.to_string())?;
            image::header(&mut out, &((end - y) * encoded_row).to_string())?;
            for row in y..end {
                for chunk in raster[row * stride..(row + 1) * stride].chunks(128) {
                    out.put(&[(127 + chunk.len()) as u8])?;
                    out.put(chunk)?;
                }
            }
            y = end;
        }
    }
    for v in [maps.len() as i64, height, ascent] {
        image::header(&mut out, &v.to_string())?
    }
    out.put(&chars)?;
    Ok(out.b)
}
