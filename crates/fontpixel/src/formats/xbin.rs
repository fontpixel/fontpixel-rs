//! XBIN fonts, validated screen compression and retained non-font sections.
/// Native read interpretation.
pub use super::w5a2_common::NativeReadOptions as XbinReadOptions;
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
/// Number of native font slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CharacterCount {
    /// Select from the largest supported source position.
    #[default]
    Auto,
    /// Exactly 256 glyphs.
    Chars256,
    /// Exactly 512 glyphs.
    Chars512,
}
/// Treatment of native non-font sections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreenMode {
    /// Retain validated native screen/palette properties.
    #[default]
    Preserve,
    /// Emit only the font and report omitted screen metadata.
    FontOnly,
}
/// XBIN output projection.
#[derive(Debug, Clone, Default)]
pub struct XbinWriteOptions {
    /// Output cell rectangle; width must be eight.
    pub cell: Option<Bounds>,
    /// Explicit gray threshold.
    pub gray_threshold: Option<u8>,
    /// Permit numeric Unicode-to-legacy projection.
    pub unicode_as_legacy: bool,
    /// Native font slot count.
    pub count: CharacterCount,
    /// Non-font section projection.
    pub screen: ScreenMode,
    /// Aggregate storage bounds.
    pub limits: ResourceLimits,
}
impl XbinWriteOptions {
    fn native(&self) -> NativeWriteOptions {
        NativeWriteOptions {
            cell: self.cell,
            gray_threshold: self.gray_threshold,
            unicode_as_legacy: self.unicode_as_legacy,
            limits: self.limits.clone(),
        }
    }
}
/// Native XBIN codec.
#[derive(Debug, Clone, Default)]
pub struct XbinCodec {
    /// Reader interpretation.
    pub read_options: XbinReadOptions,
    /// Writer projection.
    pub write_options: XbinWriteOptions,
}
impl FormatDetector for XbinCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for XbinCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for XbinCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect the exact XBIN signature.
pub fn detect(s: &[u8]) -> Confidence {
    if s.starts_with(b"XBIN\x1a") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn screen(s: &[u8], w: usize, h: usize, compressed: bool, l: &ResourceLimits) -> Result<()> {
    if (w == 0) != (h == 0) {
        return Err(invalid("XBIN screen has one zero dimension"));
    }
    let cells = w
        .checked_mul(h)
        .ok_or_else(|| invalid("XBIN screen size overflow"))?;
    check(
        cells.saturating_mul(2),
        l.max_table_bytes,
        "XBIN decoded screen bytes",
    )?;
    if !compressed {
        if s.len() != cells.saturating_mul(2) {
            return Err(invalid("XBIN screen length mismatch"));
        }
        return Ok(());
    }
    let (mut at, mut decoded) = (0usize, 0usize);
    while decoded < cells {
        let control = *s
            .get(at)
            .ok_or_else(|| invalid("truncated XBIN RLE control"))?;
        at += 1;
        let n = usize::from(control & 63) + 1;
        if n > cells - decoded {
            return Err(invalid("XBIN RLE exceeds screen dimensions"));
        }
        let bytes = match control >> 6 {
            0 => n * 2,
            1 | 2 => n + 1,
            _ => 2,
        };
        take(s, at, bytes)?;
        at += bytes;
        decoded += n;
    }
    if at != s.len() {
        return Err(invalid("trailing XBIN RLE bytes"));
    }
    Ok(())
}
fn property<'a>(f: &'a Font, name: &str) -> Result<Option<&'a [u8]>> {
    match f.properties.get(name) {
        None => Ok(None),
        Some(PropertyValue::Bytes(v)) => Ok(Some(v)),
        _ => Err(invalid("XBIN native property must be bytes")),
    }
}
fn save_property(f: &mut Font, name: &str, v: &[u8], b: &mut Budget) -> Result<()> {
    b.add(256 + v.len().saturating_mul(2))?;
    f.properties
        .push(Property::new(name, PropertyValue::Bytes(v.to_vec())));
    Ok(())
}
fn parse(
    s: &[u8],
    o: &XbinReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "XBIN input bytes")?;
    let hdr = take(s, 0, 11)?;
    if !hdr.starts_with(b"XBIN\x1a")
        || hdr[10] & 0xe0 != 0
        || hdr[10] & 2 == 0
        || !(1..=32).contains(&hdr[9])
    {
        return Err(invalid("invalid XBIN header or absent font"));
    }
    let mut at = 11;
    let palette = if hdr[10] & 1 != 0 {
        let p = take(s, at, 48)?;
        if p.iter().any(|v| *v > 63) {
            return Err(invalid("XBIN palette component exceeds 63"));
        }
        at += 48;
        Some(p)
    } else {
        None
    };
    let count = if hdr[10] & 16 != 0 { 512 } else { 256 };
    check(count, l.max_glyphs, "XBIN glyph count")?;
    let rows = usize::from(hdr[9]);
    let pixels = take(s, at, count * rows)?;
    at += pixels.len();
    screen(
        &s[at..],
        usize::from(u16le(hdr, 5)?),
        usize::from(u16le(hdr, 7)?),
        hdr[10] & 4 != 0,
        l,
    )?;
    let mut f = base("xbin", o, b)?;
    for (i, raw) in pixels.chunks_exact(rows).enumerate() {
        glyph(
            &mut f,
            Codepoint::Legacy(i as u32),
            8,
            rows as u32,
            raw,
            o,
            l,
            b,
        )?;
    }
    save_property(&mut f, "xbin.header", hdr, b)?;
    if let Some(p) = palette {
        save_property(&mut f, "xbin.palette", p, b)?;
    }
    save_property(&mut f, "xbin.screen", &s[at..], b)?;
    Ok((complete(f, "xbin")?, Report::default()))
}
/// Decode the native font and validate all optional screen data.
pub fn read(c: &mut ReadContext<'_>, o: &XbinReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
/// Encode native font data, preserving validated screen sections by default.
pub fn write(s: &FontSet, o: &WriteOptions, p: &XbinWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = (|| {
        let n = p.native();
        let mut b = source(s, &n)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let src = convert_gray(s, n.gray_threshold, &p.limits, &mut b)?;
        let f = src.single()?;
        let mut c = cell(f, &n, None)?;
        if p.cell.is_some_and(|v| v.width != 8) || c.height > 32 {
            return Err(invalid("XBIN requires width eight and height 1..32"));
        }
        c.width = 8;
        let map = legacy_map(f, &n, 511, &mut b)?;
        let count = match p.count {
            CharacterCount::Chars256 => 256,
            CharacterCount::Chars512 => 512,
            CharacterCount::Auto => {
                if map.keys().any(|v| *v >= 256) {
                    512
                } else {
                    256
                }
            }
        };
        let mut header = [0u8; 11];
        header[..5].copy_from_slice(b"XBIN\x1a");
        let (mut palette, mut image) = (&[][..], &[][..]);
        if p.screen == ScreenMode::Preserve {
            if let Some(h) = property(f, "xbin.header")? {
                if h.len() != 11 || !h.starts_with(b"XBIN\x1a") || h[10] & 0xe0 != 0 {
                    return Err(invalid("invalid retained XBIN header"));
                }
                header.copy_from_slice(h);
                palette = property(f, "xbin.palette")?.unwrap_or(&[]);
                image = property(f, "xbin.screen")?.unwrap_or(&[]);
                if (h[10] & 1 != 0 && palette.len() != 48)
                    || (h[10] & 1 == 0 && !palette.is_empty())
                    || palette.iter().any(|v| *v > 63)
                {
                    return Err(invalid("retained XBIN palette disagrees with header"));
                }
                screen(
                    image,
                    usize::from(u16le(h, 5)?),
                    usize::from(u16le(h, 7)?),
                    h[10] & 4 != 0,
                    &p.limits,
                )?;
            } else if property(f, "xbin.palette")?.is_some()
                || property(f, "xbin.screen")?.is_some()
            {
                return Err(invalid("XBIN ancillary data requires its native header"));
            }
        }
        header[9] = c.height as u8;
        header[10] = (header[10] & 13) | 2 | if count == 512 { 16 } else { 0 };
        let total = 11 + palette.len() + count * c.height as usize + image.len();
        let mut out = allocated(total, &mut b)?;
        out[..11].copy_from_slice(&header);
        out[11..11 + palette.len()].copy_from_slice(palette);
        let begin = 11 + palette.len();
        let end = begin + count * c.height as usize;
        for (i, raw) in out[begin..end]
            .chunks_exact_mut(c.height as usize)
            .enumerate()
        {
            raw.copy_from_slice(
                raster(map.get(&(i as u32)).copied(), c, &p.limits, &mut b)?.as_bytes(),
            );
        }
        out[end..].copy_from_slice(image);
        output_check(out.len(), &p.limits, &mut b)?;
        let (back, rr) = parse(&out, &Default::default(), &p.limits, &mut b)?;
        r.extend(rr);
        audit(s, &back, "xbin", &mut r, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, r, "xbin", o.strict)
}
