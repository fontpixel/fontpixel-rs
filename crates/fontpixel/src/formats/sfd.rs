//! Bounded FontForge SFD bitmap strikes; outline omission is always reported.
use super::editor_fidelity as fidelity;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Gray8, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
use fidelity::{Budget, audit, bound, loss};
use std::collections::BTreeMap;
mod ascii85;
mod writer;
/// Controls deterministic bitmap-only SFD output.
#[derive(Debug, Clone, Default)]
pub struct SfdWriteOptions {
    /// Cumulative model, input and output budgets.
    pub limits: ResourceLimits,
}
/// SFD registry adapter, retaining all bitmap strikes.
#[derive(Debug, Clone, Default)]
pub struct SfdCodec {
    /// Native writer controls.
    pub write_options: SfdWriteOptions,
}
impl FormatDetector for SfdCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for SfdCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (s, mut r) = parse(c.bytes(), c.limits(), c.retained_input_bytes())?;
        let model = s
            .fonts
            .iter()
            .try_fold(fidelity::properties_bytes(&s.properties)?, |n, f| {
                Ok::<_, Error>(n.saturating_add(fidelity::model_bytes(f)?))
            })?;
        fidelity::attach_asset(
            &mut r,
            c.name(),
            c.retained_input_bytes(),
            model,
            c.limits(),
        )?;
        Ok((s, r))
    }
}
impl FormatWriter<FontSet> for SfdCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (b, r) = write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), b),
            report: r,
        })
    }
}
/// Recognize the required first-line magic.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"SplineFontDB:") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn invalid(m: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("SFD: {m}"))
}
fn ints(s: &str) -> Result<Vec<i64>> {
    s.split_whitespace()
        .map(|v| v.parse().map_err(invalid))
        .collect()
}
fn bl(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: i32::MAX as u32,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}
/// Decode all bitmap strikes. Outline-only documents fail explicitly.
pub fn read(b: &[u8], l: &ResourceLimits) -> Result<(FontSet, Report)> {
    parse(b, l, b.len())
}
/// Encode bitmap strikes with outline identity stubs and report unsupported fields.
pub fn write(s: &FontSet, o: &SfdWriteOptions) -> Result<(Vec<u8>, Report)> {
    let extra = fidelity::properties_bytes(&s.properties)?
        .saturating_add(s.source_format.as_ref().map_or(0, String::capacity))
        .saturating_add(
            s.fonts
                .capacity()
                .saturating_mul(std::mem::size_of::<Font>()),
        );
    writer::write(s, o)
        .map_err(|e| fidelity::failed_write(e, "sfd", s.fonts.iter().enumerate(), extra, &o.limits))
}
#[derive(Default)]
struct Identity {
    name: String,
    codes: Vec<Codepoint>,
    width: Option<i64>,
    vertical_width: Option<i64>,
}
fn parse(b: &[u8], l: &ResourceLimits, retained: usize) -> Result<(FontSet, Report)> {
    bound(b.len(), l.max_input_bytes, "SFD input")?;
    let text = std::str::from_utf8(b).map_err(invalid)?;
    if detect(b) != Confidence::Certain {
        return Err(invalid("missing magic"));
    }
    let mut budget = Budget::new(retained, l)?;
    budget.add(4096)?;
    budget.add(
        text.lines()
            .count()
            .saturating_mul(32)
            .saturating_add(b.len().saturating_mul(3)),
    )?;
    let lines: Vec<&str> = text.lines().collect();
    for line in &lines {
        bound(line.len(), l.max_table_bytes, "SFD record")?;
    }
    let mut set = FontSet {
        source_format: Some("sfd".into()),
        ..Default::default()
    };
    let mut report = Report::default();
    let mut base = Font::default();
    let mut ids = BTreeMap::<i64, Identity>::new();
    let mut i = 1;
    let mut ended = false;
    let mut total = 0usize;
    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        if let Some(name) = line.strip_prefix("StartChar:") {
            budget.add(1024 + name.len())?;
            let mut identity = Identity {
                name: name.trim().to_owned(),
                ..Default::default()
            };
            let mut slot = None;
            let mut complete = false;
            let mut outline = false;
            while i < lines.len() {
                let s = lines[i].trim();
                i += 1;
                if s == "EndChar" {
                    complete = true;
                    break;
                }
                if let Some(v) = s.strip_prefix("Encoding:") {
                    let v = ints(v)?;
                    if v.len() != 3 {
                        return Err(invalid("Encoding needs three integers"));
                    }
                    slot = Some(v[2]);
                    if v[1] >= 0 {
                        identity
                            .codes
                            .push(Codepoint::Unicode(u32::try_from(v[1]).map_err(invalid)?));
                    } else if v[0] >= 0 {
                        identity
                            .codes
                            .push(Codepoint::Legacy(u32::try_from(v[0]).map_err(invalid)?));
                    }
                } else if let Some(v) = s.strip_prefix("Width:") {
                    identity.width = Some(v.trim().parse().map_err(invalid)?);
                } else if let Some(v) = s.strip_prefix("VWidth:") {
                    identity.vertical_width = Some(v.trim().parse().map_err(invalid)?);
                } else if let Some(v) = s.strip_prefix("AltUni2:") {
                    for code in v.split_whitespace() {
                        budget.add(128)?;
                        let parts: Vec<_> = code.split('.').collect();
                        if parts.len() == 3
                            && (parts[1] == "ffffffff" || parts[1] == "-1")
                            && parts[2] == "0"
                        {
                            identity.codes.push(Codepoint::Unicode(
                                u32::from_str_radix(parts[0], 16).map_err(invalid)?,
                            ));
                        } else {
                            outline = true;
                        }
                    }
                } else if !s.is_empty() {
                    outline = true;
                }
            }
            if !complete {
                return Err(invalid("unterminated outline glyph"));
            }
            let slot = slot.ok_or_else(|| invalid("outline glyph lacks Encoding"))?;
            if ids.insert(slot, identity).is_some() {
                return Err(invalid("duplicate outline glyph slot"));
            }
            bound(ids.len(), l.max_glyphs, "SFD outline identities")?;
            if outline {
                loss(
                    &mut report,
                    "sfd",
                    "outlines",
                    0,
                    None,
                    "Outline geometry or instructions were omitted",
                );
            }
            continue;
        }
        if let Some(values) = line.strip_prefix("BitmapFont:") {
            let v = ints(values)?;
            if v.len() != 5 {
                return Err(invalid("BitmapFont needs five integers"));
            }
            if ![1, 2, 4, 8].contains(&v[4]) {
                return Err(invalid("unsupported bitmap depth"));
            }
            let depth = v[4] as usize;
            bound(
                usize::try_from(v[1]).map_err(invalid)?,
                l.max_glyphs,
                "SFD potential slots",
            )?;
            bound(set.fonts.len() + 1, l.max_resources, "SFD strikes")?;
            budget.add(fidelity::model_bytes(&base)?)?;
            let mut f = Font::default();
            f.names = base.names.clone();
            f.headers = base.headers.clone();
            f.source_format = Some("sfd".into());
            f.metrics.pixel_size = Some(u32::try_from(v[0]).map_err(invalid)?);
            f.metrics.ascent = Some(v[2]);
            f.metrics.descent = Some(v[3]);
            f.properties.push(Property::new("sfd.depth", v[4]));
            let mut complete = false;
            let mut slots = std::collections::BTreeSet::new();
            while i < lines.len() {
                let s = lines[i].trim();
                i += 1;
                if s == "EndBitmapFont" {
                    complete = true;
                    break;
                }
                if let Some(v) = s.strip_prefix("Resolution:") {
                    let n = v.trim().parse::<i64>().map_err(invalid)?;
                    f.metrics.resolution = Some(Vector::new(n, n));
                    continue;
                }
                if let Some(v) = s.strip_prefix("BDFStartProperties:") {
                    let n = v.trim().parse::<usize>().map_err(invalid)?;
                    bound(n, l.max_table_bytes, "SFD properties")?;
                    for _ in 0..n {
                        let p = *lines
                            .get(i)
                            .ok_or_else(|| invalid("truncated properties"))?;
                        i += 1;
                        let (name, value) = p
                            .split_once(char::is_whitespace)
                            .ok_or_else(|| invalid("bad BDF property"))?;
                        budget.add(256usize.saturating_add(p.len()))?;
                        f.properties.push(Property::new(
                            format!("sfd.bdf.{name}"),
                            value.trim().to_owned(),
                        ));
                    }
                    if lines.get(i).map(|s| s.trim()) != Some("BDFEndProperties") {
                        return Err(invalid("missing BDFEndProperties"));
                    }
                    i += 1;
                    continue;
                }
                if s.starts_with("BDFRefChar:") {
                    return Err(invalid(
                        "bitmap composites are not supported; no partial font returned",
                    ));
                }
                if let Some(v) = s.strip_prefix("BDFChar:") {
                    total = total
                        .checked_add(1)
                        .ok_or_else(|| invalid("glyph count overflow"))?;
                    bound(total, l.max_glyphs, "SFD glyphs")?;
                    let mut v = ints(v)?;
                    if v.len() == 6 {
                        v.insert(0, v[0]);
                    }
                    if v.len() != 7 && v.len() != 8 {
                        return Err(invalid("BDFChar needs seven or eight integers"));
                    }
                    if v[0] < 0 || !slots.insert(v[0]) {
                        return Err(invalid("negative or duplicate bitmap slot"));
                    }
                    let w = u32::try_from(
                        v[4].checked_sub(v[3])
                            .and_then(|n| n.checked_add(1))
                            .ok_or_else(|| invalid("width overflow"))?,
                    )
                    .map_err(invalid)?;
                    let h = u32::try_from(
                        v[6].checked_sub(v[5])
                            .and_then(|n| n.checked_add(1))
                            .ok_or_else(|| invalid("height overflow"))?,
                    )
                    .map_err(invalid)?;
                    let pixels = u64::from(w) * u64::from(h);
                    if pixels > l.max_pixels_per_glyph as u64 {
                        return Err(Error::ResourceLimit {
                            resource: "SFD glyph pixels",
                            limit: l.max_pixels_per_glyph,
                        });
                    }
                    let stride = (w as usize)
                        .checked_mul(depth)
                        .ok_or_else(|| invalid("stride overflow"))?
                        .div_ceil(8);
                    let bytes = stride
                        .checked_mul(h as usize)
                        .ok_or_else(|| invalid("bitmap bytes overflow"))?;
                    budget.add(
                        1536 + bytes.saturating_mul(2) + (pixels as usize).saturating_mul(2),
                    )?;
                    let begin = i;
                    while i < lines.len()
                        && !lines[i].trim().starts_with("BDFChar:")
                        && !lines[i].trim().starts_with("BDFRefChar:")
                        && lines[i].trim() != "EndBitmapFont"
                    {
                        i += 1;
                    }
                    let data = ascii85::decode(&lines[begin..i], bytes)?;
                    if !(w as usize * depth).is_multiple_of(8) {
                        let padding_mask = (1u8 << (8 - w as usize * depth % 8)) - 1;
                        if data
                            .chunks(stride.max(1))
                            .any(|row| row.last().is_some_and(|v| v & padding_mask != 0))
                        {
                            loss(
                                &mut report,
                                "sfd",
                                "padding_bits",
                                set.fonts.len(),
                                Some(f.len()),
                                "Nonzero row padding normalized",
                            );
                        }
                    }
                    let bitmap: GlyphBitmap = if depth == 1 {
                        Bitmap::<Mono>::from_raw(w, h, &data, bl(l))?.into()
                    } else {
                        let mut gray = Vec::with_capacity(pixels as usize);
                        let max = (1u16 << depth) - 1;
                        for row in data.chunks(stride.max(1)).take(h as usize) {
                            for x in 0..w as usize {
                                let sample = (row[x * depth / 8] >> (8 - depth - (x * depth % 8)))
                                    & max as u8;
                                gray.push((u16::from(sample) * 255 / max) as u8);
                            }
                        }
                        Bitmap::<Gray8>::from_raw(w, h, &gray, bl(l))?.into()
                    };
                    let mut g = Glyph::new(bitmap);
                    g.metrics.bounds.x = v[3];
                    g.metrics.bounds.y = v[5];
                    g.metrics.advance = Some(Vector::new(v[2], 0));
                    if v.len() == 8 {
                        g.metrics.vertical_advance = Some(Vector::new(
                            0,
                            v[7].checked_neg()
                                .ok_or_else(|| invalid("vertical advance overflow"))?,
                        ));
                    }
                    g.properties.push(Property::new("sfd.slot", v[0]));
                    g.properties.push(Property::new("sfd.encoding", v[1]));
                    if let Some(identity) = ids.get(&v[0]) {
                        budget.add(
                            identity
                                .name
                                .len()
                                .saturating_add(identity.codes.len().saturating_mul(128)),
                        )?;
                        g.name = Some(identity.name.clone());
                        g.codepoints = identity.codes.clone();
                        if let Some(width) = identity.width {
                            g.properties.push(Property::new("sfd.outline_width", width));
                        }
                        if let Some(width) = identity.vertical_width {
                            g.properties
                                .push(Property::new("sfd.outline_vwidth", width));
                        }
                    } else if v[1] >= 0 {
                        g.codepoints
                            .push(Codepoint::Legacy(u32::try_from(v[1]).map_err(invalid)?));
                    }
                    f.add_glyph(g)?;
                    continue;
                }
                if !s.is_empty() {
                    return Err(invalid(format!("unexpected bitmap record {s:?}")));
                }
            }
            if !complete {
                return Err(invalid("unterminated bitmap strike"));
            }
            set.fonts.push(f);
            continue;
        }
        if line == "EndSplineFont" {
            ended = true;
            if lines[i..].iter().any(|s| !s.trim().is_empty()) {
                return Err(invalid("trailing records"));
            }
            break;
        }
        if let Some((k, value)) = line.split_once(':') {
            let value = value.trim();
            match k {
                "FontName" => base.names.postscript = Some(value.to_owned()),
                "FullName" => base.names.full = Some(value.to_owned()),
                "FamilyName" => base.names.family = Some(value.to_owned()),
                "Weight" => base.names.style = Some(value.to_owned()),
                "Version" => base.names.version = Some(value.to_owned()),
                "Copyright" => base.names.copyright = Some(value.to_owned()),
                "BeginChars" => {}
                "Encoding" if value == "UnicodeFull" => {}
                "OnlyBitmaps" if value == "1" => {}
                _ => {
                    budget.add(192 + line.len())?;
                    base.headers.push(Property::new(k, value));
                }
            }
        } else if !line.is_empty() && line != "EndChars" {
            budget.add(192 + line.len())?;
            base.headers.push(Property::new("sfd.record", line));
        }
    }
    if !ended || set.fonts.is_empty() {
        return Err(invalid("missing end marker or no bitmap strikes"));
    }
    set.validate(l)?;
    Ok((set, report))
}
