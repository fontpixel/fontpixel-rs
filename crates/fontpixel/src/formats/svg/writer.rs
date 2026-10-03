use super::*;
use crate::{
    Font,
    bitmap::Bounds,
    model::{Codepoint, GlyphBitmap, Vector},
};
use std::fmt::Write as _;
pub(super) struct Text {
    pub value: String,
    max: usize,
}
impl Text {
    pub fn new(max: usize) -> Self {
        Self {
            value: String::new(),
            max,
        }
    }
    pub fn add(&mut self, s: &str) -> Result<()> {
        bound(
            self.value.len().saturating_add(s.len()),
            self.max,
            "SVG output bytes",
        )?;
        self.value
            .try_reserve(s.len())
            .map_err(|_| invalid("output allocation failed"))?;
        self.value.push_str(s);
        Ok(())
    }
    pub fn attr(&mut self, name: &str, value: &str) -> Result<()> {
        self.add(" ")?;
        self.add(name)?;
        self.add("=\"")?;
        for c in value.chars() {
            if !valid_char(c) {
                return Err(invalid("XML 1.0 forbidden character"));
            }
            match c {
                '&' => self.add("&amp;")?,
                '<' => self.add("&lt;")?,
                '>' => self.add("&gt;")?,
                '"' => self.add("&quot;")?,
                '\n' => self.add("&#10;")?,
                '\r' => self.add("&#13;")?,
                '\t' => self.add("&#9;")?,
                _ => self.add(c.encode_utf8(&mut [0; 4]))?,
            }
        }
        self.add("\"")
    }
}
/// Deterministic upward-facing path data with origin in pixel units.
/// True circular dots use SVG arcs; squares reuse the SPEC contour engine.
pub fn path_data(
    bitmap: &Bitmap,
    origin: (i32, i32),
    units_per_pixel: u16,
    shape: Shape,
    dot_size: f64,
    limits: &outline::Limits,
    max_bytes: usize,
) -> Result<String> {
    validate_shape(shape, dot_size)?;
    if units_per_pixel == 0 {
        return Err(invalid("units per pixel must be positive"));
    }
    let mut out = Text::new(max_bytes);
    let scale = f64::from(units_per_pixel);
    if shape == Shape::Square {
        let cs = outline::contours(
            bitmap,
            &outline::Options {
                shape,
                dot_size,
                units_per_pixel: i32::from(units_per_pixel),
                origin,
            },
            limits,
        )?;
        for c in cs {
            if let Some(p) = c.first() {
                out.add(&format!("M{} {}", p.0, p.1))?;
                let mut prev = *p;
                for p in c.iter().skip(1) {
                    out.add(&if p.1 == prev.1 {
                        format!("H{}", p.0)
                    } else {
                        format!("V{}", p.1)
                    })?;
                    prev = *p;
                }
                out.add("Z")?;
            }
        }
    } else {
        let o = outline::Options {
            shape: Shape::SquareSeparate,
            units_per_pixel: i32::from(units_per_pixel),
            dot_size: 1.,
            origin,
        };
        let size = dot_size * scale;
        let inset = (scale - size) / 2.;
        for (x, y) in outline::pixel_origins(bitmap, &o, limits)? {
            if shape == Shape::Dot {
                let r = size / 2.;
                let cx = x + scale / 2.;
                let cy = y + scale / 2.;
                out.add(&format!(
                    "M{} {cy}a{r} {r} 0 1 0 {size} 0a{r} {r} 0 1 0 -{size} 0Z",
                    cx - r
                ))?;
            } else {
                out.add(&format!(
                    "M{} {}h{size}v{size}h-{size}Z",
                    x + inset,
                    y + inset
                ))?;
            }
        }
    }
    Ok(out.value)
}
/// Standalone SVG image with merged contours or true circular pixels.
/// Disjoint paper and ink paths preserve palette alpha, including transparent ink.
pub fn image(bitmap: &Bitmap, o: &ImageOptions) -> Result<String> {
    validate_shape(o.shape, o.dot_size)?;
    if o.image.scale == 0 {
        return Err(invalid("image scale must be positive"));
    }
    let w = bitmap
        .width()
        .checked_mul(o.image.scale)
        .ok_or_else(|| invalid("image width overflow"))?;
    let h = bitmap
        .height()
        .checked_mul(o.image.scale)
        .ok_or_else(|| invalid("image height overflow"))?;
    o.image.limits.check(
        w,
        h,
        (u64::from(w).div_ceil(8) * u64::from(h))
            .try_into()
            .map_err(|_| invalid("image size overflow"))?,
    )?;
    let d = path_data(
        bitmap,
        (0, 0),
        1,
        o.shape,
        o.dot_size,
        &o.limits,
        o.image.limits.max_bytes,
    )?;
    let mut out = Text::new(o.image.limits.max_bytes);
    out.add(&format!("<svg xmlns=\"{NS}\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {} {}\">\n<g transform=\"translate(0 {}) scale(1 -1)\">\n",bitmap.width(),bitmap.height(),bitmap.height()))?;
    for (background, color) in [(true, o.image.background), (false, o.image.foreground)] {
        if color.alpha == 0 {
            continue;
        }
        out.add("<path")?;
        out.attr(
            "fill",
            &format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue),
        )?;
        out.attr("fill-opacity", &(f64::from(color.alpha) / 255.).to_string())?;
        out.add(" fill-rule=\"evenodd\" d=\"")?;
        if background {
            out.add(&format!("M0 0h{}v{}H0Z", bitmap.width(), bitmap.height()))?;
        }
        out.add(&d)?;
        out.add("\"/>\n")?;
    }
    out.add("</g>\n</svg>\n")?;
    Ok(out.value)
}
/// Export one glyph's original bitmap rectangle, retaining padding and y direction.
/// Origin/bearings are font-layout metrics; the image viewBox is rectangle-local.
pub fn glyph_image(glyph: &Glyph, o: &ImageOptions, gray_threshold: Option<u8>) -> Result<String> {
    glyph.validate()?;
    image(&mono(glyph, gray_threshold)?, o)
}
fn bbox(b: Bounds) -> String {
    format!("{} {} {} {}", b.width, b.height, b.x, b.y)
}
fn scaled(v: i64, ppu: u16) -> Result<String> {
    let v = v
        .checked_mul(i64::from(ppu))
        .ok_or_else(|| invalid("scaled metric overflow"))?;
    if v.unsigned_abs() > 1_000_000_000 {
        return Err(invalid("scaled metric exceeds coordinate limit"));
    }
    Ok(v.to_string())
}
fn vec_attr(out: &mut Text, key: &str, v: Option<Vector>) -> Result<()> {
    if let Some(v) = v {
        out.attr(key, &format!("{} {}", v.x, v.y))?;
    }
    Ok(())
}
fn font_loss(f: &Font, r: &mut Report, fi: usize) {
    if !f.properties.is_empty()
        || !f.headers.is_empty()
        || !f.names.localized.is_empty()
        || !f.names.xlfd_fields.is_empty()
    {
        loss(
            r,
            "source-metadata",
            "Raw source properties, headers and localized/XLFD records are omitted",
            Some(fi),
            None,
        );
    }
    let m = &f.metrics;
    if m.default_character.is_some() {
        loss(
            r,
            "default-character",
            "Default character code is represented as an effective missing glyph",
            Some(fi),
            None,
        );
    }
    if m.point_size.is_some()
        || m.resolution.is_some()
        || m.metrics_set.is_some()
        || m.ppem_x.is_some()
        || m.ppem_y.is_some()
    {
        loss(
            r,
            "strike-metadata",
            "Point size, resolution, metrics-set and separate axis ppem metadata are omitted",
            Some(fi),
            None,
        );
    }
    if f.charset != Default::default() {
        loss(
            r,
            "charset",
            "Mappings are projected to Unicode; source charset identity is omitted",
            Some(fi),
            None,
        );
    }
    if m.glyph_defaults != Default::default() {
        loss(
            r,
            "defaults",
            "Font-level glyph defaults are resolved into effective glyph metrics",
            Some(fi),
            None,
        );
    }
}
/// Encode a complete ordered font set as standalone SVG 1.1 path fonts.
pub fn write(fonts: &FontSet, o: &WriteOptions) -> Result<(Vec<u8>, Report)> {
    validate_shape(o.shape, o.dot_size)?;
    if o.units_per_pixel == 0 {
        return Err(invalid("units per pixel must be positive"));
    }
    bound(fonts.fonts.len(), o.limits.max_fonts, "SVG fonts")?;
    if fonts.fonts.is_empty() {
        return Err(invalid("SVG font set is empty"));
    }
    fonts.validate(&o.limits.resources)?;
    let mut r = Report::default();
    let mut out = Text::new(o.max_output_bytes);
    let (mut total_pixels, mut total_glyphs, mut total_edges) = (0usize, 0usize, 0usize);
    if !fonts.properties.is_empty() {
        loss(
            &mut r,
            "container-metadata",
            "Container properties are omitted",
            None,
            None,
        );
    }
    out.add(&format!(
        "<svg xmlns=\"{NS}\" xmlns:fp=\"{FP}\" version=\"1.1\">\n<defs>\n"
    ))?;
    for (fi, f) in fonts.fonts.iter().enumerate() {
        font_loss(f, &mut r, fi);
        charge(
            &mut total_glyphs,
            f.len(),
            o.limits.resources.max_glyphs,
            "SVG total glyphs",
        )?;
        let ppem = f
            .metrics
            .pixel_size
            .or(f.metrics.ppem_y)
            .unwrap_or_else(|| f.metrics.bounds.map_or(16, |b| b.height.max(1)));
        if ppem == 0 || ppem > 4096 {
            return Err(invalid("native pixel size must be 1..4096"));
        }
        let ppu = o.units_per_pixel;
        out.add("<font")?;
        out.attr("id", &format!("fontpixel-{fi}"))?;
        out.attr(
            "horiz-adv-x",
            &scaled(
                f.metrics
                    .glyph_defaults
                    .advance
                    .map_or(i64::from(ppem), |v| v.x),
                ppu,
            )?,
        )?;
        out.attr("fp:ppem", &ppem.to_string())?;
        if let Some(b) = f.metrics.bounds {
            out.attr("fp:bounds", &bbox(b))?;
        }
        for (k, v) in [
            ("line-gap", f.metrics.line_gap),
            ("line-height", f.metrics.line_height),
            ("space-width", f.metrics.space_width),
        ] {
            if let Some(v) = v {
                out.attr(&format!("fp:{k}"), &v.to_string())?;
            }
        }
        if let Some(s) = f.metrics.spacing {
            out.attr(
                "fp:spacing",
                match s {
                    crate::model::Spacing::Proportional => "p",
                    crate::model::Spacing::Monospaced => "m",
                    crate::model::Spacing::CharacterCell => "c",
                },
            )?;
        }
        out.add(">\n<font-face")?;
        out.attr("units-per-em", &scaled(i64::from(ppem), ppu)?)?;
        for (k, v) in [
            ("font-family", &f.names.family),
            ("font-style", &f.names.style),
            ("fp:full", &f.names.full),
            ("fp:postscript", &f.names.postscript),
            ("fp:version", &f.names.version),
            ("fp:copyright", &f.names.copyright),
            ("fp:license", &f.names.license),
            ("fp:license-url", &f.names.license_url),
            ("fp:foundry", &f.names.foundry),
            ("fp:xlfd", &f.names.xlfd),
        ] {
            if let Some(v) = v {
                out.attr(k, v)?;
            }
        }
        for (k, v) in [
            ("ascent", f.metrics.ascent),
            ("descent", f.metrics.descent),
            ("underline-position", f.metrics.underline_position),
            ("underline-thickness", f.metrics.underline_thickness),
        ] {
            if let Some(v) = v {
                out.attr(k, &scaled(v, ppu)?)?;
            }
        }
        out.add("/>\n")?;
        let default = f
            .metrics
            .default_glyph
            .or_else(|| f.metrics.default_character.and_then(|cp| f.glyph_id(cp)));
        let codec = f.charset.codec();
        for (gid, g) in f.glyphs() {
            let gi = gid.index();
            let b = g.metrics.bounds;
            charge(
                &mut total_pixels,
                (b.width as usize)
                    .saturating_mul(b.height as usize)
                    .saturating_add(b.height as usize),
                o.limits.max_total_pixels,
                "SVG total pixels",
            )?;
            if !g.properties.is_empty()
                || g.metrics.scalable_advance.is_some()
                || g.metrics.vertical_scalable_advance.is_some()
                || g.metrics.abc.is_some()
                || g.metrics.left_bearing.is_some()
                || g.metrics.right_bearing.is_some()
                || g.metrics.image_offset.is_some()
            {
                loss(
                    &mut r,
                    "glyph-metadata",
                    "Source properties and scalable/ABC/independent bearing/image metrics are omitted",
                    Some(fi),
                    Some(gi),
                );
            }
            if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
                loss(
                    &mut r,
                    "gray-threshold",
                    "Grayscale converted with the explicit threshold",
                    Some(fi),
                    Some(gi),
                );
            }
            let bitmap = mono(g, o.gray_threshold)?;
            let origin = (
                b.x.try_into().map_err(|_| invalid("glyph x exceeds i32"))?,
                b.y.try_into().map_err(|_| invalid("glyph y exceeds i32"))?,
            );
            scaled(b.x + i64::from(b.width), ppu)?;
            scaled(b.y + i64::from(b.height), ppu)?;
            scaled(b.x, ppu)?;
            scaled(b.y, ppu)?;
            let d = path_data(
                &bitmap,
                origin,
                ppu,
                o.shape,
                o.dot_size,
                &o.limits.geometry,
                o.max_output_bytes,
            )?;
            charge(
                &mut total_edges,
                d.bytes().filter(u8::is_ascii_alphabetic).count(),
                o.limits.max_total_edges,
                "SVG total generated commands",
            )?;
            let mut common = Text::new(o.max_output_bytes);
            common.attr("fp:id", &gi.to_string())?;
            common.attr("fp:bounds", &bbox(b))?;
            if let Some(n) = &g.name {
                common.attr("glyph-name", n)?;
            }
            let advance = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .unwrap_or(Vector::new(i64::from(ppem), 0));
            common.attr("horiz-adv-x", &scaled(advance.x, ppu)?)?;
            if advance.y != 0 {
                common.attr("fp:advance-y", &advance.y.to_string())?;
            }
            let vertical = g
                .metrics
                .vertical_advance
                .or(f.metrics.glyph_defaults.vertical_advance);
            if let Some(v) = vertical {
                common.attr(
                    "vert-adv-y",
                    &scaled(
                        v.y.checked_neg()
                            .ok_or_else(|| invalid("vertical advance overflow"))?,
                        ppu,
                    )?,
                )?;
            }
            vec_attr(&mut common, "fp:vertical-advance", vertical)?;
            if let Some(v) = g
                .metrics
                .vertical_origin
                .or(f.metrics.glyph_defaults.vertical_origin)
            {
                common.attr("vert-origin-x", &scaled(v.x, ppu)?)?;
                common.attr("vert-origin-y", &scaled(v.y, ppu)?)?;
            }
            common.attr("d", &d)?;
            let mut mappings = Vec::new();
            for cp in &g.codepoints {
                let ch = match cp {
                    Codepoint::Unicode(v) => char::from_u32(*v),
                    Codepoint::Legacy(v) => codec.as_ref().map_err(invalid)?.cp2char(*v),
                };
                if let Some(c) = ch.filter(|c| valid_char(*c)) {
                    mappings.push((c.to_string(), false));
                } else {
                    loss(
                        &mut r,
                        "unrepresentable-code",
                        "Code cannot be represented in XML 1.0 Unicode",
                        Some(fi),
                        Some(gi),
                    );
                }
            }
            for seq in &g.unicode_sequences {
                let s = seq
                    .iter()
                    .copied()
                    .map(char::from_u32)
                    .collect::<Option<String>>();
                if let Some(s) = s.filter(|s| s.chars().all(valid_char)) {
                    mappings.push((s, true));
                } else {
                    loss(
                        &mut r,
                        "unrepresentable-sequence",
                        "Sequence cannot be represented in XML 1.0 Unicode",
                        Some(fi),
                        Some(gi),
                    );
                }
            }
            let additional =
                mappings.len().max(1).saturating_sub(1) + usize::from(default == Some(gid));
            charge(
                &mut total_glyphs,
                additional,
                o.limits.resources.max_glyphs,
                "SVG emitted glyph records",
            )?;
            if default == Some(gid) {
                out.add("<missing-glyph")?;
                out.add(&common.value)?;
                out.add("/>\n")?;
            }
            if mappings.is_empty() {
                out.add("<glyph")?;
                out.add(&common.value)?;
                out.add("/>\n")?;
            }
            for (s, sequence) in mappings {
                out.add("<glyph")?;
                out.add(&common.value)?;
                if sequence {
                    out.attr("fp:mapping", "sequence")?;
                }
                out.add(" unicode=\"")?;
                for c in s.chars() {
                    let mut n = String::new();
                    write!(&mut n, "&#x{:x};", c as u32).map_err(invalid)?;
                    out.add(&n)?;
                }
                out.add("\"/>\n")?;
            }
        }
        out.add("</font>\n")?;
    }
    out.add("</defs>\n</svg>\n")?;
    Ok((out.value.into_bytes(), r))
}
