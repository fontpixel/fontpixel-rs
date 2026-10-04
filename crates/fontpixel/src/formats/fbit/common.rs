#[cfg(any(feature = "fbit", feature = "mgtk", feature = "qpf", feature = "qpf2"))]
use crate::bitmap::BitmapLimits;
use crate::model::{Font, FontSet, Properties, PropertyValue};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};

pub(in crate::formats) fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
pub(in crate::formats) fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
pub(in crate::formats) fn bytes<'a>(p: &'a Properties, name: &str) -> Option<&'a [u8]> {
    match p.get(name) {
        Some(PropertyValue::Bytes(v)) => Some(v),
        _ => None,
    }
}
#[cfg(any(feature = "fbit", feature = "mgtk", feature = "qpf", feature = "qpf2"))]
pub(in crate::formats) fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_table_bytes,
    }
}
pub(in crate::formats) struct Budget<'a> {
    pub used: usize,
    #[cfg(feature = "lisa")]
    expanded: usize,
    pub font_index: Option<usize>,
    pub limits: &'a ResourceLimits,
}
impl<'a> Budget<'a> {
    pub fn new(limits: &'a ResourceLimits) -> Self {
        Self {
            used: 0,
            #[cfg(feature = "lisa")]
            expanded: 0,
            font_index: None,
            limits,
        }
    }
    pub fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| invalid("font allocation size overflow"))?;
        check(
            self.used,
            self.limits.max_total_bytes,
            "font cumulative allocation",
        )
    }
    #[cfg(feature = "lisa")]
    pub fn expansion(&mut self, n: usize) -> Result<()> {
        self.expanded = self
            .expanded
            .checked_add(n)
            .ok_or_else(|| invalid("decompression budget overflow"))?;
        check(
            self.expanded,
            self.limits.max_decompressed_bytes,
            "Lisa cumulative decompressed bytes",
        )
    }
    pub fn table(&mut self, n: usize) -> Result<()> {
        check(n, self.limits.max_table_bytes, "font table bytes")?;
        self.add(n)
    }
    #[cfg(any(feature = "fbit", feature = "mgtk", feature = "qpf", feature = "qpf2"))]
    pub fn glyph(&mut self, w: usize, h: usize) -> Result<()> {
        let pixels = w
            .checked_mul(h)
            .ok_or_else(|| invalid("glyph pixel count overflow"))?;
        check(
            pixels,
            self.limits.max_pixels_per_glyph,
            "font glyph pixels",
        )?;
        self.table(
            w.div_ceil(8)
                .checked_mul(h)
                .ok_or_else(|| invalid("glyph byte size overflow"))?,
        )?;
        self.add(1024)
    }
    pub fn loss(&mut self, r: &mut Report, code: &str, message: &str) -> Result<()> {
        self.add(256 + code.len() + message.len())?;
        r.push(ReportEntry {
            level: Level::Warning,
            impact: Impact::Loss,
            code: code.into(),
            message: message.into(),
            location: Location {
                font_index: self.font_index,
                ..Location::default()
            },
        });
        Ok(())
    }
    fn value(&mut self, v: &PropertyValue, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(invalid("font property nesting exceeds 32"));
        }
        self.add(64)?;
        match v {
            PropertyValue::Bytes(b) => self.table(b.len()),
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => self.table(s.len()),
            PropertyValue::List(v) => {
                for x in v {
                    self.value(x, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn properties(&mut self, p: &Properties) -> Result<()> {
        for p in p.iter() {
            self.table(p.name.len())?;
            self.value(&p.value, 0)?;
            if let Some(s) = &p.raw {
                self.table(s.len())?;
            }
        }
        Ok(())
    }
    pub fn model(&mut self, s: &FontSet) -> Result<()> {
        s.validate(self.limits)?;
        self.properties(&s.properties)?;
        if let Some(v) = &s.source_format {
            self.table(v.len())?;
        }
        for f in &s.fonts {
            self.add(2048)?;
            self.add(f.slot_count().saturating_sub(f.len()).saturating_mul(1024))?;
            self.properties(&f.properties)?;
            self.properties(&f.headers)?;
            self.properties(&f.names.localized)?;
            for s in [
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
                &f.source_format,
            ]
            .into_iter()
            .flatten()
            .chain(f.names.xlfd_fields.iter())
            {
                self.table(s.len())?;
            }
            self.add(f.charset.code_ranges.len().saturating_mul(8))?;
            for (_, g) in f.glyphs() {
                self.add(1024 + g.bitmap.as_bytes().len())?;
                self.add(g.codepoints.len().saturating_mul(128))?;
                self.properties(&g.properties)?;
                if let Some(s) = &g.name {
                    self.table(s.len())?;
                }
                for seq in &g.unicode_sequences {
                    self.add(64 + seq.len().saturating_mul(12))?;
                }
            }
        }
        Ok(())
    }
}
// Compare every supplied model field, allowing absent optional source values to
// acquire format-defined defaults. Ordered properties and glyph identity matter.
pub(in crate::formats) fn compare(
    s: &FontSet,
    d: &FontSet,
    prefix: &str,
    b: &mut Budget<'_>,
    r: &mut Report,
) -> Result<()> {
    if !s.properties.is_empty() && s.properties != d.properties {
        b.loss(
            r,
            &format!("{prefix}.collection_metadata"),
            "Collection properties are not represented",
        )?;
    }
    if s.fonts.len() != d.fonts.len() {
        b.loss(
            r,
            &format!("{prefix}.font_count"),
            "Font collection count changed",
        )?;
    }
    for (index, (f, n)) in s.fonts.iter().zip(&d.fonts).enumerate() {
        b.font_index = Some(index);
        compare_font(f, n, prefix, b, r)?;
    }
    b.font_index = None;
    Ok(())
}
fn compare_font(
    f: &Font,
    n: &Font,
    prefix: &str,
    b: &mut Budget<'_>,
    r: &mut Report,
) -> Result<()> {
    let mut names = false;
    macro_rules! named { ($($field:ident),*) => { $(names |= f.names.$field.is_some() && f.names.$field != n.names.$field;)* }; }
    named!(
        family,
        style,
        full,
        postscript,
        version,
        copyright,
        license,
        license_url,
        foundry,
        xlfd
    );
    names |= !f.names.xlfd_fields.is_empty() && f.names.xlfd_fields != n.names.xlfd_fields;
    names |= !f.names.localized.is_empty() && f.names.localized != n.names.localized;
    if names {
        b.loss(
            r,
            &format!("{prefix}.names"),
            "Some supplied names cannot be represented exactly",
        )?;
    }
    let mut metrics = false;
    macro_rules! metric { ($($field:ident),*) => { $(metrics |= f.metrics.$field.is_some() && f.metrics.$field != n.metrics.$field;)* }; }
    metric!(
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
        metrics_set
    );
    metrics |= f.metrics.glyph_defaults != n.metrics.glyph_defaults;
    if metrics {
        b.loss(
            r,
            &format!("{prefix}.font_metrics"),
            "Supplied font metrics or default identity changed in the target representation",
        )?;
    }
    if f.charset != crate::model::FontCharset::default() && f.charset != n.charset {
        b.loss(
            r,
            &format!("{prefix}.charset"),
            "Declared charset information is not stored by this wire format",
        )?;
    }
    // Source properties must occur in the same order; newly added native fields
    // do not themselves imply loss. Linear scan also handles duplicate keys.
    let mut target = n.properties.iter();
    if !f.properties.iter().all(|p| target.by_ref().any(|q| p == q))
        || (!f.headers.is_empty() && f.headers != n.headers)
    {
        b.loss(
            r,
            &format!("{prefix}.properties"),
            "Source properties, raw records or headers changed or were omitted",
        )?;
    }
    if !same_glyphs(f, n) {
        b.loss(r, &format!("{prefix}.glyphs"), "Glyph order, mapping identity, geometry, bitmap depth, metrics, names, sequences or properties changed")?;
    }
    Ok(())
}
fn same_glyphs(f: &Font, n: &Font) -> bool {
    f.slot_count() == n.slot_count() && f.len() == n.len() && f.glyphs().zip(n.glyphs()).all(|((id, g), (jd, h))| {
        if id != jd || g.codepoints != h.codepoints || g.bitmap != h.bitmap || g.metrics.bounds != h.metrics.bounds || g.name.is_some() && g.name != h.name || g.properties != h.properties || g.unicode_sequences != h.unicode_sequences { return false; }
        let mut same = true;
        macro_rules! field { ($($v:ident),*) => { $(same &= g.metrics.$v.is_none() || g.metrics.$v == h.metrics.$v;)* }; }
        field!(advance, scalable_advance, vertical_advance, vertical_scalable_advance, vertical_origin, abc, left_bearing, right_bearing, image_offset);
        same
    })
}

pub(in crate::formats) fn slice(data: &[u8], at: usize, length: usize) -> Result<&[u8]> {
    data.get(
        at..at
            .checked_add(length)
            .ok_or_else(|| invalid("offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated range"))
}
#[cfg(any(feature = "fbit", feature = "lisa", feature = "qpf", feature = "qpf2"))]
pub(in crate::formats) fn u16be(data: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        slice(data, at, 2)?.try_into().expect("two bytes"),
    ))
}
#[cfg(any(feature = "fbit", feature = "lisa", feature = "qpf2"))]
pub(in crate::formats) fn u32be(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        slice(data, at, 4)?.try_into().expect("four bytes"),
    ))
}
