//! SymbOS medium, small and big FNT resources, including 255-character metadata.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Native record layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SymbosVersion {
    /// One width byte and fifteen raster rows.
    Medium,
    /// One width byte and eight raster rows.
    Small,
    /// Relative offsets and variable-width columns, up to 48 rows.
    Big,
}
/// Interpretation of the second byte in a 255-character file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(rename_all = "snake_case", deny_unknown_fields)
)]
pub enum SymbosExtendedInfo {
    /// Native writing-system identifier, 2 through 31.
    WritingSystem(u8),
    /// Alternative header with WYSIWYG flag.
    Layout {
        /// Whether this font is intended for WYSIWYG rendering.
        wysiwyg: bool,
    },
}
/// Explicit input compatibility choices.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SymbosReadOptions {
    /// Accept a short medium/small character range; native files normally end at 127 or 255.
    pub allow_short_range: bool,
}
/// Native bounded output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SymbosWriteOptions {
    /// Layout; absent preserves source layout or uses medium.
    pub version: Option<SymbosVersion>,
    /// Initial code for ordinary fonts; absent preserves source value or uses 32.
    pub first_code: Option<u8>,
    /// 255-character metadata; absent preserves source extension if present.
    pub extended: Option<SymbosExtendedInfo>,
    /// Cell height; absent derives a cell containing all input ink.
    pub height: Option<u8>,
    /// Explicit permission to use Unicode numbers as legacy codes.
    pub unicode_as_legacy: bool,
    /// Explicit threshold for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Shared model, geometry and allocation limits.
    pub limits: ResourceLimits,
    /// Output length cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for SymbosWriteOptions {
    fn default() -> Self {
        Self {
            version: None,
            first_code: None,
            extended: None,
            height: None,
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
pub struct SymbosCodec {
    /// Input choices.
    pub read_options: SymbosReadOptions,
    /// Output choices.
    pub write_options: SymbosWriteOptions,
}
impl FormatDetector for SymbosCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        let b = c.bytes();
        Ok(
            if complete_structure(b, self.read_options.allow_short_range) {
                Confidence::Possible
            } else {
                Confidence::NoMatch
            },
        )
    }
}
impl FormatReader<FontSet> for SymbosCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for SymbosCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
// No native magic exists: require every record and its final boundary before
// offering even Possible evidence. The bounded structural walk allocates nothing.
fn complete_structure(b: &[u8], allow_short: bool) -> bool {
    let Some((&flags, rest)) = b.split_first() else {
        return false;
    };
    let Some(&second) = rest.first() else {
        return false;
    };
    let big = flags & 64 != 0;
    let small = !big && flags & 32 != 0;
    let height = usize::from(flags & if big { 63 } else { 15 });
    if height == 0
        || height
            > if big {
                48
            } else if small {
                8
            } else {
                15
            }
        || !big && flags & 16 != 0
    {
        return false;
    }
    let count = if flags & 128 != 0 {
        if !(2..=31).contains(&second) && second & !32 != 128 {
            return false;
        }
        255
    } else {
        if second > 127 {
            return false;
        }
        128 - usize::from(second)
    };
    if !big {
        let size = if small { 9 } else { 16 };
        let actual = (b.len() - 2) / size;
        return (b.len() - 2).is_multiple_of(size)
            && (actual == count || allow_short && actual > 0 && actual < count)
            && b[2..].chunks_exact(size).all(|glyph| glyph[0] <= 8);
    }
    let table_end = 2 + count * 2;
    if b.len() < table_end {
        return false;
    }
    let mut end = table_end;
    for i in 0..count {
        let word = 2 + 2 * i;
        let mut at = word + 1 + usize::from(u16::from_le_bytes([b[word], b[word + 1]]));
        if at < table_end {
            return false;
        }
        let Some(&width) = b.get(at) else {
            return false;
        };
        at += 1;
        let mut x = 0;
        loop {
            let Some(&column) = b.get(at) else {
                return false;
            };
            at += 1;
            if column == 0 {
                break;
            }
            if column > 8 || x + usize::from(column) > usize::from(width) || !x.is_multiple_of(8) {
                return false;
            }
            at += height;
            if at > b.len() {
                return false;
            }
            x += usize::from(column);
        }
        if x != usize::from(width) {
            return false;
        }
        end = end.max(at);
    }
    end == b.len()
}
fn extended(v: u8) -> Result<SymbosExtendedInfo> {
    if v & 0x80 != 0 {
        if v & 0x5f != 0 {
            return Err(invalid("SymbOS reserved extension bits"));
        }
        Ok(SymbosExtendedInfo::Layout {
            wysiwyg: v & 32 != 0,
        })
    } else if (2..=31).contains(&v) {
        Ok(SymbosExtendedInfo::WritingSystem(v))
    } else {
        Err(invalid("SymbOS writing system must be 2..31"))
    }
}
fn extension_byte(v: SymbosExtendedInfo) -> Result<u8> {
    match v {
        SymbosExtendedInfo::WritingSystem(v) if (2..=31).contains(&v) => Ok(v),
        SymbosExtendedInfo::Layout { wysiwyg } => Ok(128 | if wysiwyg { 32 } else { 0 }),
        _ => Err(invalid("SymbOS writing system must be 2..31")),
    }
}
fn header(b: &[u8]) -> Result<(SymbosVersion, u8, u8, usize)> {
    slice(b, 0, 2)?;
    let version = if b[0] & 64 != 0 {
        SymbosVersion::Big
    } else if b[0] & 32 != 0 {
        SymbosVersion::Small
    } else {
        SymbosVersion::Medium
    };
    let height = b[0]
        & if version == SymbosVersion::Big {
            63
        } else {
            15
        };
    let max = match version {
        SymbosVersion::Medium => 15,
        SymbosVersion::Small => 8,
        SymbosVersion::Big => 48,
    };
    if height == 0 || height > max || (version != SymbosVersion::Big && b[0] & 16 != 0) {
        return Err(invalid("invalid SymbOS height/flags"));
    }
    let first = if b[0] & 128 != 0 {
        extended(b[1])?;
        1
    } else {
        if b[1] > 127 {
            return Err(invalid(
                "SymbOS first code exceeds ordinary character range",
            ));
        }
        b[1]
    };
    let count = if b[0] & 128 != 0 {
        255
    } else {
        128 - usize::from(first)
    };
    Ok((version, height, first, count))
}
/// Decode every native character and retain unused source rows and extension metadata.
pub fn read(c: &mut ReadContext<'_>, o: &SymbosReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &SymbosReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let (version, height, first, mut count) = header(b)?;
    check(b.len(), l.max_table_bytes, "SymbOS file")?;
    let mut report = Report::default();
    if version != SymbosVersion::Big {
        let size = if version == SymbosVersion::Small {
            9
        } else {
            16
        };
        if !(b.len() - 2).is_multiple_of(size) {
            return Err(invalid("truncated SymbOS glyph record"));
        }
        let actual = (b.len() - 2) / size;
        if actual != count {
            if !o.allow_short_range || actual == 0 || actual > count {
                return Err(invalid(
                    "SymbOS character count does not match native range",
                ));
            }
            count = actual;
            budget.diagnostic(
                &mut report,
                "symbos",
                "read.short_range",
                crate::report::Impact::Repair,
                None,
                None,
                "Explicitly accepted a short SymbOS character range",
            )?;
        }
    }
    check(count, l.max_glyphs, "SymbOS glyph count")?;
    check(1, l.max_resources, "SymbOS strike")?;
    budget.items::<Font>(1)?;
    budget.items::<Glyph>(count)?;
    budget.add(count.saturating_mul(192).saturating_add(1024))?;
    let mut f = Font::default();
    f.source_format = Some("symbos".into());
    f.charset.name = Some("symbos-native".into());
    f.metrics.pixel_size = Some(u32::from(height));
    f.metrics.ascent = Some(i64::from(height));
    f.metrics.descent = Some(0);
    f.metrics.spacing = Some(Spacing::Proportional);
    f.properties.push(Property::new(
        "symbos.version",
        match version {
            SymbosVersion::Medium => 0i64,
            SymbosVersion::Small => 1,
            SymbosVersion::Big => 2,
        },
    ));
    f.properties
        .push(Property::new("symbos.first_code", i64::from(first)));
    if b[0] & 128 != 0 {
        f.properties
            .push(Property::new("symbos.extended_info", i64::from(b[1])));
    }
    let table_end = 2 + count * 2;
    if version == SymbosVersion::Big {
        slice(b, 2, count * 2)?;
    }
    for i in 0..count {
        let (width, bytes, unused) = if version == SymbosVersion::Big {
            let word = 2 + i * 2;
            let offset = usize::from(u16::from_le_bytes([b[word], b[word + 1]]));
            let mut at = word + 1 + offset;
            if at < table_end {
                return Err(invalid("SymbOS glyph overlaps offset table"));
            }
            let width = *slice(b, at, 1)?
                .first()
                .ok_or_else(|| invalid("missing SymbOS width"))?;
            at += 1;
            let row = usize::from(width).div_ceil(8);
            let n = row * usize::from(height);
            check(
                usize::from(width) * usize::from(height),
                l.max_pixels_per_glyph,
                "SymbOS glyph pixels",
            )?;
            budget.add(n.saturating_mul(2))?;
            let mut raster = vec![0; n];
            let mut x = 0usize;
            loop {
                let cw = slice(b, at, 1)?[0];
                at += 1;
                if cw == 0 {
                    break;
                }
                if cw > 8 || x + usize::from(cw) > usize::from(width) || !x.is_multiple_of(8) {
                    return Err(invalid("invalid SymbOS big glyph column"));
                }
                let col = slice(b, at, usize::from(height))?;
                at += usize::from(height);
                for (y, &v) in col.iter().enumerate() {
                    raster[y * row + x / 8] = v;
                }
                x += usize::from(cw);
            }
            if x != usize::from(width) {
                return Err(invalid("SymbOS total width and columns disagree"));
            }
            (width, raster, Vec::new())
        } else {
            let size = if version == SymbosVersion::Small {
                9
            } else {
                16
            };
            let record = slice(b, 2 + i * size, size)?;
            if record[0] > 8 {
                return Err(invalid("SymbOS small/medium width exceeds eight"));
            }
            let rows = if record[0] == 0 {
                Vec::new()
            } else {
                record[1..1 + usize::from(height)].to_vec()
            };
            budget.add(size.saturating_mul(3))?;
            (record[0], rows, record[1 + usize::from(height)..].to_vec())
        };
        check(
            usize::from(width) * usize::from(height),
            l.max_pixels_per_glyph,
            "SymbOS glyph pixels",
        )?;
        let bitmap = Bitmap::<Mono>::from_raw(
            u32::from(width),
            u32::from(height),
            &bytes,
            bitmap_limits(l),
        )?;
        let mut g = Glyph::new(bitmap);
        g.metrics.advance = Some(Vector::new(i64::from(width), 0));
        g.codepoints
            .push(Codepoint::Legacy(u32::from(first) + i as u32));
        if !unused.is_empty() {
            g.properties.push(Property::new(
                "symbos.unused_rows",
                PropertyValue::Bytes(unused),
            ));
        }
        f.add_glyph(g)?;
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("symbos".into());
    Ok((set, report))
}
/// Encode a native file and audit its complete same-limit reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &SymbosWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = encode(s, w, o, &mut report);
    finish(result, report, "symbos", w.strict)
}
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &SymbosWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    let converted = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    if converted.fonts.len() != 1 {
        return Err(invalid("SymbOS output requires one selected strike"));
    }
    let f = &converted.fonts[0];
    let version = if let Some(v) = o.version {
        v
    } else {
        match int(&f.properties, "symbos.version", 0)? {
            0 => SymbosVersion::Medium,
            1 => SymbosVersion::Small,
            2 => SymbosVersion::Big,
            _ => return Err(invalid("invalid source SymbOS version")),
        }
    };
    let ext = if let Some(v) = o.extended {
        Some(extension_byte(v)?)
    } else {
        match f.properties.get("symbos.extended_info") {
            Some(PropertyValue::Integer(n)) => {
                let v = u8::try_from(*n).map_err(|_| invalid("SymbOS extension exceeds byte"))?;
                extended(v)?;
                Some(v)
            }
            _ => None,
        }
    };
    if ext.is_some() && o.first_code.is_some_and(|v| v != 1) {
        return Err(invalid("extended SymbOS first code is always one"));
    }
    let first = if ext.is_some() {
        1
    } else {
        o.first_code.unwrap_or(
            u8::try_from(int(&f.properties, "symbos.first_code", 32)?)
                .map_err(|_| invalid("SymbOS first code exceeds byte"))?,
        )
    };
    if ext.is_none() && first > 127 {
        return Err(invalid("SymbOS first code exceeds ordinary range"));
    }
    let count = if ext.is_some() {
        255
    } else {
        128 - usize::from(first)
    };
    let end = u32::from(first) + count as u32;
    check(count, o.limits.max_glyphs, "SymbOS output glyphs")?;
    let mut map = BTreeMap::new();
    let mut top = 0i64;
    let mut bottom = 0i64;
    for (id, g) in f.glyphs() {
        top = top.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
        bottom = bottom.min(g.metrics.bounds.y);
        for &cp in &g.codepoints {
            if cp.value() < u32::from(first)
                || cp.value() >= end
                || matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy
            {
                budget.loss(
                    r,
                    "symbos",
                    "write.mapping",
                    Some(0),
                    Some(id.index()),
                    format!("Mapping {cp:?} is outside the selected native range"),
                )?;
                continue;
            }
            budget.add(64)?;
            if map.insert(cp.value(), g).is_some() {
                budget.loss(
                    r,
                    "symbos",
                    "write.duplicate",
                    Some(0),
                    Some(id.index()),
                    format!("Last glyph wins at {cp:?}"),
                )?;
            }
        }
    }
    let height = o.height.map(i64::from).unwrap_or((top - bottom).max(1));
    let max = match version {
        SymbosVersion::Medium => 15,
        SymbosVersion::Small => 8,
        SymbosVersion::Big => 48,
    };
    if height < 1 || height > max || top - bottom > height {
        return Err(invalid("SymbOS cell height cannot contain source glyphs"));
    }
    let header = height as u8
        | match version {
            SymbosVersion::Medium => 0,
            SymbosVersion::Small => 32,
            SymbosVersion::Big => 64,
        }
        | if ext.is_some() { 128 } else { 0 };
    budget.add(count.saturating_mul((height as usize + 1) * 33 + 16))?;
    let mut out = vec![header, ext.unwrap_or(first)];
    if version == SymbosVersion::Big {
        out.resize(2 + 2 * count, 0);
    }
    for i in 0..count {
        let g = map.get(&(u32::from(first) + i as u32)).copied();
        let width = g.map_or(0, |g| {
            g.metrics
                .advance
                .map_or(i64::from(g.bitmap.width()), |v| v.x)
        });
        let limit = if version == SymbosVersion::Big {
            255
        } else {
            8
        };
        if width < 0 || width > limit {
            return Err(invalid("SymbOS glyph width exceeds native layout"));
        }
        let row = (width as usize).div_ceil(8);
        let mut raster = vec![0; row * height as usize];
        if let Some(g) = g {
            if g.metrics.bounds.x < 0 || g.metrics.bounds.x + i64::from(g.bitmap.width()) > width {
                return Err(invalid("SymbOS glyph ink exceeds native advance cell"));
            }
            let bm = mono(g)?;
            let top = height - (g.metrics.bounds.y - bottom) - i64::from(bm.height());
            if top < 0 {
                return Err(invalid("SymbOS glyph exceeds vertical cell"));
            }
            insert(
                &mut raster,
                row,
                g.metrics.bounds.x as usize,
                top as usize,
                bm,
            );
        }
        if version == SymbosVersion::Big {
            let word = 2 + 2 * i;
            let offset = u16::try_from(out.len() - word - 1)
                .map_err(|_| invalid("SymbOS big glyph offset exceeds word"))?;
            out[word..word + 2].copy_from_slice(&offset.to_le_bytes());
            out.push(width as u8);
            for col in 0..row {
                out.push((width as usize - col * 8).min(8) as u8);
                for y in 0..height as usize {
                    out.push(raster[y * row + col]);
                }
            }
            out.push(0);
        } else {
            let size = if version == SymbosVersion::Small {
                9
            } else {
                16
            };
            let start = out.len();
            out.resize(start + size, 0);
            out[start] = width as u8;
            if width != 0 {
                out[start + 1..start + 1 + height as usize].copy_from_slice(&raster);
            }
            if let Some(g) = g
                && let Some(PropertyValue::Bytes(unused)) = g.properties.get("symbos.unused_rows")
                && unused.len() == size - 1 - height as usize
            {
                out[start + 1 + height as usize..start + size].copy_from_slice(unused);
            }
        }
    }
    check(
        out.len(),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "SymbOS output bytes",
    )?;
    budget.add(out.len())?;
    let mut read_budget = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = parse(&out, &Default::default(), &o.limits, &mut read_budget)?;
    r.extend(rr);
    budget.add(read_budget.used)?;
    audit(s, &back, "symbos", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
