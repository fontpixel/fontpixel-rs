//! Bounded literal C arrays: generic cells, font8x8, t-mat and Adafruit GFX.
mod lexer;
mod writer;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Bounds};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Rational, Spacing, Vector};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, ResourcePath, Result};
use lexer::{Declaration, Kind, Token};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Input/output C dialect. Auto is only valid for reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Family {
    /// Select structural GFX/font8x8/t-mat or an explicit generic option comment.
    #[default]
    Auto,
    /// Daniel Hepper's public-domain LSB-left 8x8 tables.
    Font8x8,
    /// Takayuki Matsuoka's MSB-left U+0020–007F table.
    Tmat,
    /// Literal byte arrays with explicit geometry and charset.
    Generic,
    /// Adafruit GFXfont, GFXglyph and bitmap arrays.
    Gfx,
}
/// Geometry for arrays that have no intrinsic metrics or character mapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArrayOptions {
    /// Positive fixed cell width.
    pub width: u32,
    /// Positive cell height.
    pub height: u32,
    /// Code assigned to row zero; successive rows increment it.
    pub first_code: u32,
    /// `unicode` uses Unicode tags; other explicit names retain legacy codes.
    pub charset: String,
    /// Baseline measured down from the top of a cell.
    pub baseline: i64,
    /// Least significant bit is the leftmost pixel in each byte.
    pub lsb_first: bool,
}
/// Reader selection. Geometry never comes from an unrelated array.
#[derive(Debug, Clone, Default)]
pub struct CReadOptions {
    /// Explicit dialect, or conservative structural detection.
    pub family: Family,
    /// Select a particular byte array or GFXfont declaration.
    pub array_name: Option<String>,
    /// Required for generic input unless a fontpixel option comment supplies it.
    pub generic: Option<ArrayOptions>,
}
/// Portable C writer choices.
#[derive(Debug, Clone)]
pub struct CWriteOptions {
    /// Generic or GFX; specialized upstream header families are read-only.
    pub family: Family,
    /// C identifier used as the output symbol stem.
    pub array_name: String,
    /// Generic geometry; otherwise compute a cell enclosing all source BBXs.
    pub generic: Option<ArrayOptions>,
    /// Optional inclusive output range; omitted glyphs are reported.
    pub range: Option<(u32, u32)>,
    /// Required for Gray8 reduction to one bit per pixel.
    pub gray_threshold: Option<u8>,
    /// Limits for source traversal, scratch, model and output bytes.
    pub limits: ResourceLimits,
}
impl Default for CWriteOptions {
    fn default() -> Self {
        Self {
            family: Family::Generic,
            array_name: "font".into(),
            generic: None,
            range: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
        }
    }
}
/// C source codec. Parsing never compiles or executes source text.
#[derive(Debug, Clone, Default)]
pub struct CCodec {
    /// Reader selection and cell geometry.
    pub read_options: CReadOptions,
    /// Writer dialect and field choices.
    pub write_options: CWriteOptions,
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
    fn new(used: usize, l: &ResourceLimits) -> Result<Self> {
        check(used, l.max_total_bytes, "C aggregate bytes")?;
        Ok(Self {
            used,
            max: l.max_total_bytes,
        })
    }
    fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| invalid("C allocation overflow"))?;
        check(self.used, self.max, "C aggregate bytes")
    }
    fn glyph(&mut self, n: usize, count: usize, l: &ResourceLimits) -> Result<()> {
        check(count, l.max_glyphs, "C glyph count")?;
        self.add(
            n.saturating_mul(2)
                .saturating_add(std::mem::size_of::<Glyph>() * 4 + 512),
        )
    }
}
fn loss(r: &mut Report, code: &str, glyph: Option<usize>, message: impl Into<String>) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("c.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: glyph,
            ..Default::default()
        },
        message: message.into(),
    });
}
fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}
fn cell_size(o: &ArrayOptions, l: &ResourceLimits) -> Result<usize> {
    if o.width == 0 || o.height == 0 {
        return Err(invalid("C generic geometry must be positive"));
    }
    if u64::from(o.width) * u64::from(o.height) > l.max_pixels_per_glyph as u64 {
        return Err(Error::ResourceLimit {
            resource: "C glyph pixels",
            limit: l.max_pixels_per_glyph,
        });
    }
    o.baseline
        .checked_sub(i64::from(o.height))
        .ok_or_else(|| invalid("C baseline overflow"))?;
    i64::from(o.height)
        .checked_sub(o.baseline)
        .ok_or_else(|| invalid("C descent overflow"))?;
    if o.charset.is_empty() {
        return Err(invalid("C generic charset must be explicit"));
    }
    usize::try_from(u64::from(o.width).div_ceil(8) * u64::from(o.height))
        .map_err(|_| invalid("C cell size overflow"))
}
fn cell_glyph(bytes: &[u8], cp: u32, o: &ArrayOptions, l: &ResourceLimits) -> Result<Glyph> {
    let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
        o.width,
        o.height,
        bytes,
        bitmap_limits(l),
    )?);
    g.codepoints.push(if o.charset == "unicode" {
        Codepoint::Unicode(cp)
    } else {
        Codepoint::Legacy(cp)
    });
    g.metrics.bounds.y = o.baseline - i64::from(o.height);
    g.metrics.advance = Some(Vector::new(i64::from(o.width), 0));
    Ok(g)
}
fn cell_metrics(f: &mut Font, o: &ArrayOptions) {
    f.metrics.pixel_size = Some(o.height);
    f.metrics.point_size = Some(Rational::integer(i64::from(o.height)));
    f.metrics.resolution = Some(Vector::new(72, 72));
    f.metrics.bounds = Some(Bounds::new(
        o.width,
        o.height,
        0,
        o.baseline - i64::from(o.height),
    ));
    f.metrics.ascent = Some(o.baseline);
    f.metrics.descent = Some(i64::from(o.height) - o.baseline);
    f.metrics.spacing = Some(Spacing::CharacterCell);
    f.charset.name = Some(o.charset.clone());
    f.source_format = Some("c".into());
}
fn embedded_options(s: &str, l: &ResourceLimits) -> Result<Option<ArrayOptions>> {
    let mut comments = lexer::comments(s).filter_map(|c| c.trim().strip_prefix("fontpixel-c: "));
    let Some(text) = comments.next() else {
        return Ok(None);
    };
    if comments.next().is_some() {
        return Err(invalid("multiple fontpixel C option comments"));
    }
    let text = text.trim();
    check(text.len(), l.max_table_bytes.min(65536), "C option bytes")?;
    let o = serde_json::from_str(text).map_err(|e| invalid(format!("C options: {e}")))?;
    Ok(Some(o))
}
impl FormatDetector for CCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        let Ok(s) = std::str::from_utf8(c.bytes()) else {
            return Ok(Confidence::NoMatch);
        };
        Ok(
            if s.contains("/* fontpixel-c: ") || (s.contains("GFXfont") && s.contains("GFXglyph")) {
                Confidence::Certain
            } else if s.contains("font8x8_") || (s.contains("unsigned char") && s.contains("0b")) {
                Confidence::Possible
            } else {
                Confidence::NoMatch
            },
        )
    }
}
impl FormatReader<FontSet> for CCodec {
    fn read(&self, c: &mut ReadContext<'_>, o: &ReadOptions) -> Result<(FontSet, Report)> {
        let l = &o.limits;
        let mut b = Budget::new(c.retained_input_bytes(), l)?;
        b.add(c.bytes().len())?;
        if let Some(g) = &self.read_options.generic {
            check(g.charset.len(), l.max_table_bytes, "C charset bytes")?;
            b.add(g.charset.len().saturating_mul(2))?;
        }
        let primary =
            String::from_utf8(c.bytes().to_vec()).map_err(|_| invalid("C header is not UTF-8"))?;
        let mut family = self.read_options.family;
        let root_name = c
            .name()
            .unwrap_or("font.h")
            .rsplit('/')
            .next()
            .unwrap_or("font.h")
            .to_owned();
        let mut pending = vec![(root_name.clone(), primary)];
        let mut seen = BTreeSet::from([root_name]);
        let mut f = Font::default();
        let mut report = Report::default();
        let mut merged = BTreeMap::<u32, Glyph>::new();
        let mut selected_arrays = 0usize;
        while let Some((name, text)) = pending.pop() {
            check(text.len(), l.max_table_bytes, "C header bytes")?;
            let t = lexer::lex(&text, &mut b)?;
            let ds = lexer::declarations(&t, &mut b)?;
            let generic = match &self.read_options.generic {
                Some(options) => Some(options.clone()),
                None => embedded_options(&text, l)?,
            };
            if family == Family::Auto {
                family = if generic.is_some() {
                    Family::Generic
                } else if ds.iter().any(|d| d.ty == "GFXfont") {
                    Family::Gfx
                } else if ds.iter().any(|d| d.name.starts_with("font8x8_"))
                    || t.iter()
                        .any(|t| matches!(t.kind, Kind::Directive(d) if d.contains("\"font8x8")))
                {
                    Family::Font8x8
                } else {
                    Family::Auto
                };
            }
            if family == Family::Font8x8 {
                for token in &t {
                    if let Kind::Directive(d) = token.kind
                        && let Some(include) = include(d)?
                    {
                        let path = name.rsplit_once('/').map_or_else(
                            || include.to_owned(),
                            |(parent, _)| format!("{parent}/{include}"),
                        );
                        let p = ResourcePath::new(&path)?;
                        if seen.insert(path.clone()) {
                            b.add(path.len() + 128)?;
                            let bytes =
                                c.resource_limited(&p, l.max_total_bytes.saturating_sub(b.used))?;
                            b.add(bytes.len())?;
                            pending.push((
                                path,
                                String::from_utf8(bytes)
                                    .map_err(|_| invalid("C include is not UTF-8"))?,
                            ));
                        }
                    }
                }
                for d in ds.iter().filter(|d| {
                    d.name.starts_with("font8x8_")
                        && self
                            .read_options
                            .array_name
                            .as_deref()
                            .is_none_or(|name| name == d.name)
                }) {
                    selected_arrays += 1;
                    header8((&text, &t), d, false, &mut merged, &mut report, l, &mut b)?;
                }
            } else {
                let kind = if family == Family::Auto && ds.iter().any(|d| d.ty == "GFXfont") {
                    Family::Gfx
                } else {
                    family
                };
                if kind == Family::Gfx {
                    f = gfx(&t, &ds, &self.read_options, l, &mut b)?;
                } else {
                    let choices: Vec<_> = ds
                        .iter()
                        .filter(|d| {
                            matches!(d.ty, "char" | "uint8_t")
                                && self
                                    .read_options
                                    .array_name
                                    .as_deref()
                                    .is_none_or(|n| n == d.name)
                        })
                        .collect();
                    let [d] = choices.as_slice() else {
                        return Err(invalid(
                            "C byte array is missing or ambiguous; select array_name",
                        ));
                    };
                    if kind == Family::Tmat
                        || (kind == Family::Auto
                            && generic.is_none()
                            && text.contains("(0x")
                            && text.contains("0b"))
                    {
                        header8((&text, &t), d, true, &mut merged, &mut report, l, &mut b)?;
                    } else {
                        let geometry = generic.ok_or_else(|| invalid("generic C input needs explicit width, height, first_code, charset, baseline and bit order"))?;
                        f = generic_read(&t, d, &geometry, l, &mut b)?;
                    }
                }
            }
        }
        if family == Family::Font8x8
            && self.read_options.array_name.is_some()
            && selected_arrays != 1
        {
            return Err(invalid("requested font8x8 array is missing or ambiguous"));
        }
        if !merged.is_empty() {
            let o = ArrayOptions {
                width: 8,
                height: 8,
                first_code: 0,
                charset: "unicode".into(),
                baseline: 7,
                lsb_first: family == Family::Font8x8,
            };
            for (_, g) in merged {
                f.add_glyph(g)?;
            }
            cell_metrics(&mut f, &o);
            f.names.copyright = Some(if family == Family::Font8x8 { "Marcel Sondaar; IBM public-domain VGA fonts; C headers by Daniel Hepper. Public Domain." } else { "Copyright (C) 2020, Takayuki Matsuoka. CC0-1.0" }.into());
        }
        if f.is_empty() {
            return Err(invalid("C header has no encoded glyphs"));
        }
        f.validate(l)?;
        let mut s = FontSet::from(f);
        s.source_format = Some("c".into());
        Ok((s, report))
    }
}
fn include(s: &str) -> Result<Option<&str>> {
    let Some(s) = s.trim_start_matches('#').trim().strip_prefix("include") else {
        return Ok(None);
    };
    let s = s.trim();
    if s.starts_with('<') {
        return Ok(None);
    }
    if let Some(rest) = s.strip_prefix('"') {
        let name = rest
            .split_once('"')
            .ok_or_else(|| invalid("unterminated C include"))?
            .0;
        ResourcePath::new(name)?;
        return Ok(Some(name));
    }
    Err(invalid(
        "C include must name a literal local resource or system header",
    ))
}
fn generic_read(
    t: &[Token<'_>],
    d: &Declaration<'_>,
    o: &ArrayOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Font> {
    let size = cell_size(o, l)?;
    let rows = lexer::rows(t, d, b)?;
    let nested = t[d.start + 1].kind == Kind::Punct(b'{');
    if nested && rows.iter().any(|r| r.values.len() != size) {
        return Err(invalid("C row length differs from explicit geometry"));
    }
    b.add(rows.iter().map(|r| r.values.len().saturating_mul(8)).sum())?;
    let values: Vec<_> = rows.into_iter().flat_map(|r| r.values).collect();
    if values.is_empty() || values.len() % size != 0 {
        return Err(invalid("C byte count differs from explicit geometry"));
    }
    let count = values.len() / size;
    check(count, l.max_glyphs, "C glyph count")?;
    let mut f = Font::default();
    for (i, row) in values.chunks_exact(size).enumerate() {
        let cp = o
            .first_code
            .checked_add(u32::try_from(i).map_err(|_| invalid("C code overflow"))?)
            .ok_or_else(|| invalid("C code overflow"))?;
        b.glyph(size, i + 1, l)?;
        let bytes = row
            .iter()
            .map(|&v| {
                u8::try_from(v)
                    .map(|v| if o.lsb_first { v.reverse_bits() } else { v })
                    .map_err(|_| invalid("C bitmap value exceeds byte range"))
            })
            .collect::<Result<Vec<_>>>()?;
        f.add_glyph(cell_glyph(&bytes, cp, o, l)?)?;
    }
    cell_metrics(&mut f, o);
    Ok(f)
}
fn unicode_comment(s: &str) -> Option<u32> {
    let (_, v) = s.split_once("U+")?;
    let n = v.bytes().take_while(u8::is_ascii_hexdigit).count();
    if n == 0 {
        return None;
    }
    u32::from_str_radix(&v[..n], 16).ok()
}
fn header8(
    source: (&str, &[Token<'_>]),
    d: &Declaration<'_>,
    tmat: bool,
    out: &mut BTreeMap<u32, Glyph>,
    r: &mut Report,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<()> {
    let (s, t) = source;
    let rows = lexer::rows(t, d, b)?;
    check(rows.len(), l.max_glyphs, "C header rows")?;
    b.add(rows.len().saturating_mul(256))?;
    if d.dimensions.len() != 2
        || d.dimensions[1] != Some(8)
        || rows.iter().any(|r| r.values.len() != 8)
    {
        return Err(invalid("8x8 header requires complete eight-byte rows"));
    }
    if tmat && rows.len() != 96 {
        return Err(invalid("t-mat table needs 96 rows"));
    }
    let preceding = t[..d.start]
        .iter()
        .rev()
        .find(|token| token.kind == Kind::Punct(b';'))
        .map_or(0, |token| token.end);
    let span = lexer::comments(&s[preceding..t[d.start].start])
        .filter_map(|comment| comment.split_once("unicode points "))
        .last()
        .map(|(_, v)| {
            let (a, z) = v
                .split_once('-')
                .ok_or_else(|| invalid("invalid font8x8 range"))?;
            let first = unicode_comment(a).ok_or_else(|| invalid("invalid font8x8 range start"))?;
            let last = unicode_comment(z).ok_or_else(|| invalid("invalid font8x8 range end"))?;
            if last
                .checked_sub(first)
                .and_then(|n| n.checked_add(1))
                .map(|n| n as usize)
                != Some(rows.len())
            {
                return Err(invalid("font8x8 range does not match array length"));
            }
            Ok(first)
        })
        .transpose()?;
    for (i, row) in rows.iter().enumerate() {
        let cp = if tmat {
            let comment = s[row.start..row.end]
                .split_once("//")
                .ok_or_else(|| invalid("t-mat row lacks code comment"))?
                .1
                .trim_start();
            let n = comment.bytes().take_while(u8::is_ascii_digit).count();
            let decimal = comment[..n]
                .parse::<u32>()
                .map_err(|_| invalid("t-mat decimal code"))?;
            let hex = comment
                .split_once("(0x")
                .and_then(|(_, v)| v.split_once(')'))
                .ok_or_else(|| invalid("t-mat hex code"))?
                .0;
            if u32::from_str_radix(hex, 16).ok() != Some(decimal) || decimal != 32 + i as u32 {
                return Err(invalid("t-mat codes disagree or do not cover U+0020–007F"));
            }
            decimal
        } else if let Some(first) = span {
            first + i as u32
        } else {
            let line = s[row.end..].split('\n').next().unwrap_or("");
            let Some(cp) = line.split_once("//").and_then(|(_, v)| unicode_comment(v)) else {
                loss(
                    r,
                    "read.unmapped_row",
                    Some(i),
                    "8x8 row has no upstream Unicode mapping; skipped",
                );
                continue;
            };
            cp
        };
        if char::from_u32(cp).is_none() {
            return Err(invalid("8x8 mapping is not a Unicode scalar"));
        }
        b.glyph(8, out.len() + 1, l)?;
        let bytes = row
            .values
            .iter()
            .map(|&v| {
                u8::try_from(v)
                    .map(|v| if tmat { v } else { v.reverse_bits() })
                    .map_err(|_| invalid("8x8 value exceeds byte range"))
            })
            .collect::<Result<Vec<_>>>()?;
        let o = ArrayOptions {
            width: 8,
            height: 8,
            first_code: cp,
            charset: "unicode".into(),
            baseline: 7,
            lsb_first: !tmat,
        };
        let mut g = cell_glyph(&bytes, cp, &o, l)?;
        g.name = Some(format!("uni{cp:04X}"));
        if let Some(old) = out.get(&cp) {
            if old != &g {
                return Err(invalid("conflicting duplicate C glyphs"));
            }
        } else {
            out.insert(cp, g);
        }
    }
    Ok(())
}
impl FormatWriter<FontSet> for CCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = writer::write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
fn gfx(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    o: &CReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Font> {
    let choices: Vec<_> = ds
        .iter()
        .filter(|d| d.ty == "GFXfont" && o.array_name.as_deref().is_none_or(|n| n == d.name))
        .collect();
    let [font] = choices.as_slice() else {
        return Err(invalid("GFXfont is missing or ambiguous"));
    };
    let fields: Vec<_> = t[font.start + 1..font.end]
        .split(|t| t.kind == Kind::Punct(b','))
        .filter(|v| !v.is_empty())
        .collect();
    if fields.len() != 5 {
        return Err(invalid("GFXfont needs exactly five fields"));
    }
    fn pointer<'a>(t: &[Token<'a>]) -> Result<&'a str> {
        let name = match t.last().map(|v| v.kind) {
            Some(Kind::Ident(name)) => name,
            _ => return Err(invalid("GFX pointer must name a literal array")),
        };
        let prefix = &t[..t.len() - 1];
        if !prefix.is_empty()
            && !(prefix.len() == 4
                && prefix[0].kind == Kind::Punct(b'(')
                && matches!(prefix[1].kind, Kind::Ident("uint8_t" | "GFXglyph"))
                && prefix[2].kind == Kind::Punct(b'*')
                && prefix[3].kind == Kind::Punct(b')'))
        {
            return Err(invalid("unsupported GFX pointer expression"));
        }
        Ok(name)
    }
    fn number(t: &[Token<'_>], max: i64) -> Result<u32> {
        let mut i = 0;
        let n = lexer::integer(t, &mut i)?;
        if i != t.len() || n < 0 || n > max {
            return Err(invalid("GFXfont scalar out of range"));
        }
        Ok(n as u32)
    }
    let bits_name = pointer(fields[0])?;
    let glyph_name = pointer(fields[1])?;
    let first = number(fields[2], 65535)?;
    let last = number(fields[3], 65535)?;
    let advance = number(fields[4], 255)?;
    let count = last
        .checked_sub(first)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("reversed GFX code range"))? as usize;
    check(count, l.max_glyphs, "GFX glyph count")?;
    let get = |name: &str, ty: &str| -> Result<&Declaration<'_>> {
        let mut iter = ds
            .iter()
            .filter(|d| d.name == name && (d.ty == ty || ty == "uint8_t" && d.ty == "char"));
        let d = iter
            .next()
            .ok_or_else(|| invalid("GFX referenced array is missing"))?;
        if iter.next().is_some() {
            return Err(invalid("duplicate GFX array declarations"));
        }
        Ok(d)
    };
    let bits = lexer::rows(t, get(bits_name, "uint8_t")?, b)?;
    if bits.len() != 1 {
        return Err(invalid("GFX bitmap must be flat"));
    }
    b.add(bits[0].values.len())?;
    let bits = bits[0]
        .values
        .iter()
        .map(|&n| u8::try_from(n).map_err(|_| invalid("GFX bitmap exceeds byte range")))
        .collect::<Result<Vec<_>>>()?;
    let gs = lexer::rows(t, get(glyph_name, "GFXglyph")?, b)?;
    if gs.len() != count {
        return Err(invalid("GFX glyph count differs from code range"));
    }
    let mut f = Font::default();
    for (i, r) in gs.iter().enumerate() {
        let [offset, w, h, advance, x, y] = r.values.as_slice() else {
            return Err(invalid("GFXglyph needs exactly six fields"));
        };
        if !(0..=65535).contains(offset)
            || [w, h, advance].iter().any(|n| !(0..=255).contains(*n))
            || [x, y].iter().any(|n| !(-128..=127).contains(*n))
        {
            return Err(invalid("GFXglyph scalar out of range"));
        }
        let (w, h) = (*w as u32, *h as u32);
        let pixels = w as usize * h as usize;
        check(pixels, l.max_pixels_per_glyph, "GFX glyph pixels")?;
        let stride = (w as usize).div_ceil(8);
        let size = stride * h as usize;
        b.glyph(size, i + 1, l)?;
        let src = bits
            .get(*offset as usize..(*offset as usize).saturating_add(pixels.div_ceil(8)))
            .ok_or_else(|| invalid("GFX bitmap offset extends beyond array"))?;
        let mut bytes = vec![0u8; size];
        for py in 0..h as usize {
            for px in 0..w as usize {
                let at = py * w as usize + px;
                if src[at / 8] & (128 >> (at % 8)) != 0 {
                    bytes[py * stride + px / 8] |= 128 >> (px % 8);
                }
            }
        }
        let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
            w,
            h,
            &bytes,
            bitmap_limits(l),
        )?);
        g.codepoints.push(Codepoint::Unicode(first + i as u32));
        g.metrics.bounds.x = *x;
        g.metrics.bounds.y = -(*y + i64::from(h));
        g.metrics.advance = Some(Vector::new(*advance, 0));
        f.add_glyph(g)?;
    }
    f.update_bounds()?;
    let bounds = f.metrics.bounds.unwrap_or_default();
    f.metrics.ascent = Some((bounds.y + i64::from(bounds.height)).max(0));
    f.metrics.descent = Some((-bounds.y).max(0));
    f.metrics.line_height = Some(i64::from(advance));
    f.charset.name = Some("unicode".into());
    f.source_format = Some("c".into());
    Ok(f)
}
