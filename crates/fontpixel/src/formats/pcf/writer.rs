use super::*;
use crate::formats::x11;
use crate::model::{Codepoint, GlyphId, Rational};
use std::collections::BTreeMap;

struct WireGlyph<'a> {
    source: &'a Glyph,
    id: GlyphId,
    metric: Metric,
    bits: Vec<u8>,
    name: Vec<u8>,
    swidth: i64,
    ink: Option<Metric>,
}
struct Info {
    min: Metric,
    max: Metric,
    imin: Metric,
    imax: Metric,
    overlap: i32,
    direction: u8,
    ascent: i32,
    descent: i32,
    flags: [u8; 6],
}
impl Info {
    fn new(ascent: i32, descent: i32) -> Self {
        let (min, max) = initial_bounds();
        Self {
            min,
            max,
            imin: min,
            imax: max,
            overlap: -32768,
            direction: 0,
            ascent,
            descent,
            flags: [0; 6],
        }
    }
    fn could_terminal(&self) -> bool {
        let (mn, mx) = (&self.min, &self.max);
        mn[0] >= 0
            && mx[1] <= mx[2]
            && mn[2] == mx[2]
            && mx[3] <= self.ascent
            && mx[4] <= self.descent
            && (mx[0] != 0 || mn[1] != mn[2] || mn[3] != self.ascent || mn[4] != self.descent)
            && !(mx[3] == 0 && mx[4] == 0)
    }
    fn accelerate(&mut self) {
        let (mn, mx) = (&self.min, &self.max);
        self.flags = [
            u8::from(self.overlap <= mn[0]),
            u8::from(mn == mx),
            u8::from(
                mn == mx
                    && mx[0] == 0
                    && mx[1] == mx[2]
                    && mx[3] == self.ascent
                    && mx[4] == self.descent,
            ),
            u8::from(mn[2] == mx[2]),
            u8::from(
                mn[0] >= 0
                    && self.overlap <= 0
                    && mn[3] >= -self.descent
                    && mx[3] <= self.ascent
                    && -mn[4] <= self.ascent
                    && mx[4] <= self.descent,
            ),
            0,
        ];
    }
}
fn compute_bounds(
    glyphs: &[WireGlyph<'_>],
    encoding: &BTreeMap<u32, usize>,
    all: &mut Info,
    enc: &mut Info,
) {
    let mut neg = 0usize;
    let mut pos = 0usize;
    for (info, indices) in [
        (all, (0..glyphs.len()).collect::<Vec<_>>()),
        (enc, encoding.values().copied().collect()),
    ] {
        (info.min, info.max) = initial_bounds();
        info.overlap = -32768;
        for index in indices {
            let m = glyphs[index].metric;
            minmax(&mut info.min, &mut info.max, &m);
            if m[2] < 0 {
                neg += 1;
            } else {
                pos += 1;
            }
            info.overlap = info.overlap.max(m[1] - m[2]);
        }
        info.direction = u8::from(neg > pos);
    }
}
fn check_prop(name: &str, value: i64) -> Result<()> {
    if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&value) {
        return Err(invalid(format!(
            "PCF: property {name} value {value} does not fit in 32 bits"
        )));
    }
    Ok(())
}
fn check16(name: &str, value: i64) -> Result<i32> {
    i16::try_from(value).map(i32::from).map_err(|_| {
        invalid(format!(
            "Glyph \"{name}\": metric value {value} does not fit in 16 bits"
        ))
    })
}
fn rational(v: Option<Rational>) -> f64 {
    v.map_or(0.0, |v| v.numerator() as f64 / f64::from(v.denominator()))
}
fn token(name: &str) -> Vec<u8> {
    name.as_bytes()
        .split(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0xb | 0xc))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .iter()
        .copied()
        .take(99)
        .collect()
}
fn effective_properties(font: &Font, report: &mut Report) -> Result<Properties> {
    let (_, mut properties) = x11::reconcile(font, false)?;
    let original = x11::original(font);
    // PCF's FONT property is also the normalized XLFD. A semantic edit must
    // supersede its old property, while a property-only edit still wins otherwise.
    if font.names.xlfd != original.names.xlfd && properties.get("FONT").is_some() {
        properties.set("FONT", font.names.xlfd.clone().unwrap_or_default());
    }
    if font.metrics.point_size != original.metrics.point_size
        && properties.get("POINT_SIZE").is_some()
    {
        let point = ((rational(font.metrics.point_size) as f32) as f64 * 10.0).trunc() as i64;
        properties.set("POINT_SIZE", point);
    }
    if font.metrics.resolution != original.metrics.resolution {
        let r = font.metrics.resolution.unwrap_or(Vector::new(75, 75));
        let x = if r.x < 1 { 75 } else { r.x };
        let y = if r.y < 1 { 75 } else { r.y };
        if properties.get("RESOLUTION_X").is_some() {
            properties.set("RESOLUTION_X", x);
        }
        if properties.get("RESOLUTION_Y").is_some() {
            properties.set("RESOLUTION_Y", y);
        }
        if x == y && properties.get("RESOLUTION").is_some() {
            properties.set("RESOLUTION", (x as f64 * 100.0 / 72.27).trunc() as i64);
        }
    }
    if font.source_format.as_deref() != Some("bdf") {
        return Ok(properties);
    }
    let mut unique = Vec::<Property>::new();
    let mut positions = BTreeMap::new();
    for p in properties.into_vec() {
        let key = p.name.to_lowercase();
        if let Some(&i) = positions.get(&key) {
            loss(
                report,
                "duplicate_property",
                None,
                format!(
                    "Repeated property {:?} collapsed to its last value and spelling",
                    p.name
                ),
            );
            unique[i] = p;
        } else {
            positions.insert(key, unique.len());
            unique.push(p);
        }
    }
    Ok(unique.into())
}
fn code_in_font(cp: Codepoint, font: &Font) -> Result<Option<u32>> {
    if matches!(cp, Codepoint::Legacy(_)) || x11::known_unicode(font) {
        return Ok(Some(cp.value()));
    }
    let Some(ch) = cp.scalar() else {
        return Ok(None);
    };
    Ok(font
        .charset
        .codec()
        .map_err(|e| invalid(e.to_string()))?
        .char2cp(ch))
}
fn model_losses(font: &Font, report: &mut Report) {
    let m = &font.metrics;
    let defaults = &m.glyph_defaults;
    for (present, field) in [
        (m.ppem_x.is_some(), "ppem_x"),
        (m.ppem_y.is_some(), "ppem_y"),
        (m.default_glyph.is_some(), "default_glyph ID"),
        (
            defaults.advance.is_some() || defaults.scalable_advance.is_some(),
            "font-level metric inheritance (materialized on glyphs)",
        ),
        (
            defaults.vertical_advance.is_some()
                || defaults.vertical_scalable_advance.is_some()
                || defaults.vertical_origin.is_some(),
            "vertical font defaults",
        ),
        (m.metrics_set.is_some_and(|v| v != 0), "METRICSSET"),
        (
            defaults.advance.is_some_and(|v| v.y != 0)
                || defaults.scalable_advance.is_some_and(|v| v.y != 0),
            "font-default y advances",
        ),
        (
            defaults.abc.is_some()
                || defaults.left_bearing.is_some()
                || defaults.right_bearing.is_some()
                || defaults.image_offset.is_some()
                || defaults.bounds != Bounds::default(),
            "independent font-default bounds/bearings",
        ),
        (
            !font.names.xlfd_fields.is_empty()
                && font
                    .names
                    .xlfd
                    .as_ref()
                    .map(|s| {
                        s.strip_prefix('-')
                            .unwrap_or(s)
                            .split('-')
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .as_ref()
                    != Some(&font.names.xlfd_fields),
            "independently edited XLFD fields",
        ),
        (!font.names.localized.is_empty(), "localized names"),
        (
            font.charset.windows_charset.is_some()
                || font.charset.code_page.is_some()
                || !font.charset.code_ranges.is_empty(),
            "charset extra fields",
        ),
        (
            font.charset.gl.is_some_and(|gl| {
                Some(gl)
                    != font
                        .headers
                        .get(x11::SOURCE_GL)
                        .and_then(integer)
                        .map(|v| v != 0)
            }),
            "charset GL override",
        ),
    ] {
        if present {
            loss(
                report,
                "font_field",
                None,
                format!("PCF cannot store {field}"),
            );
        }
    }
    if let Some(v) = font.headers.get("STARTFONT") {
        let version = match v {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => s.parse::<f64>().ok(),
            PropertyValue::Integer(v) => Some(*v as f64),
            _ => None,
        };
        if version != Some(2.1) {
            loss(
                report,
                "bdf_version",
                None,
                "PCF reconstructs BDF version 2.1; the source version cannot be retained".into(),
            );
        }
    }
    for p in font.headers.iter() {
        if p.raw.is_none()
            && (p.name.starts_with("bdf.")
                || [
                    "pcf.accelerators",
                    "pcf.bdf_accelerators",
                    "pcf.byte_order",
                    "pcf.default_char",
                ]
                .contains(&p.name.as_str()))
        {
            continue;
        }
        if ![
            "STARTFONT",
            "FONT",
            "SIZE",
            "FONTBOUNDINGBOX",
            "METRICSSET",
            "SWIDTH",
            "DWIDTH",
        ]
        .contains(&p.name.as_str())
        {
            loss(
                report,
                "header",
                None,
                format!("PCF cannot retain header {:?}", p.name),
            );
        }
    }
}
fn glyph_losses(g: &Glyph, id: GlyphId, report: &mut Report) {
    let m = &g.metrics;
    for (present, field) in [
        (!g.unicode_sequences.is_empty(), "Unicode sequences"),
        (m.advance.is_some_and(|v| v.y != 0), "DWIDTH y"),
        (m.scalable_advance.is_some_and(|v| v.y != 0), "SWIDTH y"),
        (
            m.vertical_advance.is_some()
                || m.vertical_scalable_advance.is_some()
                || m.vertical_origin.is_some(),
            "vertical metrics",
        ),
        (
            m.abc.is_some()
                || m.left_bearing.is_some()
                || m.right_bearing.is_some()
                || m.image_offset.is_some(),
            "independent bearing/ABC/image metrics",
        ),
    ] {
        if present {
            loss(
                report,
                "glyph_field",
                Some(id.index()),
                format!("PCF cannot store {field}"),
            );
        }
    }
    if g.properties
        .get(x11::ENCODING)
        .and_then(integer)
        .is_some_and(|n| n != -1)
    {
        loss(
            report,
            "negative_encoding",
            Some(id.index()),
            "Negative BDF source encoding cannot be retained in PCF".into(),
        );
    }
    if g.codepoints.is_empty() && g.properties.get(x11::ALT).and_then(integer).is_some() {
        loss(report,"alternate_encoding",Some(id.index()),"BDF alternate encoding becomes a PCF mapping or unencoded glyph; the alternate marker is lost".into());
    }
    for p in g.properties.iter() {
        if p.raw.is_none()
            && [
                x11::ROWS,
                x11::IMAGE,
                x11::ENCODING,
                x11::ALT,
                "pcf.attributes",
                "pcf.ink_metrics",
            ]
            .contains(&p.name.as_str())
        {
            continue;
        }
        loss(
            report,
            "glyph_property",
            Some(id.index()),
            format!("PCF cannot store glyph property {:?}", p.name),
        );
    }
}
fn build_bits(
    g: &WireGlyph<'_>,
    pad: usize,
    options: &PcfWriteOptions,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let width = (g.metric[1] - g.metric[0]) as usize;
    let height = (g.metric[3] + g.metric[4]) as usize;
    let stride = row_bytes(width, pad);
    let len = stride * height;
    bound(len, options.limits.max_table_bytes, "PCF glyph bytes")?;
    let mut bits = vec![0; len];
    if width == 0 || height == 0 {
        return Ok(bits);
    }
    let nb = width.div_ceil(8);
    if x11::unchanged_bitmap(g.source)
        && let Some(rows) = x11::source_rows(g.source)
    {
        if x11::validate_bitmap(g.source).is_err() {
            loss(
                report,
                "lenient_hex",
                Some(g.id.index()),
                "Invalid BDF hexadecimal nibbles interpreted using bdftopcf's lenient rule".into(),
            );
        }
        for (r, line) in rows.take(height).enumerate() {
            // Python str.strip also treats the ASCII information separators as whitespace.
            let raw = line
                .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                .as_bytes();
            let count = raw.len().div_ceil(2).min(nb);
            for i in 0..count {
                let mut byte = 0u8;
                for c in [raw[2 * i], raw.get(2 * i + 1).copied().unwrap_or(b'0')] {
                    if let Some(n) = char::from(c).to_digit(16) {
                        byte = byte.wrapping_shl(4).wrapping_add(n as u8);
                    }
                }
                bits[r * stride + i] = byte;
            }
        }
    } else {
        let mono;
        let b = match &g.source.bitmap {
            GlyphBitmap::Mono(b) => b,
            GlyphBitmap::Gray8(b) => {
                mono = b.to_mono(options.gray_threshold.ok_or_else(|| {
                    invalid("PCF cannot write Gray8 without an explicit gray_threshold")
                })?)?;
                loss(
                    report,
                    "grayscale",
                    Some(g.id.index()),
                    "Gray8 explicitly thresholded to one-bit pixels".into(),
                );
                &mono
            }
        };
        for (r, row) in b.as_bytes().chunks_exact(nb).enumerate() {
            bits[r * stride..r * stride + nb].copy_from_slice(row);
        }
    }
    if !width.is_multiple_of(8) {
        let mask = 0xff << (8 - width % 8);
        for r in 0..height {
            bits[r * stride + nb - 1] &= mask;
        }
    }
    Ok(bits)
}
fn reshape(g: &WireGlyph<'_>, new: &Metric, pad: usize) -> Vec<u8> {
    let src = g.metric;
    let row = row_bytes((new[1] - new[0]) as usize, pad);
    let oldrow = row_bytes((src[1] - src[0]) as usize, pad);
    let mut out = vec![0; row * (new[3] + new[4]) as usize];
    for y in (-src[3]).max(-new[3])..src[4].min(new[4]) {
        for x in src[0].max(new[0])..src[1].min(new[1]) {
            let xs = (x - src[0]) as usize;
            let xd = (x - new[0]) as usize;
            if g.bits[(y + src[3]) as usize * oldrow + xs / 8] & (0x80 >> (xs % 8)) != 0 {
                out[(y + new[3]) as usize * row + xd / 8] |= 0x80 >> (xd % 8);
            }
        }
    }
    out
}
fn ink(g: &WireGlyph<'_>, pad: usize) -> Metric {
    let m = g.metric;
    let w = (m[1] - m[0]) as usize;
    let h = (m[3] + m[4]) as usize;
    let row = row_bytes(w, pad);
    let mut left = w;
    let mut right = 0usize;
    let mut top = h;
    let mut bottom = 0usize;
    for y in 0..h {
        for x in 0..w.div_ceil(8) {
            let b = g.bits[y * row + x];
            if b != 0 {
                left = left.min(x * 8 + b.leading_zeros() as usize);
                right = right.max(x * 8 + 8 - b.trailing_zeros() as usize);
                top = top.min(y);
                bottom = bottom.max(y + 1);
            }
        }
    }
    if right == 0 {
        [m[0], m[0], m[2], 0, 0, m[5]]
    } else {
        [
            m[0] + left as i32,
            m[0] + right as i32,
            m[2],
            m[3] - top as i32,
            bottom as i32 - m[3],
            m[5],
        ]
    }
}

pub(super) fn write(font: &Font, options: &PcfWriteOptions) -> Result<(Vec<u8>, Report)> {
    let msb = options.byte_order == Order::Msb;
    let bitmsb = options.bit_order == Order::Msb;
    let pad = 1usize << options.glyph_pad.index();
    let scan = 1usize << options.scan_unit.index();
    if msb != bitmsb && scan > pad {
        return Err(invalid(
            "scanunit must not exceed glyphpad when bitorder != byteorder (bdftopcf output is undefined for that combination)",
        ));
    }
    let fmt = (options.scan_unit.index() << 4)
        | (u32::from(bitmsb) << 3)
        | (u32::from(msb) << 2)
        | options.glyph_pad.index();
    font.validate(&options.limits)?;
    let mut report = Report::default();
    model_losses(font, &mut report);
    let properties = effective_properties(font, &mut report)?;
    let mut plist = Vec::<Property>::new();
    let mut special = BTreeMap::<String, usize>::new();
    let mut asc = None;
    let mut desc = None;
    let mut default_ch = 0u16;
    for p in properties.iter() {
        if p.name.eq_ignore_ascii_case("COMMENT")
            || p.name.starts_with("COMMENT")
            || matches!(p.value, PropertyValue::Flag)
        {
            loss(
                &mut report,
                "property",
                None,
                format!("PCF omits comment/key-only property {:?}", p.name),
            );
            continue;
        }
        if let Some(value) = integer(&p.value) {
            check_prop(&p.name, value)?;
        } else if let PropertyValue::Unsigned(value) = p.value {
            return Err(invalid(format!(
                "PCF: property {} value {value} does not fit in 32 bits",
                p.name
            )));
        }
        match p.name.as_str() {
            "FONT_ASCENT" => {
                asc = integer(&p.value);
                if asc.is_none() {
                    loss(
                        &mut report,
                        "property",
                        None,
                        "Noninteger FONT_ASCENT is omitted".into(),
                    );
                }
                continue;
            }
            "FONT_DESCENT" => {
                desc = integer(&p.value);
                if desc.is_none() {
                    loss(
                        &mut report,
                        "property",
                        None,
                        "Noninteger FONT_DESCENT is omitted".into(),
                    );
                }
                continue;
            }
            "DEFAULT_CHAR" => {
                if let Some(v) = integer(&p.value) {
                    default_ch = v as u16;
                    if !(0..=65535).contains(&v) {
                        loss(
                            &mut report,
                            "default_character",
                            None,
                            "DEFAULT_CHAR truncated to 16 bits".into(),
                        );
                    }
                } else {
                    loss(
                        &mut report,
                        "property",
                        None,
                        "Noninteger DEFAULT_CHAR is omitted".into(),
                    );
                }
                continue;
            }
            _ => {}
        }
        let mut p = p.clone();
        if matches!(p.value, PropertyValue::Bytes(_) | PropertyValue::List(_)) {
            loss(
                &mut report,
                "property_type",
                None,
                format!("PCF cannot store compound/binary property {:?}", p.name),
            );
            continue;
        }
        if let PropertyValue::Decimal(s) = &p.value {
            p.value = PropertyValue::Text(s.clone());
            loss(
                &mut report,
                "decimal_property",
                None,
                format!("Decimal property {:?} stored as text", p.name),
            );
        }
        if p.name.contains('\0') {
            loss(
                &mut report,
                "nul_string",
                None,
                format!("Property name {:?} contains a NUL", p.name),
            );
            p.name = p.name.split('\0').next().unwrap_or_default().into();
        }
        if let PropertyValue::Text(s) = &mut p.value
            && s.contains('\0')
        {
            loss(
                &mut report,
                "nul_string",
                None,
                format!("Property {:?} truncated at NUL", p.name),
            );
            s.truncate(s.find('\0').unwrap_or(s.len()));
        }
        special.insert(p.name.clone(), plist.len());
        plist.push(p);
    }
    let base = x11::original(font);
    let bounds = if font.metrics.bounds != base.metrics.bounds {
        font.metrics.bounds.unwrap_or_default()
    } else {
        x11::values(&font.headers, "FONTBOUNDINGBOX")
            .filter(|v| v.len() == 4)
            .map_or(font.metrics.bounds.unwrap_or_default(), |v| {
                Bounds::new(v[0] as u32, v[1] as u32, v[2], v[3])
            })
    };
    let size = x11::values(&font.headers, "SIZE").filter(|v| v.len() == 3);
    let point = if font.metrics.point_size != base.metrics.point_size {
        rational(font.metrics.point_size)
    } else {
        size.as_ref()
            .map_or(rational(font.metrics.point_size), |s| s[0] as f64)
    };
    let resolution = if font.metrics.resolution != base.metrics.resolution {
        font.metrics.resolution.unwrap_or(Vector::new(75, 75))
    } else {
        size.as_ref().map_or(
            font.metrics.resolution.unwrap_or(Vector::new(75, 75)),
            |s| Vector::new(s[1], s[2]),
        )
    };
    let xr = if resolution.x < 1 { 75 } else { resolution.x };
    let yr = if resolution.y < 1 { 75 } else { resolution.y };
    let fontname = if font.names.xlfd != base.names.xlfd {
        font.names.xlfd.clone().unwrap_or_default()
    } else {
        x11::prop_text(&font.headers, "FONT").unwrap_or_default()
    };
    let generated = [
        Property::new("POINT_SIZE", ((point as f32) as f64 * 10.0).trunc() as i64),
        Property::new("FONT", fontname.clone()),
        Property::new("WEIGHT", -1i64),
        Property::new("RESOLUTION", (xr as f64 * 100.0 / 72.27).trunc() as i64),
        Property::new("RESOLUTION_X", xr),
        Property::new("RESOLUTION_Y", yr),
        Property::new("X_HEIGHT", -1i64),
        Property::new("QUAD_WIDTH", -1i64),
    ];
    for p in generated {
        if !special.contains_key(&p.name) && (p.name != "RESOLUTION" || xr == yr) {
            special.insert(p.name.clone(), plist.len());
            plist.push(p);
        }
    }
    for p in &mut plist {
        if let Some(v) = integer(&p.value) {
            check_prop(&p.name, v)?;
            if v > i64::from(i32::MAX) {
                loss(
                    &mut report,
                    "unsigned_property",
                    None,
                    format!(
                        "Property {:?} is stored as a 32-bit word and reads back as signed",
                        p.name
                    ),
                );
            }
        }
        if let PropertyValue::Text(s) = &mut p.value
            && let Some(i) = s.find('\0')
        {
            loss(
                &mut report,
                "nul_string",
                None,
                format!("Property {:?} truncated at NUL", p.name),
            );
            s.truncate(i);
        }
    }
    let ascent = asc.unwrap_or(i64::from(bounds.height).saturating_add(bounds.y));
    let descent = desc.unwrap_or(bounds.y.saturating_neg());
    for v in [ascent, descent, ascent.saturating_add(descent)] {
        if i16::try_from(v).is_err() {
            return Err(invalid(format!(
                "PCF: FONT_ASCENT {ascent} / FONT_DESCENT {descent} do not fit in 16 bits"
            )));
        }
    }
    if font
        .headers
        .get("pcf.default_char")
        .and_then(integer)
        .is_some_and(|n| n != i64::from(default_ch))
    {
        loss(
            &mut report,
            "default_character",
            None,
            "PCF default-character sentinel changed during reconstruction".into(),
        );
    }
    let mut target = Font::default();
    target.properties = plist.clone().into();
    x11::project(&mut target);
    target.charset.name = font.charset.name.clone();
    target.charset.gl = font.charset.gl;
    let mut ordered = Vec::new();
    let mut unencoded = Vec::new();
    for (id, g) in font.glyphs() {
        if g.codepoints.is_empty() {
            unencoded.push((id, g));
        } else if g.codepoints.iter().any(|&cp| font.glyph_id(cp) == Some(id)) {
            ordered.push((id, g));
        } else {
            loss(
                &mut report,
                "duplicate_encoding",
                Some(id.index()),
                "Earlier duplicate record omitted; effective mapping wins".into(),
            );
        }
    }
    ordered.extend(unencoded);
    let mut glyphs = Vec::with_capacity(ordered.len());
    let mut encoding = BTreeMap::new();
    let mut sizes = [0usize; 4];
    let mut digit_width = 0i64;
    let mut digit_count = 0i64;
    let mut ex_height = 0i32;
    for (id, g) in ordered {
        glyph_losses(g, id, &mut report);
        let mut name = token(g.name.as_deref().unwrap_or(""));
        if let Some(i) = name.iter().position(|&b| b == 0) {
            name.truncate(i);
        }

        let label = String::from_utf8_lossy(&name);
        if g.name.as_ref().is_some_and(|s| s.as_bytes() != name) {
            loss(
                &mut report,
                "glyph_name",
                Some(id.index()),
                "Glyph name reduced to its first whitespace token and 99 bytes".into(),
            );
        }
        let b = g.metrics.bounds;
        let right =
            b.x.checked_add(i64::from(b.width))
                .ok_or_else(|| invalid("PCF glyph right bearing overflow"))?;
        let top =
            b.y.checked_add(i64::from(b.height))
                .ok_or_else(|| invalid("PCF glyph ascent overflow"))?;
        let bottom =
            b.y.checked_neg()
                .ok_or_else(|| invalid("PCF glyph descent overflow"))?;
        let wx = g
            .metrics
            .advance
            .or(font.metrics.glyph_defaults.advance)
            .map_or(right.max(0), |v| v.x);
        for value in [wx, b.x, b.y, right, top, bottom] {
            check16(&label, value)?;
        }
        let attributes = g
            .properties
            .get("pcf.attributes")
            .and_then(integer)
            .unwrap_or(0);
        if !(0..=65535).contains(&attributes) {
            return Err(invalid("PCF glyph attributes must fit 16 unsigned bits"));
        }
        let m = [
            b.x as i32,
            right as i32,
            wx as i32,
            top as i32,
            bottom as i32,
            attributes as i32,
        ];
        let sw = g
            .metrics
            .scalable_advance
            .or(font.metrics.glyph_defaults.scalable_advance)
            .map_or_else(|| swidth(point, resolution.x, m[2], bounds.width), |v| v.x);
        check_prop("SWIDTH", sw)?;
        if sw > i64::from(i32::MAX) {
            loss(
                &mut report,
                "swidth_sign",
                Some(id.index()),
                "Unsigned SWIDTH reads back as signed 32 bits".into(),
            );
        }
        if name.len() == 1 {
            if name[0].is_ascii_digit() {
                digit_width += wx;
                digit_count += 1;
            } else if name == b"x" {
                ex_height = if top > 0 { top as i32 } else { b.height as i32 };
            }
        }
        let mut codes = Vec::new();
        for &cp in &g.codepoints {
            if font.glyph_id(cp) != Some(id) {
                loss(
                    &mut report,
                    "duplicate_encoding",
                    Some(id.index()),
                    format!("Shadowed code {} omitted", cp.value()),
                );
                continue;
            }
            if let Some(c) = code_in_font(cp, &target)? {
                codes.push(c);
            } else {
                loss(
                    &mut report,
                    "unmappable_code",
                    Some(id.index()),
                    format!("Code {} cannot map to target charset", cp.value()),
                );
            }
        }
        if g.codepoints.is_empty()
            && let Some(v) = g.properties.get(x11::ALT).and_then(integer)
            && v >= 0
        {
            if let Ok(v) = u32::try_from(v) {
                codes.push(v);
            } else {
                loss(
                    &mut report,
                    "code_range",
                    Some(id.index()),
                    format!("Alternate code {v} exceeds PCF's 16-bit range"),
                );
            }
        }
        for cp in codes {
            if cp <= 65535 && glyphs.len() < 65535 {
                if encoding.insert(cp, glyphs.len()).is_some() {
                    loss(
                        &mut report,
                        "mapping_collision",
                        Some(id.index()),
                        format!("Code {cp} maps to multiple glyphs; last wins"),
                    );
                }
            } else {
                loss(
                    &mut report,
                    "code_range",
                    Some(id.index()),
                    format!(
                        "Code {cp} or glyph index exceeds PCF's 16-bit encoding range; glyph kept unencoded"
                    ),
                );
            }
        }
        for (i, s) in sizes.iter_mut().enumerate() {
            *s = s
                .checked_add(row_bytes(b.width as usize, 1 << i) * b.height as usize)
                .ok_or_else(|| limit("PCF bitmap bytes", options.max_output_bytes))?;
        }
        glyphs.push(WireGlyph {
            source: g,
            id,
            metric: m,
            bits: Vec::new(),
            name,
            swidth: sw,
            ink: None,
        });
    }
    let pad_index = options.glyph_pad.index() as usize;
    bound(
        sizes[pad_index],
        options
            .max_output_bytes
            .min(options.limits.max_total_bytes)
            .min(options.limits.max_table_bytes),
        "PCF bitmap bytes",
    )?;
    for g in &mut glyphs {
        g.bits = build_bits(g, pad, options, &mut report)?;
    }
    let mut all = Info::new(ascent as i32, descent as i32);
    let mut enc = Info::new(ascent as i32, descent as i32);
    compute_bounds(&glyphs, &encoding, &mut all, &mut enc);
    if enc.could_terminal() {
        let new = [0, enc.min[2], enc.min[2], ascent as i32, descent as i32, 0];
        if new[1] < 0 || new[3] + new[4] < 0 {
            return Err(invalid("PCF terminal cell has negative dimensions"));
        }
        let pixels = new[1] as usize * (new[3] + new[4]) as usize;
        bound(
            pixels,
            options.limits.max_pixels_per_glyph,
            "PCF terminal glyph pixels",
        )?;
        for (i, s) in sizes.iter_mut().enumerate() {
            *s = glyphs
                .len()
                .checked_mul(row_bytes(new[1] as usize, 1 << i) * (new[3] + new[4]) as usize)
                .ok_or_else(|| limit("PCF terminal bitmap bytes", options.max_output_bytes))?;
        }
        bound(
            sizes[pad_index],
            options
                .max_output_bytes
                .min(options.limits.max_total_bytes)
                .min(options.limits.max_table_bytes),
            "PCF terminal bitmap bytes",
        )?;
        loss(
            &mut report,
            "terminal_reshape",
            None,
            "Glyph boxes padded/clipped to the shared terminal cell using bdftopcf semantics"
                .into(),
        );
        for g in &mut glyphs {
            g.bits = reshape(g, &new, pad);
            g.metric = [new[0], new[1], new[2], new[3], new[4], g.metric[5]];
        }
        compute_bounds(&glyphs, &encoding, &mut all, &mut enc);
    }
    all.accelerate();
    enc.accelerate();
    let has_ink = enc.flags[1] != 0;
    if has_ink {
        for g in &mut glyphs {
            g.ink = Some(ink(g, pad));
        }
        all.flags[5] = 1;
        enc.flags[5] = 1;
    }
    for (info, indices) in [
        (&mut all, (0..glyphs.len()).collect::<Vec<_>>()),
        (&mut enc, encoding.values().copied().collect()),
    ] {
        if !has_ink {
            info.imin = info.min;
            info.imax = info.max;
        } else {
            (info.imin, info.imax) = initial_bounds();
            for i in indices {
                if let Some(m) = glyphs[i].ink {
                    minmax(&mut info.imin, &mut info.imax, &m);
                }
            }
        }
    }
    for g in &mut glyphs {
        if let Some(v) = x11::values(&g.source.properties, "pcf.ink_metrics")
            && g.ink.map(|m| m.map(i64::from).to_vec()).as_ref() != Some(&v)
        {
            loss(
                &mut report,
                "ink_metrics",
                Some(g.id.index()),
                "Source ink metrics replaced by derived pixel bounds".into(),
            );
        }
        if !bitmsb {
            for b in &mut g.bits {
                *b = b.reverse_bits();
            }
        }
        if bitmsb != msb && scan > 1 {
            swap_units(&mut g.bits, scan);
        }
    }
    for (key, value) in [
        ("WEIGHT", 10),
        (
            "X_HEIGHT",
            if ex_height != 0 {
                i64::from(ex_height)
            } else {
                i64::from(enc.imin[3])
            },
        ),
        (
            "QUAD_WIDTH",
            if digit_count != 0 {
                digit_width / digit_count
            } else {
                i64::from(enc.imin[2] + enc.imax[2]) / 2
            },
        ),
    ] {
        if let Some(&i) = special.get(key)
            && integer(&plist[i].value) == Some(-1)
        {
            plist[i].value = PropertyValue::Integer(value);
        }
    }
    let reconstructed =
        if all.max[1] >= all.min[0] && all.max[3] + all.max[4] >= 0 && all.min[0] != 32767 {
            Bounds::new(
                (all.max[1] - all.min[0]) as u32,
                (all.max[3] + all.max[4]) as u32,
                i64::from(all.min[0]),
                -i64::from(all.max[4]),
            )
        } else {
            Bounds::default()
        };
    if font.metrics.bounds.is_some() && reconstructed != bounds {
        loss(
            &mut report,
            "font_bounds",
            None,
            "Source FONTBOUNDINGBOX replaced by accelerator bounds".into(),
        );
    }
    let reconstructed_name = plist
        .iter()
        .find_map(|p| match &p.value {
            PropertyValue::Text(s) if p.name == "FONT" => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("");
    if reconstructed_name != fontname
        && (font.names.xlfd.is_some() || font.headers.get("FONT").is_some())
    {
        loss(
            &mut report,
            "font_name",
            None,
            "PCF FONT property differs from the source FONT/XLFD header".into(),
        );
    }
    if font.metrics.resolution.is_some() || size.is_some() {
        for (key, expected) in [
            ("RESOLUTION_X", resolution.x),
            ("RESOLUTION_Y", resolution.y),
        ] {
            let actual = plist
                .iter()
                .rev()
                .find(|p| p.name.eq_ignore_ascii_case(key))
                .and_then(|p| integer(&p.value))
                .unwrap_or(75);
            if actual != expected {
                loss(
                    &mut report,
                    "resolution",
                    None,
                    format!(
                        "PCF {key} reconstructs {actual}, differing from source resolution {expected}"
                    ),
                );
            }
        }
    }
    let read_point = special
        .get("POINT_SIZE")
        .and_then(|&i| integer(&plist[i].value))
        .map(|v| (v as f64 / 10.0 + 0.5).floor());
    if read_point != Some(point) {
        loss(
            &mut report,
            "point_size",
            None,
            "Source point size differs from PCF POINT_SIZE reconstruction".into(),
        );
    }
    let bytes = emit(
        fmt,
        msb,
        &plist,
        &glyphs,
        &all,
        &enc,
        sizes,
        &encoding,
        default_ch,
        options,
        font,
        &mut report,
    )?;
    Ok((bytes, report))
}

struct Out {
    bytes: Vec<u8>,
    msb: bool,
}
impl Out {
    fn new(fmt: u32) -> Self {
        Self {
            bytes: fmt.to_le_bytes().to_vec(),
            msb: fmt & 4 != 0,
        }
    }
    fn u16(&mut self, v: i64) {
        self.bytes.extend(if self.msb {
            (v as u16).to_be_bytes()
        } else {
            (v as u16).to_le_bytes()
        });
    }
    fn u32(&mut self, v: i64) {
        self.bytes.extend(if self.msb {
            (v as u32).to_be_bytes()
        } else {
            (v as u32).to_le_bytes()
        });
    }
    fn metric(&mut self, m: &Metric, compressed: bool) {
        if compressed {
            self.bytes.extend(m[..5].iter().map(|v| (v + 128) as u8));
        } else {
            for &v in m {
                self.u16(i64::from(v));
            }
        }
    }
    fn pad(&mut self) {
        self.bytes.resize(round4(self.bytes.len()), 0);
    }
    fn string(&mut self, s: &[u8]) {
        self.bytes.extend_from_slice(s);
        self.bytes.push(0);
    }
    fn accel(&mut self, i: &Info) {
        self.bytes.extend(i.flags);
        self.bytes.extend([i.direction, 0]);
        for n in [i.ascent, i.descent, i.overlap] {
            self.u32(i64::from(n));
        }
        self.metric(&i.min, false);
        self.metric(&i.max, false);
        if i.flags[5] != 0 {
            self.metric(&i.imin, false);
            self.metric(&i.imax, false);
        }
    }
}
fn can_compress(min: &Metric, max: &Metric, n: usize) -> bool {
    n <= 65535
        && [min, max]
            .iter()
            .all(|m| m[..5].iter().all(|&v| (-128..=127).contains(&v)) && m[5] == 0)
}
#[allow(clippy::too_many_arguments)]
fn emit(
    fmt: u32,
    msb: bool,
    plist: &[Property],
    glyphs: &[WireGlyph<'_>],
    all: &Info,
    enc: &Info,
    sizes: [usize; 4],
    encoding: &BTreeMap<u32, usize>,
    default_ch: u16,
    options: &PcfWriteOptions,
    font: &Font,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let n = glyphs.len();
    let has_ink = enc.flags[5] != 0;
    let nenc = if encoding.is_empty() {
        65025
    } else {
        let minr = encoding.keys().map(|c| c >> 8).min().unwrap_or(0);
        let maxr = encoding.keys().map(|c| c >> 8).max().unwrap_or(0);
        let minc = encoding.keys().map(|c| c & 255).min().unwrap_or(0);
        let maxc = encoding.keys().map(|c| c & 255).max().unwrap_or(0);
        ((maxr - minr + 1) * (maxc - minc + 1)) as usize
    };
    let mut estimate = 1024usize
        .saturating_add(n.saturating_mul(48))
        .saturating_add(sizes[(fmt & 3) as usize])
        .saturating_add(nenc * 2);
    for p in plist {
        estimate = estimate.saturating_add(p.name.len() + 10);
        if let PropertyValue::Text(s) = &p.value {
            estimate = estimate.saturating_add(s.len() + 1);
        }
    }
    for g in glyphs {
        estimate = estimate.saturating_add(g.name.len() + 1);
    }
    // Conservative preallocation/work budget, followed by the exact output limit.
    bound(
        estimate,
        options.limits.max_total_bytes,
        "PCF output working bytes",
    )?;
    let property_strings: usize = plist
        .iter()
        .map(|p| {
            p.name.len()
                + 1
                + match &p.value {
                    PropertyValue::Text(s) => s.len() + 1,
                    _ => 0,
                }
        })
        .sum();
    let glyph_strings: usize = glyphs.iter().map(|g| g.name.len() + 1).sum();
    let metric_size = |min: &Metric, max: &Metric| {
        if can_compress(min, max, n) {
            round4(6 + 5 * n)
        } else {
            8 + 12 * n
        }
    };
    let mut table_sizes = vec![
        round4(8 + 9 * plist.len()) + 4 + round4(property_strings),
        100,
        metric_size(&all.min, &all.max),
        round4(24 + 4 * n + sizes[(fmt & 3) as usize]),
        round4(14 + 2 * nenc),
        8 + 4 * n,
        12 + 4 * n + round4(glyph_strings),
        100,
    ];
    if has_ink {
        table_sizes.push(metric_size(&all.imin, &all.imax));
    }
    for &size in &table_sizes {
        bound(size, options.limits.max_table_bytes, "PCF table bytes")?;
    }
    let exact = 8 + 16 * table_sizes.len() + table_sizes.iter().sum::<usize>() - 100
        + if has_ink { 72 } else { 48 };
    bound(exact, options.max_output_bytes, "PCF output bytes")?;
    if u32::try_from(exact + 52).is_err() {
        return Err(invalid("PCF output offsets exceed 32 bits"));
    }
    let mut tables = Vec::<(u32, usize, Out)>::new();
    let mut out = Out::new(fmt);
    out.u32(plist.len() as i64);
    let mut strings = Vec::new();
    for p in plist {
        out.u32(strings.len() as i64);
        strings.extend_from_slice(p.name.as_bytes());
        strings.push(0);
        if let PropertyValue::Text(s) = &p.value {
            out.bytes.push(1);
            out.u32(strings.len() as i64);
            strings.extend_from_slice(s.as_bytes());
            strings.push(0);
        } else {
            out.bytes.push(0);
            out.u32(integer(&p.value).unwrap_or(0));
        }
    }
    out.pad();
    out.u32(strings.len() as i64);
    out.bytes.extend(strings);
    let size = round4(out.bytes.len());
    tables.push((PROPERTIES, size, out));
    for (kind, info) in [(ACCELERATORS, all), (BDF_ACCELERATORS, enc)] {
        let mut out = Out::new(fmt | if has_ink { 0x100 } else { 0 });
        out.accel(info);
        let key = if kind == ACCELERATORS {
            "pcf.accelerators"
        } else {
            "pcf.bdf_accelerators"
        };
        if let Some(PropertyValue::Bytes(raw)) = font.headers.get(key)
            && *raw != accelerator_fields(&out.bytes[4..], msb)
        {
            loss(
                report,
                "accelerators",
                None,
                format!("Source {key} replaced by recomputed accelerator fields"),
            );
        }
        tables.push((kind, 100, out));
    }
    for (kind, min, max) in [
        (METRICS, &all.min, &all.max),
        (INK_METRICS, &all.imin, &all.imax),
    ] {
        if kind == INK_METRICS && !has_ink {
            continue;
        }
        let compressed = can_compress(min, max, n);
        let mut out = Out::new(fmt | if compressed { 0x100 } else { 0 });
        if compressed {
            out.u16(n as i64);
        } else {
            out.u32(n as i64);
        }
        for g in glyphs {
            out.metric(
                &if kind == METRICS {
                    g.metric
                } else {
                    g.ink.unwrap_or(g.metric)
                },
                compressed,
            );
        }
        let size = round4(out.bytes.len());
        tables.push((kind, size, out));
    }
    let mut out = Out::new(fmt);
    out.u32(n as i64);
    let mut offset = 0usize;
    for g in glyphs {
        out.u32(offset as i64);
        offset += g.bits.len();
    }
    for s in sizes {
        out.u32(s as i64);
    }
    for g in glyphs {
        out.bytes.extend_from_slice(&g.bits);
    }
    let size = round4(out.bytes.len());
    tables.push((BITMAPS, size, out));
    let mut out = Out::new(fmt);
    let (fr, lr, fc, lc) = if encoding.is_empty() {
        (256, 0, 256, 0)
    } else {
        (
            encoding.keys().map(|c| c >> 8).min().unwrap_or(0),
            encoding.keys().map(|c| c >> 8).max().unwrap_or(0),
            encoding.keys().map(|c| c & 255).min().unwrap_or(0),
            encoding.keys().map(|c| c & 255).max().unwrap_or(0),
        )
    };
    for v in [fc, lc, fr, lr, u32::from(default_ch)] {
        out.u16(i64::from(v));
    }
    if encoding.is_empty() {
        for _ in 0..nenc {
            out.u16(65535);
        }
    } else {
        for r in fr..=lr {
            for c in fc..=lc {
                out.u16(encoding.get(&((r << 8) | c)).copied().unwrap_or(65535) as i64);
            }
        }
    }
    let size = round4(out.bytes.len());
    tables.push((ENCODINGS, size, out));
    let mut out = Out::new(fmt);
    out.u32(n as i64);
    for g in glyphs {
        out.u32(g.swidth);
    }
    tables.push((SWIDTHS, out.bytes.len(), out));
    let mut out = Out::new(fmt);
    out.u32(n as i64);
    let mut offset = 0;
    for g in glyphs {
        out.u32(offset as i64);
        offset += g.name.len() + 1;
    }
    out.u32(offset as i64);
    for g in glyphs {
        out.string(&g.name);
    }
    let size = round4(out.bytes.len());
    tables.push((GLYPH_NAMES, size, out));
    tables.sort_by_key(|t| t.0);
    let mut end = 8 + tables.len() * 16;
    for (_, size, _) in &tables {
        bound(*size, options.limits.max_table_bytes, "PCF table bytes")?;
        end = end
            .checked_add(*size)
            .ok_or_else(|| limit("PCF output bytes", options.max_output_bytes))?;
    }
    let exact = end - 100 + tables.last().map_or(0, |t| t.2.bytes.len());
    bound(exact, options.max_output_bytes, "PCF output bytes")?;
    if u32::try_from(end).is_err() {
        return Err(invalid("PCF directory offsets exceed 32 bits"));
    }
    let mut result = Vec::with_capacity(exact);
    result.extend_from_slice(MAGIC);
    result.extend((tables.len() as u32).to_le_bytes());
    let mut offset = 8 + tables.len() * 16;
    for (kind, size, out) in &tables {
        for v in [
            *kind,
            u32::from_le_bytes(
                out.bytes[..4]
                    .try_into()
                    .map_err(|_| invalid("PCF internal table header"))?,
            ),
            *size as u32,
            offset as u32,
        ] {
            result.extend(v.to_le_bytes());
        }
        offset += size;
    }
    let mut offset = result.len();
    for (_, size, out) in tables {
        result.resize(offset, 0);
        result.extend(out.bytes);
        offset += size;
    }
    Ok(result)
}
