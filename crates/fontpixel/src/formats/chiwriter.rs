//! Native bounded ChiWriter v3/v4 and explicit FONTRIX-compatible records.
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{Codepoint, FontSet, Property, PropertyValue};
use crate::report::{Impact, Level, Location};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ReportEntry, ResourceLimits, Result};
/// Width-directory generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ChiwriterVersion {
    /// The original 94-entry width directory at byte 250.
    V3,
    /// A 256-entry width directory at byte 88.
    #[default]
    V4,
}
/// Explicit interpretation of compatible bitmap directories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ChiwriterLayout {
    /// ChiWriter stores equal-sized records consecutively.
    #[default]
    Contiguous,
    /// Compatible directory words at byte 61 are absolute file offsets.
    AbsoluteOffsets,
    /// Compatible directory words are relative to byte 344.
    RelativeOffsets,
}
/// Input interpretation. Compatible offset origins are never guessed.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ChiwriterReadOptions {
    /// Override the width-table generation.
    pub version: Option<ChiwriterVersion>,
    /// Bitmap record arrangement.
    pub layout: ChiwriterLayout,
    /// Explicit row stride; absent uses the minimum row size.
    pub stride: Option<u8>,
}
/// Native output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ChiwriterWriteOptions {
    /// Width-table generation.
    pub version: ChiwriterVersion,
    /// Compatible bitmap directory arrangement.
    pub layout: ChiwriterLayout,
    /// Explicit row stride.
    pub stride: Option<u8>,
    /// Cell width; absent preserves the source or uses the widest glyph.
    pub width: Option<u8>,
    /// Cell height; absent preserves the source or uses the tallest glyph.
    pub height: Option<u8>,
    /// Distance from the cell top to baseline.
    pub baseline: Option<u8>,
    /// V3 baseline-to-next-cell-top distance.
    pub line_gap: Option<u8>,
    /// Latin-1 filename, at most thirteen bytes.
    pub filename: Option<String>,
    /// Permit numeric Unicode positions.
    pub unicode_as_legacy: bool,
    /// Explicit gray-to-mono threshold.
    pub gray_threshold: Option<u8>,
    /// Shared ceilings.
    pub limits: ResourceLimits,
    /// Output byte cap.
    pub max_output_bytes: usize,
}
impl Default for ChiwriterWriteOptions {
    fn default() -> Self {
        Self {
            version: ChiwriterVersion::V4,
            layout: ChiwriterLayout::Contiguous,
            stride: None,
            width: None,
            height: None,
            baseline: None,
            line_gap: None,
            filename: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ChiwriterCodec {
    /// Input choices.
    pub read_options: ChiwriterReadOptions,
    /// Output choices.
    pub write_options: ChiwriterWriteOptions,
}
impl FormatDetector for ChiwriterCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for ChiwriterCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for ChiwriterCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Weak signature plus an allocation-free directory/raster-extent check.
pub fn detect(b: &[u8]) -> Confidence {
    if b.len() < 344 || !matches!(b[0], 0x10 | 0x11) {
        return Confidence::NoMatch;
    }
    let n = usize::from(b[16]);
    let first = usize::from(b[17]) + 32;
    if n == 0 || first + n > 256 {
        return Confidence::NoMatch;
    }
    let row = usize::from(b[19]).div_ceil(8);
    let height = usize::from(b[20]);
    if row == 0 || height == 0 {
        return if b.len() == 344 {
            Confidence::Possible
        } else {
            Confidence::NoMatch
        };
    }
    let payload = b.len() - 344;
    let rows = n * height;
    // A complete contiguous file also identifies a possible explicit padded stride.
    if payload.is_multiple_of(rows) && (row..=255).contains(&(payload / rows)) {
        return Confidence::Possible;
    }
    if b[0] != 0x10 || n > 94 {
        return Confidence::NoMatch;
    }
    // Compatible offset profiles may reorder or alias records. An addressed record
    // must reach the end of the input, so unrelated trailing data is not evidence.
    for base in [0usize, 344] {
        let mut last = 344;
        let mut valid = true;
        for i in 0..n {
            let at = 61 + i * 2;
            let start = base + usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
            if start < 344 || start > b.len() {
                valid = false;
                break;
            }
            last = last.max(start);
        }
        if valid {
            let remaining = b.len() - last;
            if remaining.is_multiple_of(height) && (row..=255).contains(&(remaining / height)) {
                return Confidence::Possible;
            }
        }
    }
    Confidence::NoMatch
}

fn generation(b: &[u8], v: Option<ChiwriterVersion>) -> Result<ChiwriterVersion> {
    if let Some(v) = v {
        return Ok(v);
    }
    match b[0] {
        0x11 => Ok(ChiwriterVersion::V4),
        0x10 => Ok(if b[16] > 94 {
            ChiwriterVersion::V4
        } else {
            ChiwriterVersion::V3
        }),
        _ => Err(invalid("ChiWriter file type is neither 0x10 nor 0x11")),
    }
}
/// Decode bounded rows and retain the entire original record.
pub fn read(c: &mut ReadContext<'_>, o: &ChiwriterReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &ChiwriterReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 344)?;
    let v = generation(b, o.version)?;
    let n = usize::from(b[16]);
    let first = usize::from(b[17]) + 32;
    if n == 0 || first + n > 256 || (v == ChiwriterVersion::V3 && n > 94) {
        return Err(invalid(
            "ChiWriter character directory exceeds its code range",
        ));
    }
    if o.layout != ChiwriterLayout::Contiguous && (v != ChiwriterVersion::V3 || n > 94) {
        return Err(invalid(
            "ChiWriter compatible offsets overlap the v4 width directory",
        ));
    }
    let w = u32::from(b[19]);
    let h = u32::from(b[20]);
    let row = (w as usize).div_ceil(8);
    let stride = o.stride.map(usize::from).unwrap_or(row);
    if stride < row {
        return Err(invalid("ChiWriter stride is narrower than its cell"));
    }
    let size = stride * h as usize;
    let widths = if v == ChiwriterVersion::V4 {
        88 + first
    } else {
        250
    };
    slice(b, widths, n)?;
    let y = if b[15] == 0 {
        0
    } else {
        i64::from(b[15]) - i64::from(h)
    };
    let mut f = source("chiwriter", b, n + usize::from(first != 32), l, budget)?;
    f.names.full = Some(
        latin(&b[1..14])
            .split('\0')
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("")
            .into(),
    );
    f.properties.push(Property::new(
        "chiwriter.header",
        PropertyValue::Bytes(b[..344].to_vec()),
    ));
    let mut report = Report::default();
    if first != 32 {
        budget.add(row * h as usize)?;
        let space_width = w.min(u32::from(if b[22] == 0 { b[19] } else { b[22] }));
        let blank = vec![0; (space_width as usize).div_ceil(8) * h as usize];
        let bitmap = raster(space_width, h, &blank, l, budget)?;
        glyph(
            &mut f,
            bitmap,
            32,
            0,
            y,
            i64::from(if b[22] == 0 { b[19] } else { b[22] }),
        )?;
    }
    let mut end = 344;
    for i in 0..n {
        let at = match o.layout {
            ChiwriterLayout::Contiguous => 344 + i * size,
            ChiwriterLayout::AbsoluteOffsets => {
                usize::from(u16_at(b, 61 + i * 2, ByteOrder::Little)?)
            }
            ChiwriterLayout::RelativeOffsets => {
                344 + usize::from(u16_at(b, 61 + i * 2, ByteOrder::Little)?)
            }
        };
        if size > 0 && at < 344 {
            return Err(invalid("ChiWriter bitmap offset overlaps its header"));
        }
        let raw = slice(b, at, size)?;
        end = end.max(at + size);
        let advance = if b[widths + i] == 0 {
            b[22]
        } else {
            b[widths + i]
        };
        let visible = w.min(u32::from(advance));
        let packed_row = (visible as usize).div_ceil(8);
        budget.add(packed_row * h as usize + raw.len() + 256)?;
        let mut packed = vec![0; packed_row * h as usize];
        let mut clipped = false;
        for iy in 0..h as usize {
            for ix in 0..w as usize {
                if raw[iy * stride + ix / 8] & (128 >> (ix % 8)) != 0 {
                    if ix < visible as usize {
                        packed[iy * packed_row + ix / 8] |= 128 >> (ix % 8);
                    } else {
                        clipped = true;
                    }
                }
            }
        }
        let cp = (first + i) as u32;
        if clipped {
            report.push(ReportEntry { level: Level::Warning, impact: Impact::Loss,
                code: "chiwriter.hidden_ink".into(), location: Location { font_index: Some(0), codepoint: Some(cp), ..Default::default() },
                message: "Ink beyond the character advance is retained in raw metadata but excluded from the visible bitmap.".into() });
        }
        let bitmap = raster(visible, h, &packed, l, budget)?;
        glyph(&mut f, bitmap, cp, 0, y, i64::from(advance))?;
        let id = f
            .glyph_id(Codepoint::Legacy(cp))
            .ok_or_else(|| invalid("ChiWriter glyph index"))?;
        f.edit_glyph(id, |g| {
            g.properties.push(Property::new(
                "chiwriter.raster",
                PropertyValue::Bytes(raw.to_vec()),
            ));
            Ok(())
        })?;
    }
    if o.layout == ChiwriterLayout::Contiguous && end != b.len() {
        return Err(invalid("ChiWriter bytes lie outside its bitmap records"));
    }
    let line = if v == ChiwriterVersion::V3 && b[24] != 0 {
        i64::from(b[15]) + i64::from(b[24])
    } else {
        i64::from(h)
    };
    metrics(&mut f, line);
    Ok((set(vec![f], "chiwriter"), report))
}
/// Encode a bounded native file and audit its actual reread.
pub fn write(s: &FontSet, o: &WriteOptions, v: &ChiwriterWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(
            f,
            255,
            v.unicode_as_legacy,
            "chiwriter",
            &mut report,
            &mut budget,
        )?;
        if slots.keys().any(|cp| *cp < 32) {
            return Err(invalid("ChiWriter cannot encode codes below 32"));
        }
        let first = *slots.keys().find(|cp| **cp != 32).unwrap_or(&32);
        let last = *slots
            .keys()
            .next_back()
            .ok_or_else(|| invalid("ChiWriter needs encoded glyphs"))?;
        let n = (last - first + 1) as usize;
        if n > 255 || v.version == ChiwriterVersion::V3 && n > 94 {
            return Err(invalid("ChiWriter width directory is too small"));
        }
        if v.layout != ChiwriterLayout::Contiguous && v.version != ChiwriterVersion::V3 {
            return Err(invalid("ChiWriter offsets overlap v4 width directory"));
        }
        let old = bytes(f, "chiwriter.header").filter(|b| b.len() == 344);
        let w = match v.width.or_else(|| old.map(|b| b[19])) {
            Some(w) => w,
            None => u8::try_from(f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0))
                .map_err(|_| invalid("ChiWriter cell width exceeds 255"))?,
        };
        let h = match v.height.or_else(|| old.map(|b| b[20])) {
            Some(h) => h,
            None => u8::try_from(
                f.glyphs()
                    .map(|(_, g)| g.bitmap.height())
                    .max()
                    .unwrap_or(0),
            )
            .map_err(|_| invalid("ChiWriter cell height exceeds 255"))?,
        };
        let baseline = match v.baseline.or_else(|| old.map(|b| b[15])) {
            Some(b) => b,
            None => u8::try_from(f.metrics.ascent.unwrap_or(i64::from(h)))
                .map_err(|_| invalid("ChiWriter baseline exceeds u8"))?,
        };
        let y = if baseline == 0 {
            0
        } else {
            i64::from(baseline) - i64::from(h)
        };
        let row = usize::from(w).div_ceil(8);
        let stride = v.stride.map(usize::from).unwrap_or(row);
        if stride < row {
            return Err(invalid("ChiWriter stride narrower than cell"));
        }
        let size = stride * usize::from(h);
        let total = 344 + n * size;
        if v.layout != ChiwriterLayout::Contiguous && total > 65535 {
            return Err(invalid("ChiWriter compatible offsets exceed 16 bits"));
        }
        output_limit(total, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; total];
        if let Some(old) = old {
            out[..344].copy_from_slice(old);
        }
        out[0] = if v.version == ChiwriterVersion::V4 {
            0x11
        } else {
            0x10
        };
        out[15] = baseline;
        out[16] = n as u8;
        out[17] = (first - 32) as u8;
        out[18] = 1;
        out[19] = w;
        out[20] = h;
        out[21] = v.stride.unwrap_or(0);
        out[22] = u8::try_from(
            slots
                .get(&32)
                .and_then(|g| g.metrics.advance)
                .map_or(i64::from(w), |a| a.x),
        )
        .map_err(|_| invalid("ChiWriter space width exceeds u8"))?;
        out[24] = if v.version == ChiwriterVersion::V3 {
            v.line_gap.or_else(|| old.map(|b| b[24])).unwrap_or(0)
        } else {
            0
        };
        if let Some(name) = &v.filename {
            if name.chars().count() > 13 || name.chars().any(|c| c as u32 > 255 || c == '\0') {
                return Err(invalid(
                    "ChiWriter filename needs at most 13 non-NUL Latin-1 bytes",
                ));
            }
            out[1..14].fill(0);
            for (i, c) in name.chars().enumerate() {
                out[1 + i] = c as u8;
            }
        } else if old.is_none() {
            out[1..14].copy_from_slice(b"FONT    .SET ");
        }
        let widths = if v.version == ChiwriterVersion::V4 {
            88 + first as usize
        } else {
            250
        };
        if v.version == ChiwriterVersion::V4 {
            out[88..344].fill(0);
        } else {
            out[250..344].fill(0);
        }
        for i in 0..n {
            let g = slots.get(&(first + i as u32)).copied();
            let advance = g
                .and_then(|g| g.metrics.advance)
                .map_or(i64::from(w), |a| a.x);
            out[widths + i] =
                u8::try_from(advance).map_err(|_| invalid("ChiWriter advance exceeds u8"))?;
            // Zero width is represented only when the default is also zero.
            if advance == 0 && out[22] != 0 {
                return Err(invalid(
                    "ChiWriter zero advance conflicts with nonzero default width",
                ));
            }
            let at = 344 + i * size;
            if v.layout != ChiwriterLayout::Contiguous {
                put16(
                    &mut out,
                    61 + i * 2,
                    (at - if v.layout == ChiwriterLayout::RelativeOffsets {
                        344
                    } else {
                        0
                    }) as u16,
                    ByteOrder::Little,
                );
            }
            let packed = project(
                g,
                Bounds::new(u32::from(w), u32::from(h), 0, y),
                &v.limits,
                &mut budget,
            )?;
            let original = g.and_then(|g| match g.properties.get("chiwriter.raster") {
                Some(PropertyValue::Bytes(b)) if b.len() == size => Some(b.as_slice()),
                _ => None,
            });
            if let Some(raw) = original {
                out[at..at + size].copy_from_slice(raw);
            }
            let visible = g.map_or(u32::from(w), |g| g.bitmap.width());
            for iy in 0..usize::from(h) {
                for ix in 0..usize::from(w) {
                    if ix >= visible as usize && original.is_some() {
                        continue;
                    }
                    let mask = 128 >> (ix % 8);
                    out[at + iy * stride + ix / 8] &= !mask;
                    if packed[iy * row + ix / 8] & mask != 0 {
                        out[at + iy * stride + ix / 8] |= mask;
                    }
                }
            }
        }
        let ro = ChiwriterReadOptions {
            version: Some(v.version),
            layout: v.layout,
            stride: v.stride,
        };
        let (actual, r) = parse(&out, &ro, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &actual, "chiwriter", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "chiwriter", o.strict)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&ChiwriterVersion::V3), "v3");
        assert_eq!(round_trip(&ChiwriterVersion::V4), "v4");
        rejects::<ChiwriterVersion>(r#""not_a_variant""#);
        assert_eq!(round_trip(&ChiwriterLayout::Contiguous), "contiguous");
        assert_eq!(
            round_trip(&ChiwriterLayout::AbsoluteOffsets),
            "absolute_offsets"
        );
        assert_eq!(
            round_trip(&ChiwriterLayout::RelativeOffsets),
            "relative_offsets"
        );
        rejects::<ChiwriterLayout>(r#""not_a_variant""#);
        defaults::<ChiwriterReadOptions>();
        defaults::<ChiwriterWriteOptions>();
        defaults::<ChiwriterCodec>();
    }
}
