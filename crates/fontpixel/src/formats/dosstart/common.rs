//! Bounded implementation support shared by eight small native font formats.
use super::legacy_common::{
    Budget, audit, bitmap_limits, check, convert_gray, finish, invalid, mono, source_budget,
};
use super::{Confidence, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;

/// Explicit conversion and allocation choices for the small native formats.
#[derive(Debug, Clone)]
pub struct NativeWriteOptions {
    /// Explicit grayscale-to-monochrome threshold.
    pub gray_threshold: Option<u8>,
    /// Permit Unicode numbers to serve as legacy code positions.
    pub unicode_as_legacy: bool,
    /// Override the derived cell width and height for cell-based output.
    pub cell_size: Option<(u32, u32)>,
    /// Override the first position for Dr. Halo (zero or 32).
    pub first_codepoint: Option<u32>,
    /// Resource bounds, also used to decode the completed output.
    pub limits: ResourceLimits,
    /// Additional maximum output size.
    pub max_output_bytes: usize,
}
impl Default for NativeWriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            unicode_as_legacy: false,
            cell_size: None,
            first_codepoint: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
fn mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("native size overflow"))
}
fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| invalid("native size overflow"))
}
fn u16le(b: &[u8], p: usize) -> Result<u16> {
    let b = b
        .get(p..add(p, 2)?)
        .ok_or_else(|| invalid("truncated native header"))?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn u32le(b: &[u8], p: usize) -> Result<u32> {
    let b = b
        .get(p..add(p, 4)?)
        .ok_or_else(|| invalid("truncated native header"))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn put16(b: &mut [u8], p: usize, v: u32) -> Result<()> {
    b[p..p + 2].copy_from_slice(
        &u16::try_from(v)
            .map_err(|_| invalid("native field exceeds 16 bits"))?
            .to_le_bytes(),
    );
    Ok(())
}
fn size(w: u32, h: u32, l: &ResourceLimits) -> Result<usize> {
    check(
        usize::try_from(u64::from(w) * u64::from(h)).map_err(|_| invalid("pixel size overflow"))?,
        l.max_pixels_per_glyph,
        "native glyph pixels",
    )?;
    let n = mul((w as usize).div_ceil(8), h as usize)?;
    check(n, l.max_table_bytes, "native raster bytes")?;
    Ok(n)
}
fn alloc(n: usize, budget: &mut Budget) -> Result<Vec<u8>> {
    budget.add(n)?;
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| invalid("native allocation failed"))?;
    v.resize(n, 0);
    Ok(v)
}
fn ascii(b: &[u8]) -> Result<&str> {
    if !b.is_ascii() || b.contains(&0) {
        return Err(invalid("native text must be NUL-free ASCII"));
    }
    std::str::from_utf8(b).map_err(|_| invalid("invalid native text"))
}
fn utf8_text(b: &[u8]) -> Result<&str> {
    if b.contains(&0) {
        return Err(invalid("native text contains NUL"));
    }
    std::str::from_utf8(b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b))
        .map_err(|_| invalid("invalid native UTF-8 text"))
}
fn number<T: std::str::FromStr>(s: &str) -> Result<T> {
    s.trim()
        .parse()
        .map_err(|_| invalid("invalid native integer"))
}
fn raw_property<'a>(f: &'a Font, id: &str) -> Option<&'a [u8]> {
    match f.properties.get(&format!("{id}.header")) {
        Some(PropertyValue::Bytes(v)) => Some(v),
        _ => None,
    }
}
fn base_font(id: &str, budget: &mut Budget) -> Result<Font> {
    budget.items::<Font>(1)?;
    budget.add(512)?;
    let mut f = Font::default();
    f.source_format = Some(id.into());
    Ok(f)
}
fn reserve_glyph(f: &Font, l: &ResourceLimits, budget: &mut Budget, mappings: usize) -> Result<()> {
    check(f.len().saturating_add(1), l.max_glyphs, "native glyphs")?;
    budget.items::<Option<Glyph>>(1)?;
    budget.add(add(256, mul(mappings, 160)?)?)
}
fn push_glyph(
    f: &mut Font,
    cp: Option<u32>,
    bitmap: Bitmap<Mono>,
    y: i64,
    advance: i64,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<()> {
    reserve_glyph(f, l, budget, usize::from(cp.is_some()))?;
    let mut g = Glyph::new(bitmap);
    if let Some(cp) = cp {
        g.codepoints.push(Codepoint::Legacy(cp));
    }
    g.metrics.bounds.y = y;
    g.metrics.advance = Some(Vector::new(advance, 0));
    f.add_glyph(g)?;
    Ok(())
}
fn font_metrics(f: &mut Font, w: u32, h: u32, ascent: i64, fixed: bool) -> Result<()> {
    f.metrics.pixel_size = Some(h);
    f.metrics.ascent = Some(ascent);
    f.metrics.descent = Some(
        i64::from(h)
            .checked_sub(ascent)
            .ok_or_else(|| invalid("native font descent overflow"))?,
    );
    f.metrics.bounds = Some(Bounds::new(
        w,
        h,
        0,
        ascent
            .checked_sub(i64::from(h))
            .ok_or_else(|| invalid("native font baseline overflow"))?,
    ));
    f.metrics.spacing = Some(if fixed {
        Spacing::CharacterCell
    } else {
        Spacing::Proportional
    });
    Ok(())
}
fn complete(f: Font, id: &str, r: Report) -> (FontSet, Report) {
    let mut s = FontSet::from(f);
    s.source_format = Some(id.into());
    (s, r)
}

#[derive(Clone, Copy)]
struct Layout {
    header: usize,
    width: u32,
    height: u32,
    first: u32,
    count: usize,
}
fn binary_layout(id: &str, b: &[u8], first_override: Option<u32>) -> Result<Layout> {
    let out = match id {
        "pcr" => {
            if b.len() < 11
                || !b.starts_with(b"KPG\x01")
                || ![1, 2].contains(&b[4])
                || b[5..7] != [0x20, 1]
            {
                return Err(invalid("invalid PCR header"));
            }
            let h = u32::from(b[7]);
            let n = 256 * h as usize;
            if h == 0 || usize::from(u16le(b, 9)?) != n || b.len() != 11 + n {
                return Err(invalid("PCR payload size mismatch"));
            }
            Layout {
                header: 11,
                width: 8,
                height: h,
                first: 0,
                count: 256,
            }
        }
        "plus3dos" => {
            if b.len() != 896
                || !b.starts_with(b"PLUS3DOS\x1a")
                || b[15] != 3
                || u32le(b, 11)? != 896
                || u16le(b, 16)? != 768
                || b[..127].iter().fold(0u8, |a, &v| a.wrapping_add(v)) != b[127]
            {
                return Err(invalid("invalid PLUS3DOS font header, size or checksum"));
            }
            Layout {
                header: 128,
                width: 8,
                height: 8,
                first: 32,
                count: 96,
            }
        }
        "drhalo" => {
            if b.len() < 20 || !b.starts_with(b"AH") {
                return Err(invalid("invalid Dr. Halo header"));
            }
            let w = u16le(b, 16)?;
            let h = u16le(b, 18)?;
            if w == 0 || h == 0 {
                return Err(invalid("Dr. Halo stroke fonts are not bitmap fonts"));
            }
            if w > i16::MAX as u16 || h > i16::MAX as u16 {
                return Err(invalid("negative Dr. Halo dimensions"));
            }
            let stride = usize::from(w).div_ceil(8) * usize::from(h);
            if !(b.len() - 20).is_multiple_of(stride)
                || b.len() == 20
                || usize::from(u16le(b, 12)?) != stride
            {
                return Err(invalid("Dr. Halo raster size mismatch"));
            }
            let first = first_override.unwrap_or(if b[2] == 0xc8 { 0 } else { 32 });
            Layout {
                header: 20,
                width: u32::from(w),
                height: u32::from(h),
                first,
                count: (b.len() - 20) / stride,
            }
        }
        "writeon" => {
            if b.len() < 22
                || !b.starts_with(b"ES\x01\0")
                || u16le(b, 4)? != 22
                || u16le(b, 10)? != 14
            {
                return Err(invalid("invalid Write On! header"));
            }
            let w = u32::from(u16le(b, 6)?);
            let h = u32::from(u16le(b, 8)?);
            let first = u32::from(u16le(b, 12)?);
            let last = u32::from(u16le(b, 14)?);
            if w == 0 || h == 0 || !w.is_multiple_of(8) || !h.is_multiple_of(14) || last < first {
                return Err(invalid("invalid Write On! cell or code interval"));
            }
            Layout {
                header: 22,
                width: w,
                height: h,
                first,
                count: (last - first + 1) as usize,
            }
        }
        _ => return Err(invalid("unknown binary native format")),
    };
    let n = mul(
        mul((out.width as usize).div_ceil(8), out.height as usize)?,
        out.count,
    )?;
    if add(out.header, n)? != b.len()
        || u32::try_from(out.count - 1)
            .ok()
            .and_then(|n| out.first.checked_add(n))
            .is_none()
    {
        return Err(invalid("native terminal raster boundary mismatch"));
    }
    Ok(out)
}
/// Allocation-free recognition. Text without a magic requires explicit selection.
pub fn detect(id: &str, b: &[u8]) -> Confidence {
    let b = if id == "psf2txt" {
        b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b)
    } else {
        b
    };
    match id {
        "edwin" | "mkwinfont" => Confidence::NoMatch,
        "dosstart" if b.starts_with(b"DosStartFont\n") || b.starts_with(b"DosStartFont\r\n") => {
            Confidence::Certain
        }
        "psf2txt" if b.starts_with(b"%PSF2\n") || b.starts_with(b"%PSF2\r\n") => {
            Confidence::Certain
        }
        "pcr" | "plus3dos" | "writeon" if binary_layout(id, b, None).is_ok() => Confidence::Certain,
        "drhalo" if binary_layout(id, b, None).is_ok() => Confidence::Possible,
        _ => Confidence::NoMatch,
    }
}
pub fn read(c: &mut ReadContext<'_>, id: &str, first: Option<u32>) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), id, first, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    id: &str,
    first: Option<u32>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_input_bytes, "native input bytes")?;
    check(b.len(), l.max_table_bytes, "native table bytes")?;
    match id {
        "dosstart" => read_dosstart(b, l, budget),
        "edwin" => read_edwin(b, l, budget),
        "mkwinfont" => read_fd(b, l, budget),
        "psf2txt" => read_psf2txt(b, l, budget),
        _ => read_binary(b, id, first, l, budget),
    }
}
fn read_binary(
    b: &[u8],
    id: &str,
    first: Option<u32>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let v = binary_layout(id, b, first)?;
    check(v.count, l.max_glyphs, "native glyphs")?;
    let stride = size(v.width, v.height, l)?;
    let mut f = base_font(id, budget)?;
    budget.add(v.header * 2 + 128)?;
    f.properties.push(Property::new(
        format!("{id}.header"),
        PropertyValue::Bytes(b[..v.header].to_vec()),
    ));
    let mut report = Report::default();
    for i in 0..v.count {
        let p = v.header + i * stride;
        budget.add(stride)?;
        let bitmap =
            Bitmap::<Mono>::from_raw(v.width, v.height, &b[p..p + stride], bitmap_limits(l))?;
        if bitmap.as_bytes() != &b[p..p + stride] {
            budget.loss(
                &mut report,
                id,
                "read.padding",
                Some(0),
                Some(i),
                "Nonzero right padding bits were cleared",
            )?;
        }
        push_glyph(
            &mut f,
            Some(v.first + i as u32),
            bitmap,
            0,
            i64::from(v.width),
            l,
            budget,
        )?;
    }
    font_metrics(&mut f, v.width, v.height, i64::from(v.height), true)?;
    Ok(complete(f, id, report))
}

