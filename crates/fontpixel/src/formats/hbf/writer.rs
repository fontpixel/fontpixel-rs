use super::*;
use crate::model::{GlyphBitmap, Properties, Property, PropertyValue, Spacing};
use crate::resources::{OutputAsset, OutputBundle, ResourcePath};
use std::fmt::Write as _;

/// Deterministic HBF header and bitmap-asset choices.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HbfWriteOptions {
    /// Bitmap name relative to the header directory; default is the header stem plus `.bin`.
    pub bitmap_name: Option<ResourcePath>,
    /// Maximum glyphs per bitmap asset; `None` writes one file. Must be positive.
    pub glyphs_per_file: Option<usize>,
    /// Threshold for Gray8 conversion; `None` refuses grayscale input.
    pub gray_threshold: Option<u8>,
    /// Glyph, pixel, metadata and aggregate output limits.
    pub limits: ResourceLimits,
}
struct Text {
    value: String,
    max: usize,
}
impl std::fmt::Write for Text {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if s.len() > self.max.saturating_sub(self.value.len()) {
            return Err(std::fmt::Error);
        }
        self.value
            .try_reserve(s.len())
            .map_err(|_| std::fmt::Error)?;
        self.value.push_str(s);
        Ok(())
    }
}
fn ascii(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii() && !b.is_ascii_control())
}
fn key(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !s.starts_with("HBF_")
        && !matches!(s, "COMMENT" | "ENDPROPERTIES" | "STARTPROPERTIES")
}
fn space(font: &Font, max: u32) -> Result<header::Space> {
    let get = |name: &str| -> Result<Vec<u8>> {
        let Some(value) = font.headers.get(name) else {
            return Ok(Vec::new());
        };
        let PropertyValue::List(ranges) = value else {
            return Err(invalid("invalid native byte-range metadata"));
        };
        if ranges.is_empty() || ranges.len() > 256 {
            return Err(invalid("invalid native byte ranges"));
        }
        let mut bytes = Vec::new();
        let mut prev = None;
        for r in ranges {
            let PropertyValue::List(pair) = r else {
                return Err(invalid("invalid native byte range"));
            };
            if pair.len() != 2 {
                return Err(invalid("invalid native byte range"));
            }
            let n = |p: &PropertyValue| match p {
                PropertyValue::Integer(n) => {
                    u8::try_from(*n).map_err(|_| invalid("invalid byte range"))
                }
                PropertyValue::Unsigned(n) => {
                    u8::try_from(*n).map_err(|_| invalid("invalid byte range"))
                }
                _ => Err(invalid("invalid byte range")),
            };
            let (a, b) = (n(&pair[0])?, n(&pair[1])?);
            if a > b || prev.is_some_and(|p| a <= p) {
                return Err(invalid("unordered native byte ranges"));
            }
            bytes.extend(a..=b);
            prev = Some(b);
        }
        Ok(bytes)
    };
    let mut second = get("hbf.byte2")?;
    let mut third = get("hbf.byte3")?;
    if max > 0xffff && third.is_empty() {
        third.extend(0..=255);
    }
    if (max > 255 || !third.is_empty()) && second.is_empty() {
        second.extend(0..=255);
    }
    Ok(header::Space { second, third })
}
fn scheme(font: &Font, unicode: bool) -> Result<String> {
    if let Some(PropertyValue::Text(s)) = font.headers.get("hbf.code_scheme") {
        let (name, is_unicode) = metadata::scheme(s);
        if is_unicode == unicode
            && font
                .charset
                .name
                .as_deref()
                .is_none_or(|n| n.eq_ignore_ascii_case(name))
        {
            return Ok(s.clone());
        }
    }
    if unicode {
        return Ok("Unicode".into());
    }
    let name = font
        .charset
        .name
        .as_deref()
        .or(font.charset.registry.as_deref())
        .ok_or_else(|| invalid("legacy output needs an explicit charset"))?;
    Ok(match name.to_ascii_lowercase().as_str() {
        "gb2312" => "GB2312-1980",
        "jisx0208" => "JISX0208-1990",
        "ksc5601" | "euc-kr" => "KSC5601-1987",
        "big5" => "Big5",
        _ => name,
    }
    .into())
}
fn cloned_properties(font: &Font, budget: &mut Budget, report: &mut Report) -> Result<Properties> {
    let mut props = Properties::default();
    for p in font.properties.iter() {
        let supported = key(&p.name)
            && match &p.value {
                PropertyValue::Text(s) => ascii(s),
                PropertyValue::Integer(_) | PropertyValue::Unsigned(_) => true,
                _ => false,
            };
        if !supported {
            budget.add(256 + p.name.len())?;
            loss(
                report,
                "property",
                None,
                format!(
                    "Property {:?} has no HBF integer/string representation",
                    p.name
                ),
            );
            continue;
        }
        budget.add(
            256 + p.name.len()
                + match &p.value {
                    PropertyValue::Text(s) => s
                        .len()
                        .checked_mul(4)
                        .ok_or(limit("HBF metadata bytes", budget.max))?,
                    _ => 0,
                },
        )?;
        if p.raw.is_some() {
            loss(
                report,
                "property_raw",
                None,
                "HBF regenerates properties and cannot retain opaque raw property records",
            );
        }
        if matches!(p.value,PropertyValue::Unsigned(n) if n<=i64::MAX as u64) {
            loss(
                report,
                "property_type",
                None,
                "HBF integers within i64 range read back as signed Integer values",
            );
        }
        props.push(Property::new(p.name.clone(), p.value.clone()));
    }
    Ok(props)
}
fn sync_text(props: &mut Properties, key: &str, value: &Option<String>) -> Result<()> {
    if let Some(s) = value {
        if !ascii(s) {
            return Err(invalid(format!("{key} is not printable ASCII")));
        }
        props.set(key, s.as_str());
    } else {
        props.remove(key);
    }
    Ok(())
}
fn sync_number(props: &mut Properties, key: &str, value: Option<i64>) {
    if let Some(n) = value {
        props.set(key, n);
    } else {
        props.remove(key);
    }
}
fn metadata_losses(font: &Font, projected: &Font, report: &mut Report) {
    // Compare complete structures. Only absent source fields synthesized by the
    // format's required/default fields are ignored; no existing value is erased.
    let mut target_metrics = projected.metrics.clone();
    if font.metrics.point_size.is_none() {
        target_metrics.point_size = None;
    }
    if font.metrics.resolution.is_none() {
        target_metrics.resolution = None;
    }
    if font.metrics.bounds.is_none() {
        target_metrics.bounds = None;
    }
    if font.metrics.ascent.is_none() {
        target_metrics.ascent = None;
    }
    if font.metrics.descent.is_none() {
        target_metrics.descent = None;
    }
    if font.metrics.default_character.is_none() {
        target_metrics.default_character = None;
    }
    if font.metrics != target_metrics {
        loss(
            report,
            "metadata",
            None,
            "HBF cannot retain all independent font metrics (including ppem, glyph defaults, line spacing, and stable default-glyph IDs)",
        );
    }
    let mut target_names = projected.names.clone();
    if font.names.xlfd.is_none() {
        target_names.xlfd = None;
    }
    if font.names != target_names {
        loss(
            report,
            "names",
            None,
            "HBF cannot retain all font names (including localized names and separately parsed XLFD fields)",
        );
    }
    let mut source_charset = font.charset.clone();
    let mut target_charset = projected.charset.clone();
    // Range partitioning is transport layout; the writer checks every glyph code.
    source_charset.code_ranges.clear();
    target_charset.code_ranges.clear();
    if source_charset.name.is_none() {
        target_charset.name = None;
    }
    if source_charset.gl.is_none() {
        target_charset.gl = None;
    }
    if source_charset != target_charset {
        loss(
            report,
            "charset",
            None,
            "HBF cannot retain all charset metadata (including separate Windows code-page fields)",
        );
    }
    for p in font.headers.iter() {
        if p.raw.is_some() {
            loss(
                report,
                "header_raw",
                None,
                "HBF regenerates headers and cannot retain opaque raw header records",
            );
        }
        if !matches!(
            p.name.as_str(),
            "hbf.comment" | "hbf.code_scheme" | "hbf.byte2" | "hbf.byte3"
        ) {
            loss(
                report,
                "header",
                None,
                format!("Header {:?} has no HBF field", p.name),
            );
        }
    }
}

