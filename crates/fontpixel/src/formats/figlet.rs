//! Bounded FIGfont v2 parsing, retained character art and canonical bitmap output.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::report::Impact;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
const GERMAN: [i32; 7] = [196, 214, 220, 228, 246, 252, 223];
fn required() -> impl Iterator<Item = i32> {
    (32..127).chain(GERMAN)
}
/// FIGfont reader choices; non-space bytes always project to ink.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct FigletReadOptions {}
/// Bounded FIGfont bitmap output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct FigletWriteOptions {
    /// Override common cell height; otherwise infer it from line height and glyphs.
    pub height: Option<u32>,
    /// Override baseline row count; otherwise use model ascent or glyph tops.
    pub baseline: Option<u32>,
    /// Permit nonnegative legacy code numbers as FIGfont codes.
    pub legacy_as_unicode: bool,
    /// Explicit grayscale reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Maximum output length, also bounded by reader input limits.
    pub max_output_bytes: usize,
}
impl Default for FigletWriteOptions {
    fn default() -> Self {
        Self {
            height: None,
            baseline: None,
            legacy_as_unicode: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec for FIGfont v2.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct FigletCodec {
    /// Typed input choices.
    pub read_options: FigletReadOptions,
    /// Typed output choices.
    pub write_options: FigletWriteOptions,
}
impl FormatDetector for FigletCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for FigletCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for FigletCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Recognize the FIGfont v2 signature.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"flf2a") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
struct Lines<'a> {
    b: &'a [u8],
    at: usize,
    max: usize,
}
impl<'a> Lines<'a> {
    fn next(&mut self) -> Result<Option<&'a [u8]>> {
        if self.at == self.b.len() {
            return Ok(None);
        }
        let rest = &self.b[self.at..];
        let end = rest.iter().position(|v| *v == b'\n').unwrap_or(rest.len());
        check(end, self.max, "FIGfont line bytes")?;
        self.at += end + usize::from(end < rest.len());
        Ok(Some(
            rest[..end].strip_suffix(b"\r").unwrap_or(&rest[..end]),
        ))
    }
    fn need(&mut self) -> Result<&'a [u8]> {
        self.next()?.ok_or_else(|| invalid("truncated FIGfont"))
    }
}
fn decimal(b: &[u8]) -> Result<i64> {
    std::str::from_utf8(b)
        .map_err(|_| invalid("non-ASCII FIGfont number"))?
        .parse()
        .map_err(|_| invalid("invalid FIGfont number"))
}
fn tag_number(b: &[u8]) -> Result<i32> {
    let s = std::str::from_utf8(b).map_err(|_| invalid("invalid FIGfont code tag"))?;
    let (negative, s) = if let Some(s) = s.strip_prefix('-') {
        (true, s)
    } else {
        (false, s.strip_prefix('+').unwrap_or(s))
    };
    let (radix, s) = if let Some(s) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, s)
    } else if s.len() > 1 && s.starts_with('0') {
        (8, &s[1..])
    } else {
        (10, s)
    };
    let value = i64::from_str_radix(s, radix).map_err(|_| invalid("invalid FIGfont code tag"))?;
    let value = if negative { -value } else { value };
    let value = i32::try_from(value).map_err(|_| invalid("FIGfont code tag exceeds i32"))?;
    if value == -1 {
        return Err(invalid("FIGfont code -1 is forbidden"));
    }
    Ok(value)
}
fn content(line: &[u8]) -> Result<&[u8]> {
    let end = *line
        .last()
        .ok_or_else(|| invalid("FIGfont row has no endmark"))?;
    if end == b' ' || end == b'\t' {
        return Err(invalid("FIGfont endmark cannot be whitespace"));
    }
    let n = line.iter().rposition(|b| *b != end).map_or(0, |i| i + 1);
    Ok(&line[..n])
}
/// Read FIGfont, preserving the complete source in a Bytes property.
pub fn read(c: &mut ReadContext<'_>, _: &FigletReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "FIGfont source bytes")?;
    let mut lines = Lines {
        b,
        at: 0,
        max: l.max_table_bytes,
    };
    let header = lines.need()?;
    if !header.starts_with(b"flf2a")
        || header.len() < 7
        || header[5].is_ascii_whitespace()
        || !header[6].is_ascii_whitespace()
    {
        return Err(invalid("invalid FIGfont signature or hardblank"));
    }
    let hardblank = header[5];
    let mut values = [0i64; 8];
    let mut count = 0;
    for part in header[6..]
        .split(|v| v.is_ascii_whitespace())
        .filter(|v| !v.is_empty())
    {
        if count == 8 {
            return Err(invalid("too many FIGfont header parameters"));
        }
        values[count] = decimal(part)?;
        count += 1;
    }
    if count < 5 {
        return Err(invalid("incomplete FIGfont header"));
    }
    let [
        height,
        baseline,
        max_length,
        old_layout,
        comments,
        direction,
        full_layout,
        codetags,
    ] = values;
    if height <= 0
        || baseline < 0
        || baseline > height
        || max_length < 2
        || comments < 0
        || !(-1..=63).contains(&old_layout)
        || !(0..=1).contains(&direction)
        || !(0..=32767).contains(&full_layout)
        || codetags < 0
    {
        return Err(invalid("invalid FIGfont header values"));
    }
    let height = u32::try_from(height).map_err(|_| invalid("FIGfont height exceeds u32"))?;
    check(height as usize, l.max_pixels_per_glyph, "FIGfont height")?;
    check(102, l.max_glyphs, "FIGfont required glyphs")?;
    budget.items::<Font>(1)?;
    budget.add(b.len().saturating_add(4096))?;
    let mut f = Font::default();
    f.source_format = Some("figlet".into());
    f.charset.name = Some("unicode".into());
    for (key, val) in [
        ("figlet.hardblank", i64::from(hardblank)),
        ("figlet.old_layout", old_layout),
        ("figlet.direction", direction),
        ("figlet.full_layout", full_layout),
    ] {
        f.properties.push(Property::new(key, val));
    }
    let comment_start = lines.at;
    for _ in 0..comments {
        lines.need()?;
    }
    let comment_bytes = &b[comment_start..lines.at];
    budget.add(comment_bytes.len())?;
    f.properties.push(Property::new(
        "figlet.comments",
        PropertyValue::Bytes(comment_bytes.to_vec()),
    ));
    f.properties.push(Property::new(
        "figlet.source",
        PropertyValue::Bytes(b.to_vec()),
    ));
    let mut report = Report::default();
    if direction != 0 || old_layout != -1 || full_layout != 0 {
        budget.loss(&mut report,"figlet","read.layout",Some(0),None,"FIGlet direction/fitting/smushing semantics are retained as metadata but not rendered by the bitmap model")?;
    }
    let mut max_width = 0;
    for code in required() {
        let glyph = parse_glyph(
            &mut lines,
            height,
            baseline,
            hardblank,
            code,
            None,
            l,
            budget,
            &mut report,
            f.len(),
        )?;
        max_width = max_width.max(glyph.bitmap.width());
        f.add_glyph(glyph)?;
    }
    let mut actual_tags = 0usize;
    while let Some(tag) = lines.next()? {
        if tag.is_empty() {
            return Err(invalid("empty FIGfont code tag"));
        }
        let split = tag
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(tag.len());
        let code = tag_number(&tag[..split])?;
        let name = &tag[split..];
        let name = name.trim_ascii();
        check(f.len() + 1, l.max_glyphs, "FIGfont glyphs")?;
        budget.add(name.len().saturating_mul(2).saturating_add(256))?;
        let glyph = parse_glyph(
            &mut lines,
            height,
            baseline,
            hardblank,
            code,
            Some(name),
            l,
            budget,
            &mut report,
            f.len(),
        )?;
        max_width = max_width.max(glyph.bitmap.width());
        f.add_glyph(glyph)?;
        actual_tags += 1;
    }
    if count == 8 && actual_tags as u64 != codetags as u64 {
        budget.diagnostic(
            &mut report,
            "figlet",
            "read.code_count",
            Impact::Repair,
            Some(0),
            None,
            "Header code-tag count disagrees with complete parsed glyphs",
        )?;
    }
    f.metrics.pixel_size = Some(height);
    f.metrics.line_height = Some(i64::from(height));
    f.metrics.ascent = Some(baseline);
    f.metrics.descent = Some(i64::from(height) - baseline);
    f.metrics.bounds = Some(Bounds::new(
        max_width,
        height,
        0,
        baseline - i64::from(height),
    ));
    f.metrics.spacing = Some(Spacing::Proportional);
    f.metrics.space_width = f
        .glyph_for_codepoint(Codepoint::Unicode(32))
        .and_then(|g| g.metrics.advance.map(|a| a.x));
    if f.glyph_for_codepoint(Codepoint::Unicode(0)).is_some() {
        f.metrics.default_character = Some(Codepoint::Unicode(0));
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("figlet".into());
    Ok((set, report))
}
// Each parameter is an independent source field or shared resource contract.
#[allow(clippy::too_many_arguments)]
fn parse_glyph(
    lines: &mut Lines<'_>,
    height: u32,
    baseline: i64,
    hardblank: u8,
    code: i32,
    name: Option<&[u8]>,
    l: &ResourceLimits,
    budget: &mut Budget,
    report: &mut Report,
    index: usize,
) -> Result<Glyph> {
    let first = content(lines.need()?)?;
    let width = u32::try_from(first.len()).map_err(|_| invalid("FIGfont row width exceeds u32"))?;
    check(
        (width as usize).saturating_mul(height as usize),
        l.max_pixels_per_glyph,
        "FIGfont glyph pixels",
    )?;
    let stride = (width as usize).div_ceil(8);
    let bytes = stride
        .checked_mul(height as usize)
        .ok_or_else(|| invalid("FIGfont bitmap size overflow"))?;
    budget.items::<Option<Glyph>>(1)?;
    budget.add(bytes.saturating_mul(2).saturating_add(256))?;
    let mut packed = vec![0; bytes];
    let mut art = false;
    let mut hard = false;
    for y in 0..height as usize {
        let row = if y == 0 {
            first
        } else {
            content(lines.need()?)?
        };
        if row.len() != width as usize {
            return Err(invalid("FIGfont glyph rows have inconsistent widths"));
        }
        for (x, &v) in row.iter().enumerate() {
            if v != b' ' {
                packed[y * stride + x / 8] |= 0x80 >> (x % 8);
                hard |= v == hardblank;
                art |= v != b'#' && v != hardblank;
            }
        }
    }
    if hard {
        budget.loss(report,"figlet","read.hardblank",Some(0),Some(index),"Hardblank is projected to ink under the non-space rule; exact art is retained in figlet.source")?;
    }
    if art {
        budget.loss(report,"figlet","read.character_art",Some(0),Some(index),"Character art projects to monochrome ink; exact characters are retained in figlet.source")?;
    }
    let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
        width,
        height,
        &packed,
        bitmap_limits(l),
    )?);
    g.codepoints.push(if code < 0 {
        Codepoint::Legacy(code as u32)
    } else {
        Codepoint::Unicode(code as u32)
    });
    g.metrics.bounds = Bounds::new(width, height, 0, baseline - i64::from(height));
    g.metrics.advance = Some(Vector::new(i64::from(width), 0));
    g.metrics.left_bearing = Some(0);
    g.metrics.right_bearing = Some(0);
    if let Some(name) = name {
        g.properties
            .push(Property::new("figlet.code", i64::from(code)));
        if !name.is_empty() {
            g.name = Some(latin(name));
        }
    }
    Ok(g)
}
fn selected<'a>(
    f: &'a Font,
    v: &FigletWriteOptions,
    budget: &mut Budget,
    r: &mut Report,
) -> Result<BTreeMap<i32, &'a Glyph>> {
    let mut slots = BTreeMap::new();
    for (id, g) in f.glyphs() {
        if g.codepoints.is_empty() {
            budget.loss(
                r,
                "figlet",
                "write.unencoded_glyph",
                Some(0),
                Some(id.index()),
                "Unencoded glyph has no FIGfont code",
            )?;
        }
        for &cp in &g.codepoints {
            let code = match cp {
                Codepoint::Unicode(n) => i32::try_from(n).ok(),
                Codepoint::Legacy(n)
                    if (n as i32) < -1
                        && int(&g.properties, "figlet.code", 0)? == i64::from(n as i32) =>
                {
                    Some(n as i32)
                }
                Codepoint::Legacy(n) if v.legacy_as_unicode => i32::try_from(n).ok(),
                _ => None,
            };
            if let Some(code) = code {
                budget.add(64)?;
                if slots.insert(code, g).is_some() {
                    budget.loss(
                        r,
                        "figlet",
                        "write.duplicate_mapping",
                        Some(0),
                        Some(id.index()),
                        format!("Duplicate FIGfont code {code}; last glyph used"),
                    )?;
                }
            } else {
                budget.loss(
                    r,
                    "figlet",
                    "write.unrepresentable_mapping",
                    Some(0),
                    Some(id.index()),
                    format!("Mapping {cp:?} is not representable"),
                )?;
            }
        }
    }
    if slots.is_empty() {
        return Err(invalid("no FIGfont-representable mappings"));
    }
    Ok(slots)
}
fn dimensions(
    g: Option<&Glyph>,
    baseline: i64,
    height: u32,
    default_advance: Option<Vector>,
) -> Result<(usize, usize, usize)> {
    let Some(g) = g else {
        return Ok((0, 0, 0));
    };
    let bb = g.metrics.bounds;
    let width = g
        .metrics
        .advance
        .or(default_advance)
        .map_or(bb.x + i64::from(bb.width), |v| v.x);
    let top = baseline - bb.y - i64::from(bb.height);
    if bb.x < 0
        || width < bb.x + i64::from(bb.width)
        || top < 0
        || top + i64::from(bb.height) > i64::from(height)
    {
        return Err(invalid(
            "FIGfont cell cannot contain glyph bounds and advance without clipping",
        ));
    }
    Ok((
        usize::try_from(width).map_err(|_| invalid("FIGfont advance exceeds usize"))?,
        bb.x as usize,
        top as usize,
    ))
}
/// Write canonical `#` art with full-size layout and complete loss diagnostics.
pub fn write(s: &FontSet, o: &WriteOptions, v: &FigletWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = selected(f, v, &mut budget, &mut report)?;
        let baseline = v
            .baseline
            .map(i64::from)
            .or(f.metrics.ascent)
            .unwrap_or_else(|| {
                slots
                    .values()
                    .map(|g| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                    .max()
                    .unwrap_or(1)
                    .max(0)
            });
        let baseline = i64::from(
            u32::try_from(baseline).map_err(|_| invalid("FIGfont baseline exceeds u32"))?,
        );
        let needed = slots
            .values()
            .map(|g| baseline - g.metrics.bounds.y)
            .max()
            .unwrap_or(1)
            .max(1);
        let height = v
            .height
            .map(i64::from)
            .unwrap_or_else(|| f.metrics.line_height.unwrap_or(needed).max(needed));
        let height = u32::try_from(height)
            .ok()
            .filter(|h| *h > 0 && baseline >= 0 && baseline <= i64::from(*h))
            .ok_or_else(|| invalid("invalid FIGfont output height/baseline"))?;
        let extra = slots
            .keys()
            .filter(|c| !(32..127).contains(*c) && !GERMAN.contains(c))
            .count();
        check(102 + extra, v.limits.max_glyphs, "FIGfont output glyphs")?;
        let comments = match f.properties.get("figlet.comments") {
            Some(PropertyValue::Bytes(b)) => b.as_slice(),
            None => &[],
            _ => return Err(invalid("figlet.comments must be Bytes")),
        };
        if !comments.is_empty() && !comments.ends_with(b"\n") {
            return Err(invalid("FIGfont comments must end with newline"));
        }
        let comment_count = comments.iter().filter(|&&b| b == b'\n').count();
        let mut max_width = 0;
        let mut glyph_bytes = 0usize;
        for code in required().chain(
            slots
                .keys()
                .copied()
                .filter(|c| !(32..127).contains(c) && !GERMAN.contains(c)),
        ) {
            let g = slots.get(&code).copied();
            let (width, _, _) = dimensions(g, baseline, height, f.metrics.glyph_defaults.advance)?;
            check(
                width.saturating_mul(height as usize),
                v.limits.max_pixels_per_glyph,
                "FIGfont output glyph pixels",
            )?;
            max_width = max_width.max(width);
            glyph_bytes = glyph_bytes
                .checked_add(
                    width
                        .checked_add(2)
                        .ok_or_else(|| invalid("FIGfont row size overflow"))?
                        .checked_mul(height as usize)
                        .and_then(|n| n.checked_add(1))
                        .ok_or_else(|| invalid("FIGfont glyph output overflow"))?,
                )
                .ok_or_else(|| invalid("FIGfont size overflow"))?;
            if !(32..127).contains(&code) && !GERMAN.contains(&code) {
                glyph_bytes = glyph_bytes
                    .checked_add(16 + g.and_then(|g| g.name.as_ref()).map_or(0, String::len))
                    .ok_or_else(|| invalid("FIGfont tag size overflow"))?;
            }
        }
        let header = format!(
            "flf2a$ {height} {baseline} {} -1 {comment_count} 0 0 {extra}\n",
            max_width
                .checked_add(2)
                .ok_or_else(|| invalid("FIGfont maximum row width overflow"))?
        );
        let cap = header
            .len()
            .checked_add(comments.len())
            .and_then(|n| n.checked_add(glyph_bytes))
            .ok_or_else(|| invalid("FIGfont output overflow"))?;
        check(
            cap,
            v.max_output_bytes
                .min(v.limits.max_input_bytes)
                .min(v.limits.max_table_bytes),
            "FIGfont output bytes",
        )?;
        budget.add(cap)?;
        let mut out = Vec::with_capacity(cap);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(comments);
        for code in required().chain(
            slots
                .keys()
                .copied()
                .filter(|c| !(32..127).contains(c) && !GERMAN.contains(c)),
        ) {
            let g = slots.get(&code).copied();
            if !(32..127).contains(&code) && !GERMAN.contains(&code) {
                out.extend_from_slice(code.to_string().as_bytes());
                if let Some(name) = g.and_then(|g| g.name.as_ref()) {
                    out.push(b' ');
                    for ch in name.chars() {
                        if ch == '\n' || ch == '\r' {
                            return Err(invalid("FIGfont tag name contains newline"));
                        }
                        out.push(
                            u8::try_from(u32::from(ch))
                                .map_err(|_| invalid("FIGfont tag name is not Latin-1"))?,
                        );
                    }
                }
                out.push(b'\n');
            }
            let (width, left, top) =
                dimensions(g, baseline, height, f.metrics.glyph_defaults.advance)?;
            let bm = g.map(mono).transpose()?;
            for y in 0..height as usize {
                let start = out.len();
                out.resize(start + width, b' ');
                if let Some(bm) = bm.filter(|b| y >= top && y - top < b.height() as usize) {
                    let row = &bm.as_bytes()[(y - top) * bm.stride()..(y - top + 1) * bm.stride()];
                    for x in 0..bm.width() as usize {
                        if row[x / 8] & (0x80 >> (x % 8)) != 0 {
                            out[start + left + x] = b'#';
                        }
                    }
                }
                out.push(b'@');
                if y + 1 == height as usize {
                    out.push(b'@');
                }
                out.push(b'\n');
            }
        }
        let (target, rr) = parse(&out, &v.limits, &mut budget)?;
        report.extend(rr);
        audit(s, &target, "figlet", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "figlet", o.strict)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::defaults;

    #[test]
    fn options_serde_defaults_and_rejections() {
        defaults::<FigletReadOptions>();
        defaults::<FigletWriteOptions>();
        defaults::<FigletCodec>();
    }
}