struct Output {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Output {
    fn new(o: &NativeWriteOptions) -> Self {
        Self {
            bytes: Vec::new(),
            maximum: o
                .max_output_bytes
                .min(o.limits.max_input_bytes)
                .min(o.limits.max_table_bytes),
        }
    }
    fn bytes(&mut self, b: &[u8], budget: &mut Budget) -> Result<()> {
        let n = add(self.bytes.len(), b.len())?;
        check(n, self.maximum, "native output bytes")?;
        // Reserving exponentially avoids quadratic small-text append behavior.
        if n > self.bytes.capacity() {
            let cap = n
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.maximum);
            budget.add(cap - self.bytes.capacity())?;
            self.bytes
                .try_reserve_exact(cap - self.bytes.len())
                .map_err(|_| invalid("native output allocation failed"))?;
        }
        self.bytes.extend_from_slice(b);
        Ok(())
    }
    fn text(&mut self, s: &str, budget: &mut Budget) -> Result<()> {
        self.bytes(s.as_bytes(), budget)
    }
}
fn positions<'a>(
    f: &'a Font,
    o: &NativeWriteOptions,
    budget: &mut Budget,
) -> Result<BTreeMap<u32, &'a Glyph>> {
    let mut map = BTreeMap::new();
    for (_, g) in f.glyphs() {
        for cp in &g.codepoints {
            let p = match *cp {
                Codepoint::Legacy(v) => Some(v),
                Codepoint::Unicode(v) if o.unicode_as_legacy => Some(v),
                _ => None,
            };
            if let Some(p) = p
                && let std::collections::btree_map::Entry::Vacant(e) = map.entry(p)
            {
                budget.add(128)?;
                e.insert(g);
            }
        }
    }
    if map.is_empty() {
        return Err(invalid(
            "native output requires legacy labels (or explicit unicode_as_legacy)",
        ));
    }
    Ok(map)
}
fn cell(f: &Font, o: &NativeWriteOptions) -> Result<Bounds> {
    let mut b = f.metrics.bounds.unwrap_or_default();
    if f.metrics.bounds.is_none() {
        for (_, g) in f.glyphs() {
            b.width = b.width.max(g.metrics.bounds.width);
            b.height = b.height.max(g.metrics.bounds.height);
        }
    }
    if let Some((w, h)) = o.cell_size {
        b.width = w;
        b.height = h;
    }
    if b.width == 0 || b.height == 0 {
        return Err(invalid("native output needs a positive cell"));
    }
    size(b.width, b.height, &o.limits)?;
    Ok(b)
}
fn raster(
    g: Option<&Glyph>,
    b: Bounds,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Bitmap<Mono>> {
    let n = size(b.width, b.height, l)?;
    budget.add(n)?;
    match g {
        Some(g) => mono(g)?.crop_bounds(Bounds::new(
            b.width,
            b.height,
            b.x.checked_sub(g.metrics.bounds.x)
                .ok_or_else(|| invalid("cell x overflow"))?,
            b.y.checked_sub(g.metrics.bounds.y)
                .ok_or_else(|| invalid("cell y overflow"))?,
        )),
        None => Bitmap::<Mono>::with_limits(b.width, b.height, bitmap_limits(l)),
    }
}
pub fn write(
    set: &FontSet,
    o: &WriteOptions,
    opt: &NativeWriteOptions,
    id: &str,
) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(set, &opt.limits)?;
        let source = convert_gray(set, opt.gray_threshold, &opt.limits, &mut budget)?;
        let f = source.single()?;
        let mut out = Output::new(opt);
        match id {
            "dosstart" => write_dosstart(f, opt, &mut out, &mut budget)?,
            "edwin" => write_edwin(f, opt, &mut out, &mut budget)?,
            "mkwinfont" => write_fd(f, opt, &mut out, &mut budget)?,
            "psf2txt" => write_psf2txt(f, opt, &mut out, &mut budget)?,
            _ => write_binary(f, opt, id, &mut out, &mut budget)?,
        }
        let (decoded, read_report) = parse(&out.bytes, id, None, &opt.limits, &mut budget)?;
        report.extend(read_report);
        audit(set, &decoded, id, &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out.bytes))
    })();
    finish(result, report, id, o.strict)
}
fn write_binary(
    f: &Font,
    o: &NativeWriteOptions,
    id: &str,
    out: &mut Output,
    budget: &mut Budget,
) -> Result<()> {
    let map = positions(f, o, budget)?;
    let mut b = cell(f, o)?;
    let (header, first, count) = match id {
        "pcr" => {
            b.width = 8;
            (11, 0, 256)
        }
        "plus3dos" => {
            b.width = 8;
            b.height = 8;
            (128, 32, 96)
        }
        "drhalo" => {
            let first = o.first_codepoint.unwrap_or(
                if *map
                    .first_key_value()
                    .ok_or_else(|| invalid("empty font"))?
                    .0
                    < 32
                {
                    0
                } else {
                    32
                },
            );
            if ![0, 32].contains(&first) || b.width > i16::MAX as u32 || b.height > i16::MAX as u32
            {
                return Err(invalid(
                    "Dr. Halo first position must be 0/32 and dimensions fit signed 16 bits",
                ));
            }
            let last = *map.last_key_value().ok_or_else(|| invalid("empty font"))?.0;
            if last < first {
                return Err(invalid("empty Dr. Halo interval"));
            }
            (
                20,
                first,
                usize::try_from(last - first).map_err(|_| invalid("code interval overflow"))? + 1,
            )
        }
        "writeon" => {
            if !b.width.is_multiple_of(8) || !b.height.is_multiple_of(14) {
                return Err(invalid(
                    "Write On! requires width multiple of 8 and height multiple of 14; set cell_size explicitly",
                ));
            }
            let first = *map
                .first_key_value()
                .ok_or_else(|| invalid("empty font"))?
                .0;
            let last = *map.last_key_value().ok_or_else(|| invalid("empty font"))?.0;
            if last > 127 {
                return Err(invalid("Write On! output positions must fit 0..127"));
            }
            (22, first, (last - first + 1) as usize)
        }
        _ => return Err(invalid("unknown binary writer")),
    };
    check(count, o.limits.max_glyphs, "native output glyphs")?;
    let n = size(b.width, b.height, &o.limits)?;
    check(
        add(header, mul(count, n)?)?,
        out.maximum,
        "native output bytes",
    )?;
    let mut h = alloc(header, budget)?;
    if let Some(src) = raw_property(f, id)
        && src.len() == header
    {
        h.copy_from_slice(src);
    }
    match id {
        "pcr" => {
            if ![1, 2].contains(&h[4]) {
                h[4] = 2;
            }
            h[..4].copy_from_slice(b"KPG\x01");
            h[5..7].copy_from_slice(&[0x20, 1]);
            h[7] = u8::try_from(b.height).map_err(|_| invalid("PCR height exceeds 255"))?;
            put16(
                &mut h,
                9,
                u32::try_from(count * n).map_err(|_| invalid("PCR payload overflow"))?,
            )?;
        }
        "plus3dos" => {
            if raw_property(f, id).is_none() {
                h[9] = 1;
                h[18..20].copy_from_slice(&0x3d00u16.to_le_bytes());
                h[22] = b'f';
            }
            h[..9].copy_from_slice(b"PLUS3DOS\x1a");
            h[11..15].copy_from_slice(&896u32.to_le_bytes());
            h[15] = 3;
            put16(&mut h, 16, 768)?;
            h[127] = h[..127].iter().fold(0u8, |a, &v| a.wrapping_add(v));
        }
        "drhalo" => {
            if raw_property(f, id).is_none() {
                h[4] = 0x0a;
                put16(&mut h, 6, if first == 0 { 0x100 } else { 0x300 })?;
                put16(&mut h, 10, 127)?;
                put16(&mut h, 14, if first == 0 { 1 } else { 2 })?;
            }
            h[..2].copy_from_slice(b"AH");
            h[2] = if first == 0 { 0xc8 } else { 0xdf };
            put16(
                &mut h,
                12,
                u32::try_from(n).map_err(|_| invalid("Dr. Halo glyph too large"))?,
            )?;
            put16(&mut h, 16, b.width)?;
            put16(&mut h, 18, b.height)?;
        }
        "writeon" => {
            h[..4].copy_from_slice(b"ES\x01\0");
            put16(&mut h, 4, 22)?;
            put16(&mut h, 6, b.width)?;
            put16(&mut h, 8, b.height)?;
            put16(&mut h, 10, 14)?;
            put16(&mut h, 12, first)?;
            put16(&mut h, 14, first + count as u32 - 1)?;
            if raw_property(f, id).is_none() {
                put16(&mut h, 18, b.width)?;
            }
        }
        _ => unreachable!(),
    }
    out.bytes(&h, budget)?;
    for i in 0..count {
        let r = raster(map.get(&(first + i as u32)).copied(), b, &o.limits, budget)?;
        out.bytes(r.as_bytes(), budget)?;
    }
    Ok(())
}

