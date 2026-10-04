//! Internal accounting, fixed point metrics and complete conversion diagnostics.
#[cfg(any(feature = "pk", feature = "gf"))]
use crate::bitmap::{Bitmap, BitmapLimits};
use crate::model::{Font, Glyph, Properties, PropertyValue};
#[cfg(any(feature = "pk", feature = "gf"))]
use crate::model::{FontCharset, FontSet, GlyphBitmap, Property, Rational, Vector};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};

/// Common output controls for TeX PK and GF.
#[cfg(any(feature = "pk", feature = "gf"))]
#[derive(Debug, Clone)]
pub struct TexWriteOptions {
    /// Override design size in TeX points (72.27 points per inch).
    pub design_size: Option<Rational>,
    /// Override the integral horizontal and vertical dpi.
    pub resolution: Option<Vector>,
    /// Explicit threshold required for grayscale source glyphs.
    pub gray_threshold: Option<u8>,
    /// Shared resource and geometry limits.
    pub limits: ResourceLimits,
    /// Maximum generated file length, also capped by the reader input limit.
    pub max_output_bytes: usize,
}
#[cfg(any(feature = "pk", feature = "gf"))]
impl Default for TexWriteOptions {
    fn default() -> Self {
        Self {
            design_size: None,
            resolution: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}
pub(crate) fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
pub(crate) fn limit() -> Error {
    Error::ResourceLimit {
        resource: "TeX/Plan 9 aggregate bytes",
        limit: 0,
    }
}
pub(crate) fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
pub(crate) struct Budget {
    pub used: usize,
    pub max: usize,
}
impl Budget {
    pub fn new(n: usize, l: &ResourceLimits) -> Result<Self> {
        let mut s = Self {
            used: 0,
            max: l.max_total_bytes,
        };
        s.add(n)?;
        Ok(s)
    }
    pub fn add(&mut self, n: usize) -> Result<()> {
        self.used = self.used.checked_add(n).ok_or(Error::ResourceLimit {
            resource: "TeX/Plan 9 aggregate bytes",
            limit: self.max,
        })?;
        check(self.used, self.max, "TeX/Plan 9 aggregate bytes")
    }
    pub fn remaining(&self) -> usize {
        self.max - self.used
    }
}
pub(crate) struct Cursor<'a> {
    pub b: &'a [u8],
    pub p: usize,
}
impl<'a> Cursor<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .p
            .checked_add(n)
            .ok_or_else(|| invalid("field length overflow"))?;
        let b = self
            .b
            .get(self.p..end)
            .ok_or_else(|| invalid("truncated font record"))?;
        self.p = end;
        Ok(b)
    }
    #[cfg(any(feature = "pk", feature = "gf"))]
    pub fn u(&mut self, n: usize) -> Result<u32> {
        let b = self.take(n)?;
        Ok(b.iter().fold(0, |x, &y| (x << 8) | u32::from(y)))
    }
    #[cfg(any(feature = "pk", feature = "gf"))]
    pub fn s(&mut self, n: usize) -> Result<i64> {
        let v = self.u(n)?;
        Ok(match n {
            1 => i64::from(v as i8),
            2 => i64::from(v as i16),
            3 => i64::from((v << 8) as i32 >> 8),
            4 => i64::from(v as i32),
            _ => return Err(invalid("invalid integer width")),
        })
    }
    pub fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
}
pub(crate) struct Output {
    pub b: Vec<u8>,
    pub max: usize,
}
impl Output {
    pub fn new(max: usize) -> Self {
        Self { b: Vec::new(), max }
    }
    pub fn put(&mut self, b: &[u8]) -> Result<()> {
        let n = self.b.len().checked_add(b.len()).ok_or_else(limit)?;
        check(n, self.max, "encoded font bytes")?;
        self.b.try_reserve(b.len()).map_err(|_| limit())?;
        self.b.extend_from_slice(b);
        Ok(())
    }
    #[cfg(any(feature = "pk", feature = "gf"))]
    pub fn u(&mut self, v: u32, n: usize) -> Result<()> {
        self.put(&v.to_be_bytes()[4 - n..])
    }
    #[cfg(any(feature = "pk", feature = "gf"))]
    pub fn byte(&mut self, v: u8) -> Result<()> {
        self.put(&[v])
    }
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}
pub(crate) fn bitmap_size(w: u32, h: u32, l: &ResourceLimits) -> Result<usize> {
    let area = u64::from(w) * u64::from(h);
    if area > l.max_pixels_per_glyph as u64 {
        return Err(Error::ResourceLimit {
            resource: "glyph pixels",
            limit: l.max_pixels_per_glyph,
        });
    }
    let n = usize::try_from(u64::from(w).div_ceil(8) * u64::from(h)).map_err(|_| limit())?;
    check(n, l.max_total_bytes, "glyph bytes")?;
    Ok(n)
}
/// Set a contiguous MSB-first bit range, touching boundary bytes and whole bytes.
pub(crate) fn black(row: &mut [u8], start: usize, len: usize) {
    if len == 0 {
        return;
    }
    let end = start + len;
    let first = start / 8;
    let last = (end - 1) / 8;
    if first == last {
        row[first] |= (255u8 >> (start % 8)) & (255u8 << (7 - (end - 1) % 8));
        return;
    }
    row[first] |= 255 >> (start % 8);
    row[first + 1..last].fill(255);
    row[last] |= 255 << (7 - (end - 1) % 8);
}
/// Copy a bounded unaligned bit span without expanding individual pixels.
#[cfg(any(feature = "pk", feature = "plan9"))]
pub(crate) fn copy_bits(src: &[u8], start: usize, dst: &mut [u8], dest: usize, len: usize) {
    let mut done = 0;
    while done < len {
        let n = (8 - (dest + done) % 8).min(len - done);
        let s = start + done;
        let sh = s % 8;
        let mut v = u16::from(src[s / 8]) << 8;
        if sh + n > 8 {
            v |= u16::from(src[s / 8 + 1]);
        }
        let value = ((v << sh) >> 8) as u8;
        let mask = 255u8 << (8 - n);
        dst[(dest + done) / 8] |= (value & mask) >> ((dest + done) % 8);
        done += n;
    }
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn int(p: &Properties, k: &str) -> Option<i64> {
    match p.get(k) {
        Some(PropertyValue::Integer(v)) => Some(*v),
        Some(PropertyValue::Unsigned(v)) => i64::try_from(*v).ok(),
        _ => None,
    }
}
pub(crate) fn bytes<'a>(p: &'a Properties, k: &str) -> Option<&'a [u8]> {
    match p.get(k) {
        Some(PropertyValue::Bytes(v)) => Some(v),
        _ => None,
    }
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn add_int(p: &mut Properties, k: &str, v: i64) {
    p.push(Property::new(k, v));
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn round_div(n: i128, d: i128) -> i128 {
    if n < 0 {
        -((-n + d / 2) / d)
    } else {
        (n + d / 2) / d
    }
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn tex_header(f: &mut Font, ds: u32, cs: u32, hp: u32, vp: u32) -> Result<()> {
    if ds == 0 || hp == 0 || vp == 0 {
        return Err(invalid(
            "TeX design size and pixel resolution must be positive",
        ));
    }
    for (k, v) in [
        ("tex.design_size", ds),
        ("tex.checksum", cs),
        ("tex.hppp", hp),
        ("tex.vppp", vp),
    ] {
        add_int(&mut f.headers, k, i64::from(v));
    }
    f.charset = FontCharset {
        name: Some("unknown".into()),
        ..Default::default()
    };
    f.metrics.point_size = Some(Rational::new(i64::from(ds), 1 << 20)?);
    f.metrics.resolution = Some(Vector::new(
        round_div(i128::from(hp) * 7227, 6553600) as i64,
        round_div(i128::from(vp) * 7227, 6553600) as i64,
    ));
    let px = round_div(i128::from(ds) * i128::from(hp), 1i128 << 36);
    let py = round_div(i128::from(ds) * i128::from(vp), 1i128 << 36);
    f.metrics.ppem_x = Some(u32::try_from(px).map_err(|_| invalid("TeX ppem exceeds u32"))?);
    f.metrics.ppem_y = Some(u32::try_from(py).map_err(|_| invalid("TeX ppem exceeds u32"))?);
    f.metrics.pixel_size = f.metrics.ppem_y;
    Ok(())
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn tex_glyph(g: &mut Glyph, tfm: i32, dx: i64, dy: i64) {
    g.metrics.advance = Some(Vector::new(
        round_div(i128::from(dx), 65536) as i64,
        round_div(i128::from(dy), 65536) as i64,
    ));
    g.metrics.scalable_advance = Some(Vector::new(
        round_div(i128::from(tfm) * 1000, 1 << 20) as i64,
        0,
    ));
    for (k, v) in [
        ("tex.tfm_width", i64::from(tfm)),
        ("tex.dx", dx),
        ("tex.dy", dy),
    ] {
        add_int(&mut g.properties, k, v);
    }
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn header_values(f: &Font, o: &TexWriteOptions) -> Result<(u32, u32, u32, u32)> {
    let ds = if o.design_size.is_none()
        && f.metrics.point_size
            == int(&f.headers, "tex.design_size").and_then(|v| Rational::new(v, 1 << 20).ok())
    {
        int(&f.headers, "tex.design_size").unwrap_or(10 << 20)
    } else {
        let p = o
            .design_size
            .or(f.metrics.point_size)
            .unwrap_or(Rational::integer(10));
        i64::try_from(round_div(
            i128::from(p.numerator()) * (1 << 20),
            i128::from(p.denominator()),
        ))
        .map_err(|_| invalid("design size overflow"))?
    };
    let resolution = o
        .resolution
        .or(f.metrics.resolution)
        .unwrap_or(Vector::new(72, 72));
    let ppp = |key: &str, dpi: i64| -> Result<u32> {
        let old = int(&f.headers, key);
        let n = if o.resolution.is_none()
            && old.is_some_and(|v| round_div(i128::from(v) * 7227, 6553600) == i128::from(dpi))
        {
            i128::from(old.unwrap_or(0))
        } else {
            round_div(i128::from(dpi) * 6553600, 7227)
        };
        u32::try_from(n).map_err(|_| invalid("TeX pixel resolution outside u32"))
    };
    let ds = u32::try_from(ds).map_err(|_| invalid("TeX design size outside u32"))?;
    let hp = ppp("tex.hppp", resolution.x)?;
    let vp = ppp("tex.vppp", resolution.y)?;
    if ds == 0 || hp == 0 || vp == 0 {
        return Err(invalid("TeX size and resolution must be positive"));
    }
    Ok((
        ds,
        u32::try_from(int(&f.headers, "tex.checksum").unwrap_or(0))
            .map_err(|_| invalid("TeX checksum outside u32"))?,
        hp,
        vp,
    ))
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn glyph_values(g: &Glyph, hp: u32, ds: u32) -> Result<(i32, i64, i64)> {
    let advance = g
        .metrics
        .advance
        .unwrap_or(Vector::new(i64::from(g.bitmap.width()), 0));
    let fixed = |key: &str, value: i64, scale: i128| -> Result<i64> {
        let raw = int(&g.properties, key);
        let v = if raw
            .is_some_and(|v| round_div(i128::from(v) * scale, 1 << 20) == i128::from(value))
        {
            i128::from(raw.unwrap_or(0))
        } else {
            round_div(i128::from(value) * (1 << 20), scale)
        };
        i64::try_from(v).map_err(|_| invalid("TeX fixed point metric outside i64"))
    };
    let dx = fixed("tex.dx", advance.x, 16)?;
    let dy = fixed("tex.dy", advance.y, 16)?;
    let tfm = if let Some(s) = g.metrics.scalable_advance {
        i32::try_from(fixed("tex.tfm_width", s.x, 1000)?)
            .map_err(|_| invalid("TeX TFM width outside i32"))?
    } else {
        i32::try_from(round_div(
            i128::from(advance.x) * (1i128 << 56),
            i128::from(ds) * i128::from(hp),
        ))
        .map_err(|_| invalid("TeX TFM width outside i32"))?
    };
    Ok((tfm, dx, dy))
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn mono(g: &Glyph, threshold: Option<u8>, l: &ResourceLimits) -> Result<Bitmap> {
    match &g.bitmap {
        GlyphBitmap::Mono(b) => {
            Bitmap::from_raw(b.width(), b.height(), b.as_bytes(), bitmap_limits(l))
        }
        GlyphBitmap::Gray8(b) => {
            let t =
                threshold.ok_or_else(|| invalid("TeX output of Gray8 requires gray_threshold"))?;
            b.to_mono(t)
        }
    }
}
pub(crate) fn loss(
    r: &mut Report,
    id: &str,
    code: &str,
    g: Option<usize>,
    cp: Option<u32>,
    message: impl Into<String>,
) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("{id}.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: g,
            codepoint: cp,
            asset: None,
        },
        message: message.into(),
    });
}
pub(crate) fn fatal(mut r: Report, id: &str, message: &str) -> Error {
    r.push(ReportEntry {
        level: Level::Error,
        impact: Impact::None,
        code: format!("{id}.no_representable_glyphs"),
        location: Location {
            font_index: Some(0),
            ..Default::default()
        },
        message: message.into(),
    });
    Error::ReportedErrors(r)
}
pub(crate) fn property_cost(p: &Properties, b: &mut Budget) -> Result<()> {
    for p in p.iter() {
        b.add(
            512 + p.name.capacity().saturating_mul(16) + p.raw.as_ref().map_or(0, Vec::capacity),
        )?;
        let mut stack = vec![&p.value];
        while let Some(v) = stack.pop() {
            b.add(64)?;
            match v {
                PropertyValue::Bytes(v) => b.add(v.capacity())?,
                PropertyValue::Text(v) | PropertyValue::Decimal(v) => {
                    b.add(v.capacity().saturating_mul(4))?
                }
                PropertyValue::List(v) => {
                    b.add(
                        v.capacity()
                            .saturating_mul(std::mem::size_of::<PropertyValue>() + 16),
                    )?;
                    stack.extend(v)
                }
                _ => {}
            }
        }
    }
    Ok(())
}
pub(crate) fn model_budget(f: &Font, l: &ResourceLimits) -> Result<Budget> {
    f.validate(l)?;
    let mut b = Budget::new(8192, l)?;
    // Deleted slots retain their full enum storage; reserve Vec growth as well.
    b.add(
        f.slot_count()
            .saturating_mul(std::mem::size_of::<Option<Glyph>>())
            .saturating_mul(2),
    )?;
    property_cost(&f.headers, &mut b)?;
    property_cost(&f.properties, &mut b)?;
    property_cost(&f.names.localized, &mut b)?;
    for n in [
        &f.source_format,
        &f.names.family,
        &f.names.style,
        &f.names.full,
        &f.names.postscript,
        &f.names.version,
        &f.names.copyright,
        &f.names.license,
        &f.names.license_url,
        &f.names.foundry,
        &f.names.xlfd,
        &f.charset.name,
        &f.charset.registry,
        &f.charset.encoding,
    ]
    .into_iter()
    .flatten()
    {
        b.add(n.capacity().saturating_mul(4) + 256)?
    }
    b.add(f.names.xlfd_fields.capacity().saturating_mul(32))?;
    for n in &f.names.xlfd_fields {
        b.add(n.capacity().saturating_mul(4) + 64)?
    }
    b.add(f.charset.code_ranges.capacity().saturating_mul(32))?;
    // Per-glyph 6144 covers source ownership, temporary structures and every fixed
    // metric/metadata diagnostic. Per-code 512 covers mapping vectors, BTree nodes
    // and one alias diagnostic; source metadata is compared only once per glyph.
    for (_, g) in f.glyphs() {
        b.add(g.unicode_sequences.capacity().saturating_mul(32))?;
        b.add(
            6144 + g.bitmap.as_bytes().len().saturating_mul(3)
                + g.codepoints.capacity().saturating_mul(512),
        )?;
        for s in &g.unicode_sequences {
            b.add(256 + s.capacity().saturating_mul(16))?
        }
        if let Some(n) = &g.name {
            b.add(n.capacity().saturating_mul(4) + 128)?
        }
        property_cost(&g.properties, &mut b)?;
    }
    Ok(b)
}
pub(crate) fn compare_properties(
    a: &Properties,
    b: &Properties,
    id: &str,
    scope: &str,
    g: Option<usize>,
    r: &mut Report,
) {
    let mut target = b.iter();
    for p in a.iter() {
        if target.next() != Some(p) {
            loss(
                r,
                id,
                "property",
                g,
                None,
                format!(
                    "{scope} property {:?} or its order/raw record changes",
                    p.name
                ),
            );
        }
    }
    if b.len() > a.len() {
        loss(
            r,
            id,
            "property_projection",
            g,
            None,
            format!("{scope} gains target-native property projections"),
        );
    }
}
pub(crate) fn compare_font(a: &Font, b: &Font, id: &str, r: &mut Report) {
    if a.names != b.names {
        loss(
            r,
            id,
            "names",
            None,
            None,
            "Font name, license or localized name metadata changes",
        )
    }
    compare_properties(
        &a.names.localized,
        &b.names.localized,
        id,
        "Localized name",
        None,
        r,
    );
    if a.charset != b.charset {
        loss(
            r,
            id,
            "charset",
            None,
            None,
            "Charset metadata or interpretation changes",
        )
    }
    compare_properties(&a.headers, &b.headers, id, "Header", None, r);
    compare_properties(&a.properties, &b.properties, id, "Font", None, r);
    macro_rules! m{($($k:ident),*)=>{$(if a.metrics.$k!=b.metrics.$k{loss(r,id,"font_metric",None,None,concat!("Font metric ",stringify!($k)," changes"));})*};}
    m!(
        pixel_size,
        ppem_x,
        ppem_y,
        point_size,
        resolution,
        ascent,
        descent,
        bounds,
        line_gap,
        line_height,
        underline_position,
        underline_thickness,
        default_glyph,
        default_character,
        space_width,
        spacing,
        glyph_defaults,
        metrics_set
    );
    if a.slot_count() != b.slot_count() {
        loss(
            r,
            id,
            "slots",
            None,
            None,
            "Glyph table slot count or deleted slots change",
        )
    }
}
pub(crate) fn compare_glyph(
    a: &Glyph,
    b: &Glyph,
    src: usize,
    dst: usize,
    id: &str,
    r: &mut Report,
) {
    if src != dst {
        loss(
            r,
            id,
            "slot",
            Some(src),
            None,
            format!("Glyph moves to slot {dst}"),
        );
    }
    if a.codepoints != b.codepoints {
        if a.codepoints.is_empty() {
            loss(
                r,
                id,
                "mapping",
                Some(src),
                None,
                "Unencoded glyph receives a target slot mapping",
            );
        }
        for cp in &a.codepoints {
            loss(
                r,
                id,
                "mapping",
                Some(src),
                Some(cp.value()),
                "Codepoint domain, alias identity or mapping order changes",
            );
        }
    }
    if a.unicode_sequences != b.unicode_sequences {
        for _ in &a.unicode_sequences {
            loss(
                r,
                id,
                "sequence",
                Some(src),
                None,
                "Unicode sequence is not represented",
            );
        }
    }
    if a.name != b.name {
        loss(r, id, "glyph_name", Some(src), None, "Glyph name changes")
    }
    if a.bitmap != b.bitmap {
        loss(
            r,
            id,
            "bitmap",
            Some(src),
            None,
            "Glyph bitmap rectangle, depth or samples change",
        )
    }
    compare_properties(&a.properties, &b.properties, id, "Glyph", Some(src), r);
    macro_rules! m{($($k:ident),*)=>{$(if a.metrics.$k!=b.metrics.$k{loss(r,id,"glyph_metric",Some(src),None,concat!("Glyph metric ",stringify!($k)," changes"));})*};}
    m!(
        bounds,
        advance,
        scalable_advance,
        vertical_advance,
        vertical_scalable_advance,
        vertical_origin,
        abc,
        left_bearing,
        right_bearing,
        image_offset
    );
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn container_losses(
    s: &FontSet,
    id: &str,
    r: &mut Report,
    l: &ResourceLimits,
    n: usize,
) -> Result<()> {
    let mut b = model_budget(s.single()?, l)?;
    b.add(n)?;
    b.add(container_allowance(s, l)?)?;
    for p in s.properties.iter() {
        loss(
            r,
            id,
            "container_property",
            None,
            None,
            format!("Container property {:?} is not represented", p.name),
        );
    }
    report_cost(r, &mut b)
}
pub(crate) fn report_cost(r: &Report, b: &mut Budget) -> Result<()> {
    b.add(
        r.entries
            .capacity()
            .saturating_mul(std::mem::size_of::<ReportEntry>()),
    )?;
    for e in &r.entries {
        b.add(
            e.code.capacity()
                + e.message.capacity()
                + e.location.asset.as_ref().map_or(0, String::capacity),
        )?;
    }
    Ok(())
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn special(
    c: &mut Cursor<'_>,
    op: u8,
    base: u8,
    prefix: &str,
    p: &mut Properties,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<()> {
    let value = if op < base + 4 {
        let n = c.u(usize::from(op - base + 1))? as usize;
        check(n, l.max_table_bytes, "TeX special bytes")?;
        b.add(n.saturating_mul(2) + 256)?;
        PropertyValue::Bytes(c.take(n)?.to_vec())
    } else {
        b.add(256)?;
        PropertyValue::Integer(c.s(4)?)
    };
    p.push(Property::new(
        format!(
            "{prefix}.{}",
            if op < base + 4 {
                "special"
            } else {
                "numspecial"
            }
        ),
        value,
    ));
    Ok(())
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn write_specials(
    p: &Properties,
    prefix: &str,
    base: u8,
    out: &mut Output,
    l: &ResourceLimits,
) -> Result<()> {
    for p in p.iter() {
        if p.name == format!("{prefix}.special") {
            if let PropertyValue::Bytes(v) = &p.value {
                check(v.len(), l.max_table_bytes, "TeX special bytes")?;
                out.byte(base + 3)?;
                out.u(
                    u32::try_from(v.len()).map_err(|_| invalid("special exceeds u32"))?,
                    4,
                )?;
                out.put(v)?
            }
        } else if p.name == format!("{prefix}.numspecial")
            && let PropertyValue::Integer(v) = p.value
        {
            out.byte(base + 4)?;
            out.u(
                i32::try_from(v).map_err(|_| invalid("numeric special exceeds i32"))? as u32,
                4,
            )?
        }
    }
    Ok(())
}
#[cfg(any(feature = "pk", feature = "gf"))]
pub(crate) fn mappings<'a>(
    f: &'a Font,
    id: &str,
    r: &mut Report,
    l: &ResourceLimits,
) -> Result<Vec<(usize, u32, &'a Glyph)>> {
    let mut v = Vec::new();
    for (gid, g) in f.glyphs() {
        if g.codepoints.is_empty() {
            loss(
                r,
                id,
                "unencoded",
                Some(gid.index()),
                None,
                "Unencoded glyph omitted",
            );
        }
        for cp in &g.codepoints {
            check(v.len() + 1, l.max_glyphs, "output glyph mappings")?;
            v.push((gid.index(), cp.value(), g));
        }
    }
    if v.is_empty() {
        return Err(fatal(
            std::mem::take(r),
            id,
            "No mapped glyphs can be written",
        ));
    }
    Ok(v)
}

/// Preserve every omission already found when a later writer check is fatal.
pub(crate) fn write_error(error: Error, mut report: Report, id: &str) -> Error {
    if report.entries.is_empty() {
        return error;
    }
    if let Error::ReportedErrors(other) = error {
        report.extend(other);
    } else {
        report.push(ReportEntry {
            level: Level::Error,
            impact: Impact::None,
            code: format!("{id}.write_failed"),
            location: Location {
                font_index: Some(0),
                ..Default::default()
            },
            message: error.to_string(),
        });
    }
    Error::ReportedErrors(report)
}

/// Reserve container strings/properties before entering an inner writer.
pub(crate) fn container_allowance(
    set: &crate::model::FontSet,
    limits: &ResourceLimits,
) -> Result<usize> {
    let mut budget = Budget::new(0, limits)?;
    property_cost(&set.properties, &mut budget)?;
    if let Some(source) = &set.source_format {
        budget.add(source.capacity().saturating_add(128))?;
    }
    Ok(budget.used)
}
/// Emit per-source metadata once, plus one diagnostic for each split alias.
pub(crate) fn compare_mapped(
    target: &Font,
    maps: &[(usize, u32, &Glyph)],
    id: &str,
    r: &mut Report,
) {
    let mut seen = std::collections::BTreeSet::new();
    for ((gid, g), &(source, code, original)) in target.glyphs().zip(maps) {
        if seen.insert(source) {
            compare_glyph(original, g, source, gid.index(), id, r);
        } else {
            loss(
                r,
                id,
                "alias_copy",
                Some(source),
                Some(code),
                format!(
                    "Alias is materialized as independent target glyph slot {}",
                    gid.index()
                ),
            );
        }
    }
}
