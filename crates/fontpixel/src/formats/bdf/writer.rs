use super::source::*;
use super::*;
use crate::model::{Codepoint, GlyphId, Properties, Property};
use std::collections::BTreeMap;

struct Sink {
    text: String,
    max: usize,
}
impl Sink {
    fn append(&mut self, s: &str) -> Result<()> {
        if self.text.len().saturating_add(s.len()) > self.max {
            return Err(Error::ResourceLimit {
                resource: "BDF output bytes",
                limit: self.max,
            });
        }
        self.text
            .try_reserve(s.len())
            .map_err(|_| Error::ResourceLimit {
                resource: "BDF output allocation",
                limit: self.max,
            })?;
        self.text.push_str(s);
        Ok(())
    }
    fn line(&mut self, s: &str) -> Result<()> {
        self.append(s)?;
        self.append("\n")
    }
}
fn loss(report: &mut Report, code: &str, glyph: Option<(usize, Option<u32>)>, message: String) {
    diagnostic(report, Level::Warning, Impact::Loss, code, glyph, message);
}
fn signed32(value: i64, field: &str) -> Result<()> {
    if i32::try_from(value).is_err() {
        return Err(Error::InvalidInput(format!(
            "Cannot write BDF: {field} value {value} does not fit signed 32 bits"
        )));
    }
    Ok(())
}
fn safe_line(value: &str, field: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        return Err(Error::InvalidInput(format!(
            "Cannot write BDF: {field} contains a line break"
        )));
    }
    Ok(())
}
fn ver(v: Option<&PropertyValue>) -> Result<String> {
    let s = v.and_then(text_value).unwrap_or_else(|| "2.1".into());
    let n: f64 = s
        .parse()
        .map_err(|_| Error::InvalidInput("Cannot write BDF: invalid version".into()))?;
    if n.is_finite() && n.fract() == 0.0 {
        return Ok(format!("{n:.0}.0"));
    }
    let s = format!("{n:?}");
    if let Some((base, e)) = s.split_once('e') {
        let n: i32 = e
            .parse()
            .map_err(|_| Error::InvalidInput("Cannot write BDF version exponent".into()))?;
        Ok(format!(
            "{base}e{}{abs:02}",
            if n < 0 { "-" } else { "+" },
            abs = n.unsigned_abs()
        ))
    } else {
        Ok(s)
    }
}
fn required(h: &Properties) -> Result<(Vec<i64>, Vec<i64>)> {
    let size = values(h, "SIZE");
    let bb = values(h, "FONTBOUNDINGBOX");
    let mut missing = Vec::new();
    if size.as_ref().is_none_or(|v| v.len() != 3) {
        missing.extend(["pointsize", "xres", "yres"]);
    }
    if bb.as_ref().is_none_or(|v| v.len() != 4) {
        missing.extend(["fbbx", "fbby", "fbbxoff", "fbbyoff"]);
    }
    if !missing.is_empty() {
        return Err(Error::InvalidInput(format!(
            "Cannot write BDF: missing header(s) {} (font not loaded?)",
            missing.join(", ")
        )));
    }
    let size = size.unwrap_or_default();
    let bb = bb.unwrap_or_default();
    for &n in &size {
        signed32(n, "SIZE")?;
    }
    for &n in &bb {
        signed32(n, "FONTBOUNDINGBOX")?;
    }
    if bb[0] < 0 || bb[1] < 0 {
        return Err(Error::InvalidInput(
            "Cannot write BDF: negative FONTBOUNDINGBOX size".into(),
        ));
    }
    Ok((size, bb))
}
fn unique<'a>(properties: &'a Properties, report: &mut Report) -> Vec<&'a Property> {
    let mut result: Vec<&Property> = Vec::new();
    let mut positions = BTreeMap::new();
    for property in properties
        .iter()
        .filter(|p| !p.name.eq_ignore_ascii_case("COMMENT"))
    {
        let key = property.name.to_lowercase();
        if let Some(&i) = positions.get(&key) {
            loss(
                report,
                "bdf.write.duplicate_property",
                None,
                format!(
                    "Repeated property {:?} collapsed to its last value and spelling",
                    property.name
                ),
            );
            result[i] = property;
        } else {
            positions.insert(key, result.len());
            result.push(property);
        }
    }
    result
}
fn is_notice(name: &str) -> bool {
    ["COPYRIGHT", "LICENSE", "LICENSE_URL"]
        .iter()
        .any(|notice| name.eq_ignore_ascii_case(notice))
}
// Unlike str::lines(), keep empty final lines and treat a lone CR as a break.
// CRLF is one delimiter. Every yielded slice borrows the effective property.
fn notice_lines(value: &str) -> impl Iterator<Item = &str> {
    let mut remaining = Some(value);
    std::iter::from_fn(move || {
        let text = remaining?;
        if let Some(index) = text.find(['\r', '\n']) {
            let delimiter = if text.as_bytes()[index] == b'\r'
                && text.as_bytes().get(index + 1) == Some(&b'\n')
            {
                2
            } else {
                1
            };
            remaining = Some(&text[index + delimiter..]);
            Some(&text[..index])
        } else {
            remaining = None;
            Some(text)
        }
    })
}
fn write_multiline_notice(
    out: &mut Sink,
    property: &Property,
    value: &str,
    report: &mut Report,
) -> Result<()> {
    // Count the exact extra wire bytes before emitting any part of this notice:
    // each COMMENT wrapper is 11 bytes; the property wrapper is name.len()+4;
    // its quote escaping adds one byte per quote, and joins add three bytes.
    // No per-line vector, escaped copy or joined notice string is allocated.
    let maximum = out.max;
    let add = |total: &mut usize, amount: usize| -> Result<()> {
        *total = total
            .checked_add(amount)
            .filter(|size| *size <= maximum)
            .ok_or(Error::ResourceLimit {
                resource: "BDF output bytes",
                limit: maximum,
            })?;
        Ok(())
    };
    let mut total = out.text.len();
    add(&mut total, property.name.len())?;
    add(&mut total, 4)?;
    for (index, line) in notice_lines(value).enumerate() {
        add(&mut total, 11)?;
        add(&mut total, line.len())?;
        add(&mut total, line.len())?;
        add(
            &mut total,
            line.bytes().filter(|byte| *byte == b'"').count(),
        )?;
        if index != 0 {
            add(&mut total, 3)?;
        }
    }
    for line in notice_lines(value) {
        // Match write_comment exactly: its quoted wrapper does not double
        // interior quotes, and the reader strips only the outside wrapper.
        out.append("COMMENT \"")?;
        out.append(line)?;
        out.line("\"")?;
    }
    out.append(&property.name)?;
    out.append(" \"")?;
    for (index, line) in notice_lines(value).enumerate() {
        if index != 0 {
            out.append(" | ")?;
        }
        for (quote_index, fragment) in line.split('"').enumerate() {
            if quote_index != 0 {
                out.append("\"\"")?;
            }
            out.append(fragment)?;
        }
    }
    out.line("\"")?;
    loss(
        report,
        "bdf.write.notice_line_breaks",
        None,
        format!(
            "{} notice line breaks were preserved as COMMENT lines and projected to ' | ' in the property; this does not establish modification or distribution permission",
            property.name,
        ),
    );
    Ok(())
}
fn write_properties(out: &mut Sink, properties: &Properties, report: &mut Report) -> Result<()> {
    let props = unique(properties, report);
    let comments: Vec<_> = properties
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("COMMENT"))
        .collect();
    if props.is_empty() && comments.is_empty() {
        return Ok(());
    }
    out.line(&format!("STARTPROPERTIES {}", props.len()))?;
    for p in comments {
        write_comment(out, &p.value)?;
    }
    for p in props {
        if p.name.is_empty() || p.name.contains(ws) {
            return Err(Error::InvalidInput(
                "Cannot write BDF: property name must be a nonempty token".into(),
            ));
        }
        if let PropertyValue::Text(value) | PropertyValue::Decimal(value) = &p.value
            && is_notice(&p.name)
            && value.contains(['\n', '\r'])
        {
            write_multiline_notice(out, p, value, report)?;
            continue;
        }
        let line = match &p.value {
            PropertyValue::Flag => p.name.clone(),
            PropertyValue::Integer(v) => {
                if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(v) {
                    return Err(Error::InvalidInput(format!(
                        "Cannot write BDF: property {} value {v} is outside the signed/unsigned 32-bit range",
                        p.name
                    )));
                }
                format!("{} {v}", p.name)
            }
            PropertyValue::Unsigned(v) => {
                if *v > u64::from(u32::MAX) {
                    return Err(Error::InvalidInput(format!(
                        "Cannot write BDF: property {} value {v} is outside the signed/unsigned 32-bit range",
                        p.name
                    )));
                }
                format!("{} {v}", p.name)
            }
            PropertyValue::Text(v) | PropertyValue::Decimal(v) => {
                safe_line(v, &p.name)?;
                format!("{} \"{}\"", p.name, v.replace('"', "\"\""))
            }
            PropertyValue::Bytes(_) | PropertyValue::List(_) => {
                // Preserve the property's existence as a flag; never fake a portable string encoding.
                loss(
                    report,
                    "bdf.write.property_type",
                    None,
                    format!(
                        "Property {:?} has an unsupported compound/binary value; written as a flag",
                        p.name
                    ),
                );
                p.name.clone()
            }
        };
        out.line(&line)?;
    }
    out.line("ENDPROPERTIES")
}
fn write_comment(out: &mut Sink, v: &PropertyValue) -> Result<()> {
    if let PropertyValue::List(v) = v {
        for c in v {
            write_comment(out, c)?;
        }
        return Ok(());
    }
    let s = text_value(v)
        .ok_or_else(|| Error::InvalidInput("Cannot write BDF: comment must be text".into()))?;
    safe_line(&s, "COMMENT")?;
    out.line(&format!("COMMENT \"{s}\""))
}
fn glyph_losses(glyph: &Glyph, id: GlyphId, report: &mut Report) {
    let location = Some((id.index(), glyph.codepoints.first().map(|c| c.value())));
    if !glyph.unicode_sequences.is_empty() {
        loss(
            report,
            "bdf.write.sequences",
            location,
            "BDF cannot store Unicode sequence mappings".into(),
        );
    }
    if glyph.codepoints.len() > 1 {
        loss(report, "bdf.write.alias_identity", location, "Multiple aliases are expanded into separate BDF glyph records; shared glyph identity is lost".into());
    }
    let m = &glyph.metrics;
    for (present, name) in [
        (m.abc.is_some(), "ABC widths"),
        (m.left_bearing.is_some(), "independent left bearing"),
        (m.right_bearing.is_some(), "independent right bearing"),
        (m.image_offset.is_some(), "image offset"),
    ] {
        if present {
            loss(
                report,
                "bdf.write.glyph_metric",
                location,
                format!("BDF cannot store {name}"),
            );
        }
    }
    for property in glyph.properties.iter() {
        if property.raw.is_some() {
            loss(
                report,
                "bdf.write.glyph_property",
                location,
                format!(
                    "BDF SPEC output omits source glyph field {:?}",
                    property.name
                ),
            );
        } else if property.name == ENCODING {
            if integer(&property.value).is_some_and(|n| n != -1) {
                loss(
                    report,
                    "bdf.write.negative_encoding",
                    location,
                    "Negative source ENCODING normalized to -1".into(),
                );
            }
        } else if property.name != ROWS && property.name != IMAGE && property.name != ALT {
            loss(
                report,
                "bdf.write.glyph_property",
                location,
                format!(
                    "BDF SPEC output omits source glyph field {:?}",
                    property.name
                ),
            );
        }
    }
}
fn code_in_font(cp: Codepoint, font: &Font) -> Result<Option<u32>> {
    if matches!(cp, Codepoint::Legacy(_)) || known_unicode(font) {
        return Ok(Some(cp.value()));
    }
    let Some(ch) = cp.scalar() else {
        return Ok(None);
    };
    let codec = font
        .charset
        .codec()
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    Ok(codec.char2cp(ch))
}
struct Record<'a> {
    id: GlyphId,
    glyph: &'a Glyph,
    code: Option<u32>,
}
fn records<'a>(font: &'a Font, target: &Font, report: &mut Report) -> Result<Vec<Record<'a>>> {
    let mut encoded = Vec::new();
    let mut unencoded = Vec::new();
    let mut last = BTreeMap::new();
    for (id, glyph) in font.glyphs() {
        glyph.validate()?;
        glyph_losses(glyph, id, report);
        if glyph.codepoints.is_empty() {
            unencoded.push(Record {
                id,
                glyph,
                code: None,
            });
            continue;
        }
        let mut mapped = false;
        for &cp in &glyph.codepoints {
            if font.glyph_id(cp) != Some(id) {
                loss(
                    report,
                    "bdf.write.duplicate_encoding",
                    Some((id.index(), Some(cp.value()))),
                    "Earlier duplicate glyph omitted from canonical BDF; later mapping wins".into(),
                );
                continue;
            }
            if let Some(code) = code_in_font(cp, target)? {
                signed32(i64::from(code), "ENCODING")?;
                last.insert(code, encoded.len());
                encoded.push(Record {
                    id,
                    glyph,
                    code: Some(code),
                });
                mapped = true;
            } else {
                loss(
                    report,
                    "bdf.write.unmappable_code",
                    Some((id.index(), Some(cp.value()))),
                    "Code cannot be mapped to the BDF target charset".into(),
                );
            }
        }
        if !mapped
            && glyph
                .codepoints
                .iter()
                .all(|&cp| font.glyph_id(cp) == Some(id))
        {
            unencoded.push(Record {
                id,
                glyph,
                code: None,
            });
        }
    }
    let mut result = Vec::with_capacity(encoded.len() + unencoded.len());
    for (i, record) in encoded.into_iter().enumerate() {
        if record.code.is_some_and(|cp| last.get(&cp) == Some(&i)) {
            result.push(record);
        } else {
            loss(
                report,
                "bdf.write.mapping_collision",
                Some((record.id.index(), record.code)),
                "Charset conversion produced duplicate codes; last glyph wins".into(),
            );
        }
    }
    result.extend(unencoded);
    Ok(result)
}
fn write_glyph(
    out: &mut Sink,
    record: &Record<'_>,
    options: &BdfWriteOptions,
    unicode: bool,
    catalogue_pixels: Option<u32>,
    report: &mut Report,
) -> Result<()> {
    let glyph = record.glyph;
    let name = glyph.name.clone().unwrap_or_else(|| match record.code {
        None if options.catalogue_family.is_some() => "uni-001".into(),
        Some(cp) if options.catalogue_family.is_some() => format!("uni{cp:04X}"),
        None => "unencoded".into(),
        Some(cp) if !unicode => format!("char{cp}"),
        Some(cp) if cp <= 0xffff => format!("uni{cp:04X}"),
        Some(cp) => format!("u{cp:05X}"),
    });
    if name.trim_matches(ws) != name {
        loss(
            report,
            "bdf.write.name_whitespace",
            Some((record.id.index(), record.code)),
            "Leading/trailing STARTCHAR whitespace is not retained by BDF readers".into(),
        );
    }
    safe_line(&name, "STARTCHAR")?;
    out.line(&format!("STARTCHAR {name}"))?;
    match record.code {
        Some(cp) => out.line(&format!("ENCODING {cp}"))?,
        None => {
            let alt = glyph.properties.get(ALT).and_then(integer);
            if let Some(n) = alt.filter(|_| options.catalogue_family.is_none()) {
                signed32(n, "alternate ENCODING")?;
                out.line(&format!("ENCODING -1 {n}"))?;
            } else {
                out.line("ENCODING -1")?;
                if alt.is_some() {
                    loss(
                        report,
                        "bdf.write.catalogue_alternate",
                        Some((record.id.index(), None)),
                        "Catalogue BDF omits alternate unencoded glyph codes".into(),
                    );
                }
            }
        }
    }
    if options.catalogue_family.is_some() && glyph.metrics.scalable_advance.is_none() {
        let advance = glyph.metrics.advance.map_or(0, |v| v.x);
        let pixels = catalogue_pixels.unwrap_or(1);
        let swidth = ((advance as f64) * 1000.0 / f64::from(pixels.max(1))).round_ties_even();
        out.line(&format!("SWIDTH {swidth:.0} 0"))?;
    }
    for key in METRICS {
        if let Some(v) = vector(&glyph.metrics, key) {
            signed32(v.x, key)?;
            signed32(v.y, key)?;
            if options.catalogue_family.is_some() {
                if !matches!(key, "SWIDTH" | "DWIDTH") {
                    loss(
                        report,
                        "bdf.write.catalogue_vertical",
                        Some((record.id.index(), record.code)),
                        format!("Catalogue BDF omits {key}"),
                    );
                    continue;
                }
                if v.y != 0 {
                    loss(
                        report,
                        "bdf.write.catalogue_advance_y",
                        Some((record.id.index(), record.code)),
                        format!("Catalogue BDF sets {key} y to zero"),
                    );
                }
                out.line(&format!("{key} {} 0", v.x))?;
                continue;
            }
            out.line(&format!("{key} {} {}", v.x, v.y))?;
        }
    }
    let b = glyph.metrics.bounds;
    for n in [i64::from(b.width), i64::from(b.height), b.x, b.y] {
        signed32(n, "BBX")?;
    }
    out.line(&format!("BBX {} {} {} {}", b.width, b.height, b.x, b.y))?;
    out.line("BITMAP")?;
    if unchanged_bitmap(glyph)
        && let Some(mut rows) = source_rows(glyph)
    {
        if options.catalogue_family.is_some() {
            validate_bitmap(glyph)?;
            // The catalogue retains unused bits in the last source byte, while
            // normalizing row length and hex case. The Mono model masks those
            // bits, so use provenance only while the raster is still unchanged.
            let width = (u64::from(b.width).div_ceil(8) * 2) as usize;
            if (width as u64 + 1) * u64::from(b.height)
                > out.max.saturating_sub(out.text.len()) as u64
            {
                return Err(Error::ResourceLimit {
                    resource: "BDF output bytes",
                    limit: out.max,
                });
            }
            let mut row = String::new();
            row.try_reserve(width).map_err(|_| Error::ResourceLimit {
                resource: "BDF hex row allocation",
                limit: out.max,
            })?;
            for _ in 0..b.height {
                let source = rows.next().unwrap_or_default().as_bytes();
                row.clear();
                for x in 0..width {
                    row.push(char::from(
                        source.get(x).copied().unwrap_or(b'0').to_ascii_uppercase(),
                    ));
                }
                out.line(&row)?;
            }
        } else {
            for row in rows {
                safe_line(row, "bitmap row")?;
                out.line(row)?;
            }
        }
    } else {
        let bitmap = mono(glyph, options.gray_threshold)?;
        if matches!(glyph.bitmap, GlyphBitmap::Gray8(_)) {
            loss(
                report,
                "bdf.write.gray_quantization",
                Some((record.id.index(), record.code)),
                format!(
                    "Gray8 quantized at threshold {}",
                    options.gray_threshold.unwrap_or(128)
                ),
            );
        }
        let bytes_per_row = bitmap.stride().saturating_mul(2);
        let needed = if bitmap.width() == 0 && options.catalogue_family.is_none() {
            0
        } else {
            (bytes_per_row as u64 + 1) * u64::from(bitmap.height())
        };
        if needed > out.max.saturating_sub(out.text.len()) as u64 {
            return Err(Error::ResourceLimit {
                resource: "BDF output bytes",
                limit: out.max,
            });
        }
        let mut row = String::new();
        row.try_reserve(bytes_per_row)
            .map_err(|_| Error::ResourceLimit {
                resource: "BDF hex row allocation",
                limit: out.max,
            })?;
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for y in 0..if bitmap.width() == 0 && options.catalogue_family.is_none() {
            0
        } else {
            bitmap.height() as usize
        } {
            row.clear();
            for &b in &bitmap.as_bytes()[y * bitmap.stride()..(y + 1) * bitmap.stride()] {
                row.push(char::from(HEX[(b >> 4) as usize]));
                row.push(char::from(HEX[(b & 15) as usize]));
            }
            out.line(&row)?;
        }
    }
    out.line("ENDCHAR")
}