fn read_dosstart(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let mut lines = ascii(b)?.lines();
    if lines.next() != Some("DosStartFont") {
        return Err(invalid("missing DosStartFont signature"));
    }
    let name = lines
        .next()
        .ok_or_else(|| invalid("missing DosStart name"))?;
    let (version, ascent) = lines
        .next()
        .and_then(|v| v.split_once(','))
        .ok_or_else(|| invalid("missing DosStart format/ascent"))?;
    if number::<u8>(version)? != 1 {
        return Err(invalid("DosStart stroke format is not a bitmap font"));
    }
    let ascent = number::<i64>(ascent)?;
    let mut f = base_font("dosstart", budget)?;
    budget.add(name.len() * 2)?;
    f.names.family = Some(name.into());
    let mut width = 0;
    let mut height = 0;
    let mut report = Report::default();
    for line in lines.filter(|v| !v.trim().is_empty()) {
        check(f.len().saturating_add(1), l.max_glyphs, "DosStart glyphs")?;
        let (vector, advance) = line
            .split_once(',')
            .ok_or_else(|| invalid("DosStart glyph lacks advance"))?;
        if vector.len() < 6 {
            return Err(invalid("short DosStart dimensions"));
        }
        let w = number::<u32>(&vector[..3])?;
        let h = number::<u32>(&vector[3..6])?;
        let pixels = vector[6..].trim().as_bytes();
        let n = size(w, h, l)?;
        if (w == 0 && !pixels.is_empty())
            || (w != 0
                && (!pixels.len().is_multiple_of(w as usize)
                    || pixels.len() / w as usize > h as usize))
        {
            return Err(invalid("DosStart raster dimensions disagree"));
        }
        let rows = if w == 0 { 0 } else { pixels.len() / w as usize };
        let offset = h as usize - rows;
        let mut data = alloc(n, budget)?;
        let stride = (w as usize).div_ceil(8);
        for (i, &v) in pixels.iter().enumerate() {
            match v {
                b'H' | b'h' => {
                    data[(i / w as usize + offset) * stride + i % w as usize / 8] |=
                        0x80 >> (i % w as usize % 8)
                }
                b'G' | b'g' => {}
                _ => return Err(invalid("invalid DosStart pixel")),
            }
        }
        if rows != h as usize {
            budget.loss(
                &mut report,
                "dosstart",
                "read.short_raster",
                Some(0),
                Some(f.len()),
                "Historical short raster padded with blank top rows",
            )?;
        }
        budget.add(n)?;
        let bitmap = Bitmap::<Mono>::from_raw(w, h, &data, bitmap_limits(l))?;
        let cp = 32u32
            .checked_add(u32::try_from(f.len()).map_err(|_| invalid("DosStart code overflow"))?)
            .ok_or_else(|| invalid("DosStart code overflow"))?;
        push_glyph(
            &mut f,
            Some(cp),
            bitmap,
            ascent
                .checked_sub(i64::from(h))
                .ok_or_else(|| invalid("DosStart ascent overflow"))?,
            number(advance)?,
            l,
            budget,
        )?;
        width = width.max(w);
        height = height.max(h);
    }
    if f.is_empty() {
        return Err(invalid("DosStart has no glyphs"));
    }
    font_metrics(&mut f, width, height, ascent, false)?;
    Ok(complete(f, "dosstart", report))
}
fn safe_text(s: &str) -> Result<&str> {
    if s.bytes().any(|v| matches!(v, 0 | b'\n' | b'\r')) {
        Err(invalid("native scalar text must be one NUL-free line"))
    } else {
        Ok(s)
    }
}
fn write_dosstart(
    f: &Font,
    o: &NativeWriteOptions,
    out: &mut Output,
    budget: &mut Budget,
) -> Result<()> {
    let map = positions(f, o, budget)?;
    check(96, o.limits.max_glyphs, "DosStart output glyphs")?;
    let b = cell(f, o)?;
    let ascent = f.metrics.ascent.unwrap_or(
        b.y.checked_add(i64::from(b.height))
            .ok_or_else(|| invalid("native cell top overflow"))?,
    );
    let name = f
        .names
        .family
        .as_deref()
        .or(f.names.full.as_deref())
        .unwrap_or("");
    if !name.is_ascii() {
        return Err(invalid("DosStart font names must be ASCII"));
    }
    out.text("DosStartFont\r\n", budget)?;
    out.text(safe_text(name)?, budget)?;
    out.text(&format!("\r\n1, {ascent}\r\n"), budget)?;
    for cp in 32..128 {
        let g = map.get(&cp).copied();
        let (w, h, advance) = if let Some(g) = g {
            (
                u32::try_from(i64::from(g.metrics.bounds.width) + g.metrics.bounds.x.max(0))
                    .map_err(|_| invalid("DosStart width overflow"))?,
                u32::try_from(
                    ascent
                        .checked_sub(g.metrics.bounds.y)
                        .ok_or_else(|| invalid("DosStart height overflow"))?,
                )
                .map_err(|_| invalid("DosStart glyph exceeds ascent"))?,
                g.metrics
                    .advance
                    .map_or(i64::from(g.metrics.bounds.width), |v| v.x),
            )
        } else {
            (b.width, b.height, i64::from(b.width))
        };
        if w > 999 || h > 999 {
            return Err(invalid("DosStart dimensions exceed three decimal digits"));
        }
        let r = raster(
            g,
            Bounds::new(
                w,
                h,
                0,
                ascent
                    .checked_sub(i64::from(h))
                    .ok_or_else(|| invalid("native baseline overflow"))?,
            ),
            &o.limits,
            budget,
        )?;
        out.text(&format!("{w:03}{h:03}"), budget)?;
        for y in 0..h {
            for x in 0..w {
                out.bytes(if r.get_pixel(x, y)? == 0 { b"G" } else { b"H" }, budget)?;
            }
        }
        out.text(&format!(", {advance}\r\n"), budget)?;
    }
    Ok(())
}

