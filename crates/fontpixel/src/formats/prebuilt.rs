//! Native bounded Adobe version-3 prebuilt bitmap collections.
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
pub use crate::bitmap::{BitOrder as PrebuiltBitOrder, ByteOrder as PrebuiltByteOrder};
use crate::bitmap::{Bitmap, Gray8};
use crate::model::{FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Rational, Vector};
use crate::report::{Impact, Level, Location};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ReportEntry, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Native grayscale precision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum PrebuiltDepth {
    /// One bit, represented as Mono.
    #[default]
    Mono = 1,
    /// Four grayscale levels.
    Gray2 = 2,
    /// Sixteen grayscale levels.
    Gray4 = 4,
    /// 256 grayscale levels.
    Gray8 = 8,
}
/// Explicit input profile.
#[derive(Debug, Clone)]
pub struct PrebuiltReadOptions {
    /// Require an endian variant; absent follows the header selector.
    pub byte_order: Option<PrebuiltByteOrder>,
    /// Pixel-group significance, independent of numeric byte order. Native NeXT output is MSB-first.
    pub bitmap_bit_order: PrebuiltBitOrder,
}
impl Default for PrebuiltReadOptions {
    fn default() -> Self {
        Self {
            byte_order: None,
            bitmap_bit_order: PrebuiltBitOrder::MsbFirst,
        }
    }
}
/// Native collection output options.
#[derive(Debug, Clone)]
pub struct PrebuiltWriteOptions {
    /// Numeric field byte order.
    pub byte_order: PrebuiltByteOrder,
    /// Pixel-group significance; LSB-first is an explicit compatibility profile.
    pub bitmap_bit_order: PrebuiltBitOrder,
    /// Override all strike depths; absent preserves or infers each strike.
    pub depth: Option<PrebuiltDepth>,
    /// Allow rounding gray samples to the selected native levels.
    pub quantize: bool,
    /// Explicit threshold if a Gray8 glyph is written at one bit.
    pub gray_threshold: Option<u8>,
    /// Override the native character-set name.
    pub charset_name: Option<String>,
    /// Override the font identifier.
    pub identifier: Option<u32>,
    /// Shared ceilings.
    pub limits: ResourceLimits,
    /// Output cap.
    pub max_output_bytes: usize,
}
impl Default for PrebuiltWriteOptions {
    fn default() -> Self {
        Self {
            byte_order: PrebuiltByteOrder::Big,
            bitmap_bit_order: PrebuiltBitOrder::MsbFirst,
            depth: None,
            quantize: false,
            gray_threshold: None,
            charset_name: None,
            identifier: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
pub struct PrebuiltCodec {
    /// Input choices.
    pub read_options: PrebuiltReadOptions,
    /// Output choices.
    pub write_options: PrebuiltWriteOptions,
}
impl FormatDetector for PrebuiltCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PrebuiltCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PrebuiltCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Byte order and version are structural evidence, not a unique magic.
pub fn detect(b: &[u8]) -> Confidence {
    if b.len() < 52 {
        return Confidence::NoMatch;
    }
    let e = if b[..2] == [0, 0] {
        ByteOrder::Little
    } else {
        ByteOrder::Big
    };
    if u16_at(b, 2, e).is_ok_and(|v| v == 3)
        && u16_at(b, 16, e).is_ok_and(|n| n > 0)
        && u16_at(b, 18, e).is_ok_and(|n| n > 0)
    {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn loss(r: &mut Report, code: &str, message: &str, fi: usize) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("prebuilt.{code}"),
        location: Location {
            font_index: Some(fi),
            ..Default::default()
        },
        message: message.into(),
    });
}
fn string<'a>(b: &'a [u8], at: usize, l: &ResourceLimits) -> Result<&'a [u8]> {
    let tail = b
        .get(at..)
        .ok_or_else(|| invalid("Prebuilt string pointer outside file"))?;
    let n = tail
        .iter()
        .take(l.max_table_bytes.saturating_add(1))
        .position(|v| *v == 0)
        .ok_or_else(|| invalid("Prebuilt unterminated or excessive string"))?;
    Ok(&tail[..n])
}
fn values(p: &crate::model::Properties, key: &str, n: usize) -> Result<Option<Vec<i32>>> {
    match p.get(key) {
        None => Ok(None),
        Some(PropertyValue::List(v)) if v.len() == n => v
            .iter()
            .map(|v| match v {
                PropertyValue::Integer(x) => i32::try_from(*x)
                    .map_err(|_| invalid("Prebuilt fixed-point metadata exceeds i32")),
                _ => Err(invalid("Prebuilt fixed-point metadata requires integers")),
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        _ => Err(invalid("Prebuilt fixed-point metadata has the wrong shape")),
    }
}
fn list(data: impl IntoIterator<Item = i32>) -> PropertyValue {
    PropertyValue::List(
        data.into_iter()
            .map(|n| PropertyValue::Integer(i64::from(n)))
            .collect(),
    )
}
/// Decode every matrix and every native character name.
pub fn read(c: &mut ReadContext<'_>, o: &PrebuiltReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &PrebuiltReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 52)?;
    check(b.len(), l.max_table_bytes, "Prebuilt input tables")?;
    let e = if b[..2] == [0, 0] {
        ByteOrder::Little
    } else {
        ByteOrder::Big
    };
    if o.byte_order.is_some_and(|x| x != e) {
        return Err(invalid(
            "Prebuilt byte order differs from requested profile",
        ));
    }
    if u16_at(b, 2, e)? != 3 {
        return Err(invalid("Prebuilt version is not 3"));
    }
    let n = usize::from(u16_at(b, 16, e)?);
    let m = usize::from(u16_at(b, 18, e)?);
    if n > 32767 || m > 32767 {
        return Err(invalid("Prebuilt signed count exceeds 32767"));
    }
    if n == 0 || m == 0 {
        return Err(invalid("Prebuilt collection has no characters or matrices"));
    }
    let total = n
        .checked_mul(m)
        .ok_or_else(|| invalid("Prebuilt count product overflow"))?;
    check(total, l.max_glyphs, "Prebuilt aggregate glyphs")?;
    let offset = |at| u32_at(b, at, e).map(|v| v as usize);
    let fname = string(b, offset(8)?, l)?;
    let charset = string(b, offset(12)?, l)?;
    let widths = slice(b, offset(20)?, n * 8)?;
    let names = slice(b, offset(24)?, offset(28)?)?;
    if names.last() != Some(&0) {
        return Err(invalid("Prebuilt names need a final NUL"));
    }
    budget.items::<&[u8]>(n)?;
    let mut labels = Vec::with_capacity(n);
    let mut pos = 0;
    for _ in 0..n {
        let end = names[pos..]
            .iter()
            .position(|v| *v == 0)
            .ok_or_else(|| invalid("Prebuilt name count exceeds string table"))?
            + pos;
        let label = &names[pos..end];
        if label.is_empty() || labels.last().is_some_and(|previous| *previous >= label) {
            return Err(invalid(
                "Prebuilt character names must be nonempty, unique and sorted",
            ));
        }
        labels.push(label);
        pos = end + 1;
    }
    if pos != names.len() {
        return Err(invalid("Prebuilt excess character names"));
    }
    let matrices = slice(b, offset(32)?, m * 28)?;
    let masks = slice(b, offset(36)?, total * 12)?;
    let vw_at = offset(40)?;
    let vm_at = offset(44)?;
    let vw = if vw_at == 0 {
        None
    } else {
        Some(slice(b, vw_at, n * 8)?)
    };
    let vm = if vm_at == 0 {
        None
    } else {
        Some(slice(b, vm_at, total * 4)?)
    };
    budget.items::<crate::model::Font>(m)?;
    let mut fonts = Vec::with_capacity(m);
    let mut report = Report::default();
    let mut raster_total = 0usize;
    for fi in 0..m {
        let matrix = &matrices[fi * 28..fi * 28 + 28];
        let depth = u32_at(matrix, 24, e)?;
        if !matches!(depth, 1 | 2 | 4 | 8) {
            return Err(invalid("Prebuilt mask depth is not 1, 2, 4 or 8"));
        }
        let mut f = source("prebuilt", matrix, n, l, budget)?;
        f.properties.push(Property::new(
            "prebuilt.bitmap_bit_order",
            PropertyValue::Text(
                if o.bitmap_bit_order == PrebuiltBitOrder::MsbFirst {
                    "msb"
                } else {
                    "lsb"
                }
                .into(),
            ),
        ));
        budget.add(fname.len() * 2 + charset.len() * 2 + 1024)?;
        f.names.family = Some(latin(fname));
        f.properties.push(Property::new(
            "prebuilt.charset",
            PropertyValue::Text(latin(charset)),
        ));
        f.properties.push(Property::new(
            "prebuilt.identifier",
            PropertyValue::Unsigned(u64::from(u32_at(b, 4, e)?)),
        ));
        let mut mv = [0i32; 7];
        for (i, value) in mv.iter_mut().enumerate() {
            *value = u32_at(matrix, i * 4, e)? as i32;
        }
        f.properties
            .push(Property::new("prebuilt.matrix", list(mv)));
        f.metrics.point_size = Some(Rational::new(i64::from(mv[3]), 65536)?);
        if mv[3] != 0 {
            f.metrics.resolution = Some(Vector::new(i64::from(mv[0]) * 75 / i64::from(mv[3]), 75));
        }
        if mv[1] != 0 || mv[2] != 0 || mv[4] != 0 || mv[5] != 0 {
            loss(
                &mut report,
                "affine_matrix",
                "The complete affine matrix is retained; point-size/resolution fields alone do not describe rotation, shear or translation.",
                fi,
            );
        }
        if mv[3] != 0 && (i64::from(mv[0]) * 75) % i64::from(mv[3]) != 0 {
            loss(
                &mut report,
                "fractional_resolution",
                "The exact matrix is retained; integer DPI is rounded toward zero.",
                fi,
            );
        }
        for (ci, label) in labels.iter().enumerate() {
            let mask = &masks[(fi * n + ci) * 12..(fi * n + ci + 1) * 12];
            let w = u32::from(mask[0]);
            let h = u32::from(mask[1]);
            let pixels = w as usize * h as usize;
            check(pixels, l.max_pixels_per_glyph, "Prebuilt glyph pixels")?;
            let row = (w as usize * depth as usize).div_ceil(8);
            let len = row * h as usize;
            raster_total = raster_total
                .checked_add(len)
                .ok_or_else(|| invalid("Prebuilt raster sum overflow"))?;
            let raw = slice(b, u32_at(mask, 8, e)? as usize, len)?;
            let output_row = if depth == 1 {
                (w as usize).div_ceil(8)
            } else {
                w as usize
            };
            budget.add(output_row * h as usize + label.len() * 2 + 1024)?;
            let mut packed = vec![0; output_row * h as usize];
            let levels = (1u16 << depth) - 1;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let bit = x * depth as usize;
                    let shift = if o.bitmap_bit_order == PrebuiltBitOrder::MsbFirst {
                        8 - depth as usize - bit % 8
                    } else {
                        bit % 8
                    };
                    let value = (u16::from(raw[y * row + bit / 8]) >> shift) & levels;
                    if depth == 1 {
                        if value != 0 {
                            packed[y * output_row + x / 8] |= 128 >> (x % 8);
                        }
                    } else {
                        packed[y * output_row + x] = (value * 255 / levels) as u8;
                    }
                }
            }
            let bitmap = if depth == 1 {
                GlyphBitmap::Mono(raster(w, h, &packed, l, budget)?)
            } else {
                budget.add(packed.len())?;
                GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(w, h, &packed, bitmap_limits(l))?)
            };
            let mut g = Glyph::new(bitmap);
            g.name = Some(latin(label));
            let x = -i64::from(mask[2] as i8);
            let y = i64::from(mask[3] as i8) - i64::from(h);
            g.metrics.bounds.x = x;
            g.metrics.bounds.y = y;
            let advance = Vector::new(i64::from(mask[4] as i8), i64::from(mask[5] as i8));
            g.metrics.advance = Some(advance);
            g.metrics.left_bearing = Some(x);
            g.metrics.right_bearing = Some(advance.x - x - i64::from(w));
            let hx = u32_at(widths, ci * 8, e)? as i32;
            let hy = u32_at(widths, ci * 8 + 4, e)? as i32;
            g.metrics.scalable_advance = Some(Vector::new(
                i64::from(hx) * 1000 / 65536,
                i64::from(hy) * 1000 / 65536,
            ));
            g.properties
                .push(Property::new("prebuilt.width", list([hx, hy])));
            g.properties.push(Property::new(
                "prebuilt.mask_pad",
                PropertyValue::Unsigned(u64::from(u16_at(mask, 6, e)?)),
            ));
            if let Some(vw) = vw {
                let vx = u32_at(vw, ci * 8, e)? as i32;
                let vy = u32_at(vw, ci * 8 + 4, e)? as i32;
                g.metrics.vertical_scalable_advance = Some(Vector::new(
                    i64::from(vx) * 1000 / 65536,
                    i64::from(vy) * 1000 / 65536,
                ));
                g.properties
                    .push(Property::new("prebuilt.vertical_width", list([vx, vy])));
            }
            if let Some(vm) = vm {
                let d = &vm[(fi * n + ci) * 4..(fi * n + ci + 1) * 4];
                g.metrics.vertical_advance =
                    Some(Vector::new(i64::from(d[2] as i8), i64::from(d[3] as i8)));
                g.metrics.vertical_origin = Some(Vector::new(
                    i64::from(d[0] as i8) - i64::from(mask[2] as i8),
                    i64::from(mask[3] as i8) - i64::from(d[1] as i8),
                ));
                g.properties.push(Property::new(
                    "prebuilt.vertical_metrics",
                    PropertyValue::Bytes(d.to_vec()),
                ));
            }
            f.add_glyph(g)?;
        }
        if vw.is_some() || vm.is_some() {
            loss(
                &mut report,
                "vertical_interpretation",
                "Native vertical arrays are retained exactly; origin projection uses the documented offset vectors, with no original-tool vertical fixture available.",
                fi,
            );
        }
        // Width conversion may be fractional even though raw fixed values survive.
        if f.glyphs().any(|(_, g)| {
            values(&g.properties, "prebuilt.width", 2)
                .ok()
                .flatten()
                .is_some_and(|v| v.iter().any(|x| (i64::from(*x) * 1000) % 65536 != 0))
        }) {
            loss(
                &mut report,
                "fractional_scalable_width",
                "Exact 16.16 widths are retained; per-1000 integer model widths are rounded toward zero.",
                fi,
            );
        }
        let line = f
            .glyphs()
            .map(|(_, g)| i64::from(g.bitmap.height()))
            .max()
            .unwrap_or(0);
        metrics(&mut f, line);
        fonts.push(f);
    }
    if raster_total != offset(48)? {
        return Err(invalid(
            "Prebuilt declared mask-data total differs from referenced masks",
        ));
    }
    budget.add(b.len() + 256)?;
    let mut s = set(fonts, "prebuilt");
    s.properties.push(Property::new(
        "prebuilt.file",
        PropertyValue::Bytes(b.to_vec()),
    ));
    Ok((s, report))
}
fn latin_bytes(s: &str) -> Result<Vec<u8>> {
    if s.chars().any(|c| c as u32 > 255 || c == '\0') {
        return Err(invalid("Prebuilt names require non-NUL Latin-1"));
    }
    Ok(s.chars().map(|c| c as u8).collect())
}
fn fixed(g: &Glyph, key: &str, metric: Option<Vector>) -> Result<[i32; 2]> {
    let desired = metric.unwrap_or_default();
    if let Some(v) = values(&g.properties, key, 2)?
        && i64::from(v[0]) * 1000 / 65536 == desired.x
        && i64::from(v[1]) * 1000 / 65536 == desired.y
    {
        return Ok([v[0], v[1]]);
    }
    let convert = |n: i64| {
        n.checked_mul(65536)
            .and_then(|v| i32::try_from(v / 1000).ok())
            .ok_or_else(|| invalid("Prebuilt scalable width exceeds 16.16 range"))
    };
    Ok([convert(desired.x)?, convert(desired.y)?])
}
/// Write native endian, grayscale, multi-matrix and vertical arrays.
pub fn write(s: &FontSet, o: &WriteOptions, v: &PrebuiltWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        if s.fonts.is_empty() || s.fonts.len() > 32767 {
            return Err(invalid("Prebuilt requires 1..32767 matrices"));
        }
        budget.items::<BTreeMap<String, &Glyph>>(s.fonts.len())?;
        let mut strikes = Vec::with_capacity(s.fonts.len());
        for f in &s.fonts {
            budget.add(f.len().saturating_mul(256))?;
            let mut map = BTreeMap::new();
            for (id, g) in f.glyphs() {
                let name = g.name.clone().unwrap_or_else(|| {
                    g.codepoints.first().map_or_else(
                        || format!("glyph{}", id.index()),
                        |cp| match cp {
                            crate::model::Codepoint::Legacy(n) => format!("code{n:08X}"),
                            crate::model::Codepoint::Unicode(c) => format!("uni{:04X}", *c),
                        },
                    )
                });
                latin_bytes(&name)?;
                if name.is_empty() || map.insert(name, g).is_some() {
                    return Err(invalid("Prebuilt glyph names must be nonempty and unique"));
                }
            }
            strikes.push(map);
        }
        let n = strikes[0].len();
        let m = strikes.len();
        if n == 0 || n > 32767 {
            return Err(invalid("Prebuilt requires 1..32767 character names"));
        }
        check(n * m, v.limits.max_glyphs, "Prebuilt aggregate glyphs")?;
        for map in &strikes[1..] {
            if !map.keys().eq(strikes[0].keys()) {
                return Err(invalid(
                    "Prebuilt matrices must share the same character-name set",
                ));
            }
        }
        let family = s.fonts[0].names.family.as_deref().unwrap_or("Font");
        let fname = latin_bytes(family)?;
        let charset = v
            .charset_name
            .as_deref()
            .or_else(|| match s.fonts[0].properties.get("prebuilt.charset") {
                Some(PropertyValue::Text(t)) => Some(t.as_str()),
                _ => None,
            })
            .unwrap_or(family);
        let cname = latin_bytes(charset)?;
        for f in &s.fonts[1..] {
            if f.names.family.as_deref().unwrap_or("Font") != family {
                return Err(invalid("Prebuilt matrices must share a family name"));
            }
        }
        budget.add(fname.len() + cname.len() + n * 256)?;
        let mut names = Vec::new();
        for name in strikes[0].keys() {
            names.extend(latin_bytes(name)?);
            names.push(0);
        }
        let vertical_width = strikes
            .iter()
            .flat_map(|s| s.values())
            .any(|g| g.metrics.vertical_scalable_advance.is_some());
        let vertical_metric = strikes
            .iter()
            .flat_map(|s| s.values())
            .any(|g| g.metrics.vertical_advance.is_some() || g.metrics.vertical_origin.is_some());
        let fname_at = 52;
        let cname_at = fname_at + fname.len() + 1;
        let names_at = cname_at + cname.len() + 1;
        let widths_at = (names_at + names.len() + 3) & !3;
        let matrices_at = widths_at + n * 8;
        let masks_at = matrices_at + m * 28;
        let vw_at = if vertical_width {
            masks_at + n * m * 12
        } else {
            0
        };
        let vm_at = if vertical_metric {
            masks_at + n * m * 12 + if vertical_width { n * 8 } else { 0 }
        } else {
            0
        };
        let data_at = masks_at
            + n * m * 12
            + if vertical_width { n * 8 } else { 0 }
            + if vertical_metric { n * m * 4 } else { 0 };
        output_limit(data_at, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; data_at];
        let e = v.byte_order;
        put16(&mut out, 0, if e == ByteOrder::Big { 1 } else { 0 }, e);
        put16(&mut out, 2, 3, e);
        let identifier = v.identifier.unwrap_or(
            u32::try_from(int(&s.fonts[0].properties, "prebuilt.identifier", 0)?)
                .map_err(|_| invalid("Prebuilt identifier exceeds u32"))?,
        );
        put32(&mut out, 4, identifier, e);
        for (at, value) in [
            (8, fname_at),
            (12, cname_at),
            (20, widths_at),
            (24, names_at),
            (28, names.len()),
            (32, matrices_at),
            (36, masks_at),
            (40, vw_at),
            (44, vm_at),
        ] {
            put32(
                &mut out,
                at,
                u32::try_from(value).map_err(|_| invalid("Prebuilt offset exceeds u32"))?,
                e,
            );
        }
        put16(&mut out, 16, n as u16, e);
        put16(&mut out, 18, m as u16, e);
        out[fname_at..fname_at + fname.len()].copy_from_slice(&fname);
        out[cname_at..cname_at + cname.len()].copy_from_slice(&cname);
        out[names_at..names_at + names.len()].copy_from_slice(&names);
        for (ci, (name, g)) in strikes[0].iter().enumerate() {
            let horizontal = fixed(g, "prebuilt.width", g.metrics.scalable_advance)?;
            let vertical = fixed(
                g,
                "prebuilt.vertical_width",
                g.metrics.vertical_scalable_advance,
            )?;
            for map in &strikes[1..] {
                let other = map
                    .get(name)
                    .ok_or_else(|| invalid("Prebuilt shared name missing"))?;
                if fixed(other, "prebuilt.width", other.metrics.scalable_advance)? != horizontal
                    || vertical_width
                        && fixed(
                            other,
                            "prebuilt.vertical_width",
                            other.metrics.vertical_scalable_advance,
                        )? != vertical
                {
                    return Err(invalid(
                        "Prebuilt matrices disagree on shared scalable widths",
                    ));
                }
            }
            for k in 0..2 {
                put32(
                    &mut out,
                    widths_at + ci * 8 + k * 4,
                    horizontal[k] as u32,
                    e,
                );
                if vertical_width {
                    put32(&mut out, vw_at + ci * 8 + k * 4, vertical[k] as u32, e);
                }
            }
        }
        let mut raster_total = 0usize;
        for (fi, (f, map)) in s.fonts.iter().zip(&strikes).enumerate() {
            let existing = values(&f.properties, "prebuilt.matrix", 7)?;
            let inferred = if map
                .values()
                .all(|g| matches!(g.bitmap, GlyphBitmap::Mono(_)))
            {
                1
            } else {
                8
            };
            let depth = v
                .depth
                .map(|d| d as u32)
                .or_else(|| existing.as_ref().map(|a| a[6] as u32))
                .unwrap_or(inferred);
            if !matches!(depth, 1 | 2 | 4 | 8) {
                return Err(invalid("Prebuilt output depth is not 1, 2, 4 or 8"));
            }
            let mut matrix = if let Some(a) = existing {
                a
            } else {
                let p = f.metrics.point_size.unwrap_or(Rational::integer(i64::from(
                    f.metrics.pixel_size.unwrap_or(12),
                )));
                let d = p
                    .numerator()
                    .checked_mul(65536)
                    .and_then(|n| i32::try_from(n / i64::from(p.denominator())).ok())
                    .ok_or_else(|| invalid("Prebuilt point size exceeds fixed-point range"))?;
                vec![d, 0, 0, d, 0, 0, depth as i32]
            };
            matrix[6] = depth as i32;
            for (k, value) in matrix.into_iter().enumerate() {
                put32(&mut out, matrices_at + fi * 28 + k * 4, value as u32, e);
            }
            let mut rounded = false;
            for (ci, g) in map.values().enumerate() {
                let w = g.bitmap.width();
                let h = g.bitmap.height();
                if w > 255 || h > 255 {
                    return Err(invalid("Prebuilt byte dimensions exceed 255"));
                }
                let row = (w as usize * depth as usize).div_ceil(8);
                let len = row * h as usize;
                let start = out.len();
                check(
                    start + len,
                    v.max_output_bytes
                        .min(v.limits.max_input_bytes)
                        .min(v.limits.max_table_bytes),
                    "Prebuilt output bytes",
                )?;
                budget.add(len.saturating_mul(2))?;
                out.resize(start + len, 0);
                raster_total += len;
                let at = masks_at + (fi * n + ci) * 12;
                out[at] = w as u8;
                out[at + 1] = h as u8;
                let hx = -g.metrics.bounds.x;
                let hy = g.metrics.bounds.y + i64::from(h);
                let advance = g.metrics.advance.unwrap_or(Vector::new(i64::from(w), 0));
                for (k, value) in [hx, hy, advance.x, advance.y].into_iter().enumerate() {
                    out[at + 2 + k] = i8::try_from(value)
                        .map_err(|_| invalid("Prebuilt mask offset or advance exceeds i8"))?
                        as u8;
                }
                put16(
                    &mut out,
                    at + 6,
                    u16::try_from(int(&g.properties, "prebuilt.mask_pad", 0)?)
                        .map_err(|_| invalid("Prebuilt padding exceeds u16"))?,
                    e,
                );
                put32(
                    &mut out,
                    at + 8,
                    u32::try_from(start)
                        .map_err(|_| invalid("Prebuilt data pointer exceeds u32"))?,
                    e,
                );
                if vertical_metric {
                    let origin = g.metrics.vertical_origin.unwrap_or_default();
                    let advance = g.metrics.vertical_advance.unwrap_or_default();
                    for (k, value) in [
                        hx.checked_add(origin.x)
                            .ok_or_else(|| invalid("Prebuilt vertical x overflow"))?,
                        hy.checked_sub(origin.y)
                            .ok_or_else(|| invalid("Prebuilt vertical y overflow"))?,
                        advance.x,
                        advance.y,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        out[vm_at + (fi * n + ci) * 4 + k] = i8::try_from(value)
                            .map_err(|_| invalid("Prebuilt vertical metric exceeds i8"))?
                            as u8;
                    }
                }
                let levels = (1u16 << depth) - 1;
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let sample = match &g.bitmap {
                            GlyphBitmap::Mono(b) => {
                                if b.as_bytes()[y * b.stride() + x / 8] & (128 >> (x % 8)) == 0 {
                                    0
                                } else {
                                    255
                                }
                            }
                            GlyphBitmap::Gray8(b) => b.as_bytes()[y * b.stride() + x],
                        };
                        let value = if depth == 1 && matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
                            let threshold = v.gray_threshold.ok_or_else(|| {
                                invalid("Prebuilt Gray8 to mono requires an explicit threshold")
                            })?;
                            rounded |= sample != 0 && sample != 255;
                            u16::from(sample >= threshold)
                        } else {
                            let scaled = u16::from(sample) * levels;
                            if !scaled.is_multiple_of(255) {
                                if !v.quantize {
                                    return Err(invalid(
                                        "Prebuilt grayscale sample is not exactly representable at selected depth",
                                    ));
                                }
                                rounded = true;
                            }
                            (scaled + 127) / 255
                        };
                        let bit = x * depth as usize;
                        let shift = if v.bitmap_bit_order == PrebuiltBitOrder::MsbFirst {
                            8 - depth as usize - bit % 8
                        } else {
                            bit % 8
                        };
                        out[start + y * row + bit / 8] |= (value as u8) << shift;
                    }
                }
            }
            if rounded {
                loss(
                    &mut report,
                    "gray_quantization",
                    "Gray samples were explicitly quantized to the selected native precision.",
                    fi,
                );
            }
        }
        put32(
            &mut out,
            48,
            u32::try_from(raster_total).map_err(|_| invalid("Prebuilt data length exceeds u32"))?,
            e,
        );
        let (actual, r) = parse(
            &out,
            &PrebuiltReadOptions {
                bitmap_bit_order: v.bitmap_bit_order,
                ..Default::default()
            },
            &v.limits,
            &mut budget,
        )?;
        report.extend(r);
        audit(s, &actual, "prebuilt", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "prebuilt", o.strict)
}
