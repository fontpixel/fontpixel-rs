//! Shared bounded cell-font support (HEX/PSF); not a public file format.
use crate::bitmap::{Bitmap, BitmapLimits, Bounds};
use crate::model::{
    Font, FontCharset, FontMetrics, FontNames, Glyph, GlyphBitmap, GlyphMetrics, Spacing, Vector,
};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};
use std::borrow::Cow;

pub(super) fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
pub(super) fn limit(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}
pub(super) fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(limit(resource, max))
    } else {
        Ok(())
    }
}
pub(super) struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    pub fn new(input_bytes: usize, limits: &ResourceLimits) -> Result<Self> {
        let mut out = Self {
            used: 0,
            max: limits.max_total_bytes,
        };
        out.add(input_bytes)?;
        Ok(out)
    }
    pub fn add(&mut self, bytes: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or(limit("console model bytes", self.max))?;
        check(self.used, self.max, "console model bytes")
    }
}
pub(super) fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
        max_dimension: u32::MAX,
    }
}
pub(super) fn cell_size(w: u32, h: u32, l: &ResourceLimits) -> Result<usize> {
    if w == 0 || h == 0 {
        return Err(invalid("console cell dimensions must be positive"));
    }
    let pixels = u64::from(w) * u64::from(h);
    if pixels > l.max_pixels_per_glyph as u64 {
        return Err(limit("console glyph pixels", l.max_pixels_per_glyph));
    }
    let n = usize::try_from(u64::from(w).div_ceil(8) * u64::from(h))
        .map_err(|_| limit("console glyph bytes", l.max_total_bytes))?;
    check(n, l.max_total_bytes, "console glyph bytes")?;
    Ok(n)
}
pub(super) fn metrics(w: u32, h: u32, descent: u32) -> FontMetrics {
    FontMetrics {
        pixel_size: Some(h),
        point_size: Some(crate::model::Rational::integer(i64::from(h))),
        resolution: Some(Vector::new(72, 72)),
        ascent: Some(i64::from(h) - i64::from(descent)),
        descent: Some(i64::from(descent)),
        bounds: Some(Bounds::new(w, h, 0, -i64::from(descent))),
        spacing: Some(Spacing::CharacterCell),
        ..Default::default()
    }
}
pub(super) fn glyph_metrics(cell: Bounds) -> GlyphMetrics {
    GlyphMetrics {
        bounds: cell,
        advance: Some(Vector::new(i64::from(cell.width), 0)),
        ..Default::default()
    }
}
pub(super) fn diag(
    r: &mut Report,
    format: &str,
    code: &str,
    impact: Impact,
    index: Option<usize>,
    cp: Option<u32>,
    message: impl Into<String>,
) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact,
        code: format!("{format}.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: index,
            codepoint: cp,
            asset: None,
        },
        message: message.into(),
    });
}
pub(super) fn font_losses(
    font: &Font,
    expected: &FontMetrics,
    format: &str,
    r: &mut Report,
    keep_comments: bool,
    unicode: bool,
) {
    if font.names != FontNames::default() {
        diag(
            r,
            format,
            "write.names",
            Impact::Loss,
            None,
            None,
            "Font names, copyright and license metadata have no target fields",
        );
    }
    let charset = FontCharset {
        name: Some(if unicode { "unicode" } else { "unknown" }.into()),
        ..Default::default()
    };
    if font.charset != FontCharset::default() && font.charset != charset {
        diag(
            r,
            format,
            "write.charset",
            Impact::Loss,
            None,
            None,
            "Source charset metadata has no target fields; mappings are not implicitly converted",
        );
    }
    for p in font.headers.iter() {
        if keep_comments
            && p.name == "hex.comment"
            && matches!(&p.value, crate::model::PropertyValue::Text(_))
        {
            continue;
        }
        diag(
            r,
            format,
            "write.property",
            Impact::Loss,
            None,
            None,
            format!("Property {:?} has no target field", p.name),
        );
    }
    for p in font.properties.iter() {
        diag(
            r,
            format,
            "write.property",
            Impact::Loss,
            None,
            None,
            format!("Property {:?} has no target field", p.name),
        );
    }
    macro_rules! field { ($($field:ident),* $(,)?) => { $(if font.metrics.$field.is_some() && font.metrics.$field!=expected.$field {
        diag(r,format,"write.font_metric",Impact::Loss,None,None,concat!("Font metric ",stringify!($field)," is not representable unchanged"));
    })* }; }
    field!(
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
    if font.metrics.glyph_defaults != GlyphMetrics::default() {
        diag(
            r,
            format,
            "write.glyph_defaults",
            Impact::Loss,
            None,
            None,
            "Font-level glyph defaults have no target fields",
        );
    }
}
pub(super) fn glyph_losses(
    g: &Glyph,
    expected: &GlyphMetrics,
    format: &str,
    index: usize,
    r: &mut Report,
) {
    if g.name.is_some() {
        diag(
            r,
            format,
            "write.glyph_name",
            Impact::Loss,
            Some(index),
            None,
            "Glyph name has no target field",
        );
    }
    for p in g.properties.iter() {
        diag(
            r,
            format,
            "write.glyph_property",
            Impact::Loss,
            Some(index),
            None,
            format!("Glyph property {:?} has no target field", p.name),
        );
    }
    if g.metrics.bounds != expected.bounds {
        diag(
            r,
            format,
            "write.bounds",
            Impact::Loss,
            Some(index),
            None,
            "Glyph bounds are normalized to the target cell",
        );
    }
    macro_rules! field { ($($field:ident),* $(,)?) => { $(if g.metrics.$field.is_some() && g.metrics.$field!=expected.$field {
        diag(r,format,"write.glyph_metric",Impact::Loss,Some(index),None,concat!("Glyph metric ",stringify!($field)," is not representable unchanged"));
    })* }; }
    field!(
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
pub(super) fn raster<'a>(
    g: &'a Glyph,
    cell: Bounds,
    threshold: Option<u8>,
    l: &ResourceLimits,
) -> Result<Cow<'a, Bitmap>> {
    cell_size(cell.width, cell.height, l)?;
    if let GlyphBitmap::Mono(b) = &g.bitmap
        && g.metrics.bounds == cell
    {
        return Ok(Cow::Borrowed(b));
    }
    let b = match &g.bitmap {
        GlyphBitmap::Mono(b) => {
            Bitmap::from_raw(b.width(), b.height(), b.as_bytes(), bitmap_limits(l))?
        }
        GlyphBitmap::Gray8(b) => {
            let t = threshold.ok_or_else(|| {
                invalid("Gray8 console output requires an explicit gray_threshold")
            })?;
            let b = crate::bitmap::Bitmap::<crate::bitmap::Gray8>::from_raw(
                b.width(),
                b.height(),
                b.as_bytes(),
                bitmap_limits(l),
            )?;
            b.to_mono(t)?
        }
    };
    let x = cell
        .x
        .checked_sub(g.metrics.bounds.x)
        .ok_or_else(|| invalid("cell x overflow"))?;
    let y = cell
        .y
        .checked_sub(g.metrics.bounds.y)
        .ok_or_else(|| invalid("cell y overflow"))?;
    Ok(Cow::Owned(b.crop_bounds(Bounds::new(
        cell.width,
        cell.height,
        x,
        y,
    ))?))
}
pub(super) struct Output {
    pub bytes: Vec<u8>,
    max: usize,
}
impl Output {
    pub fn new(max: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max,
        }
    }
    pub fn add(&mut self, b: &[u8]) -> Result<()> {
        let n = self
            .bytes
            .len()
            .checked_add(b.len())
            .ok_or(limit("console output bytes", self.max))?;
        check(n, self.max, "console output bytes")?;
        self.bytes
            .try_reserve(b.len())
            .map_err(|_| limit("console output allocation", self.max))?;
        self.bytes.extend_from_slice(b);
        Ok(())
    }
}