pub(super) fn write(font: &Font, options: &BdfWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut report = Report::default();
    let (mut h, mut p) = reconcile(font, true)?;
    if let Some(family) = &options.catalogue_family {
        catalogue_headers(font, family, &mut h, &mut p, &mut report)?;
    }
    let (size, bb) = required(&h)?;
    let mut target = Font::default();
    target.headers = h.clone();
    target.properties = p.clone();
    project(&mut target);
    let source_gl = font
        .headers
        .get(SOURCE_GL)
        .and_then(integer)
        .map(|n| n != 0);
    target.charset.gl = if font.charset.gl != source_gl {
        font.charset.gl
    } else {
        Some(crate::charset::detect_gl(
            &target.charset.canonical_name(),
            target
                .charset
                .encoding
                .as_deref()
                .map(|s| s.trim_matches(ws)),
            font.codepoints().map(|(cp, _)| cp.value()),
        ))
    };
    let mut records = records(font, &target, &mut report)?;
    if options.catalogue_family.is_some() {
        records.sort_by_key(|record| record.code.unwrap_or(u32::MAX));
    }
    let mut out = Sink {
        text: String::new(),
        max: options.max_output_bytes,
    };
    out.line(&format!("STARTFONT {}", ver(h.get("STARTFONT"))?))?;
    if options.catalogue_family.is_some() {
        for property in h.iter().filter(|p| p.name == "COMMENT") {
            catalogue_comment(&mut out, property)?;
        }
    }
    let name = h.get("FONT").and_then(text_value).unwrap_or_default();
    if name.trim_matches(ws) != name {
        loss(
            &mut report,
            "bdf.write.name_whitespace",
            None,
            "Leading/trailing FONT whitespace is not retained by BDF readers".into(),
        );
    }
    safe_line(&name, "FONT")?;
    out.line(&format!("FONT {name}"))?;
    if let Some(raw) = options
        .catalogue_family
        .as_ref()
        .and_then(|_| source_line(&h, "SIZE"))
    {
        out.line(raw)?;
    } else {
        out.line(&format!("SIZE {} {} {}", size[0], size[1], size[2]))?;
    }
    out.line(&format!(
        "FONTBOUNDINGBOX {} {} {} {}",
        bb[0], bb[1], bb[2], bb[3]
    ))?;
    if let Some(n) = h.get("METRICSSET").and_then(integer)
        && n != 0
    {
        signed32(n, "METRICSSET")?;
        out.line(&format!("METRICSSET {n}"))?;
    }
    if let Some(n) = h.get("CONTENTVERSION").and_then(integer) {
        signed32(n, "CONTENTVERSION")?;
        out.line(&format!("CONTENTVERSION {n}"))?;
    }
    for key in METRICS {
        if let Some(v) = values(&h, key)
            && v.len() == 2
        {
            signed32(v[0], key)?;
            signed32(v[1], key)?;
            out.line(&format!("{key} {} {}", v[0], v[1]))?;
        }
    }
    let mut seen = BTreeMap::<&str, usize>::new();
    for property in h.iter() {
        let name = property.name.as_str();
        if name.starts_with("bdf.") && property.raw.is_none() {
            continue;
        }
        if name == "COMMENT" {
            if options.catalogue_family.is_none() {
                write_comment(&mut out, &property.value)?;
            }
        } else if matches!(
            name,
            "STARTFONT" | "FONT" | "SIZE" | "FONTBOUNDINGBOX" | "METRICSSET" | "CONTENTVERSION"
        ) || METRICS.contains(&name)
        {
            let n = seen.entry(name).or_default();
            *n += 1;
            if *n > 1 {
                loss(
                    &mut report,
                    "bdf.write.duplicate_header",
                    None,
                    format!("Repeated header {name} collapsed to its last value"),
                );
            }
        } else {
            loss(
                &mut report,
                "bdf.write.unknown_header",
                None,
                format!("Unknown source header {name:?} omitted from SPEC BDF output"),
            );
        }
    }
    write_properties(&mut out, &p, &mut report)?;
    if records.len() > i32::MAX as usize {
        return Err(Error::InvalidInput(
            "BDF glyph count exceeds signed 32 bits".into(),
        ));
    }
    out.line(&format!("CHARS {}", records.len()))?;
    for record in &records {
        write_glyph(
            &mut out,
            record,
            options,
            known_unicode(&target),
            options
                .catalogue_family
                .as_ref()
                .map(|_| font.metrics.pixel_size.unwrap_or(1)),
            &mut report,
        )?;
    }
    out.line("ENDFONT")?;
    for property in font.names.localized.iter() {
        loss(
            &mut report,
            "bdf.write.localized_name",
            None,
            format!("BDF cannot store localized name {:?}", property.name),
        );
    }
    if model_defaults_changed(&font.metrics, &original(font).metrics) {
        loss(
            &mut report,
            "bdf.write.extra_font_metrics",
            None,
            "Independent ppem/default-glyph references are not represented in BDF".into(),
        );
    }
    let defaults = &font.metrics.glyph_defaults;
    if defaults.bounds != crate::bitmap::Bounds::default()
        || defaults.abc.is_some()
        || defaults.left_bearing.is_some()
        || defaults.right_bearing.is_some()
        || defaults.image_offset.is_some()
    {
        loss(
            &mut report,
            "bdf.write.extra_global_metrics",
            None,
            "Non-BDF glyph-default bounds, ABC, bearings or image offsets are not represented"
                .into(),
        );
    }
    let base = original(font);
    if font.names.xlfd_fields != base.names.xlfd_fields && font.names.xlfd == base.names.xlfd {
        loss(&mut report, "bdf.write.xlfd_fields", None, "Edited decomposed XLFD fields are not substituted for the authoritative full XLFD name".into());
    }
    if font.charset.windows_charset.is_some()
        || font.charset.code_page.is_some()
        || !font.charset.code_ranges.is_empty()
    {
        loss(
            &mut report,
            "bdf.write.charset_metadata",
            None,
            "Windows charset byte, code-page number and explicit code ranges are not BDF fields"
                .into(),
        );
    }
    if options.catalogue_family.is_some() {
        if out.text.chars().any(|c| u32::from(c) > 255) {
            loss(
                &mut report,
                "bdf.write.catalogue_latin1",
                None,
                "Catalogue BDF replaces characters outside Latin-1 with '?'".into(),
            );
        }
        let bytes = out
            .text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect();
        Ok((bytes, report))
    } else {
        Ok((out.text.into_bytes(), report))
    }
}

