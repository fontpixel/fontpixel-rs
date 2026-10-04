//! Bounded native TeX PXL raster fonts and optional TFM width sidecars.
use super::historical_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Property, PropertyValue, Rational, Vector};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Report, ResourceLimits, Result};
/// PXL storage alignment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PxlVersion {
    /// Word pointers and 32-bit row padding.
    #[default]
    V1001,
    /// Byte pointers and 8-bit row padding.
    V1002,
}
impl PxlVersion {
    fn id(self) -> u32 {
        match self {
            Self::V1001 => 1001,
            Self::V1002 => 1002,
        }
    }
    fn unit(self) -> usize {
        if self == Self::V1001 { 4 } else { 1 }
    }
}
/// Optional explicit TFM resource; no implicit filename lookup.
#[derive(Debug, Clone, Default)]
pub struct PxlReadOptions {
    /// Require one PXL identifier.
    pub version: Option<PxlVersion>,
    /// Apply this bounded TFM width table.
    pub tfm: Option<ResourcePath>,
}
/// Bounded output and optional TFM generation.
#[derive(Debug, Clone)]
pub struct PxlWriteOptions {
    /// Raster variant.
    pub version: PxlVersion,
    /// Optional TFM output path.
    pub tfm: Option<ResourcePath>,
    /// Magnification units (five per dpi); absent preserves source or uses 1000.
    pub magnification: Option<u32>,
    /// Explicit numeric Unicode projection.
    pub unicode_as_legacy: bool,
    /// Gray reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Aggregate output size cap.
    pub max_output_bytes: usize,
}
impl Default for PxlWriteOptions {
    fn default() -> Self {
        Self {
            version: PxlVersion::V1001,
            tfm: None,
            magnification: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
pub struct PxlCodec {
    /// Input choices.
    pub read_options: PxlReadOptions,
    /// Output choices.
    pub write_options: PxlWriteOptions,
}
impl FormatDetector for PxlCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PxlCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PxlCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn variant(b: &[u8]) -> Result<PxlVersion> {
    match u32_at(b, 0, ByteOrder::Big)? {
        1001 => Ok(PxlVersion::V1001),
        1002 => Ok(PxlVersion::V1002),
        _ => Err(invalid("unrecognized PXL identifier")),
    }
}
/// Recognize a supported PXL identifier.
pub fn detect(b: &[u8]) -> Confidence {
    if variant(b).is_ok() {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn integer(p: &crate::model::Properties, key: &str, default: i64) -> Result<i64> {
    match p.get(key) {
        None => Ok(default),
        Some(PropertyValue::Integer(x)) => Ok(*x),
        Some(PropertyValue::Unsigned(x)) => {
            i64::try_from(*x).map_err(|_| invalid("PXL property overflow"))
        }
        _ => Err(invalid(format!("{key} must be integer"))),
    }
}
fn rounded(n: i128, d: i128) -> Result<i64> {
    if d <= 0 {
        return Err(invalid("invalid PXL metric divisor"));
    }
    i64::try_from(if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    })
    .map_err(|_| invalid("PXL metric overflow"))
}
fn advances(tfm: i32, design: u32, mag: u32) -> Result<(i64, i64)> {
    Ok((
        rounded(
            i128::from(tfm) * i128::from(design) * i128::from(mag) * 100,
            (1i128 << 40) * 5 * 7227,
        )?,
        rounded(i128::from(tfm) * 1000, 1 << 20)?,
    ))
}
/// Decode directory pointers, fixed-point metrics and explicit sidecar data.
pub fn read(c: &mut ReadContext<'_>, o: &PxlReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    let tfm = if let Some(p) = &o.tfm {
        let raw = c.resource_limited(p, budget.max.saturating_sub(budget.used))?;
        budget.add(raw.len())?;
        Some(raw)
    } else {
        None
    };
    parse(
        c.bytes(),
        o.version,
        tfm.as_deref(),
        c.limits(),
        &mut budget,
    )
}
fn parse(
    b: &[u8],
    require: Option<PxlVersion>,
    tfm: Option<&[u8]>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let v = variant(b)?;
    if require.is_some_and(|r| r != v) {
        return Err(invalid("PXL identifier differs from requested version"));
    }
    if b.len() < 2072 {
        return Err(invalid("truncated PXL directory/postamble"));
    }
    let post = b.len() - 20;
    let dir = post - 2048;
    if u32_at(b, post + 16, ByteOrder::Big)? != v.id() {
        return Err(invalid("PXL closing identifier mismatch"));
    }
    let pointer = u32_at(b, post + 12, ByteOrder::Big)? as usize;
    let pd = pointer
        .checked_mul(v.unit())
        .ok_or_else(|| invalid("PXL pointer overflow"))?;
    if pd != dir || dir < 4 {
        return Err(invalid("PXL directory pointer mismatch"));
    }
    let checksum = u32_at(b, post, ByteOrder::Big)?;
    let mag = u32_at(b, post + 4, ByteOrder::Big)?;
    let design = u32_at(b, post + 8, ByteOrder::Big)?;
    if mag == 0 || design == 0 || design > i32::MAX as u32 {
        return Err(invalid(
            "PXL magnification/design must be positive fixed-point values",
        ));
    }
    let mut f = source("pxl", b, 128, l, budget)?;
    for (k, x) in [
        ("pxl.checksum", checksum),
        ("pxl.magnification", mag),
        ("pxl.design_size", design),
    ] {
        f.properties
            .push(Property::new(k, PropertyValue::Unsigned(u64::from(x))));
    }
    f.metrics.point_size = Some(Rational::new(i64::from(design), 1 << 20)?);
    let dpi = rounded(i128::from(mag), 5)?;
    f.metrics.resolution = Some(Vector::new(dpi, dpi));
    let ppem = rounded(
        i128::from(design) * i128::from(mag) * 100,
        (1i128 << 20) * 5 * 7227,
    )?;
    f.metrics.ppem_x = u32::try_from(ppem).ok();
    f.metrics.ppem_y = u32::try_from(ppem).ok();
    for cp in 0..128 {
        let p = dir + cp * 16;
        let entry = &b[p..p + 16];
        if entry.iter().all(|x| *x == 0) {
            continue;
        }
        let w = u32::from(u16_at(b, p, ByteOrder::Big)?);
        let h = u32::from(u16_at(b, p + 2, ByteOrder::Big)?);
        let x = i64::from(u16_at(b, p + 4, ByteOrder::Big)? as i16);
        let y = i64::from(u16_at(b, p + 6, ByteOrder::Big)? as i16);
        let ptr = u32_at(b, p + 8, ByteOrder::Big)? as usize;
        let tfmw = u32_at(b, p + 12, ByteOrder::Big)? as i32;
        let row = (w as usize).div_ceil(8);
        let stored = row.div_ceil(v.unit()) * v.unit();
        let n = stored
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("PXL raster size overflow"))?;
        let bitmap = if ptr == 0 {
            if w != 0 || h != 0 || x != 0 || y != 0 {
                return Err(invalid(
                    "zero PXL pointer requires empty dimensions and offsets",
                ));
            }
            raster(0, 0, &[], l, budget)?
        } else {
            let at = ptr
                .checked_mul(v.unit())
                .ok_or_else(|| invalid("PXL raster pointer overflow"))?;
            if at < 4 || at.checked_add(n).is_none_or(|end| end > dir) {
                return Err(invalid("PXL raster pointer overlaps header or directory"));
            }
            check(
                w as usize * h as usize,
                l.max_pixels_per_glyph,
                "PXL pixels",
            )?;
            budget.add(row * h as usize)?;
            let mut packed = vec![0; row * h as usize];
            for j in 0..h as usize {
                packed[j * row..(j + 1) * row].copy_from_slice(slice(b, at + j * stored, row)?);
            }
            raster(w, h, &packed, l, budget)?
        };
        let (advance, scalable) = advances(tfmw, design, mag)?;
        glyph(
            &mut f,
            bitmap,
            cp as u32,
            -x,
            if ptr == 0 { 0 } else { y + 1 - i64::from(h) },
            advance,
        )?;
        let id = f
            .glyph_id(Codepoint::Legacy(cp as u32))
            .ok_or_else(|| invalid("PXL glyph index"))?;
        f.edit_glyph(id, |g| {
            g.metrics.scalable_advance = Some(Vector::new(scalable, 0));
            g.properties.push(Property::new(
                "tex.tfm_width",
                PropertyValue::Integer(i64::from(tfmw)),
            ));
            Ok(())
        })?;
    }
    let mut report = Report::default();
    if let Some(t) = tfm {
        apply_tfm(&mut f, t, checksum, design, mag, l, budget, &mut report)?;
    }
    Ok((set(f, "pxl"), report))
}
#[allow(clippy::too_many_arguments)]
fn apply_tfm(
    f: &mut Font,
    t: &[u8],
    checksum: u32,
    design: u32,
    mag: u32,
    l: &ResourceLimits,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<()> {
    check(t.len(), l.max_table_bytes, "TFM table")?;
    slice(t, 0, 24)?;
    let mut fields = [0usize; 12];
    for (i, n) in fields.iter_mut().enumerate() {
        *n = usize::from(u16_at(t, i * 2, ByteOrder::Big)?)
    }
    let [lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np] = fields;
    if fields.iter().any(|n| *n >= 32768)
        || lh < 2
        || bc > ec + 1
        || ec > 255
        || nw == 0
        || nw > 256
        || nh == 0
        || nd == 0
        || ni == 0
        || ne > 256
    {
        return Err(invalid("TFM header ranges invalid"));
    }
    let nc = if ec >= bc { ec - bc + 1 } else { 0 };
    if lf != 6 + lh + nc + nw + nh + nd + ni + nl + nk + ne + np || t.len() != lf * 4 {
        return Err(invalid("TFM word counts disagree"));
    }
    if (u32_at(t, 24, ByteOrder::Big)? != 0
        && checksum != 0
        && u32_at(t, 24, ByteOrder::Big)? != checksum)
        || u32_at(t, 28, ByteOrder::Big)? != design
        || design < (1 << 20)
    {
        return Err(invalid("TFM/PXL checksum or design-size mismatch"));
    }
    let chars = (6 + lh) * 4;
    let widths = chars + nc * 4;
    for at in [
        widths,
        widths + nw * 4,
        widths + (nw + nh) * 4,
        widths + (nw + nh + nd) * 4,
    ] {
        if u32_at(t, at, ByteOrder::Big)? != 0 {
            return Err(invalid("TFM zero metric entries must be zero"));
        }
    }
    for i in 0..nw {
        let value = u32_at(t, widths + i * 4, ByteOrder::Big)? as i32;
        if !(-(16 << 20)..(16 << 20)).contains(&value) {
            return Err(invalid("TFM width exceeds sixteen design units"));
        }
    }
    let mut layout = false;
    for cp in bc..=ec {
        let info = &t[chars + (cp - bc) * 4..chars + (cp - bc + 1) * 4];
        let wi = usize::from(info[0]);
        if wi >= nw
            || usize::from(info[1] >> 4) >= nh
            || usize::from(info[1] & 15) >= nd
            || usize::from(info[2] >> 2) >= ni
        {
            return Err(invalid("TFM character metric index outside table"));
        }
        if wi == 0 {
            continue;
        }
        layout |= info[2] & 3 != 0 || info[3] != 0;
        let raw = u32_at(t, widths + wi * 4, ByteOrder::Big)? as i32;
        if let Some(id) = f.glyph_id(Codepoint::Legacy(cp as u32)) {
            let (advance, scalable) = advances(raw, design, mag)?;
            f.edit_glyph(id, |g| {
                g.metrics.advance = Some(Vector::new(advance, 0));
                g.metrics.right_bearing =
                    Some(advance - g.metrics.bounds.x - i64::from(g.bitmap.width()));
                g.metrics.scalable_advance = Some(Vector::new(scalable, 0));
                g.properties
                    .set("tex.tfm_width", PropertyValue::Integer(i64::from(raw)));
                Ok(())
            })?;
        }
    }
    budget.add(t.len() + 256)?;
    f.properties
        .push(Property::new("pxl.tfm", PropertyValue::Bytes(t.to_vec())));
    if layout || nh > 1 || nd > 1 || ni > 1 || nl > 0 || nk > 0 || ne > 0 || np > 0 {
        budget.loss(report,"pxl","read.tfm_layout",Some(0),None,"TFM height, depth, italic, ligature, kern and parameter semantics remain in the original TFM bytes; only advances are applied")?;
    }
    Ok(())
}
fn tfm_bytes(
    widths: &[Option<i32>; 128],
    checksum: u32,
    design: u32,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    if design < (1 << 20) {
        return Err(invalid("TFM design must be at least one point"));
    }
    budget.add(1024)?;
    let mut unique = vec![0i32];
    for w in widths.iter().flatten() {
        if !(-(16 << 20)..(16 << 20)).contains(w) {
            return Err(invalid("TFM width exceeds sixteen design units"));
        }
        if !unique[1..].contains(w) {
            unique.push(*w)
        }
    }
    let n = (6 + 2 + 128 + unique.len() + 3) * 4;
    budget.add(n)?;
    let mut out = vec![0; n];
    for (i, v) in [n / 4, 2, 0, 127, unique.len(), 1, 1, 1, 0, 0, 0, 0]
        .into_iter()
        .enumerate()
    {
        put16(&mut out, i * 2, v as u16, ByteOrder::Big);
    }
    put32(&mut out, 24, checksum, ByteOrder::Big);
    put32(&mut out, 28, design, ByteOrder::Big);
    for (i, w) in widths.iter().enumerate() {
        if let Some(w) = w {
            out[32 + i * 4] = (unique[1..].iter().position(|x| x == w).unwrap_or(0) + 1) as u8;
        }
    }
    for (i, w) in unique.iter().enumerate() {
        put32(&mut out, 32 + 512 + i * 4, *w as u32, ByteOrder::Big);
    }
    Ok(out)
}
/// Encode native PXL rasters and an optional minimal, matching TFM sidecar.
pub fn write(s: &FontSet, o: &WriteOptions, v: &PxlWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(f, 127, v.unicode_as_legacy, "pxl", &mut report, &mut budget)?;
        let mag = v.magnification.unwrap_or(
            u32::try_from(integer(&f.properties, "pxl.magnification", 1000)?)
                .map_err(|_| invalid("PXL magnification overflow"))?,
        );
        if mag == 0 {
            return Err(invalid("PXL magnification is zero"));
        }
        let checksum = u32::try_from(integer(&f.properties, "pxl.checksum", 0)?)
            .map_err(|_| invalid("PXL checksum exceeds u32"))?;
        let point = f.metrics.point_size.unwrap_or(Rational::integer(10));
        let design = u32::try_from(rounded(
            i128::from(point.numerator()) * (1 << 20),
            i128::from(point.denominator()),
        )?)
        .map_err(|_| invalid("PXL design size out of range"))?;
        if design == 0 || design > i32::MAX as u32 {
            return Err(invalid(
                "PXL design size is not positive signed fixed-point",
            ));
        }
        let mut size = 2072usize;
        for g in slots.values() {
            let b = mono(g)?;
            let row =
                (b.width() as usize).div_ceil(8).div_ceil(v.version.unit()) * v.version.unit();
            size = size
                .checked_add(
                    row.checked_mul(b.height() as usize)
                        .ok_or_else(|| invalid("PXL glyph size overflow"))?,
                )
                .ok_or_else(|| invalid("PXL file size overflow"))?;
        }
        output_limit(size, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; size];
        let dir = size - 2068;
        let post = size - 20;
        put32(&mut out, 0, v.version.id(), ByteOrder::Big);
        let mut at = 4;
        let mut widths = [None; 128];
        for (&cp, g) in &slots {
            let b = mono(g)?;
            let w = u16::try_from(b.width()).map_err(|_| invalid("PXL width exceeds u16"))?;
            let h = u16::try_from(b.height()).map_err(|_| invalid("PXL height exceeds u16"))?;
            let x = i16::try_from(-g.metrics.bounds.x)
                .map_err(|_| invalid("PXL x offset exceeds i16"))?;
            let y = i16::try_from(g.metrics.bounds.y + i64::from(h) - 1)
                .map_err(|_| invalid("PXL y offset exceeds i16"))?;
            let old = integer(&g.properties, "tex.tfm_width", 0)?;
            let tfmw = if let Some(scaled) = g.metrics.scalable_advance {
                if rounded(i128::from(old) * 1000, 1 << 20)? == scaled.x {
                    old
                } else {
                    rounded(i128::from(scaled.x) * (1 << 20), 1000)?
                }
            } else {
                let adv = g
                    .metrics
                    .advance
                    .or(f.metrics.glyph_defaults.advance)
                    .map_or(i64::from(w), |a| a.x);
                rounded(
                    i128::from(adv) * (1i128 << 40) * 5 * 7227,
                    i128::from(design) * i128::from(mag) * 100,
                )?
            };
            let tfmw = i32::try_from(tfmw).map_err(|_| invalid("PXL TFM width exceeds i32"))?;
            widths[cp as usize] = Some(tfmw);
            let p = dir + cp as usize * 16;
            let empty = w == 0 || h == 0;
            if !empty {
                put16(&mut out, p, w, ByteOrder::Big);
                put16(&mut out, p + 2, h, ByteOrder::Big);
                put16(&mut out, p + 4, x as u16, ByteOrder::Big);
                put16(&mut out, p + 6, y as u16, ByteOrder::Big);
                put32(
                    &mut out,
                    p + 8,
                    u32::try_from(at / v.version.unit())
                        .map_err(|_| invalid("PXL pointer exceeds u32"))?,
                    ByteOrder::Big,
                );
            }
            put32(&mut out, p + 12, tfmw as u32, ByteOrder::Big);
            let row = b.stride().div_ceil(v.version.unit()) * v.version.unit();
            for j in 0..b.height() as usize {
                out[at..at + b.stride()]
                    .copy_from_slice(&b.as_bytes()[j * b.stride()..(j + 1) * b.stride()]);
                at += row;
            }
        }
        for (off, x) in [
            (0, checksum),
            (4, mag),
            (8, design),
            (
                12,
                u32::try_from(dir / v.version.unit())
                    .map_err(|_| invalid("PXL directory pointer exceeds u32"))?,
            ),
            (16, v.version.id()),
        ] {
            put32(&mut out, post + off, x, ByteOrder::Big);
        }
        let tfm = if v.tfm.is_some() {
            check(1, v.limits.max_resources, "PXL sidecars")?;
            let t = tfm_bytes(&widths, checksum, design, &mut budget)?;
            check(
                t.len(),
                v.limits.max_resource_bytes.min(v.limits.max_table_bytes),
                "TFM output bytes",
            )?;
            check(
                size.saturating_add(t.len()),
                v.max_output_bytes,
                "PXL aggregate output bytes",
            )?;
            Some(t)
        } else {
            None
        };
        let (actual, r) = parse(
            &out,
            Some(v.version),
            tfm.as_deref(),
            &v.limits,
            &mut budget,
        )?;
        report.extend(r);
        audit(s, &actual, "pxl", &mut report, &mut budget)?;
        let mut assets = vec![OutputAsset {
            path: o.primary_name.clone(),
            bytes: out,
        }];
        if let (Some(path), Some(bytes)) = (&v.tfm, tfm) {
            assets.push(OutputAsset {
                path: path.clone(),
                bytes,
            });
        }
        OutputBundle::new(o.primary_name.clone(), assets)
    })();
    finish(result, report, "pxl", o.strict)
}
