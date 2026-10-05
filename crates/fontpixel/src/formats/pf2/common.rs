// Shared, feature-independent implementation details for W4.2 codecs.
use crate::bitmap::{Bitmap, BitmapLimits, Bounds};
use crate::charset::{CharsetCodec, canonical_charset};
#[cfg(any(feature = "fontx2", feature = "hzk"))]
use crate::model::Vector;
use crate::model::{
    Codepoint, Font, FontSet, Glyph, GlyphBitmap, GlyphId, GlyphMetrics, Properties,
};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};
use std::collections::BTreeMap;

pub(super) fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
pub(super) fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
pub(super) struct Budget {
    pub used: usize,
    max: usize,
}
impl Budget {
    pub fn new(retained: usize, count: usize, l: &ResourceLimits) -> Result<Self> {
        check(count, l.max_glyphs, "font glyph count")?;
        let mut b = Self {
            used: 0,
            max: l.max_total_bytes,
        };
        b.add(retained)?;
        b.add(count.checked_mul(2048).ok_or(Error::ResourceLimit {
            resource: "font model allocation",
            limit: l.max_total_bytes,
        })?)?;
        Ok(b)
    }
    pub fn add(&mut self, n: usize) -> Result<()> {
        self.used = self.used.checked_add(n).ok_or(Error::ResourceLimit {
            resource: "font aggregate bytes",
            limit: self.max,
        })?;
        check(self.used, self.max, "font aggregate bytes")
    }
}
pub(super) fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
        max_dimension: 65535,
    }
}
pub(super) struct Diagnostics {
    pub report: Report,
    format: &'static str,
    used: usize,
    max: usize,
    max_mappings: usize,
}
impl Diagnostics {
    pub fn new(format: &'static str, l: &ResourceLimits) -> Self {
        Self {
            report: Report::default(),
            format,
            max_mappings: l.max_glyphs,
            used: 0,
            max: l.max_total_bytes,
        }
    }
    pub fn for_font(format: &'static str, f: &Font, l: &ResourceLimits) -> Result<Self> {
        let mut d = Self::new(format, l);
        d.reserve(source_cost(f, l)?)?;
        Ok(d)
    }
    /// [`for_font`](Self::for_font), except that a model the caller has
    /// reserved is only validated, not charged.
    #[cfg(feature = "pf2")]
    pub fn for_font_charged(
        format: &'static str,
        f: &Font,
        l: &ResourceLimits,
        charge: super::ModelCharge,
    ) -> Result<Self> {
        if charge == super::ModelCharge::Estimate {
            return Self::for_font(format, f, l);
        }
        source_cost(f, &unbounded(l))?;
        Ok(Self::new(format, l))
    }
    pub fn reserve(&mut self, n: usize) -> Result<()> {
        self.used = self.used.saturating_add(n);
        check(self.used, self.max, "font aggregate writer bytes")
    }
    pub fn emit(
        &mut self,
        code: &str,
        impact: Impact,
        glyph: Option<usize>,
        cp: Option<u32>,
        message: impl Into<String>,
    ) -> Result<()> {
        let message = message.into();
        self.used = self
            .used
            .saturating_add(message.len().saturating_add(code.len()).saturating_add(512));
        check(self.used, self.max, "font diagnostics")?;
        self.report.push(ReportEntry {
            level: Level::Warning,
            impact,
            code: format!("{}.{}", self.format, code),
            location: Location {
                font_index: Some(0),
                glyph_index: glyph,
                codepoint: cp,
                asset: None,
            },
            message,
        });
        Ok(())
    }
    pub fn loss(
        &mut self,
        code: &str,
        glyph: Option<usize>,
        cp: Option<u32>,
        message: impl Into<String>,
    ) -> Result<()> {
        self.emit(code, Impact::Loss, glyph, cp, message)
    }
    pub fn fatal(mut self, message: impl Into<String>) -> Error {
        self.report.push(ReportEntry {
            level: Level::Error,
            impact: Impact::None,
            code: format!("{}.write.failed", self.format),
            location: Location {
                font_index: Some(0),
                ..Default::default()
            },
            message: message.into(),
        });
        Error::ReportedErrors(self.report)
    }
    pub fn cost(&self) -> usize {
        self.used
    }
}
pub(super) fn container(set: &FontSet, d: &mut Diagnostics) -> Result<()> {
    container_charged(set, d, super::ModelCharge::Estimate)
}
/// [`container`]; a reserved model's container properties are validated
/// but not charged.
pub(super) fn container_charged(
    set: &FontSet,
    d: &mut Diagnostics,
    charge: super::ModelCharge,
) -> Result<()> {
    let mut retained = Budget {
        used: 0,
        max: match charge {
            super::ModelCharge::Estimate => d.max,
            super::ModelCharge::Reserved => usize::MAX,
        },
    };
    property_cost(&set.properties, &mut retained)?;
    d.reserve(charge.of(retained.used))?;
    for p in set.properties.iter() {
        d.loss(
            "write.container_property",
            None,
            None,
            format!("Container property {:?} is not representable", p.name),
        )?;
    }
    Ok(())
}
pub(super) fn properties(
    source: &Properties,
    output: &Properties,
    kind: &str,
    id: Option<usize>,
    d: &mut Diagnostics,
) -> Result<()> {
    if source == output {
        return Ok(());
    }
    d.loss(
        "write.property_order",
        id,
        None,
        format!("{kind} ordered properties or duplicate multiplicity change"),
    )?;
    let mut destination = output.iter();
    for p in source.iter() {
        if destination.next() != Some(p) {
            d.loss(
                "write.property",
                id,
                None,
                format!(
                    "{kind} property {:?} is changed, reordered, or not retained",
                    p.name
                ),
            )?;
        }
    }
    Ok(())
}
pub(super) fn metadata(f: &Font, out: &Font, d: &mut Diagnostics) -> Result<()> {
    if f.source_format != out.source_format {
        d.loss(
            "write.source_format",
            None,
            None,
            "Source format provenance changes",
        )?;
    }
    if f.slot_count() != f.len() {
        d.loss(
            "write.removed_slots",
            None,
            None,
            "Removed glyph slots cannot be retained in the target glyph table",
        )?;
    }
    macro_rules! compare { ($group:ident,$($field:ident),*) => { $(if f.$group.$field != out.$group.$field { d.loss(concat!("write.",stringify!($group),".",stringify!($field)),None,None,concat!(stringify!($group),".",stringify!($field)," changes in the target representation"))?; })* }; }
    compare!(
        names,
        family,
        style,
        full,
        postscript,
        version,
        copyright,
        license,
        license_url,
        foundry,
        xlfd,
        xlfd_fields
    );
    properties(
        &f.names.localized,
        &out.names.localized,
        "Localized name",
        None,
        d,
    )?;
    compare!(
        metrics,
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
    if f.charset != out.charset {
        d.loss(
            "write.charset",
            None,
            None,
            "Charset identity or source charset metadata changes",
        )?;
    }
    properties(&f.headers, &out.headers, "Header", None, d)?;
    properties(&f.properties, &out.properties, "Font", None, d)
}
pub(super) fn glyph_metadata(
    g: &Glyph,
    out: &GlyphMetrics,
    id: usize,
    d: &mut Diagnostics,
) -> Result<()> {
    macro_rules! compare { ($($field:ident),*) => { $(if g.metrics.$field != out.$field { d.loss(concat!("write.glyph_metric.",stringify!($field)),Some(id),None,concat!("Glyph metric ",stringify!($field)," changes"))?; })* }; }
    compare!(
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
    if g.name.is_some() {
        d.loss(
            "write.glyph_name",
            Some(id),
            None,
            "Glyph name is not representable",
        )?;
    }
    for seq in &g.unicode_sequences {
        d.loss(
            "write.sequence",
            Some(id),
            seq.first().copied(),
            "Unicode sequence is not representable",
        )?;
    }
    properties(&g.properties, &Properties::default(), "Glyph", Some(id), d)
}
pub(super) fn mono(
    g: &Glyph,
    cell: Option<Bounds>,
    threshold: Option<u8>,
    id: usize,
    d: &mut Diagnostics,
) -> Result<Bitmap> {
    let bm = if let Some(cell) = cell {
        g.bitmap_in_cell(cell)?
    } else {
        g.bitmap.clone()
    };
    match bm {
        GlyphBitmap::Mono(b) => Ok(b),
        GlyphBitmap::Gray8(b) => {
            let t = threshold
                .ok_or_else(|| invalid("Gray input requires an explicit gray_threshold"))?;
            d.loss(
                "write.gray_threshold",
                Some(id),
                None,
                format!("Grayscale quantized at threshold {t}"),
            )?;
            b.to_mono(t)
        }
    }
}
pub(super) fn exact_codec(name: &str, gl: Option<bool>) -> Result<CharsetCodec> {
    if name.is_empty() {
        return Err(invalid("An unnamed Legacy charset cannot be converted"));
    }
    let codec = CharsetCodec::new(name, gl).map_err(|e| invalid(e.to_string()))?;
    if !codec.known() {
        return Err(invalid(format!(
            "Unknown charset {name:?}; an exact mapping is required"
        )));
    }
    Ok(codec)
}
struct Mapper {
    direct: bool,
    unicode_target: bool,
    native_pf2: bool,
    source: std::result::Result<CharsetCodec, String>,
    target: std::result::Result<CharsetCodec, String>,
}
impl Mapper {
    fn new(f: &Font, target: &str) -> Self {
        let key = canonical_charset(target);
        let source = f.charset.canonical_name();
        Self {
            direct: source == key && f.charset.gl != Some(true),
            unicode_target: key == "unicode",
            native_pf2: f.source_format.as_deref() == Some("pf2"),
            source: exact_codec(&source, f.charset.gl).map_err(|e| e.to_string()),
            target: exact_codec(&key, Some(false)).map_err(|e| e.to_string()),
        }
    }
    fn map(&self, cp: Codepoint) -> std::result::Result<Option<u32>, &str> {
        match cp {
            Codepoint::Legacy(v) if self.direct => Ok(Some(v)),
            Codepoint::Unicode(v) if self.unicode_target => {
                Ok((char::from_u32(v).is_some() || self.native_pf2 && joined_code(v)).then_some(v))
            }
            _ => {
                let unicode = match cp {
                    Codepoint::Unicode(v) => Some(v),
                    Codepoint::Legacy(v) => {
                        self.source.as_ref().map_err(String::as_str)?.cp2unicode(v)
                    }
                };
                if self.unicode_target {
                    return Ok(unicode.filter(|&v| char::from_u32(v).is_some()));
                }
                let codec = self.target.as_ref().map_err(String::as_str)?;
                Ok(unicode.and_then(|v| codec.unicode2cp(v)))
            }
        }
    }
}
pub(super) fn mappings(
    f: &Font,
    target: &str,
    accept: impl Fn(u32) -> bool,
    d: &mut Diagnostics,
) -> Result<BTreeMap<u32, GlyphId>> {
    d.reserve(
        1024 + target.len().saturating_mul(4)
            + f.charset
                .name
                .as_ref()
                .map_or(0, |s| s.len().saturating_mul(4)),
    )?;
    let mapper = Mapper::new(f, target);
    let mut map = BTreeMap::new();
    for (id, g) in f.glyphs() {
        if g.codepoints.is_empty() {
            d.loss(
                "write.omitted",
                Some(id.index()),
                None,
                "Unencoded glyph cannot be stored",
            )?;
        }
        for &cp in &g.codepoints {
            let result = mapper.map(cp);
            let mapped = match result {
                Ok(value) => value.filter(|&v| accept(v)),
                Err(e) => {
                    d.loss(
                        "write.unmapped",
                        Some(id.index()),
                        Some(cp.value()),
                        e.to_string(),
                    )?;
                    None
                }
            };
            if let Some(code) = mapped {
                if !map.contains_key(&code) {
                    check(
                        map.len().saturating_add(1),
                        d.max_mappings,
                        "output mapping count",
                    )?;
                    d.reserve(192)?;
                }
                if let Some(old) = map.insert(code, id)
                    && old != id
                {
                    d.loss(
                        "write.duplicate",
                        Some(old.index()),
                        Some(code),
                        "Later glyph replaces this duplicate mapping",
                    )?;
                }
                if cp != Codepoint::Legacy(code) && target != "unicode"
                    || target == "unicode" && cp != Codepoint::Unicode(code)
                {
                    d.loss(
                        "write.codepoint",
                        Some(id.index()),
                        Some(cp.value()),
                        format!("Code mapping becomes {target} 0x{code:X}"),
                    )?;
                }
            } else {
                d.loss(
                    "write.omitted",
                    Some(id.index()),
                    Some(cp.value()),
                    format!("Code is outside the selected {target} repertoire"),
                )?;
            }
        }
    }
    Ok(map)
}
#[cfg(any(feature = "fontx2", feature = "hzk"))]
pub(super) fn cell_metrics(width: u32, height: u32) -> GlyphMetrics {
    GlyphMetrics {
        bounds: Bounds::new(width, height, 0, 0),
        advance: Some(Vector::new(i64::from(width), 0)),
        ..Default::default()
    }
}
#[cfg(any(feature = "fontx2", feature = "hzk"))]
pub(super) fn padding_nonzero(raw: &[u8], width: u32, height: u32) -> bool {
    let stride = (width as usize).div_ceil(8);
    !width.is_multiple_of(8)
        && stride != 0
        && raw
            .chunks_exact(stride)
            .take(height as usize)
            .any(|r| r[stride - 1] & ((1 << (8 - width % 8)) - 1) != 0)
}
// Shift whole row bytes between continuous and row-aligned MSB-first packing.
#[cfg(feature = "pf2")]
pub(super) fn unpack(raw: &[u8], w: u32, h: u32, l: &ResourceLimits) -> Result<Bitmap> {
    let stride = (w as usize).div_ceil(8);
    let mut rows = vec![0; stride * h as usize];
    for y in 0..h as usize {
        let start = y * w as usize;
        let shift = start % 8;
        for x in 0..stride {
            let index = start / 8 + x;
            rows[y * stride + x] = raw.get(index).copied().unwrap_or(0) << shift
                | if shift == 0 {
                    0
                } else {
                    raw.get(index + 1).copied().unwrap_or(0) >> (8 - shift)
                };
        }
    }
    Bitmap::from_raw(w, h, &rows, bitmap_limits(l))
}
#[cfg(feature = "pf2")]
pub(super) fn pack(b: &Bitmap) -> Vec<u8> {
    let w = b.width() as usize;
    let stride = w.div_ceil(8);
    let mut raw = vec![0; (w * b.height() as usize).div_ceil(8)];
    for y in 0..b.height() as usize {
        let start = y * w;
        let shift = start % 8;
        for x in 0..stride {
            let v = b.as_bytes()[y * stride + x];
            let at = start / 8 + x;
            if let Some(out) = raw.get_mut(at) {
                *out |= v >> shift;
            }
            if shift != 0
                && let Some(out) = raw.get_mut(at + 1)
            {
                *out |= v << (8 - shift);
            }
        }
    }
    raw
}
#[cfg(feature = "hzk")]
pub(super) fn transpose(raw: &[u8], w: u32, h: u32) -> Vec<u8> {
    let source_stride = (w as usize).div_ceil(8);
    let target_stride = (h as usize).div_ceil(8);
    let mut out = vec![0; target_stride * w as usize];
    for y in 0..h as usize {
        for byte in 0..source_stride {
            let mut value = raw[y * source_stride + byte];
            while value != 0 {
                let bit = value.leading_zeros() as usize;
                let x = byte * 8 + bit;
                if x < w as usize {
                    out[x * target_stride + y / 8] |= 128 >> (y % 8);
                }
                value &= !(128 >> bit);
            }
        }
    }
    out
}

#[cfg(any(feature = "fontx2", feature = "hzk"))]
pub(super) fn cell_font_metrics(width: u32, height: u32) -> crate::model::FontMetrics {
    crate::model::FontMetrics {
        bounds: Some(cell_metrics(width, height).bounds),
        pixel_size: Some(height),
        point_size: Some(crate::model::Rational::integer(i64::from(height))),
        resolution: Some(Vector::new(72, 72)),
        ascent: Some(i64::from(height)),
        descent: Some(0),
        ..Default::default()
    }
}

pub(super) fn joined_code(code: u32) -> bool {
    code & 0xc000_0000 != 0 && char::from_u32(code & 0x3fff_ffff).is_some()
}

/// `l` without its aggregate byte cap, for validating a reserved model's
/// estimate without charging it.
#[cfg(feature = "pf2")]
fn unbounded(l: &ResourceLimits) -> ResourceLimits {
    ResourceLimits {
        max_total_bytes: usize::MAX,
        ..l.clone()
    }
}
// Allocation-free source accounting includes retained glyph slots, cmap entries,
// strings, aliases, sequences, raw records, and recursively structured properties.
pub(super) fn source_cost(f: &Font, l: &ResourceLimits) -> Result<usize> {
    let mut b = Budget::new(0, 0, l)?;
    b.add(
        std::mem::size_of::<Font>()
            + f.slot_count()
                .saturating_mul(std::mem::size_of::<Glyph>().saturating_mul(2) + 128),
    )?;
    for (_, g) in f.glyphs() {
        b.add(
            g.bitmap.as_bytes().len().saturating_mul(2)
                + g.codepoints.capacity().saturating_mul(64)
                + g.name
                    .as_ref()
                    .map_or(0, |s| s.capacity().saturating_mul(4)),
        )?;
        b.add(g.unicode_sequences.capacity().saturating_mul(48))?;
        for seq in &g.unicode_sequences {
            b.add(seq.capacity().saturating_mul(16))?;
        }
        property_cost(&g.properties, &mut b)?;
    }
    for name in [
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
        &f.charset.registry,
        &f.charset.encoding,
        &f.charset.name,
        &f.source_format,
    ]
    .into_iter()
    .flatten()
    {
        b.add(name.capacity().saturating_mul(4))?;
    }
    b.add(
        f.names.xlfd_fields.capacity().saturating_mul(48)
            + f.charset.code_ranges.capacity().saturating_mul(16),
    )?;
    for s in &f.names.xlfd_fields {
        b.add(s.capacity().saturating_mul(4))?;
    }
    for props in [&f.names.localized, &f.headers, &f.properties] {
        property_cost(props, &mut b)?;
    }
    Ok(b.used)
}
fn property_cost(p: &Properties, b: &mut Budget) -> Result<()> {
    b.add(
        p.len()
            .saturating_mul(std::mem::size_of::<crate::model::Property>())
            .saturating_mul(2),
    )?;
    for p in p.iter() {
        b.add(
            p.name.capacity().saturating_mul(4)
                + p.raw.as_ref().map_or(0, |r| r.capacity().saturating_mul(2)),
        )?;
        value_cost(&p.value, 0, b)?;
    }
    Ok(())
}
fn value_cost(value: &crate::model::PropertyValue, depth: usize, b: &mut Budget) -> Result<()> {
    use crate::model::PropertyValue;
    if depth > 64 {
        return Err(invalid("Property nesting exceeds 64 levels"));
    }
    match value {
        PropertyValue::Text(s) | PropertyValue::Decimal(s) => b.add(s.capacity().saturating_mul(4)),
        PropertyValue::Bytes(v) => b.add(v.capacity().saturating_mul(2)),
        PropertyValue::List(v) => {
            b.add(
                v.capacity()
                    .saturating_mul(std::mem::size_of::<PropertyValue>())
                    .saturating_mul(2),
            )?;
            for item in v {
                value_cost(item, depth + 1, b)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