// Arbitrary-width decimal arithmetic uses byte/limb vectors and a checked work cap.
// The cap prevents attacker-controlled quadratic big-integer work under tiny input.
fn decimal_row(s: &str, w: u32, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<u8>> {
    if s.is_empty() || !s.bytes().all(|v| v.is_ascii_digit()) {
        return Err(invalid("invalid EDWIN decimal row"));
    }
    let n = (w as usize).div_ceil(8);
    let mut row = alloc(n, budget)?;
    let mut used = 0;
    let mut work = 0usize;
    for digit in s.bytes() {
        let mut carry = u16::from(digit - b'0');
        work = add(work, used + 1)?;
        check(work, l.max_table_bytes, "EDWIN decimal arithmetic work")?;
        for byte in row.iter_mut().take(used) {
            let v = u16::from(*byte) * 10 + carry;
            *byte = v as u8;
            carry = v >> 8;
        }
        if carry != 0 {
            if used == n {
                return Err(invalid("EDWIN row exceeds declared width"));
            }
            row[used] = carry as u8;
            used += 1;
        }
    }
    if !w.is_multiple_of(8) && row.last().is_some_and(|v| *v >> (w % 8) != 0) {
        return Err(invalid("EDWIN row exceeds declared width"));
    }
    for byte in &mut row {
        *byte = byte.reverse_bits();
    }
    Ok(row)
}
fn row_decimal(row: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<String> {
    let max_limbs = add(mul(row.len(), 8)?.div_ceil(29), 1)?;
    budget.add(mul(max_limbs, 4)?)?;
    let mut limbs = Vec::<u32>::new();
    limbs
        .try_reserve_exact(max_limbs)
        .map_err(|_| invalid("EDWIN decimal allocation failed"))?;
    limbs.push(0);
    let mut work = 0;
    for byte in row.iter().rev() {
        let mut carry = u64::from(byte.reverse_bits());
        work = add(work, limbs.len())?;
        check(work, l.max_table_bytes, "EDWIN decimal arithmetic work")?;
        for limb in &mut limbs {
            let v = u64::from(*limb) * 256 + carry;
            *limb = (v % 1_000_000_000) as u32;
            carry = v / 1_000_000_000;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }
    let maximum = mul(limbs.len(), 9)?;
    budget.add(maximum)?;
    let mut s = String::new();
    s.try_reserve_exact(maximum)
        .map_err(|_| invalid("EDWIN text allocation failed"))?;
    use std::fmt::Write as _;
    for (i, v) in limbs.iter().rev().enumerate() {
        if i == 0 {
            write!(&mut s, "{v}")
        } else {
            write!(&mut s, "{v:09}")
        }
        .map_err(|_| invalid("EDWIN decimal formatting failed"))?;
    }
    Ok(s)
}
fn read_edwin(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let mut lines = utf8_text(b)?.lines().filter(|v| !v.trim().is_empty());
    let mut range = lines
        .next()
        .ok_or_else(|| invalid("EDWIN has no range"))?
        .split_whitespace();
    let first = number::<u32>(
        range
            .next()
            .ok_or_else(|| invalid("EDWIN lacks first code"))?,
    )?;
    let last = number::<u32>(
        range
            .next()
            .ok_or_else(|| invalid("EDWIN lacks last code"))?,
    )?;
    if range.next().is_some() || last < first {
        return Err(invalid("invalid EDWIN range"));
    }
    let count = usize::try_from(u64::from(last) - u64::from(first) + 1)
        .map_err(|_| invalid("EDWIN range overflow"))?;
    check(count, l.max_glyphs, "EDWIN glyphs")?;
    let mut f = base_font("edwin", budget)?;
    let mut maxw = 0;
    let mut maxh = 0;
    for line in lines.clone() {
        let h = number::<u32>(
            line.split_whitespace()
                .nth(1)
                .ok_or_else(|| invalid("EDWIN missing height"))?,
        )?;
        maxh = maxh.max(h);
    }
    for i in 0..count {
        let mut fields = lines
            .next()
            .ok_or_else(|| invalid("truncated EDWIN glyphs"))?
            .split_whitespace();
        let label = fields
            .next()
            .and_then(|v| v.strip_suffix(':'))
            .ok_or_else(|| invalid("EDWIN label lacks colon"))?;
        let h = number::<u32>(
            fields
                .next()
                .ok_or_else(|| invalid("EDWIN missing height"))?,
        )?;
        let w = number::<u32>(
            fields
                .next()
                .ok_or_else(|| invalid("EDWIN missing width"))?,
        )?;
        let n = size(w, h, l)?;
        let mut bytes = alloc(n, budget)?;
        let stride = (w as usize).div_ceil(8);
        for y in 0..h as usize {
            let row = decimal_row(
                fields.next().ok_or_else(|| invalid("EDWIN missing row"))?,
                w,
                l,
                budget,
            )?;
            bytes[y * stride..(y + 1) * stride].copy_from_slice(&row);
        }
        if fields.next().is_some() {
            return Err(invalid("EDWIN excess row data"));
        }
        reserve_glyph(&f, l, budget, 1)?;
        budget.add(n)?;
        let mut g = Glyph::new(Bitmap::<Mono>::from_raw(w, h, &bytes, bitmap_limits(l))?);
        g.codepoints.push(Codepoint::Legacy(first + i as u32));
        g.metrics.bounds.y = i64::from(maxh) - i64::from(h);
        g.metrics.advance = Some(Vector::new(i64::from(w), 0));
        if !label.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            budget.add(label.len() * 2)?;
            g.name = Some(label.into());
        }
        f.add_glyph(g)?;
        maxw = maxw.max(w);
        maxh = maxh.max(h);
    }
    if lines.next().is_some() {
        return Err(invalid("EDWIN excess glyphs"));
    }
    font_metrics(&mut f, maxw, maxh, i64::from(maxh), false)?;
    Ok(complete(f, "edwin", Report::default()))
}
fn write_edwin(
    f: &Font,
    o: &NativeWriteOptions,
    out: &mut Output,
    budget: &mut Budget,
) -> Result<()> {
    let map = positions(f, o, budget)?;
    let first = *map
        .first_key_value()
        .ok_or_else(|| invalid("empty font"))?
        .0;
    let last = *map.last_key_value().ok_or_else(|| invalid("empty font"))?.0;
    let count = usize::try_from(u64::from(last) - u64::from(first) + 1)
        .map_err(|_| invalid("EDWIN interval overflow"))?;
    check(count, o.limits.max_glyphs, "EDWIN output glyphs")?;
    out.text(&format!("{first} {last}\n"), budget)?;
    let fallback = cell(f, o)?;
    for cp in first..=last {
        let g = map.get(&cp).copied();
        let b = g.map_or(fallback, |g| g.metrics.bounds);
        let r = raster(g, b, &o.limits, budget)?;
        if let Some(name) = g.and_then(|g| g.name.as_deref()) {
            if name.is_empty()
                || name.bytes().any(|v| v.is_ascii_whitespace() || v == b':')
                || name.as_bytes().first().is_some_and(u8::is_ascii_digit)
            {
                return Err(invalid(
                    "EDWIN glyph name must be a nonnumeric token without colon",
                ));
            }
            out.text(safe_text(name)?, budget)?;
        } else {
            out.text(&cp.to_string(), budget)?;
        }
        out.text(&format!(": {} {}", b.height, b.width), budget)?;
        let stride = (b.width as usize).div_ceil(8);
        for y in 0..b.height as usize {
            let s = row_decimal(
                &r.as_bytes()[y * stride..(y + 1) * stride],
                &o.limits,
                budget,
            )?;
            out.text(" ", budget)?;
            out.text(&s, budget)?;
        }
        out.text("\n", budget)?;
    }
    Ok(())
}

fn native_prop(
    props: &mut crate::model::Properties,
    id: &str,
    key: &str,
    value: &str,
    budget: &mut Budget,
) -> Result<()> {
    budget.items::<Property>(1)?;
    budget.add(mul(add(add(id.len(), key.len())?, value.len())?, 2)?)?;
    props.push(Property::new(format!("{id}.{key}"), value));
    Ok(())
}
fn field(s: &str) -> Result<(&str, &str)> {
    s.split_once(char::is_whitespace)
        .map(|(k, v)| (k, v.trim()))
        .ok_or_else(|| invalid("native property lacks a value"))
}
fn read_fd(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let mut lines = utf8_text(b)?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .peekable();
    let mut f = base_font("mkwinfont", budget)?;
    let mut h = None;
    let mut ascent = None;
    let mut maxw = 0;
    while let Some(&line) = lines.peek() {
        if line.starts_with("char ") {
            break;
        }
        lines.next();
        if let Some(v) = line.strip_prefix('#') {
            native_prop(
                &mut f.headers,
                "mkwinfont",
                "comment",
                v.trim_start(),
                budget,
            )?;
            continue;
        }
        let (key, value) = field(line)?;
        match key {
            "height" => h = Some(number::<u32>(value)?),
            "ascent" => ascent = Some(number::<i64>(value)?),
            "facename" => {
                budget.add(value.len() * 2)?;
                f.names.family = Some(value.into());
            }
            "copyright" => {
                budget.add(value.len() * 2)?;
                f.names.copyright = Some(value.into());
            }
            "pointsize" => {
                f.metrics.point_size = Some(crate::model::Rational::integer(number(value)?))
            }
            _ => native_prop(&mut f.headers, "mkwinfont", key, value, budget)?,
        }
    }
    let h = h.ok_or_else(|| invalid("mkwinfont requires height"))?;
    if h == 0 {
        return Err(invalid("mkwinfont height must be positive"));
    }
    let ascent = ascent.ok_or_else(|| invalid("mkwinfont requires ascent"))?;
    let y = ascent
        .checked_sub(i64::from(h))
        .ok_or_else(|| invalid("mkwinfont ascent overflow"))?;
    while let Some(line) = lines.next() {
        if let Some(v) = line.strip_prefix('#') {
            native_prop(
                &mut f.headers,
                "mkwinfont",
                "comment",
                v.trim_start(),
                budget,
            )?;
            continue;
        }
        let (key, value) = field(line)?;
        if key != "char" {
            return Err(invalid("expected mkwinfont char declaration"));
        }
        check(f.len().saturating_add(1), l.max_glyphs, "mkwinfont glyphs")?;
        let cp = number::<u32>(value)?;
        if cp > 255 {
            return Err(invalid("mkwinfont character exceeds 255"));
        }
        let mut w = None;
        let mut properties = crate::model::Properties::default();
        while let Some(&line) = lines.peek() {
            if line
                .as_bytes()
                .first()
                .is_some_and(|v| matches!(v, b'0' | b'1'))
            {
                break;
            }
            lines.next();
            if let Some(v) = line.strip_prefix('#') {
                native_prop(
                    &mut properties,
                    "mkwinfont",
                    "comment",
                    v.trim_start(),
                    budget,
                )?;
                continue;
            }
            let (key, value) = field(line)?;
            if key == "width" {
                w = Some(number::<u32>(value)?);
            } else if key == "char" {
                return Err(invalid("mkwinfont glyph has no raster"));
            } else {
                native_prop(&mut properties, "mkwinfont", key, value, budget)?;
            }
        }
        let w = w.ok_or_else(|| invalid("mkwinfont glyph requires width"))?;
        if w == 0 {
            return Err(invalid("mkwinfont bitmap width must be positive"));
        }
        let n = size(w, h, l)?;
        let mut data = alloc(n, budget)?;
        let stride = (w as usize).div_ceil(8);
        for row in 0..h as usize {
            let line = lines
                .next()
                .ok_or_else(|| invalid("truncated mkwinfont raster"))?;
            if line.len() != w as usize {
                return Err(invalid("mkwinfont row width mismatch"));
            }
            for (x, v) in line.bytes().enumerate() {
                match v {
                    b'1' => data[row * stride + x / 8] |= 0x80 >> (x % 8),
                    b'0' => {}
                    _ => return Err(invalid("invalid mkwinfont pixel")),
                }
            }
        }
        reserve_glyph(&f, l, budget, 1)?;
        budget.add(n)?;
        let mut g = Glyph::new(Bitmap::<Mono>::from_raw(w, h, &data, bitmap_limits(l))?);
        g.codepoints.push(Codepoint::Legacy(cp));
        g.metrics.bounds.y = y;
        g.metrics.advance = Some(Vector::new(i64::from(w), 0));
        g.properties = properties;
        f.add_glyph(g)?;
        maxw = maxw.max(w);
    }
    if f.is_empty() {
        return Err(invalid("mkwinfont has no glyphs"));
    }
    font_metrics(&mut f, maxw, h, ascent, false)?;
    Ok(complete(f, "mkwinfont", Report::default()))
}
fn write_props(
    props: &crate::model::Properties,
    id: &str,
    separator: &str,
    comment: &str,
    out: &mut Output,
    budget: &mut Budget,
) -> Result<()> {
    let prefix = format!("{id}.");
    for p in props.iter() {
        let Some(key) = p.name.strip_prefix(&prefix) else {
            continue;
        };
        let PropertyValue::Text(value) = &p.value else {
            continue;
        };
        if key == "comment" {
            out.text(comment, budget)?;
        } else {
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'_' | b'-'))
            {
                return Err(invalid("invalid native property key"));
            }
            out.text(key, budget)?;
            out.text(separator, budget)?;
        }
        out.text(safe_text(value)?, budget)?;
        out.text("\n", budget)?;
    }
    Ok(())
}
fn write_fd(f: &Font, o: &NativeWriteOptions, out: &mut Output, budget: &mut Budget) -> Result<()> {
    let map = positions(f, o, budget)?;
    let b = cell(f, o)?;
    let ascent = f.metrics.ascent.unwrap_or(
        b.y.checked_add(i64::from(b.height))
            .ok_or_else(|| invalid("native cell top overflow"))?,
    );
    if let Some(name) = f.names.family.as_deref().or(f.names.full.as_deref()) {
        out.text("facename ", budget)?;
        out.text(safe_text(name)?, budget)?;
        out.text("\n", budget)?;
    }
    if let Some(value) = f.names.copyright.as_deref() {
        out.text("copyright ", budget)?;
        out.text(safe_text(value)?, budget)?;
        out.text("\n", budget)?;
    }
    out.text(&format!("height {}\nascent {ascent}\n", b.height), budget)?;
    if let Some(p) = f.metrics.point_size {
        if p.denominator() != 1 {
            return Err(invalid("mkwinfont point size must be integral"));
        }
        out.text(&format!("pointsize {}\n", p.numerator()), budget)?;
    }
    write_props(&f.headers, "mkwinfont", " ", "# ", out, budget)?;
    for (cp, g) in map {
        if cp > 255 {
            continue;
        }
        let width = u32::try_from(g.metrics.advance.map_or(
            i64::from(g.metrics.bounds.width) + g.metrics.bounds.x.max(0),
            |v| v.x,
        ))
        .map_err(|_| invalid("mkwinfont negative advance"))?;
        if width == 0 {
            return Err(invalid("mkwinfont output width must be positive"));
        }
        let r = raster(
            Some(g),
            Bounds::new(
                width,
                b.height,
                0,
                ascent
                    .checked_sub(i64::from(b.height))
                    .ok_or_else(|| invalid("native baseline overflow"))?,
            ),
            &o.limits,
            budget,
        )?;
        out.text(&format!("\nchar {cp}\nwidth {width}\n"), budget)?;
        write_props(&g.properties, "mkwinfont", " ", "# ", out, budget)?;
        for y in 0..b.height {
            for x in 0..width {
                out.bytes(if r.get_pixel(x, y)? == 0 { b"0" } else { b"1" }, budget)?;
            }
            out.text("\n", budget)?;
        }
    }
    Ok(())
}

