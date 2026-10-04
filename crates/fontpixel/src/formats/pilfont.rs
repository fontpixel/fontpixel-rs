//! Native Pillow PILfont metrics with an independently encoded mask atlas.
use super::image_bundle_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Property, PropertyValue, Vector};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Error, Report, ResourceLimits, Result};
/// Native input resource selection.
#[derive(Debug, Clone, Default)]
pub struct PilfontReadOptions {
    /// Explicit authorized mask image; otherwise try the native same-stem names.
    pub atlas: Option<ResourcePath>,
}
/// Native PILfont output options.
#[derive(Debug, Clone)]
pub struct PilfontWriteOptions {
    /// Maximum atlas shelf width, default 800; glyphs wider than this fail.
    pub max_width: u32,
    /// Explicit numerical projection from Unicode values into native byte slots.
    pub unicode_as_legacy: bool,
    /// Cumulative input, atlas, output and readback budget.
    pub limits: ResourceLimits,
    /// Additional aggregate output cap.
    pub max_output_bytes: usize,
}
impl Default for PilfontWriteOptions {
    fn default() -> Self {
        Self {
            max_width: 800,
            unicode_as_legacy: false,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(PilfontCodec, PilfontReadOptions, PilfontWriteOptions);
/// Recognize the native signature.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"PILfont\n") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
struct Directory {
    header: Vec<u8>,
    info: Vec<u8>,
    height: i64,
    records: Vec<(u32, [i16; 10])>,
}
fn line<'a>(b: &'a [u8], at: &mut usize) -> Result<&'a [u8]> {
    let tail = slice(b, *at, b.len().saturating_sub(*at))?;
    let n = tail
        .iter()
        .position(|&c| c == b'\n')
        .ok_or_else(|| invalid("unterminated PILfont header line"))?;
    *at += n + 1;
    Ok(&tail[..n])
}
fn header_height(b: &[u8]) -> Result<i64> {
    let field = b
        .split(|&c| c == b';')
        .nth(6)
        .ok_or_else(|| invalid("PILfont metadata misses height"))?;
    let h = std::str::from_utf8(field)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&h| h > 0 && h <= i64::from(i32::MAX))
        .ok_or_else(|| invalid("invalid PILfont line height"))?;
    Ok(h)
}
fn directory(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<Directory> {
    check(b.len(), l.max_table_bytes, "PILfont metrics bytes")?;
    if !b.starts_with(b"PILfont\n") {
        return Err(invalid("invalid PILfont signature"));
    }
    let mut at = 8;
    let header = line(b, &mut at)?;
    let height = header_height(header)?;
    let info_at = at;
    loop {
        let start = at;
        let s = line(b, &mut at)?;
        if s == b"DATA" {
            let info = &b[info_at..start];
            slice(b, at, 5120)?;
            if at + 5120 != b.len() {
                return Err(invalid("PILfont directory length mismatch"));
            }
            budget.add(header.len().saturating_add(info.len()).saturating_mul(2))?;
            budget.items::<(u32, [i16; 10])>(256)?;
            let mut records = Vec::new();
            for cp in 0..256u32 {
                let mut rec = [0; 10];
                for (j, v) in rec.iter_mut().enumerate() {
                    *v = r16(b, at + cp as usize * 20 + j * 2)? as i16;
                }
                if rec.iter().all(|&v| v == 0) {
                    continue;
                }
                let dx = i32::from(rec[4]) - i32::from(rec[2]);
                let dy = i32::from(rec[5]) - i32::from(rec[3]);
                let sx = i32::from(rec[8]) - i32::from(rec[6]);
                let sy = i32::from(rec[9]) - i32::from(rec[7]);
                if sx < 0 || sy < 0 || dx != sx || dy != sy || rec[6] < 0 || rec[7] < 0 {
                    return Err(invalid("invalid/unscaled PILfont rectangles"));
                }
                records.push((cp, rec));
            }
            allocation(records.len(), l, budget)?;
            return Ok(Directory {
                header: header.to_vec(),
                info: info.to_vec(),
                height,
                records,
            });
        }
    }
}
fn model(
    d: Directory,
    r: Raster,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    if r.color != image_codec::ColorType::L8 {
        return Err(invalid(
            "PILfont atlas must be a monochrome or eight-bit grayscale mask",
        ));
    }
    let mut f = Font::default();
    f.metrics.line_height = Some(d.height);
    f.properties.push(Property::new(
        "pilfont.header",
        PropertyValue::Bytes(d.header),
    ));
    f.properties
        .push(Property::new("pilfont.info", PropertyValue::Bytes(d.info)));
    for (cp, m) in d.records {
        let w = (i32::from(m[8]) - i32::from(m[6])) as u32;
        let h = (i32::from(m[9]) - i32::from(m[7])) as u32;
        if m[8] as u32 > r.width || m[9] as u32 > r.height {
            return Err(invalid("PILfont source rectangle exceeds atlas"));
        }
        let n = area(w, h)?;
        check(n, l.max_pixels_per_glyph, "PILfont glyph pixels")?;
        budget.add(n)?;
        let mut coverage = Vec::with_capacity(n);
        for y in 0..if w == 0 { 0 } else { h } {
            let start = ((m[7] as usize + y as usize) * r.width as usize + m[6] as usize) * 4;
            for p in r.pixels[start..start + w as usize * 4].as_chunks::<4>().0 {
                coverage.push(p[0]);
            }
        }
        let mut g = glyph(
            w,
            h,
            i64::from(m[2]),
            -i64::from(m[5]),
            Vector::new(i64::from(m[0]), -i64::from(m[1])),
            &coverage,
            false,
            l,
            budget,
        )?;
        g.codepoints.push(Codepoint::Legacy(cp));
        f.add_glyph(g)?;
    }
    Ok((set(f, "pilfont"), Report::default()))
}
fn atlas_name(name: &str, extension: &str) -> Result<ResourcePath> {
    let basename = name.rfind('/').map_or(0, |i| i + 1);
    let stem = name[basename..]
        .rfind('.')
        .filter(|&i| i > 0)
        .map_or(name, |i| &name[..basename + i]);
    ResourcePath::new(format!("{stem}.{extension}"))
}
/// Read the native directory and a caller-authorized atlas.
pub fn read(c: &mut ReadContext<'_>, v: &PilfontReadOptions) -> Result<(FontSet, Report)> {
    let l = c.limits().clone();
    let mut budget = Budget::new(c.retained_input_bytes(), &l)?;
    budget.add(
        c.name()
            .map_or(0, str::len)
            .saturating_add(v.atlas.as_ref().map_or(0, |p| p.as_str().len()))
            .saturating_mul(8),
    )?;
    let d = directory(c.bytes(), &l, &mut budget)?;
    let paths = if let Some(path) = &v.atlas {
        vec![path.clone()]
    } else {
        let name = c
            .name()
            .ok_or_else(|| invalid("PILfont needs input filename or atlas option"))?;
        ["png", "gif", "pbm"]
            .into_iter()
            .map(|ext| atlas_name(name, ext))
            .collect::<Result<Vec<_>>>()?
    };
    let mut selected = None;
    for path in paths {
        match c.resource_limited(&path, budget.max.saturating_sub(budget.used)) {
            Ok(bytes) => {
                budget.add(bytes.len())?;
                selected = Some((path, bytes));
                break;
            }
            Err(Error::MissingResource(_)) => {}
            Err(e) => return Err(e),
        }
    }
    let (path, bytes) =
        selected.ok_or_else(|| Error::MissingResource("PILfont mask atlas".into()))?;
    let raster = decode(&bytes, path.as_str(), &l, &mut budget)?;
    model(d, raster, &l, &mut budget)
}
fn bytes_property<'a>(f: &'a Font, key: &str) -> Result<Option<&'a [u8]>> {
    match f.properties.get(key) {
        None => Ok(None),
        Some(PropertyValue::Bytes(b)) => Ok(Some(b)),
        _ => Err(invalid(format!("{key} must be bytes"))),
    }
}
/// Encode real PILfont plus native PNG mask sidecar, then audit a bounded readback.
pub fn write(s: &FontSet, o: &WriteOptions, v: &PilfontWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        budget.add(
            o.primary_name
                .as_str()
                .len()
                .saturating_mul(8)
                .saturating_add(256),
        )?;
        for ext in ["png", "gif", "pbm"] {
            if atlas_name(o.primary_name.as_str(), ext)? == o.primary_name {
                return Err(invalid(
                    "PILfont primary name shadows a native atlas lookup name",
                ));
            }
        }
        let f = s.single()?;
        let map = select(f, false, 255, v.unicode_as_legacy, &mut budget)?;
        check(map.len(), v.limits.max_glyphs, "PILfont output slots")?;
        if v.max_width == 0 || v.max_width > 32767 {
            return Err(invalid("PILfont atlas width must be 1..32767"));
        }
        let line_height = f
            .metrics
            .line_height
            .or(f.metrics.pixel_size.map(i64::from))
            .unwrap_or_else(|| {
                map.values()
                    .map(|g| i64::from(g.bitmap.height()))
                    .max()
                    .unwrap_or(1)
                    .max(1)
            });
        if line_height <= 0 || line_height > i64::from(i32::MAX) {
            return Err(invalid("invalid PILfont line height"));
        }
        let header = if let Some(raw) = bytes_property(f, "pilfont.header")? {
            if raw.contains(&b'\n') || header_height(raw)? != line_height {
                return Err(invalid("PILfont header contradicts line height"));
            }
            raw.to_vec()
        } else {
            format!(";;;;;;{line_height};").into_bytes()
        };
        let info = bytes_property(f, "pilfont.info")?.unwrap_or(&[]);
        if (!info.is_empty() && !info.ends_with(b"\n"))
            || info.split(|&c| c == b'\n').any(|s| s == b"DATA")
        {
            return Err(invalid("invalid PILfont information lines"));
        }
        budget.add(header.len().saturating_mul(2))?;
        budget.items::<(u32, u32, u32)>(map.len())?;
        let mut placements = Vec::with_capacity(map.len());
        let (mut x, mut y, mut shelf, mut width) = (0u32, 0u32, 0u32, 1u32);
        for (&cp, g) in &map {
            let (w, h) = (g.bitmap.width(), g.bitmap.height());
            if w > v.max_width || h > 32767 {
                return Err(invalid("PILfont glyph exceeds atlas field bounds"));
            }
            if x + w > v.max_width {
                x = 0;
                y = y
                    .checked_add(shelf)
                    .ok_or_else(|| invalid("PILfont atlas height overflow"))?;
                shelf = 0
            }
            placements.push((cp, x, y));
            x += w;
            width = width.max(x);
            shelf = shelf.max(h);
        }
        let height = y
            .checked_add(shelf)
            .ok_or_else(|| invalid("PILfont atlas height overflow"))?
            .max(1);
        if height > 32767 {
            return Err(invalid("PILfont atlas exceeds signed coordinate range"));
        }
        let n = area(width, height)?;
        check(n, v.limits.max_decompressed_bytes, "PILfont atlas pixels")?;
        budget.add(n)?;
        let mut pixels = vec![0; n];
        let head = 8 + header.len() + 1 + info.len() + 5;
        let mut metrics = output(
            head.saturating_add(5120),
            v.max_output_bytes,
            &v.limits,
            &mut budget,
        )?;
        metrics[..8].copy_from_slice(b"PILfont\n");
        metrics[8..8 + header.len()].copy_from_slice(&header);
        metrics[8 + header.len()] = b'\n';
        metrics[9 + header.len()..head - 5].copy_from_slice(info);
        metrics[head - 5..head].copy_from_slice(b"DATA\n");
        for (cp, x, y) in placements {
            let g = map[&cp];
            let (w, h) = (g.bitmap.width(), g.bitmap.height());
            let adv = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .unwrap_or(Vector::new(i64::from(w), 0));
            let top = -(g.metrics.bounds.y + i64::from(h));
            let bottom = -g.metrics.bounds.y;
            let values = [
                adv.x,
                adv.y
                    .checked_neg()
                    .ok_or_else(|| invalid("PILfont vertical advance overflow"))?,
                g.metrics.bounds.x,
                top,
                g.metrics.bounds.x + i64::from(w),
                bottom,
                i64::from(x),
                i64::from(y),
                i64::from(x + w),
                i64::from(y + h),
            ];
            for (j, value) in values.into_iter().enumerate() {
                let n = i16::try_from(value)
                    .map_err(|_| invalid("PILfont field exceeds signed i16"))?;
                let at = head + cp as usize * 20 + j * 2;
                metrics[at..at + 2].copy_from_slice(&n.to_be_bytes());
            }
            for gy in 0..if w == 0 { 0 } else { h } {
                for gx in 0..w {
                    pixels[((y + gy) * width + x + gx) as usize] = pixel(g, gx, gy);
                }
            }
        }
        let atlas = png(
            width,
            height,
            &pixels,
            false,
            v.max_output_bytes,
            &v.limits,
            &mut budget,
        )?;
        check(
            metrics.len().saturating_add(atlas.len()),
            v.max_output_bytes,
            "PILfont aggregate output",
        )?;
        check(3, v.limits.max_resources, "PILfont atlas lookup operations")?;
        let dir = directory(&metrics, &v.limits, &mut budget)?;
        let raster = decode(&atlas, "atlas.pbm", &v.limits, &mut budget)?;
        let (target, r) = model(dir, raster, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "pilfont", &mut report, &mut budget)?;
        let path = atlas_name(o.primary_name.as_str(), "pbm")?;
        if path == o.primary_name {
            return Err(invalid(
                "PILfont primary name must differ from .pbm sidecar",
            ));
        }
        OutputBundle::new(
            o.primary_name.clone(),
            vec![
                OutputAsset {
                    path: o.primary_name.clone(),
                    bytes: metrics,
                },
                OutputAsset { path, bytes: atlas },
            ],
        )
    })();
    finish(result, report, "pilfont", o.strict)
}