fn byte_ranges(bytes: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for &b in bytes {
        if let Some((_, last)) = out.last_mut()
            && *last != 255
            && b == *last + 1
        {
            *last = b;
            continue;
        }
        out.push((b, b));
    }
    out
}
/// Encode a uniform monochrome font and bounded bitmap sidecars. The caller owns persistence.
pub fn write(font: &Font, primary: &ResourcePath, options: &HbfWriteOptions) -> Result<Written> {
    let l = &options.limits;
    font.validate(l)?;
    if font.is_empty() {
        return Err(invalid("cannot write an empty font"));
    }
    let mut budget = Budget::new(l.max_total_bytes);
    budget.add(
        font.len()
            .checked_mul(2048)
            .ok_or(limit("HBF model bytes", l.max_total_bytes))?,
    )?;
    let mut report = Report::default();
    for value in [
        &font.names.family,
        &font.names.style,
        &font.names.full,
        &font.names.postscript,
        &font.names.version,
        &font.names.copyright,
        &font.names.foundry,
        &font.names.license,
        &font.names.license_url,
        &font.names.xlfd,
        &font.charset.name,
        &font.charset.registry,
        &font.charset.encoding,
    ]
    .into_iter()
    .flatten()
    {
        budget.add(
            value
                .len()
                .checked_mul(6)
                .ok_or(limit("HBF metadata bytes", l.max_total_bytes))?,
        )?;
    }
    for p in font.headers.iter() {
        budget.add(256 + p.name.len())?;
        if let PropertyValue::Text(s) = &p.value {
            budget.add(
                s.len()
                    .checked_mul(4)
                    .ok_or(limit("HBF metadata bytes", l.max_total_bytes))?,
            )?;
        }
    }
    check(
        font.charset.code_ranges.len(),
        l.max_glyphs,
        "HBF code ranges",
    )?;
    budget.add(
        font.charset
            .code_ranges
            .len()
            .checked_mul(64)
            .ok_or(limit("HBF code ranges", l.max_total_bytes))?,
    )?;
    let range_starts: std::collections::BTreeSet<_> =
        font.charset.code_ranges.iter().map(|&(a, _)| a).collect();
    let mut glyphs: Vec<_> = font.glyphs().collect();
    let bitmap = glyphs[0].1.metrics.bounds;
    let size = cell_bytes(bitmap, l)?;
    let bounds = font.metrics.bounds.unwrap_or(bitmap);
    contained(bitmap, bounds)?;
    let mut unicode = None;
    for (id, g) in &glyphs {
        if g.metrics.bounds != bitmap {
            return Err(invalid("HBF requires uniform positive glyph bounds"));
        }
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
            if options.gray_threshold.is_none() {
                return Err(invalid(
                    "Gray8 HBF output requires an explicit gray_threshold",
                ));
            }
            loss(
                &mut report,
                "gray_threshold",
                Some(id.index()),
                "Gray8 pixels are thresholded to monochrome",
            );
        }
        if g.codepoints.len() != 1 || !g.unicode_sequences.is_empty() {
            return Err(invalid(
                "HBF requires exactly one code per glyph and no Unicode sequences",
            ));
        }
        let cp = g.codepoints[0];
        let is_unicode = matches!(cp, Codepoint::Unicode(_));
        if cp.value() > 0xffffff
            || (is_unicode && cp.scalar().is_none())
            || unicode.is_some_and(|u| u != is_unicode)
        {
            return Err(invalid("invalid or mixed-domain HBF codepoints"));
        }
        unicode = Some(is_unicode);
        if g.metrics != glyph_metrics(bitmap, bounds) {
            loss(
                &mut report,
                "glyph_metrics",
                Some(id.index()),
                "HBF keeps only the common bitmap bounds and font-box-width advance",
            );
        }
        if g.name.is_some() {
            loss(
                &mut report,
                "glyph_name",
                Some(id.index()),
                "HBF has no glyph names",
            );
        }
        if !g.properties.is_empty() {
            loss(
                &mut report,
                "glyph_properties",
                Some(id.index()),
                "HBF has no per-glyph properties",
            );
        }
    }
    glyphs.sort_by_key(|(_, g)| g.codepoints[0].value());
    if glyphs
        .windows(2)
        .any(|w| w[0].1.codepoints == w[1].1.codepoints)
    {
        return Err(invalid("duplicate codepoints cannot be stored in HBF"));
    }
    if font.slot_count() != font.len()
        || glyphs
            .iter()
            .enumerate()
            .any(|(n, (id, _))| id.index() != n)
    {
        loss(
            &mut report,
            "glyph_order",
            None,
            "Glyphs are emitted in increasing code order with compact IDs",
        );
    }
    let space = space(
        font,
        glyphs
            .last()
            .ok_or_else(|| invalid("empty font"))?
            .1
            .codepoints[0]
            .value(),
    )?;
    for (_, g) in &glyphs {
        space.rank(g.codepoints[0].value())?;
    }
    for name in ["hbf.code_scheme", "hbf.byte2", "hbf.byte3"] {
        if font.headers.iter().filter(|p| p.name == name).count() > 1 {
            return Err(invalid("duplicate native HBF structural header metadata"));
        }
    }
    if font
        .headers
        .get("hbf.code_scheme")
        .is_some_and(|v| !matches!(v, PropertyValue::Text(_)))
    {
        return Err(invalid("HBF code scheme metadata must be text"));
    }
    let mut scheme = scheme(font, unicode == Some(true))?.trim().to_owned();
    if scheme.eq_ignore_ascii_case("Unicode 1.1")
        && glyphs
            .last()
            .is_some_and(|(_, g)| g.codepoints[0].value() > 0xffff)
    {
        scheme = "Unicode".into();
    }
    if !ascii(&scheme) || scheme.is_empty() {
        return Err(invalid("invalid code scheme"));
    }
    let per_file = options.glyphs_per_file.unwrap_or(font.len());
    if per_file == 0 {
        return Err(invalid("glyphs_per_file must be positive"));
    }
    let files = font.len().div_ceil(per_file);
    check(files, l.max_resources, "HBF output assets")?;
    let header_dir = primary.as_str().rsplit_once('/').map_or("", |(dir, _)| dir);
    let filename = primary.as_str().rsplit('/').next().unwrap_or("font.hbf");
    let stem = filename.rsplit_once('.').map_or(filename, |(s, _)| s);
    let base = options
        .bitmap_name
        .as_ref()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| format!("{stem}.bin"));
    if !ascii(&base) || base.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(invalid("bitmap filename must be one printable ASCII token"));
    }
    let mut assets = Vec::new();
    let mut ranges = Vec::new();
    budget.add(
        glyphs
            .len()
            .checked_mul(size)
            .ok_or(limit("HBF bitmap bytes", l.max_total_bytes))?,
    )?;
    for (file_index, chunk) in glyphs.chunks(per_file).enumerate() {
        let name = if files == 1 {
            base.clone()
        } else {
            let (suffix, ext) = base.rsplit_once('.').unwrap_or((&base, "bin"));
            format!("{suffix}-{:04}.{ext}", file_index + 1)
        };
        let relative = ResourcePath::new(name.clone())?;
        let path = ResourcePath::new(if header_dir.is_empty() {
            name
        } else {
            format!("{header_dir}/{name}")
        })?;
        let length = chunk
            .len()
            .checked_mul(size)
            .ok_or(limit("HBF bitmap bytes", l.max_resource_bytes))?;
        check(length, l.max_resource_bytes, "HBF bitmap asset bytes")?;
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| limit("HBF bitmap allocation", l.max_total_bytes))?;
        let mut start = 0;
        for (i, (_, g)) in chunk.iter().enumerate() {
            let cp = g.codepoints[0].value();
            match &g.bitmap {
                GlyphBitmap::Mono(b) => data.extend_from_slice(b.as_bytes()),
                GlyphBitmap::Gray8(b) => {
                    let threshold = options
                        .gray_threshold
                        .ok_or_else(|| invalid("missing Gray8 threshold"))?;
                    for row in b.as_bytes().chunks_exact(bitmap.width as usize) {
                        for pixels in row.chunks(8) {
                            let mut byte = 0u8;
                            for (i, &pixel) in pixels.iter().enumerate() {
                                if pixel >= threshold {
                                    byte |= 128 >> i;
                                }
                            }
                            data.push(byte);
                        }
                    }
                }
            }
            let boundary = i + 1 == chunk.len()
                || space.rank(chunk[i + 1].1.codepoints[0].value())? != space.rank(cp)? + 1
                || (i + 1 < chunk.len()
                    && range_starts.contains(&chunk[i + 1].1.codepoints[0].value()));
            if boundary {
                budget.add(256 + relative.as_str().len())?;
                ranges.push(header::Range {
                    first: chunk[start].1.codepoints[0].value(),
                    last: cp,
                    path: relative.clone(),
                    offset: start * size,
                });
                start = i + 1;
            }
        }
        assets.push(OutputAsset { path, bytes: data });
    }
    let mut props = cloned_properties(font, &mut budget, &mut report)?;
    macro_rules! name {
        ($field:ident,$key:literal) => {
            sync_text(&mut props, $key, &font.names.$field)?;
        };
    }
    name!(family, "FAMILY_NAME");
    name!(style, "ADD_STYLE_NAME");
    name!(full, "FULL_NAME");
    name!(postscript, "POSTSCRIPT_NAME");
    name!(version, "FONT_VERSION");
    name!(copyright, "COPYRIGHT");
    name!(foundry, "FOUNDRY");
    name!(license, "LICENSE");
    name!(license_url, "LICENSE_URL");
    sync_text(&mut props, "CHARSET_REGISTRY", &font.charset.registry)?;
    sync_text(&mut props, "CHARSET_ENCODING", &font.charset.encoding)?;
    sync_number(
        &mut props,
        "PIXEL_SIZE",
        font.metrics.pixel_size.map(i64::from),
    );
    // Preserve absent inferred ascent/descent fields on native roundtrips.
    let ascent = bounds
        .y
        .checked_add(i64::from(bounds.height))
        .ok_or_else(|| invalid("ascent overflow"))?;
    let descent = bounds
        .y
        .checked_neg()
        .ok_or_else(|| invalid("descent overflow"))?;
    for (key, value, derived) in [
        ("FONT_ASCENT", font.metrics.ascent, ascent),
        ("FONT_DESCENT", font.metrics.descent, descent),
    ] {
        if props.get(key).is_some() || value.is_some_and(|v| v != derived) {
            sync_number(&mut props, key, value);
        }
    }
    sync_number(
        &mut props,
        "UNDERLINE_POSITION",
        font.metrics.underline_position,
    );
    sync_number(
        &mut props,
        "UNDERLINE_THICKNESS",
        font.metrics.underline_thickness,
    );
    if let Some(spacing) = font.metrics.spacing {
        let text = match spacing {
            Spacing::Proportional => "P",
            Spacing::Monospaced => "M",
            Spacing::CharacterCell => "C",
        };
        if !matches!(props.get("SPACING"),Some(PropertyValue::Text(s)) if s.eq_ignore_ascii_case(text))
        {
            props.set("SPACING", text);
        }
    } else {
        props.remove("SPACING");
    }
    let default = font
        .metrics
        .default_character
        .unwrap_or(glyphs[0].1.codepoints[0]);
    if matches!(default, Codepoint::Unicode(_)) != (unicode == Some(true)) {
        return Err(invalid(
            "default character encoding differs from glyph encoding",
        ));
    }
    props.set("DEFAULT_CHAR", i64::from(default.value()));
    let point = match font.metrics.point_size {
        Some(p) if p.denominator() == 1 && p.numerator() > 0 => p.numerator(),
        Some(_) => return Err(invalid("HBF SIZE needs a positive integral point size")),
        None => i64::from(font.metrics.pixel_size.unwrap_or(bounds.height)),
    };
    if point <= 0 {
        return Err(invalid("HBF SIZE needs a positive integral point size"));
    }
    let resolution = font.metrics.resolution.unwrap_or(Vector::new(72, 72));
    if resolution.x <= 0 || resolution.y <= 0 {
        return Err(invalid("HBF resolution must be positive"));
    }
    let fontname = font
        .names
        .xlfd
        .as_deref()
        .or(font.names.full.as_deref())
        .or(font.names.family.as_deref())
        .unwrap_or("fontpixel");
    if !ascii(fontname) || fontname.trim().is_empty() {
        return Err(invalid("invalid FONT name"));
    }
    let mut projected = Font::default();
    projected.properties = props.clone();
    projected.names.xlfd = Some(fontname.trim().into());
    projected.metrics.point_size = Some(crate::model::Rational::integer(point));
    projected.metrics.resolution = Some(resolution);
    projected.metrics.bounds = Some(bounds);
    metadata::project(
        &mut projected,
        &scheme,
        &ranges,
        bounds,
        &mut Report::default(),
    )?;
    for (_, g) in &glyphs {
        metadata::check_code(&projected, g.codepoints[0].value())?;
    }
    metadata_losses(font, &projected, &mut report);
    let mut out = Text {
        value: String::new(),
        max: budget.remaining().min(l.max_input_bytes),
    };
    let overflow = || limit("HBF header output bytes", l.max_total_bytes);
    writeln!(out,"HBF_START_FONT 1.1\nHBF_CODE_SCHEME {scheme}\nFONT {fontname}\nSIZE {point} {} {}\nHBF_BITMAP_BOUNDING_BOX {} {} {} {}\nFONTBOUNDINGBOX {} {} {} {}",resolution.x,resolution.y,bitmap.width,bitmap.height,bitmap.x,bitmap.y,bounds.width,bounds.height,bounds.x,bounds.y).map_err(|_|overflow())?;
    for p in font.headers.iter().filter(|p| p.name == "hbf.comment") {
        if let PropertyValue::Text(s) = &p.value
            && ascii(s)
        {
            if s != s.trim() {
                loss(
                    &mut report,
                    "comment_whitespace",
                    None,
                    "Leading/trailing comment whitespace is canonicalized",
                );
            }
            writeln!(out, "COMMENT {s}").map_err(|_| overflow())?;
        } else {
            loss(
                &mut report,
                "comment",
                None,
                "Non-ASCII or non-text comment cannot be written to HBF",
            );
        }
    }
    writeln!(out, "STARTPROPERTIES {}", props.len()).map_err(|_| overflow())?;
    for p in props.iter() {
        write!(out, "{} ", p.name).map_err(|_| overflow())?;
        match &p.value {
            PropertyValue::Integer(n) => write!(out, "{n}"),
            PropertyValue::Unsigned(n) => write!(out, "{n}"),
            PropertyValue::Text(s) => {
                out.write_char('"').map_err(|_| overflow())?;
                for ch in s.chars() {
                    out.write_char(ch).map_err(|_| overflow())?;
                    if ch == '"' {
                        out.write_char('"').map_err(|_| overflow())?;
                    }
                }
                out.write_char('"')
            }
            _ => return Err(invalid("unsupported property after filtering")),
        }
        .map_err(|_| overflow())?;
        out.write_char('\n').map_err(|_| overflow())?;
    }
    writeln!(out, "ENDPROPERTIES\nCHARS {}", glyphs.len()).map_err(|_| overflow())?;
    for (n, bytes) in [(2, &space.second), (3, &space.third)] {
        if bytes.is_empty() {
            continue;
        }
        let ranges = byte_ranges(bytes);
        writeln!(out, "HBF_START_BYTE_{n}_RANGES {}", ranges.len()).map_err(|_| overflow())?;
        for (a, b) in ranges {
            writeln!(out, "HBF_BYTE_{n}_RANGE 0x{a:02X}-0x{b:02X}").map_err(|_| overflow())?;
        }
        writeln!(out, "HBF_END_BYTE_{n}_RANGES").map_err(|_| overflow())?;
    }
    writeln!(out, "HBF_START_CODE_RANGES {}", ranges.len()).map_err(|_| overflow())?;
    for r in &ranges {
        writeln!(
            out,
            "HBF_CODE_RANGE 0x{:X}-0x{:X} {} {}",
            r.first, r.last, r.path, r.offset
        )
        .map_err(|_| overflow())?;
    }
    writeln!(out, "HBF_END_CODE_RANGES\nHBF_END_FONT").map_err(|_| overflow())?;
    for line in out.value.lines() {
        check(line.len(), l.max_table_bytes, "HBF output record bytes")?;
    }
    // A successful bounded write must also fit the matching reader's retained budget.
    let mut read_budget = Budget::new(l.max_total_bytes);
    read_budget.add(
        out.value
            .len()
            .checked_mul(33)
            .ok_or(limit("HBF roundtrip bytes", l.max_total_bytes))?,
    )?;
    read_budget.add(
        glyphs
            .len()
            .checked_mul(
                size.checked_mul(2)
                    .and_then(|n| n.checked_add(1536))
                    .ok_or(limit("HBF roundtrip bytes", l.max_total_bytes))?,
            )
            .ok_or(limit("HBF roundtrip bytes", l.max_total_bytes))?,
    )?;
    for asset in &assets {
        read_budget.add(128 + asset.path.as_str().len())?;
    }
    assets.push(OutputAsset {
        path: primary.clone(),
        bytes: out.value.into_bytes(),
    });
    Ok(Written {
        output: OutputBundle::new(primary.clone(), assets)?,
        report,
    })
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::defaults;

    #[test]
    fn options_serde_defaults_and_rejections() {
        defaults::<HbfWriteOptions>();
    }
}
