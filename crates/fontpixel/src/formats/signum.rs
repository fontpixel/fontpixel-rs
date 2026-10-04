//! Native Signum! editor and printer character sets.
use super::historical_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, FontSet, Property, PropertyValue, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Signum! editor/printer raster family.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SignumVersion {
    /// Sixteen-column editor font.
    #[default]
    E24,
    /// Nine-needle printer.
    P9,
    /// Twenty-four-needle printer.
    P24,
    /// Laser printer.
    L30,
}
impl SignumVersion {
    fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::E24 => b"eset0001",
            Self::P9 => b"ps090001",
            Self::P24 => b"ps240001",
            Self::L30 => b"ls300001",
        }
    }
    fn metrics(self) -> (i64, i64) {
        match self {
            Self::E24 => (18, 24),
            Self::P9 => (36, 48),
            Self::P24 => (58, 64),
            Self::L30 => (48, 52),
        }
    }
}
/// Explicit overrides for Signum's inferred baseline convention.
#[derive(Debug, Clone, Default)]
pub struct SignumReadOptions {
    /// Require this format variant; absent detects magic.
    pub version: Option<SignumVersion>,
    /// Override the conventional baseline.
    pub baseline: Option<i64>,
    /// Override line height.
    pub line_height: Option<i64>,
}
/// Bounded Signum output choices.
#[derive(Debug, Clone)]
pub struct SignumWriteOptions {
    /// Variant, default editor E24.
    pub version: SignumVersion,
    /// Permit Unicode numbers as legacy positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource bounds.
    pub limits: ResourceLimits,
    /// Output cap, bounded by reader limits as well.
    pub max_output_bytes: usize,
}
impl Default for SignumWriteOptions {
    fn default() -> Self {
        Self {
            version: SignumVersion::E24,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec for all four Signum families.
#[derive(Debug, Clone, Default)]
pub struct SignumCodec {
    /// Input interpretation.
    pub read_options: SignumReadOptions,
    /// Output choices.
    pub write_options: SignumWriteOptions,
}
impl FormatDetector for SignumCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for SignumCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for SignumCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn version(b: &[u8]) -> Result<SignumVersion> {
    [
        SignumVersion::E24,
        SignumVersion::P9,
        SignumVersion::P24,
        SignumVersion::L30,
    ]
    .into_iter()
    .find(|v| b.starts_with(v.magic()))
    .ok_or_else(|| invalid("unrecognized Signum magic"))
}
/// Recognize the eight-byte editor or printer signature.
pub fn detect(b: &[u8]) -> Confidence {
    if version(b).is_ok() {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Decode the authoritative character offsets and retain opaque fields.
pub fn read(c: &mut ReadContext<'_>, o: &SignumReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &SignumReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let v = version(b)?;
    if o.version.is_some_and(|x| x != v) {
        return Err(invalid("Signum variant does not match requested family"));
    }
    slice(b, 0, 652)?;
    let typ = u32_at(b, 8, ByteOrder::Big)?;
    if typ != 16 && typ != 128 {
        return Err(invalid("unsupported Signum font type"));
    }
    let size = u32_at(b, 140, ByteOrder::Big)? as usize;
    let data = slice(b, 652, size)?;
    if b.len() != 652 + size {
        return Err(invalid(
            "Signum character-buffer size disagrees with file length",
        ));
    }
    let mut f = source("signum", b, 127, l, budget)?;
    f.properties.push(Property::new(
        "signum.header",
        PropertyValue::Bytes(b[..144].to_vec()),
    ));
    let (baseline, line) = v.metrics();
    let baseline = o.baseline.unwrap_or(baseline);
    let line = o.line_height.unwrap_or(line);
    if !(0..=65535).contains(&baseline) || !(1..=65535).contains(&line) {
        return Err(invalid(
            "Signum baseline/line-height outside supported bounds",
        ));
    }
    for cp in 0..127 {
        let at = u32_at(b, 144 + cp * 4, ByteOrder::Big)? as usize;
        let header = slice(data, at, 4)?;
        let h = u32::from(header[1]);
        let row = if v == SignumVersion::E24 {
            2
        } else {
            usize::from(header[2])
        };
        let n = row * h as usize;
        let raw = slice(data, at + 4, n)?;
        let pad = if !n.is_multiple_of(2) {
            slice(data, at + 4 + n, 1)?[0]
        } else {
            0
        };
        if h == 0 && header[2] == 0 && header[3] == 0 {
            continue;
        }
        let w = row as u32 * 8;
        let full = raster(w, h, raw, l, budget)?;
        let width = full
            .bbox()
            .width
            .saturating_add(full.bbox().x.max(0) as u32);
        budget.add((width as usize).div_ceil(8) * h as usize)?;
        let bitmap: Bitmap<Mono> = full.crop_bounds(Bounds::new(width, h, 0, 0))?;
        let advance = if v == SignumVersion::E24 {
            i64::from(header[2])
        } else {
            i64::from(width)
        };
        glyph(
            &mut f,
            bitmap,
            cp as u32,
            0,
            baseline - i64::from(header[0]) - i64::from(h),
            advance,
        )?;
        let id = f
            .glyph_id(Codepoint::Legacy(cp as u32))
            .ok_or_else(|| invalid("Signum missing glyph"))?;
        f.edit_glyph(id, |g| {
            g.properties.push(Property::new(
                "signum.fields",
                PropertyValue::Bytes(vec![row as u8, header[3], pad]),
            ));
            Ok(())
        })?;
    }
    metrics(&mut f, baseline, line);
    if v != SignumVersion::E24 {
        f.metrics.resolution = Some(Vector::new(300, 300));
    }
    Ok((set(f, "signum"), Report::default()))
}
/// Encode one editor/printer set and audit all representational changes.
pub fn write(s: &FontSet, o: &WriteOptions, v: &SignumWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(
            f,
            126,
            v.unicode_as_legacy,
            "signum",
            &mut report,
            &mut budget,
        )?;
        let mut size: usize = 652 + 127 * 4;
        for g in slots.values() {
            let row = if v.version == SignumVersion::E24 {
                2
            } else {
                (g.bitmap.width() as usize).div_ceil(8)
            };
            let n = row
                .checked_mul(g.bitmap.height() as usize)
                .ok_or_else(|| invalid("Signum raster overflow"))?;
            size = size
                .checked_add(n + n % 2)
                .ok_or_else(|| invalid("Signum output overflow"))?;
        }
        output_limit(size, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; size];
        if let Some(header) = bytes(f, "signum.header") {
            out[..144].copy_from_slice(slice(header, 0, 144)?);
        }
        out[..8].copy_from_slice(v.version.magic());
        if u32_at(&out, 8, ByteOrder::Big)? == 0 {
            put32(&mut out, 8, 128, ByteOrder::Big);
        }
        put32(&mut out, 140, (size - 652) as u32, ByteOrder::Big);
        let baseline = v.version.metrics().0;
        let mut off = 652;
        for cp in 0..127 {
            put32(&mut out, 144 + cp * 4, (off - 652) as u32, ByteOrder::Big);
            let g = slots.get(&(cp as u32)).copied();
            let h = g.map_or(0, |g| g.bitmap.height());
            let w = g.map_or(0, |g| g.bitmap.width());
            let row = if v.version == SignumVersion::E24 {
                2
            } else {
                (w as usize).div_ceil(8)
            };
            let top = g.map_or(0, |g| baseline - g.metrics.bounds.y - i64::from(h));
            out[off] = u8::try_from(top).map_err(|_| invalid("Signum top exceeds u8"))?;
            out[off + 1] = u8::try_from(h).map_err(|_| invalid("Signum height exceeds u8"))?;
            out[off + 2] = if v.version == SignumVersion::E24 {
                u8::try_from(
                    g.and_then(|g| g.metrics.advance.or(f.metrics.glyph_defaults.advance))
                        .map_or(0, |a| a.x),
                )
                .map_err(|_| invalid("Signum editor advance exceeds u8"))?
            } else {
                u8::try_from(row).map_err(|_| invalid("Signum printer row exceeds u8"))?
            };
            let fields = g.and_then(|g| match g.properties.get("signum.fields") {
                Some(PropertyValue::Bytes(b)) => Some(b),
                _ => None,
            });
            out[off + 3] = fields.and_then(|v| v.get(1)).copied().unwrap_or(0);
            off += 4;
            let raw = project(
                g,
                Bounds::new(row as u32 * 8, h, 0, baseline - top - i64::from(h)),
                &v.limits,
                &mut budget,
            )?;
            out[off..off + raw.len()].copy_from_slice(&raw);
            off += raw.len();
            if raw.len() % 2 != 0 {
                out[off] = fields.and_then(|v| v.get(2)).copied().unwrap_or(0);
                off += 1;
            }
        }
        let (target, rr) = parse(&out, &Default::default(), &v.limits, &mut budget)?;
        report.extend(rr);
        audit(s, &target, "signum", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "signum", o.strict)
}