fn unicode_labels(s: &str, g: &mut Glyph, budget: &mut Budget) -> Result<()> {
    let mut s = s.trim();
    while !s.is_empty() {
        let value = s
            .strip_prefix('[')
            .and_then(|v| v.split_once(']'))
            .ok_or_else(|| invalid("invalid PSF2TXT Unicode bracket"))?;
        let mut sequence = Vec::new();
        for cp in value.0.split('+') {
            let cp = u32::from_str_radix(cp, 16)
                .map_err(|_| invalid("invalid PSF2TXT Unicode value"))?;
            if char::from_u32(cp).is_none() {
                return Err(invalid("PSF2TXT Unicode value is not a scalar"));
            }
            budget.add(160)?;
            sequence.push(cp);
        }
        if sequence.len() == 1 {
            g.codepoints.push(Codepoint::Unicode(sequence[0]));
        } else {
            budget.items::<Vec<u32>>(1)?;
            g.unicode_sequences.push(sequence);
        }
        s = value
            .1
            .trim_start()
            .strip_prefix(';')
            .ok_or_else(|| invalid("PSF2TXT Unicode bracket lacks semicolon"))?
            .trim_start();
    }
    Ok(())
}
fn read_psf2txt(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let mut lines = utf8_text(b)?.lines().peekable();
    if lines.next().map(str::trim) != Some("%PSF2") {
        return Err(invalid("missing PSF2TXT signature"));
    }
    let mut f = base_font("psf2txt", budget)?;
    let mut w = None;
    let mut h = None;
    let mut count = None;
    let mut version = None;
    let mut flags = None;
    while let Some(&line) = lines.peek() {
        let line = line.trim();
        if line == "%" {
            break;
        }
        lines.next();
        if line.is_empty() {
            continue;
        }
        if let Some(v) = line.strip_prefix("//") {
            native_prop(&mut f.headers, "psf2txt", "comment", v.trim_start(), budget)?;
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("invalid PSF2TXT header line"))?;
        let value = value.trim();
        match key {
            "Width" => w = Some(number::<u32>(value)?),
            "Height" => h = Some(number::<u32>(value)?),
            "Length" => count = Some(number::<usize>(value)?),
            "Version" => version = Some(number::<u32>(value)?),
            "Flags" => flags = Some(number::<u32>(value)?),
            _ => native_prop(&mut f.headers, "psf2txt", key, value, budget)?,
        }
    }
    let w = w.ok_or_else(|| invalid("PSF2TXT missing Width"))?;
    let h = h.ok_or_else(|| invalid("PSF2TXT missing Height"))?;
    let count = count.ok_or_else(|| invalid("PSF2TXT missing Length"))?;
    let flags = flags.ok_or_else(|| invalid("PSF2TXT missing Flags"))?;
    let version = version.ok_or_else(|| invalid("PSF2TXT missing Version"))?;
    if w == 0 || h == 0 || count == 0 || flags > 1 {
        return Err(invalid("invalid PSF2TXT dimensions, count or flags"));
    }
    check(count, l.max_glyphs, "PSF2TXT glyphs")?;
    let n = size(w, h, l)?;
    budget.items::<Property>(2)?;
    budget.add(128)?;
    f.properties
        .push(Property::new("psf2txt.Version", i64::from(version)));
    f.properties
        .push(Property::new("psf2txt.Flags", i64::from(flags)));
    for i in 0..count {
        while lines.peek().is_some_and(|s| s.trim().is_empty()) {
            lines.next();
        }
        if lines.next().map(str::trim) != Some("%") {
            return Err(invalid("PSF2TXT missing glyph separator"));
        }
        let mut g = None;
        let mut props = crate::model::Properties::default();
        let mut label = None;
        while let Some(&line) = lines.peek() {
            let line = line.trim();
            if line == "%" {
                break;
            }
            lines.next();
            if line.is_empty() {
                continue;
            }
            if let Some(v) = line.strip_prefix("//") {
                native_prop(&mut props, "psf2txt", "comment", v.trim_start(), budget)?;
                continue;
            }
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| invalid("invalid PSF2TXT glyph field"))?;
            match key {
                "Bitmap" => {
                    if g.is_some() {
                        return Err(invalid("duplicate PSF2TXT Bitmap"));
                    }
                    let mut bytes = alloc(n, budget)?;
                    let stride = (w as usize).div_ceil(8);
                    let mut row = value.trim();
                    if row.is_empty() {
                        row = lines
                            .next()
                            .ok_or_else(|| invalid("missing PSF2TXT bitmap row"))?
                            .trim();
                    }
                    for y in 0..h as usize {
                        let pixels = row.trim_end_matches('\\').trim();
                        if pixels.len() != w as usize {
                            return Err(invalid("PSF2TXT bitmap width mismatch"));
                        }
                        for (x, v) in pixels.bytes().enumerate() {
                            match v {
                                b'#' => bytes[y * stride + x / 8] |= 0x80 >> (x % 8),
                                b'-' => {}
                                _ => return Err(invalid("invalid PSF2TXT bitmap pixel")),
                            }
                        }
                        if y + 1 < h as usize {
                            row = lines
                                .next()
                                .ok_or_else(|| invalid("truncated PSF2TXT bitmap"))?
                                .trim();
                        }
                    }
                    budget.add(n)?;
                    g = Some(Glyph::new(Bitmap::<Mono>::from_raw(
                        w,
                        h,
                        &bytes,
                        bitmap_limits(l),
                    )?));
                }
                "Unicode" => {
                    if label.is_some() || flags == 0 {
                        return Err(invalid("duplicate or flag-disabled PSF2TXT Unicode field"));
                    }
                    label = Some(value);
                }
                _ => native_prop(&mut props, "psf2txt", key, value.trim(), budget)?,
            }
        }
        reserve_glyph(&f, l, budget, 1)?;
        let mut g = g.ok_or_else(|| invalid("PSF2TXT glyph lacks Bitmap"))?;
        if let Some(label) = label {
            unicode_labels(label, &mut g, budget)?;
        }
        if g.codepoints.is_empty() && g.unicode_sequences.is_empty() {
            g.codepoints.push(Codepoint::Legacy(
                u32::try_from(i).map_err(|_| invalid("PSF2TXT glyph index overflow"))?,
            ));
        }
        g.metrics.advance = Some(Vector::new(i64::from(w), 0));
        g.properties = props;
        f.add_glyph(g)?;
    }
    if lines.any(|s| !s.trim().is_empty()) {
        return Err(invalid("PSF2TXT excess glyph records"));
    }
    font_metrics(&mut f, w, h, i64::from(h), true)?;
    Ok(complete(f, "psf2txt", Report::default()))
}
fn write_psf2txt(
    f: &Font,
    o: &NativeWriteOptions,
    out: &mut Output,
    budget: &mut Budget,
) -> Result<()> {
    if f.is_empty() {
        return Err(invalid("PSF2TXT has no glyphs"));
    }
    let b = cell(f, o)?;
    check(f.len(), o.limits.max_glyphs, "PSF2TXT output glyphs")?;
    let version = super::legacy_common::int(&f.properties, "psf2txt.Version", 0)?;
    let mut flags = super::legacy_common::int(&f.properties, "psf2txt.Flags", 1)?;
    if f.glyphs().any(|(_, g)| {
        !g.unicode_sequences.is_empty()
            || g.codepoints
                .iter()
                .any(|v| matches!(v, Codepoint::Unicode(_)))
    }) {
        flags = 1;
    }
    if !(0..=u32::MAX as i64).contains(&version) || ![0, 1].contains(&flags) {
        return Err(invalid("invalid PSF2TXT version or flags"));
    }
    out.text(
        &format!(
            "%PSF2\nVersion: {version}\nFlags: {flags}\nLength: {}\nWidth: {}\nHeight: {}\n",
            f.len(),
            b.width,
            b.height
        ),
        budget,
    )?;
    write_props(&f.headers, "psf2txt", ": ", "// ", out, budget)?;
    for (_, g) in f.glyphs() {
        let r = raster(Some(g), b, &o.limits, budget)?;
        out.text("%\nBitmap: ", budget)?;
        for y in 0..b.height {
            for x in 0..b.width {
                out.bytes(if r.get_pixel(x, y)? == 0 { b"-" } else { b"#" }, budget)?;
            }
            if y + 1 < b.height {
                out.text(" \\\n        ", budget)?;
            } else {
                out.text("\n", budget)?;
            }
        }
        write_props(&g.properties, "psf2txt", ": ", "// ", out, budget)?;
        let has = g
            .codepoints
            .iter()
            .any(|v| matches!(v, Codepoint::Unicode(_)))
            || !g.unicode_sequences.is_empty();
        if has {
            out.text("Unicode: ", budget)?;
            for cp in &g.codepoints {
                if let Codepoint::Unicode(cp) = cp {
                    out.text(&format!("[{cp:08x}];"), budget)?;
                }
            }
            for sequence in &g.unicode_sequences {
                out.text("[", budget)?;
                for (i, cp) in sequence.iter().enumerate() {
                    if i != 0 {
                        out.text("+", budget)?;
                    }
                    out.text(&format!("{cp:08x}"), budget)?;
                }
                out.text("];", budget)?;
            }
            out.text("\n", budget)?;
        }
    }
    Ok(())
}