// A deliberately opt-in dialect matching fontpixel.com's bdfwrite.py. Source
// provenance is reusable only while it still describes the effective value.
fn source_line<'a>(headers: &'a Properties, name: &str) -> Option<&'a str> {
    let property = headers.iter().rev().find(|p| p.name == name)?;
    let raw = std::str::from_utf8(property.raw.as_deref()?).ok()?;
    let (key, value) = kv(raw);
    if key != name || raw.contains(['\n', '\r']) {
        return None;
    }
    let equal = match name {
        "SIZE" => {
            let parsed: Option<Vec<i64>> =
                value.split_whitespace().map(|v| v.parse().ok()).collect();
            parsed.is_some() && parsed == values(headers, name)
        }
        "COMMENT" => {
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            property.value == PropertyValue::Text(value.into())
        }
        _ => false,
    };
    equal.then_some(raw)
}

fn catalogue_comment(out: &mut Sink, property: &Property) -> Result<()> {
    // Validate one record independently, preserving repeated COMMENT lines.
    let one = Properties::from(vec![property.clone()]);
    if let Some(raw) = source_line(&one, "COMMENT") {
        out.line(raw)
    } else {
        write_comment(out, &property.value)
    }
}

fn catalogue_headers(
    font: &Font,
    family: &str,
    h: &mut Properties,
    p: &mut Properties,
    report: &mut Report,
) -> Result<()> {
    if family.trim().is_empty() || family.contains(['\n', '\r']) {
        return Err(Error::InvalidInput(
            "BDF catalogue family must be a nonempty single line".into(),
        ));
    }
    let pixels = i64::from(font.metrics.pixel_size.unwrap_or(1));
    if h.get("STARTFONT").is_some() && ver(h.get("STARTFONT"))? != "2.1" {
        loss(
            report,
            "bdf.write.catalogue_version",
            None,
            "Catalogue BDF normalizes STARTFONT to 2.1".into(),
        );
    }
    h.set("STARTFONT", PropertyValue::Decimal("2.1".into()));
    for key in ["METRICSSET", "CONTENTVERSION"].into_iter().chain(METRICS) {
        if let Some(value) = h.get(key) {
            // The reader synthesizes the default horizontal METRICSSET=0.
            // Omitting that default changes no meaning and is not a loss.
            if key != "METRICSSET" || integer(value) != Some(0) {
                loss(
                    report,
                    "bdf.write.catalogue_header",
                    None,
                    format!("Catalogue BDF omits source header {key}"),
                );
            }
            h.remove(key);
        }
    }
    let bounds = font.metrics.bounds.unwrap_or_default();
    p.set(
        "FONT_ASCENT",
        font.metrics
            .ascent
            .unwrap_or(i64::from(bounds.height) + bounds.y),
    );
    p.set("FONT_DESCENT", font.metrics.descent.unwrap_or(-bounds.y));
    p.set("PIXEL_SIZE", pixels);
    let latin1 = p
        .get("CHARSET_REGISTRY")
        .and_then(text_value)
        .is_some_and(|s| s.eq_ignore_ascii_case("ISO8859"))
        && p.get("CHARSET_ENCODING").and_then(text_value).as_deref() == Some("1")
        && font.codepoints().all(|(cp, _)| cp.value() <= 255);
    if !latin1 {
        p.set("CHARSET_REGISTRY", "ISO10646");
        p.set("CHARSET_ENCODING", "1");
    }
    let mut name = h.get("FONT").and_then(text_value).filter(|s| !s.is_empty());
    if name.is_none() {
        name = p.get("FONT").and_then(text_value).filter(|s| !s.is_empty());
        p.remove("FONT");
    }
    let name = name.unwrap_or_else(|| {
        format!(
            "-opf-{family}-medium-r-normal--{pixels}-{}-72-72-p-{}-iso10646-1",
            pixels * 10,
            pixels * 10
        )
    });
    let mut fields: Vec<_> = name.split('-').map(str::to_owned).collect();
    let name = if !latin1 && name.starts_with('-') && fields.len() == 15 {
        if !fields[13].eq_ignore_ascii_case("ISO10646") || fields[14] != "1" {
            fields[13] = "ISO10646".into();
            fields[14] = "1".into();
        }
        fields.join("-")
    } else {
        name
    };
    h.set("FONT", name);
    // A source SIZE (including its original spacing) wins. Newly authored
    // fonts use pixels at 72 dpi, as the catalogue pipeline does.
    if h.get("SIZE").is_none() {
        h.set(
            "SIZE",
            PropertyValue::List(vec![pixels.into(), 72.into(), 72.into()]),
        );
    }
    if h.get("FONTBOUNDINGBOX").is_none() {
        h.set(
            "FONTBOUNDINGBOX",
            PropertyValue::List(vec![
                i64::from(bounds.width).into(),
                i64::from(bounds.height).into(),
                bounds.x.into(),
                bounds.y.into(),
            ]),
        );
    }
    Ok(())
}
