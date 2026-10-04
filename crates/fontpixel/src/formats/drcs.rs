//! DEC VT320/VT510 downloadable character-set data, without terminal execution.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Bounds, Mono};
use crate::model::{
    Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Spacing, Vector,
};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Terminal profile governing omitted DECDLD dimensions and legal matrices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Terminal {
    /// VT320: default 15 by 12 (9 by 12 in 132-column mode).
    #[default]
    Vt320,
    /// VT510: default 10 by 16 (6 by 16 in 132-column mode).
    Vt510,
}
/// Downloaded character-set size. Codes remain legacy graphic-set positions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DrcsCharset {
    /// Positions 0x21 through 0x7e.
    #[default]
    NinetyFour,
    /// Positions 0x20 through 0x7f.
    NinetySix,
}
/// DRCS reading interpretation.
#[derive(Debug, Clone, Default)]
pub struct DrcsReadOptions {
    /// Terminal profile; dimensions cannot be inferred from omitted parameters.
    pub terminal: Terminal,
    /// Baseline down from the cell top; omitted means the cell bottom.
    pub baseline: Option<i64>,
}
/// Bounded DRCS writer options.
#[derive(Debug, Clone)]
pub struct DrcsWriteOptions {
    /// Terminal profile limiting downloadable matrices.
    pub terminal: Terminal,
    /// Output cell; omitted derives a common enclosing cell.
    pub cell: Option<Bounds>,
    /// Output set size.
    pub charset: DrcsCharset,
    /// One final byte with up to two intermediate bytes; default is space-at.
    pub designator: String,
    /// Use C1 DCS/ST bytes instead of seven-bit escape sequences.
    pub eight_bit_controls: bool,
    /// Explicit threshold required for Gray8 output.
    pub gray_threshold: Option<u8>,
    /// Bounds on source, output, model and temporary storage.
    pub limits: ResourceLimits,
}
impl Default for DrcsWriteOptions {
    fn default() -> Self {
        Self {
            terminal: Terminal::Vt320,
            cell: None,
            charset: DrcsCharset::NinetyFour,
            designator: " @".into(),
            eight_bit_controls: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
        }
    }
}
/// Native DRCS codec.
#[derive(Debug, Clone, Default)]
pub struct DrcsCodec {
    /// Reader interpretation.
    pub read_options: DrcsReadOptions,
    /// Writer projection and limits.
    pub write_options: DrcsWriteOptions,
}
fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| invalid("DRCS budget overflow"))?;
        check(self.used, self.max, "DRCS aggregate bytes")
    }
}
fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: 65535,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}
fn loss(
    r: &mut Report,
    b: &mut Budget,
    code: &str,
    glyph: Option<usize>,
    message: impl Into<String>,
) -> Result<()> {
    let message = message.into();
    b.add(512 + message.len() + code.len())?;
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("drcs.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: glyph,
            ..Default::default()
        },
        message,
    });
    Ok(())
}
fn designator(s: &[u8]) -> bool {
    !s.is_empty()
        && s.len() <= 3
        && (0x30..=0x7e).contains(s.last().unwrap_or(&0))
        && s[..s.len() - 1].iter().all(|c| (0x20..=0x2f).contains(c))
}
fn geometry(p: &[u32; 8], terminal: Terminal) -> Result<(u32, u32)> {
    if p[0] > 2
        || p[2] > 2
        || p[5] > 2
        || p[7] > 1
        || ![0, 1, 2, 11, 12, 21, 22].contains(&p[4])
        || p[3] == 1
    {
        return Err(invalid("invalid DEC download parameters"));
    }

    let narrow = matches!(p[4], 2 | 12 | 22);
    let (maxw, maxh) = match terminal {
        Terminal::Vt320 => (if narrow { 9 } else { 15 }, 12),
        Terminal::Vt510 => (
            if narrow { 6 } else { 10 },
            match p[4] {
                11 | 12 => 10,
                21 | 22 => 8,
                _ => 16,
            },
        ),
    };
    let (w, h) = if (2..=4).contains(&p[3]) {
        (p[3] + 3, 10)
    } else {
        (
            if p[3] == 0 { maxw } else { p[3] },
            if p[6] == 0 { maxh } else { p[6] },
        )
    };
    if (terminal == Terminal::Vt320 && p[0] > 1)
        || (terminal == Terminal::Vt510 && p[5] != 2 && p[3] > if narrow { 5 } else { 8 })
    {
        return Err(invalid(
            "DRCS parameters invalid for selected terminal profile",
        ));
    }
    if w < 5 || w > maxw || h == 0 || h > maxh {
        return Err(invalid(
            "DRCS matrix is not legal for the selected terminal profile",
        ));
    }
    Ok((w, h))
}
/// Conservative structural recognition of a DECDLD introducer and final byte.
pub fn detect(bytes: &[u8]) -> Confidence {
    let Some(rest) = bytes
        .strip_prefix(b"\x1bP")
        .or_else(|| bytes.strip_prefix(b"\x90"))
    else {
        return Confidence::NoMatch;
    };
    let p = rest.iter().position(|c| !c.is_ascii_digit() && *c != b';');
    if p.is_some_and(|i| rest[i] == b'{') {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
impl FormatDetector for DrcsCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for DrcsCodec {
    fn read(&self, c: &mut ReadContext<'_>, o: &ReadOptions) -> Result<(FontSet, Report)> {
        parse(
            c.bytes(),
            &self.read_options,
            &o.limits,
            c.retained_input_bytes(),
        )
    }
}
/// Decode a single standalone DECDLD download under caller-supplied limits.
pub fn read(bytes: &[u8], o: &DrcsReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    parse(bytes, o, l, bytes.len())
}
fn parse(
    bytes: &[u8],
    o: &DrcsReadOptions,
    l: &ResourceLimits,
    retained: usize,
) -> Result<(FontSet, Report)> {
    check(
        bytes.len(),
        l.max_input_bytes.min(l.max_table_bytes),
        "DRCS input bytes",
    )?;
    let mut b = Budget {
        used: 0,
        max: l.max_total_bytes,
    };
    b.add(retained)?;
    let src = bytes
        .strip_prefix(b"\x1bP")
        .or_else(|| bytes.strip_prefix(b"\x90"))
        .ok_or_else(|| invalid("missing DRCS DCS introducer"))?;
    let end = src
        .iter()
        .position(|&c| c == b'{')
        .ok_or_else(|| invalid("missing DECDLD final byte"))?;
    if end > 87 {
        return Err(invalid(
            "DRCS parameter string exceeds bounded decimal fields",
        ));
    }
    let mut p = [0u32; 8];
    let parts = src[..end].split(|&c| c == b';');
    for (i, part) in parts.enumerate() {
        if i >= 8 {
            return Err(invalid("too many DRCS parameters"));
        }
        for &c in part {
            if !c.is_ascii_digit() {
                return Err(invalid("nondecimal DRCS parameter"));
            }
            p[i] = p[i]
                .checked_mul(10)
                .and_then(|v| v.checked_add(u32::from(c - b'0')))
                .ok_or_else(|| invalid("DRCS parameter overflow"))?;
        }
    }
    let (w, h) = geometry(&p, o.terminal)?;
    let charset = if p[7] == 0 {
        DrcsCharset::NinetyFour
    } else {
        DrcsCharset::NinetySix
    };
    let first = 32
        + if charset == DrcsCharset::NinetyFour {
            p[1].max(1)
        } else {
            p[1]
        };
    let last = if charset == DrcsCharset::NinetyFour {
        126
    } else {
        127
    };
    if first > last {
        return Err(invalid("DRCS starting character outside set"));
    }
    let mut at = end + 1;
    let name_start = at;
    while src.get(at).is_some_and(|c| (0x20..=0x2f).contains(c)) {
        at += 1;
        if at - name_start > 2 {
            return Err(invalid("DRCS designator has too many intermediates"));
        }
    }
    at += 1;
    let name = src
        .get(name_start..at)
        .filter(|s| designator(s))
        .ok_or_else(|| invalid("invalid DRCS set designator"))?;
    let body_start = at;
    while at < src.len() && !matches!(src[at], 0x1b | 0x9c) {
        at += 1;
    }
    let body = &src[body_start..at];
    let rest = src
        .get(at..)
        .ok_or_else(|| invalid("truncated DRCS terminator"))?;
    let trailing = rest
        .strip_prefix(b"\x1b\\")
        .or_else(|| rest.strip_prefix(b"\x9c"))
        .ok_or_else(|| invalid("missing DRCS ST terminator"))?;
    let trailing = trailing.trim_ascii();
    if !trailing.is_empty() {
        let expected = [b"\x1b(".as_slice(), name].concat();
        if trailing != expected {
            return Err(invalid(
                "DRCS file contains unsupported additional terminal commands",
            ));
        }
    }
    if body.is_empty() {
        return Err(invalid("DRCS download has no glyph patterns"));
    }
    let body = body.trim_ascii_end();
    let body = body.strip_suffix(b";").unwrap_or(body);
    let count = body.split(|&c| c == b';').count();
    if p[2] == 1
        && (first
            != if charset == DrcsCharset::NinetyFour {
                33
            } else {
                32
            }
            || count as u32 != last - first + 1)
    {
        return Err(invalid("partial DRCS reload needs previous terminal state"));
    }
    check(count, l.max_glyphs, "DRCS glyph count")?;
    if count as u32 > last - first + 1 {
        return Err(invalid("DRCS download extends past character set"));
    }
    let baseline = o.baseline.unwrap_or(i64::from(h));
    let y = baseline
        .checked_sub(i64::from(h))
        .ok_or_else(|| invalid("DRCS baseline overflow"))?;
    i64::from(h)
        .checked_sub(baseline)
        .ok_or_else(|| invalid("DRCS descent overflow"))?;
    check(
        w as usize * h as usize,
        l.max_pixels_per_glyph,
        "DRCS glyph pixels",
    )?;
    let stride = (w as usize).div_ceil(8);
    let size = stride * h as usize;
    let mut f = Font::default();
    for (i, pattern) in body.split(|&c| c == b';').enumerate() {
        b.add(size * 3 + 2048)?;
        let mut pixels = vec![0u8; size];
        let mut x = 0u32;
        let mut row = 0u32;
        for &c in pattern {
            match c {
                b'\r' | b'\n' => {}
                b'/' => {
                    row += 6;
                    x = 0;
                    if row >= h {
                        return Err(invalid("DRCS sixel row outside matrix"));
                    }
                }
                b'?'..=b'~' => {
                    if x >= w {
                        return Err(invalid("DRCS sixel column outside matrix"));
                    }
                    let sixel = c - b'?';
                    for bit in 0..6 {
                        if sixel & (1 << bit) != 0 {
                            if row + bit >= h {
                                return Err(invalid("DRCS set pixel beyond matrix height"));
                            }
                            pixels[(row + bit) as usize * stride + x as usize / 8] |=
                                128 >> (x % 8);
                        }
                    }
                    x += 1;
                }
                _ => return Err(invalid("unsupported DRCS pattern control byte")),
            }
        }
        let mut g = Glyph::new(Bitmap::<Mono>::from_raw(w, h, &pixels, bitmap_limits(l))?);
        g.metrics.bounds.y = y;
        g.metrics.advance = Some(Vector::new(i64::from(w), 0));
        g.codepoints.push(Codepoint::Legacy(first + i as u32));
        f.add_glyph(g)?;
    }
    f.metrics.bounds = Some(Bounds::new(w, h, 0, y));
    f.metrics.ascent = Some(baseline);
    f.metrics.descent = Some(i64::from(h) - baseline);
    f.metrics.line_height = Some(i64::from(h));
    f.metrics.spacing = Some(Spacing::CharacterCell);
    f.charset.name = Some("dec-drcs".into());
    f.source_format = Some("drcs".into());
    b.add(4096)?;
    for (key, value) in [
        ("drcs.font_number", p[0]),
        ("drcs.erase", p[2]),
        ("drcs.width_parameter", p[3]),
        ("drcs.screen", p[4]),
        ("drcs.usage", p[5]),
        ("drcs.height_parameter", p[6]),
        ("drcs.charset_size", p[7]),
    ] {
        f.properties.push(Property::new(key, i64::from(value)));
    }
    f.properties.push(Property::new(
        "drcs.designator",
        String::from_utf8(name.to_vec()).map_err(|_| invalid("non-ASCII DRCS designator"))?,
    ));
    f.properties.push(Property::new(
        "drcs.terminal",
        match o.terminal {
            Terminal::Vt320 => "vt320",
            Terminal::Vt510 => "vt510",
        },
    ));
    f.validate(l)?;
    let mut set = FontSet::from(f);
    set.source_format = Some("drcs".into());
    Ok((set, Report::default()))
}
impl FormatWriter<FontSet> for DrcsCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(set, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
fn property_cost(p: &crate::model::Properties, b: &mut Budget) -> Result<()> {
    let mut stack = Vec::new();
    for p in p.iter() {
        b.add(p.name.len() + p.raw.as_ref().map_or(0, Vec::len) + 128)?;
        stack.push(&p.value);
    }
    while let Some(v) = stack.pop() {
        b.add(64)?;
        match v {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => b.add(s.len())?,
            PropertyValue::Bytes(v) => b.add(v.len())?,
            PropertyValue::List(v) => {
                b.add(v.len().saturating_mul(16))?;
                stack.extend(v);
            }
            _ => {}
        }
    }
    Ok(())
}
/// Emit a deterministic full-cell download; all representational changes are reported.
pub fn write(set: &FontSet, o: &DrcsWriteOptions) -> Result<(Vec<u8>, Report)> {
    let l = &o.limits;
    set.validate(l)?;
    let f = set.single()?;
    let mut b = Budget {
        used: 0,
        max: l.max_total_bytes,
    };
    let mut r = Report::default();
    property_cost(&set.properties, &mut b)?;
    property_cost(&f.properties, &mut b)?;
    property_cost(&f.headers, &mut b)?;
    let names = &f.names;
    for value in [
        &names.family,
        &names.style,
        &names.full,
        &names.postscript,
        &names.version,
        &names.copyright,
        &names.license,
        &names.license_url,
        &names.foundry,
        &names.xlfd,
        &f.charset.name,
        &f.charset.registry,
        &f.charset.encoding,
        &f.source_format,
        &set.source_format,
    ]
    .into_iter()
    .flatten()
    {
        b.add(value.len())?;
    }
    for value in &names.xlfd_fields {
        b.add(value.len().saturating_add(64))?;
    }
    b.add(f.charset.code_ranges.len().saturating_mul(8))?;
    property_cost(&names.localized, &mut b)?;
    let mut selected = BTreeMap::new();
    let low = if o.charset == DrcsCharset::NinetyFour {
        33
    } else {
        32
    };
    let high = if o.charset == DrcsCharset::NinetyFour {
        126
    } else {
        127
    };
    let mut inferred: Option<Bounds> = None;
    for (id, g) in f.glyphs() {
        b.add(
            2048 + g.bitmap.as_bytes().len().saturating_mul(2)
                + g.codepoints.len().saturating_mul(32),
        )?;
        property_cost(&g.properties, &mut b)?;
        if let Some(name) = &g.name {
            b.add(name.len())?;
        }
        for sequence in &g.unicode_sequences {
            b.add(sequence.len().saturating_mul(4).saturating_add(64))?;
        }
        let cp = g.codepoints.iter().find_map(|cp| {
            if let Codepoint::Legacy(v) = cp {
                if (*v >= low) && (*v <= high) {
                    Some(*v)
                } else {
                    None
                }
            } else {
                None
            }
        });
        if let Some(cp) = cp {
            if selected.insert(cp, (id.index(), g)).is_some() {
                return Err(invalid("duplicate DRCS output mapping"));
            }
            let bb = g.metrics.bounds;
            if bb.width > 0 && bb.height > 0 {
                inferred = Some(if let Some(old) = inferred {
                    let x = old.x.min(bb.x);
                    let y = old.y.min(bb.y);
                    let right = (old.x + i64::from(old.width)).max(bb.x + i64::from(bb.width));
                    let top = (old.y + i64::from(old.height)).max(bb.y + i64::from(bb.height));
                    Bounds::new(
                        u32::try_from(right - x).map_err(|_| invalid("DRCS width overflow"))?,
                        u32::try_from(top - y).map_err(|_| invalid("DRCS height overflow"))?,
                        x,
                        y,
                    )
                } else {
                    bb
                });
            }
        } else {
            loss(
                &mut r,
                &mut b,
                "write.unmapped",
                Some(id.index()),
                "Glyph lacks a Legacy code in the selected DRCS set and was omitted",
            )?;
        }
    }
    if selected.is_empty() {
        return Err(invalid("DRCS output has no representable Legacy glyphs"));
    }
    let cell = o.cell.or(inferred).unwrap_or(Bounds::new(5, 1, 0, 0));
    let p = [
        0,
        0,
        2,
        cell.width,
        0,
        2,
        cell.height,
        u32::from(o.charset == DrcsCharset::NinetySix),
    ];
    geometry(&p, o.terminal)?;
    if !designator(o.designator.as_bytes()) {
        return Err(invalid("invalid DRCS output designator"));
    }
    let first = *selected.first_key_value().expect("selected nonempty").0;
    let last = *selected.last_key_value().expect("selected nonempty").0;
    check(
        (last - first + 1) as usize,
        l.max_glyphs,
        "DRCS output glyph count",
    )?;
    check(
        cell.width as usize * cell.height as usize,
        l.max_pixels_per_glyph,
        "DRCS output cell pixels",
    )?;
    let mut text = String::new();
    write!(
        text,
        "0;{};2;{};0;2;{};{}{{{}",
        first - 32,
        cell.width,
        cell.height,
        p[7],
        o.designator
    )
    .expect("String write");
    b.add(
        (last - first + 1) as usize
            * (cell.width as usize * (cell.height as usize).div_ceil(6) + 32)
            * 4,
    )?;
    for cp in first..=last {
        if cp != first {
            text.push(';');
        }
        let g = selected.get(&cp).map(|v| v.1);
        if let Some(g) = g {
            if matches!(g.bitmap, GlyphBitmap::Gray8(_)) && o.gray_threshold.is_none() {
                return Err(invalid("Gray8 DRCS output requires gray_threshold"));
            }
        } else {
            loss(
                &mut r,
                &mut b,
                "write.added_empty",
                None,
                format!("Added blank character slot {cp}"),
            )?;
        }
        for row in (0..cell.height).step_by(6) {
            if row != 0 {
                text.push('/');
            }
            for x in 0..cell.width {
                let mut sixel = 0u8;
                if let Some(g) = g {
                    for bit in 0..6 {
                        if row + bit >= cell.height {
                            break;
                        }
                        let px = cell
                            .x
                            .checked_add(i64::from(x))
                            .and_then(|v| v.checked_sub(g.metrics.bounds.x))
                            .ok_or_else(|| invalid("DRCS x overflow"))?;
                        let py = g
                            .metrics
                            .bounds
                            .y
                            .checked_add(i64::from(g.bitmap.height()))
                            .and_then(|v| {
                                cell.y
                                    .checked_add(i64::from(cell.height))
                                    .and_then(|top| v.checked_sub(top))
                            })
                            .and_then(|v| v.checked_add(i64::from(row + bit)))
                            .ok_or_else(|| invalid("DRCS y overflow"))?;
                        if px >= 0
                            && py >= 0
                            && px < i64::from(g.bitmap.width())
                            && py < i64::from(g.bitmap.height())
                        {
                            let on = match &g.bitmap {
                                GlyphBitmap::Mono(v) => v.get_pixel(px as u32, py as u32)? != 0,
                                GlyphBitmap::Gray8(v) => {
                                    v.get_pixel(px as u32, py as u32)?
                                        >= o.gray_threshold.unwrap_or(128)
                                }
                            };
                            if on {
                                sixel |= 1 << bit;
                            }
                        }
                    }
                }
                text.push(char::from(b'?' + sixel));
            }
        }
    }
    text.push(';');
    let mut out = if o.eight_bit_controls {
        vec![0x90]
    } else {
        vec![0x1b, b'P']
    };
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(if o.eight_bit_controls {
        b"\x9c"
    } else {
        b"\x1b\\"
    });
    check(
        out.len(),
        l.max_input_bytes.min(l.max_table_bytes),
        "DRCS output bytes",
    )?;
    b.add(out.len() * 2)?;
    let (readback, _) = read(
        &out,
        &DrcsReadOptions {
            terminal: o.terminal,
            baseline: None,
        },
        &ResourceLimits {
            max_total_bytes: l.max_total_bytes.saturating_sub(b.used),
            ..l.clone()
        },
    )?;
    let back = readback.single()?;
    if f.names != back.names {
        loss(
            &mut r,
            &mut b,
            "write.names",
            None,
            "DRCS does not retain font names",
        )?;
    }
    if f.metrics != back.metrics {
        loss(
            &mut r,
            &mut b,
            "write.font_metrics",
            None,
            "DRCS fixed cell and bottom baseline change global metrics",
        )?;
    }
    if f.charset != back.charset {
        loss(
            &mut r,
            &mut b,
            "write.charset",
            None,
            "DRCS character positions retain legacy set semantics",
        )?;
    }
    if f.headers != back.headers
        || f.properties != back.properties
        || set.properties != readback.properties
    {
        loss(
            &mut r,
            &mut b,
            "write.properties",
            None,
            "Source properties differ from native DRCS parameters",
        )?;
    }
    if f.source_format != back.source_format || set.source_format != readback.source_format {
        loss(
            &mut r,
            &mut b,
            "write.source_format",
            None,
            "Source format provenance changes",
        )?;
    }
    if f.slot_count() != back.slot_count()
        || selected.iter().any(|(&cp, (id, _))| {
            back.glyph_id(Codepoint::Legacy(cp))
                .is_none_or(|v| v.index() != *id)
        })
    {
        loss(
            &mut r,
            &mut b,
            "write.glyph_slots",
            None,
            "DRCS output reordered or normalized stable glyph slots",
        )?;
    }
    for (&cp, (id, g)) in &selected {
        let target = back
            .glyph_for_codepoint(Codepoint::Legacy(cp))
            .ok_or_else(|| invalid("DRCS readback mapping missing"))?;
        if g.bitmap != target.bitmap {
            loss(
                &mut r,
                &mut b,
                "write.pixels",
                Some(*id),
                "DRCS normalized, cropped or reduced the glyph raster",
            )?;
        }
        if g.metrics != target.metrics {
            loss(
                &mut r,
                &mut b,
                "write.glyph_metrics",
                Some(*id),
                "DRCS cell changed glyph metrics",
            )?;
        }
        if g.codepoints != target.codepoints || g.unicode_sequences != target.unicode_sequences {
            loss(
                &mut r,
                &mut b,
                "write.mappings",
                Some(*id),
                "DRCS retains one legacy mapping and no sequences",
            )?;
        }
        if g.name != target.name || g.properties != target.properties {
            loss(
                &mut r,
                &mut b,
                "write.glyph_metadata",
                Some(*id),
                "DRCS drops glyph names and properties",
            )?;
        }
    }
    Ok((out, r))
}
