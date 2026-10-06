//! Native bounded HP PCL bitmap soft-font streams.
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Rational, Vector};
use crate::report::{Impact, Level, Location};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ReportEntry, ResourceLimits, Result};
/// Raster orientation in the physical printer coordinate system.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HppclOrientation {
    /// Zero degrees.
    #[default]
    Portrait = 0,
    /// Ninety degrees counterclockwise.
    Landscape = 1,
    /// One hundred eighty degrees.
    ReversePortrait = 2,
    /// Two hundred seventy degrees counterclockwise.
    ReverseLandscape = 3,
}
/// Raster encoding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HppclCompression {
    /// Class 1 byte-padded raster rows.
    #[default]
    Raw,
    /// Class 2 alternating runs and repeated rows.
    Rle,
}
/// Input compatibility policy.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HppclReadOptions {
    /// Accept the historical monobit writer's 16-byte descriptor-size field
    /// when an exact class-1 payload proves its actual descriptor is 14 bytes.
    pub allow_legacy_descriptor_size: bool,
}
/// Native bounded output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HppclWriteOptions {
    /// Physical raster orientation.
    pub orientation: HppclOrientation,
    /// Character compression.
    pub compression: HppclCompression,
    /// Maximum character-command payload, including its common/descriptor bytes.
    pub continuation_bytes: usize,
    /// Use format 20 and these explicit horizontal/vertical resolutions.
    pub resolution: Option<(u16, u16)>,
    /// Explicit PCL symbol-set number; no Unicode encoding is inferred.
    pub symbol_set: Option<u16>,
    /// Permit numeric Unicode positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction.
    pub gray_threshold: Option<u8>,
    /// Shared ceilings.
    pub limits: ResourceLimits,
    /// Output cap.
    pub max_output_bytes: usize,
}
impl Default for HppclWriteOptions {
    fn default() -> Self {
        Self {
            orientation: HppclOrientation::Portrait,
            compression: HppclCompression::Raw,
            continuation_bytes: 32767,
            resolution: None,
            symbol_set: None,
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
pub struct HppclCodec {
    /// Input interpretation.
    pub read_options: HppclReadOptions,
    /// Output choices.
    pub write_options: HppclWriteOptions,
}
impl FormatDetector for HppclCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for HppclCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for HppclCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Recognize a bounded initial font-download command.
pub fn detect(b: &[u8]) -> Confidence {
    if !b.starts_with(b"\x1b)s") {
        return Confidence::NoMatch;
    }
    let digits = b[3..]
        .iter()
        .take(10)
        .take_while(|v| v.is_ascii_digit())
        .count();
    if digits > 0 && b.get(3 + digits) == Some(&b'W') {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn note(report: &mut Report, code: &str, message: &str, cp: Option<u32>, impact: Impact) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact,
        code: format!("hppcl.{code}"),
        location: Location {
            codepoint: cp,
            ..Default::default()
        },
        message: message.into(),
    });
}
struct Pending {
    code: u32,
    descriptor: Vec<u8>,
    data: Vec<u8>,
}
fn rotated(
    raw: &[u8],
    w: u32,
    h: u32,
    turn: u8,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(u32, u32, Vec<u8>)> {
    check(
        (w as usize).saturating_mul(h as usize),
        l.max_pixels_per_glyph,
        "PCL character pixels",
    )?;
    let (dw, dh) = if turn.is_multiple_of(2) {
        (w, h)
    } else {
        (h, w)
    };
    let row = (w as usize).div_ceil(8);
    let dr = (dw as usize).div_ceil(8);
    budget.add(dr * dh as usize)?;
    let mut out = vec![0; dr * dh as usize];
    for y in 0..h {
        for x in 0..w {
            if raw[y as usize * row + x as usize / 8] & (128 >> (x % 8)) == 0 {
                continue;
            }
            let (dx, dy) = match turn % 4 {
                0 => (x, y),
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            out[dy as usize * dr + dx as usize / 8] |= 128 >> (dx % 8);
        }
    }
    Ok((dw, dh, out))
}
fn unpack(
    data: &[u8],
    w: u32,
    h: u32,
    class: u8,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    check(
        (w as usize).saturating_mul(h as usize),
        l.max_pixels_per_glyph,
        "PCL character pixels",
    )?;
    let row = (w as usize).div_ceil(8);
    let n = row * h as usize;
    budget.add(n)?;
    if class == 1 {
        if data.len() != n {
            return Err(invalid("PCL raw character payload size mismatch"));
        }
        return Ok(data.to_vec());
    }
    if class != 2 {
        return Err(invalid("PCL character class is not raw or RLE bitmap"));
    }
    let mut out = vec![0; n];
    let mut at = 0;
    let mut y = 0;
    while y < h as usize {
        let repeat = usize::from(slice(data, at, 1)?[0]) + 1;
        at += 1;
        if repeat > h as usize - y {
            return Err(invalid("PCL repeated rows exceed character height"));
        }
        let mut x = 0usize;
        let mut black = false;
        while x < w as usize {
            let run = usize::from(slice(data, at, 1)?[0]);
            at += 1;
            if run > w as usize - x {
                return Err(invalid("PCL RLE run exceeds character width"));
            }
            if black {
                for ix in x..x + run {
                    out[y * row + ix / 8] |= 128 >> (ix % 8);
                }
            }
            x += run;
            black = !black;
        }
        for iy in 1..repeat {
            let (before, after) = out.split_at_mut((y + iy) * row);
            after[..row].copy_from_slice(&before[y * row..(y + 1) * row]);
        }
        y += repeat;
    }
    if at != data.len() {
        return Err(invalid("PCL trailing compressed character bytes"));
    }
    Ok(out)
}
fn finish_character(
    p: Pending,
    f: &mut Font,
    l: &ResourceLimits,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<()> {
    let d = &p.descriptor;
    let w = u32::from(u16_at(d, 10, ByteOrder::Big)?);
    let h = u32::from(u16_at(d, 12, ByteOrder::Big)?);
    if w > 16384 || h > 16384 || d[4] > 3 {
        return Err(invalid(
            "PCL character geometry/orientation exceeds specification",
        ));
    }
    let head = bytes(f, "hppcl.header").ok_or_else(|| invalid("PCL header metadata missing"))?;
    if d[4] != head[12] {
        return Err(invalid("PCL character orientation differs from its font"));
    }
    let pitch = i64::from(u16_at(head, 16, ByteOrder::Big)?);
    let delta = i64::from(u16_at(d, 14, ByteOrder::Big)? as i16);
    let advance = if head[13] == 0 {
        pitch.div_euclid(4)
    } else {
        delta.div_euclid(4)
    };
    if delta % 4 != 0 || head[13] == 0 && (pitch % 4 != 0 || head[40] != 0) {
        note(
            report,
            "fractional_advance",
            "Fractional device advance is retained in its descriptor and rounded down in integer device metrics.",
            Some(p.code),
            Impact::Loss,
        );
    }
    let left = i64::from(u16_at(d, 6, ByteOrder::Big)? as i16);
    let top = i64::from(u16_at(d, 8, ByteOrder::Big)? as i16);
    if !(-16384..=16384).contains(&left) || !(-16384..=16384).contains(&top) {
        return Err(invalid("PCL character offset exceeds specification"));
    }
    let raw = unpack(&p.data, w, h, d[3], l, budget)?;
    let (dw, dh, packed) = rotated(&raw, w, h, d[4], l, budget)?;
    let (x, y) = match d[4] {
        0 => (left, top - i64::from(h) + 1),
        1 => (top - i64::from(h) + 1, -left - i64::from(w) + 1),
        2 => (-left - i64::from(w) + 1, -top),
        _ => (-top, left),
    };
    let bitmap = raster(dw, dh, &packed, l, budget)?;
    budget.items::<Glyph>(1)?;
    budget.add(1024 + d.len())?;
    let mut g = Glyph::new(bitmap);
    g.codepoints.push(Codepoint::Legacy(p.code));
    g.metrics.bounds.x = x;
    g.metrics.bounds.y = y;
    g.metrics.advance = Some(Vector::new(advance, 0));
    g.metrics.left_bearing = Some(x);
    g.metrics.right_bearing = Some(advance - x - i64::from(dw));
    g.properties.push(Property::new(
        "hppcl.descriptor",
        PropertyValue::Bytes(d.to_vec()),
    ));
    if let Some(id) = f.glyph_id(Codepoint::Legacy(p.code)) {
        f.replace_glyph(id, g)?;
        note(
            report,
            "redefined_character",
            "A later download replaces the earlier definition, as on the printer; the stream retains both records.",
            Some(p.code),
            Impact::Loss,
        );
    } else {
        f.add_glyph(g)?;
    }
    Ok(())
}
fn font_header(d: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<Font> {
    slice(d, 0, 64)?;
    let size = usize::from(u16_at(d, 0, ByteOrder::Big)?);
    if size < 64 || size > d.len() || !matches!(d[2], 0 | 5 | 6 | 7 | 9 | 12 | 16 | 20) {
        return Err(invalid("PCL descriptor is not a supported bitmap header"));
    }
    if d[12] > 3 || d[13] > 2 {
        return Err(invalid(
            "PCL font orientation/spacing outside specification",
        ));
    }
    if d[2] == 20 && size < 68 {
        return Err(invalid("PCL resolution descriptor is truncated"));
    }
    let mut f = source("hppcl", d, 0, l, budget)?;
    budget.add(d.len() + 1024)?;
    f.properties.push(Property::new(
        "hppcl.header",
        PropertyValue::Bytes(d.to_vec()),
    ));
    f.names.full = Some(latin(&d[48..64]).trim_matches(['\0', ' ']).into());
    if size < d.len() {
        f.names.copyright = Some(latin(&d[size..]));
    }
    let (xdpi, ydpi) = if d[2] == 20 {
        (
            u16_at(d, 64, ByteOrder::Big)?,
            u16_at(d, 66, ByteOrder::Big)?,
        )
    } else {
        (300, 300)
    };
    if xdpi == 0 || ydpi == 0 {
        return Err(invalid("PCL zero resolution"));
    }
    f.metrics.resolution = Some(Vector::new(i64::from(xdpi), i64::from(ydpi)));
    f.metrics.point_size = Some(Rational::new(
        i64::from(u16_at(d, 18, ByteOrder::Big)?) * 72,
        u32::from(ydpi) * 4,
    )?);
    f.metrics.underline_position = Some(i64::from(d[30] as i8));
    f.metrics.underline_thickness = Some(i64::from(d[31]));
    Ok(f)
}
fn finish_font(mut f: Font) -> Result<Font> {
    let h = bytes(&f, "hppcl.header").ok_or_else(|| invalid("PCL missing header"))?;
    let base = i64::from(u16_at(h, 6, ByteOrder::Big)?);
    let cell = i64::from(u16_at(h, 10, ByteOrder::Big)?);
    let text = i64::from(u16_at(h, 32, ByteOrder::Big)?).div_euclid(4);
    metrics(&mut f, if text == 0 { cell } else { text });
    f.metrics.ascent = Some(base);
    f.metrics.descent = Some(cell - base);
    Ok(f)
}
/// Read one or several downloaded fonts.
pub fn read(c: &mut ReadContext<'_>, o: &HppclReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
// Shared with Dashen so embedded streams consume the same cumulative allowance.
pub(crate) fn parse(
    b: &[u8],
    o: &HppclReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "PCL stream bytes")?;
    let mut at = 0;
    let mut group: Option<(u8, u8)> = None;
    let mut code = None;
    let mut current: Option<Font> = None;
    let mut pending: Option<Pending> = None;
    let mut fonts = Vec::new();
    let mut report = Report::default();
    let mut count = 0;
    let mut ignored = false;
    while at < b.len() {
        if b[at] == 27 {
            at += 1;
            let first = *slice(b, at, 1)?
                .first()
                .ok_or_else(|| invalid("PCL escape"))?;
            at += 1;
            if (0x21..=0x2f).contains(&first) || first == b'*' {
                let second = slice(b, at, 1)?[0];
                at += 1;
                group = Some((first, second));
            } else {
                group = None;
                ignored = true;
                continue;
            }
        } else if group.is_none() {
            ignored = true;
            at += 1;
            continue;
        }
        let prefix = group.ok_or_else(|| invalid("PCL command group"))?;
        let negative = b.get(at) == Some(&b'-');
        if negative || b.get(at) == Some(&b'+') {
            at += 1;
        }
        let mut value = 0usize;
        let mut digits = 0;
        while let Some(d) = b.get(at).filter(|d| d.is_ascii_digit()) {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(usize::from(*d - b'0')))
                .ok_or_else(|| invalid("PCL numeric command overflow"))?;
            at += 1;
            digits += 1;
            if digits > 10 {
                return Err(invalid("PCL command numeric field is too long"));
            }
        }
        let term = slice(b, at, 1)?[0];
        at += 1;
        if !term.is_ascii_alphabetic() {
            return Err(invalid("PCL command needs an alphabetic terminator"));
        }
        if term.is_ascii_uppercase() {
            group = None;
        }
        let upper = term.to_ascii_uppercase();
        let record = if upper == b'W' || upper == b'V' {
            if negative {
                return Err(invalid("PCL negative record length"));
            }
            let r = slice(b, at, value)?;
            at += value;
            Some(r)
        } else {
            None
        };
        match (prefix, upper) {
            ((b')', b's'), b'W') => {
                if let Some(p) = pending.take() {
                    finish_character(
                        p,
                        current
                            .as_mut()
                            .ok_or_else(|| invalid("PCL character precedes font"))?,
                        l,
                        budget,
                        &mut report,
                    )?;
                }
                if let Some(f) = current.take() {
                    budget.items::<Font>(1)?;
                    fonts.push(finish_font(f)?);
                }
                current = Some(font_header(
                    record.ok_or_else(|| invalid("PCL font payload missing"))?,
                    l,
                    budget,
                )?);
                code = None;
            }
            ((b'*', b'c'), b'E') => {
                if negative {
                    return Err(invalid("PCL negative character code"));
                }
                code = Some(
                    u32::try_from(value).map_err(|_| invalid("PCL character code exceeds u32"))?,
                );
            }
            ((b'(', b's'), b'W') => {
                let d = record.ok_or_else(|| invalid("PCL character payload missing"))?;
                slice(d, 0, 2)?;
                if d[0] != 4 {
                    return Err(invalid(
                        "PCL character format is not LaserJet bitmap format 4",
                    ));
                }
                let cp = code.ok_or_else(|| invalid("PCL character has no selection code"))?;
                if d[1] != 0 {
                    let p = pending
                        .as_mut()
                        .ok_or_else(|| invalid("PCL continuation without initial character"))?;
                    if p.code != cp {
                        return Err(invalid("PCL continuation changes character code"));
                    }
                    budget.add(d.len().saturating_mul(2))?;
                    p.data.extend_from_slice(&d[2..]);
                } else {
                    if let Some(p) = pending.take() {
                        finish_character(
                            p,
                            current
                                .as_mut()
                                .ok_or_else(|| invalid("PCL character precedes font"))?,
                            l,
                            budget,
                            &mut report,
                        )?;
                    }
                    slice(d, 0, 16)?;
                    let mut ds = usize::from(d[2]) + 2;
                    if ds < 16 {
                        return Err(invalid("PCL character descriptor is shorter than 14 bytes"));
                    }
                    if o.allow_legacy_descriptor_size && d[2] == 16 && d[3] == 1 {
                        let raw = usize::from(u16_at(d, 10, ByteOrder::Big)?).div_ceil(8)
                            * usize::from(u16_at(d, 12, ByteOrder::Big)?);
                        if d.len() == raw + 16 {
                            ds = 16;
                            note(
                                &mut report,
                                "legacy_descriptor_size",
                                "Accepted a legacy writer's 16-byte size field for a proven 14-byte descriptor.",
                                Some(cp),
                                Impact::Repair,
                            );
                        }
                    }
                    slice(d, 0, ds)?;
                    count += 1;
                    check(count, l.max_glyphs, "PCL aggregate character records")?;
                    budget.add(d.len().saturating_mul(2))?;
                    pending = Some(Pending {
                        code: cp,
                        descriptor: d[..ds].to_vec(),
                        data: d[ds..].to_vec(),
                    });
                }
            }
            _ => {
                ignored = true;
            }
        }
    }
    if let Some(p) = pending {
        finish_character(
            p,
            current
                .as_mut()
                .ok_or_else(|| invalid("PCL character precedes font"))?,
            l,
            budget,
            &mut report,
        )?;
    }
    if let Some(f) = current {
        budget.items::<Font>(1)?;
        fonts.push(finish_font(f)?);
    }
    if fonts.is_empty() {
        return Err(invalid("PCL stream contains no bitmap font definition"));
    }
    if ignored {
        note(
            &mut report,
            "commands_not_modelled",
            "Non-font commands are retained in the source stream but have no font-model equivalent.",
            None,
            Impact::Loss,
        );
    }
    budget.add(b.len() + 256)?;
    let mut s = set(fonts, "hppcl");
    s.properties.push(Property::new(
        "hppcl.stream",
        PropertyValue::Bytes(b.to_vec()),
    ));
    Ok((s, report))
}
fn pack_rle(
    raw: &[u8],
    w: u32,
    h: u32,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let row = (w as usize).div_ceil(8);
    let bound = (w as usize + 2).saturating_mul(h as usize);
    check(bound, l.max_table_bytes, "PCL compressed output bound")?;
    budget.add(bound)?;
    let mut out = Vec::with_capacity(bound);
    let mut y = 0;
    while y < h as usize {
        let line = &raw[y * row..(y + 1) * row];
        let mut repeated = 1;
        while repeated < 256
            && y + repeated < h as usize
            && &raw[(y + repeated) * row..(y + repeated + 1) * row] == line
        {
            repeated += 1;
        }
        out.push((repeated - 1) as u8);
        let mut x = 0;
        let mut black = false;
        while x < w as usize {
            let start = x;
            while x < w as usize && (line[x / 8] & (128 >> (x % 8)) != 0) == black {
                x += 1;
            }
            let mut run = x - start;
            while run > 255 {
                out.extend_from_slice(&[255, 0]);
                run -= 255;
            }
            out.push(run as u8);
            black = !black;
        }
        y += repeated;
    }
    Ok(out)
}
fn append(
    out: &mut Vec<u8>,
    data: &[u8],
    v: &HppclWriteOptions,
    budget: &mut Budget,
) -> Result<()> {
    let total = out
        .len()
        .checked_add(data.len())
        .ok_or_else(|| invalid("PCL output size overflow"))?;
    check(
        total,
        v.max_output_bytes
            .min(v.limits.max_input_bytes)
            .min(v.limits.max_table_bytes),
        "PCL output bytes",
    )?;
    budget.add(data.len().saturating_mul(2))?;
    out.extend_from_slice(data);
    Ok(())
}
fn number_command(
    out: &mut Vec<u8>,
    prefix: &str,
    n: usize,
    end: char,
    v: &HppclWriteOptions,
    budget: &mut Budget,
) -> Result<()> {
    let cmd = format!("\x1b{prefix}{n}{end}");
    append(out, cmd.as_bytes(), v, budget)
}
/// Write a stream and verify all output under the same ceilings.
pub fn write(s: &FontSet, o: &WriteOptions, v: &HppclWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let out = encode(&converted.fonts, v, &mut budget, &mut report)?;
        let (actual, r) = parse(&out, &HppclReadOptions::default(), &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &actual, "hppcl", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "hppcl", o.strict)
}
// Dashen passes its already-accounted monochrome strikes and shared budget.
pub(crate) fn encode(
    fonts: &[Font],
    v: &HppclWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    if fonts.is_empty() {
        return Err(invalid("PCL needs at least one font"));
    }
    if !(17..=32767).contains(&v.continuation_bytes) {
        return Err(invalid("PCL continuation payload must be 17..32767 bytes"));
    }
    let mut out = Vec::new();
    let mut aggregate = 0usize;
    for f in fonts {
        let slots = select(f, 65535, v.unicode_as_legacy, "hppcl", report, budget)?;
        aggregate += slots.len();
        check(aggregate, v.limits.max_glyphs, "PCL aggregate glyphs")?;
        let old = bytes(f, "hppcl.header");
        let old_res = old
            .filter(|b| b.len() >= 68 && b[2] == 20)
            .map(|b| (u16_at(b, 64, ByteOrder::Big), u16_at(b, 66, ByteOrder::Big)))
            .map(|(x, y)| Ok::<_, crate::Error>((x?, y?)))
            .transpose()?;
        let res = v.resolution.or(old_res);
        if res.is_some_and(|(x, y)| x == 0 || y == 0) {
            return Err(invalid("PCL resolution must be nonzero"));
        }
        let size = if res.is_some() { 68 } else { 64 };
        let copyright = f.names.copyright.as_deref().unwrap_or("");
        if copyright.chars().any(|c| c as u32 > 255) {
            return Err(invalid("PCL copyright must be Latin-1"));
        }
        budget.add(size + copyright.len())?;
        let mut head = vec![0; size];
        if let Some(b) = old.filter(|b| b.len() >= 64) {
            head[..64].copy_from_slice(&b[..64]);
        }
        put16(&mut head, 0, size as u16, ByteOrder::Big);
        head[2] = if res.is_some() { 20 } else { 0 };
        head[3] = 3;
        head[12] = v.orientation as u8;
        head[13] = 1;
        let ascent = f.metrics.ascent.unwrap_or_else(|| {
            f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                .max()
                .unwrap_or(0)
        });
        let descent = f.metrics.descent.unwrap_or_else(|| {
            -f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y)
                .min()
                .unwrap_or(0)
        });
        let cell = u16::try_from(
            ascent
                .checked_add(descent)
                .ok_or_else(|| invalid("PCL cell height overflow"))?,
        )
        .map_err(|_| invalid("PCL cell height exceeds u16"))?;
        put16(
            &mut head,
            6,
            u16::try_from(ascent).map_err(|_| invalid("PCL baseline exceeds u16"))?,
            ByteOrder::Big,
        );
        let width = f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0);
        put16(
            &mut head,
            8,
            u16::try_from(width).map_err(|_| invalid("PCL cell width exceeds u16"))?,
            ByteOrder::Big,
        );
        put16(&mut head, 10, cell, ByteOrder::Big);
        if let Some(symbol) = v.symbol_set {
            put16(&mut head, 14, symbol, ByteOrder::Big);
        }
        if old.is_none() {
            put16(
                &mut head,
                18,
                cell.checked_mul(4)
                    .ok_or_else(|| invalid("PCL font height exceeds quarter-dot range"))?,
                ByteOrder::Big,
            );
        }
        let line = f.metrics.line_height.unwrap_or(i64::from(cell));
        put16(
            &mut head,
            32,
            u16::try_from(
                line.checked_mul(4)
                    .ok_or_else(|| invalid("PCL line height overflow"))?,
            )
            .map_err(|_| invalid("PCL text height exceeds u16"))?,
            ByteOrder::Big,
        );
        put16(
            &mut head,
            36,
            *slots
                .keys()
                .next()
                .ok_or_else(|| invalid("PCL needs encoded glyphs"))? as u16,
            ByteOrder::Big,
        );
        put16(
            &mut head,
            38,
            *slots
                .keys()
                .next_back()
                .ok_or_else(|| invalid("PCL needs encoded glyphs"))? as u16,
            ByteOrder::Big,
        );
        if let Some((x, y)) = res {
            put16(&mut head, 64, x, ByteOrder::Big);
            put16(&mut head, 66, y, ByteOrder::Big);
        }
        if let Some(name) = &f.names.full {
            if name.chars().count() > 16 || name.chars().any(|c| c as u32 > 255 || c == '\0') {
                return Err(invalid("PCL font name exceeds 16 Latin-1 bytes"));
            }
            head[48..64].fill(0);
            for (i, c) in name.chars().enumerate() {
                head[48 + i] = c as u8;
            }
        }
        head.extend(copyright.chars().map(|c| c as u8));
        number_command(&mut out, ")s", head.len(), 'W', v, budget)?;
        append(&mut out, &head, v, budget)?;
        for (&cp, g) in &slots {
            let bitmap = mono(g)?;
            let turn = v.orientation as u8;
            let (rw, rh, raw) = rotated(
                bitmap.as_bytes(),
                bitmap.width(),
                bitmap.height(),
                (4 - turn) % 4,
                &v.limits,
                budget,
            )?;
            if rw > 16384 || rh > 16384 {
                return Err(invalid("PCL bitmap dimensions exceed 16384"));
            }
            let x = g.metrics.bounds.x;
            let y = g.metrics.bounds.y;
            let w = i64::from(bitmap.width());
            let h = i64::from(bitmap.height());
            let (left, top) = match turn {
                0 => (x, y + h - 1),
                1 => (-y - h + 1, x + w - 1),
                2 => (-x - w + 1, -y),
                _ => (y, -x),
            };
            if !(-16384..=16384).contains(&left) || !(-16384..=16384).contains(&top) {
                return Err(invalid("PCL offsets exceed signed 14-bit range"));
            }
            let advance = g.metrics.advance.map_or(w, |a| a.x);
            let mut delta = advance
                .checked_mul(4)
                .ok_or_else(|| invalid("PCL advance overflow"))?;
            if let Some(PropertyValue::Bytes(d)) = g.properties.get("hppcl.descriptor")
                && d.len() >= 16
            {
                let original = i64::from(u16_at(d, 14, ByteOrder::Big)? as i16);
                if original.div_euclid(4) == advance {
                    delta = original;
                }
            }
            let delta =
                i16::try_from(delta).map_err(|_| invalid("PCL quarter-dot advance exceeds i16"))?;
            let mut d = [0; 16];
            d[0] = 4;
            d[2] = 14;
            d[3] = if v.compression == HppclCompression::Raw {
                1
            } else {
                2
            };
            d[4] = turn;
            put16(&mut d, 6, left as i16 as u16, ByteOrder::Big);
            put16(&mut d, 8, top as i16 as u16, ByteOrder::Big);
            put16(&mut d, 10, rw as u16, ByteOrder::Big);
            put16(&mut d, 12, rh as u16, ByteOrder::Big);
            put16(&mut d, 14, delta as u16, ByteOrder::Big);
            let data = if v.compression == HppclCompression::Rle {
                pack_rle(&raw, rw, rh, &v.limits, budget)?
            } else {
                raw
            };
            number_command(&mut out, "*c", cp as usize, 'E', v, budget)?;
            let initial = data.len().min(v.continuation_bytes - 16);
            number_command(&mut out, "(s", initial + 16, 'W', v, budget)?;
            append(&mut out, &d, v, budget)?;
            append(&mut out, &data[..initial], v, budget)?;
            let mut at = initial;
            while at < data.len() {
                let end = (at + v.continuation_bytes - 2).min(data.len());
                number_command(&mut out, "(s", end - at + 2, 'W', v, budget)?;
                append(&mut out, &[4, 1], v, budget)?;
                append(&mut out, &data[at..end], v, budget)?;
                at = end;
            }
        }
    }
    Ok(out)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&HppclOrientation::Portrait), "portrait");
        assert_eq!(round_trip(&HppclOrientation::Landscape), "landscape");
        assert_eq!(
            round_trip(&HppclOrientation::ReversePortrait),
            "reverse_portrait"
        );
        assert_eq!(
            round_trip(&HppclOrientation::ReverseLandscape),
            "reverse_landscape"
        );
        rejects::<HppclOrientation>(r#""not_a_variant""#);
        assert_eq!(round_trip(&HppclCompression::Raw), "raw");
        assert_eq!(round_trip(&HppclCompression::Rle), "rle");
        rejects::<HppclCompression>(r#""not_a_variant""#);
        defaults::<HppclReadOptions>();
        defaults::<HppclWriteOptions>();
        defaults::<HppclCodec>();
    }
}
