use super::*;

type Fields = Vec<(String, String)>;
struct RawGlyph<'a> {
    labels: Vec<String>,
    rows: Vec<&'a str>,
    fields: Fields,
}

pub(super) fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(FontSet, Report)> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid("input is not UTF-8"))?
        .trim_start_matches('\u{feff}');
    if !valid_text(text) {
        return Err(invalid("forbidden control or noncharacter"));
    }
    let mut budget = Budget {
        used: 0,
        max: limits.max_total_bytes,
        record_max: limits.max_table_bytes,
    };
    budget.charge(bytes.len())?;
    let normalized;
    let text = if text.contains('\r') {
        budget.charge(text.len().saturating_mul(2))?;
        normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        text
    };
    let count = text
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        .saturating_add(1);
    budget.charge(count.saturating_mul(std::mem::size_of::<&str>()))?;
    let lines: Vec<&str> = text.split('\n').collect();
    for line in &lines {
        if line.len() > limits.max_table_bytes {
            return Err(limit("YAFF line bytes", limits.max_table_bytes));
        }
    }
    let mut set = FontSet {
        source_format: Some("yaff".into()),
        ..FontSet::default()
    };
    let mut pos = 0;
    let mut total_glyphs: usize = 0;
    let mut container = None;
    while pos < lines.len() {
        if lines[pos].trim_end() == "---" {
            pos += 1;
            if pos == lines.len() {
                break;
            }
        }
        let start = pos;
        while pos < lines.len() && lines[pos].trim_end() != "---" {
            pos += 1;
        }
        let section = &lines[start..pos];
        // A final LF after a boundary is not another empty section.
        if start > 0 && pos == lines.len() && section.iter().all(|l| l.is_empty()) {
            break;
        }
        let (fields, raw) = parse_section(section, &mut budget, limits)?;
        total_glyphs = total_glyphs
            .checked_add(raw.len())
            .ok_or_else(|| limit("YAFF glyph count", limits.max_glyphs))?;
        if total_glyphs > limits.max_glyphs {
            return Err(limit("YAFF glyph count", limits.max_glyphs));
        }
        if let Some(value) = field(&fields, "fontpixel.container") {
            if container.is_some() || !set.fonts.is_empty() {
                return Err(invalid(
                    "container metadata must occur once in first section",
                ));
            }
            container = Some(json::<ContainerMeta>(value, &mut budget, limits)?);
        }
        budget.charge(std::mem::size_of::<Font>())?;
        let font = build_font(fields, raw, &mut budget, limits)?;
        set.fonts.push(font);
    }
    if let Some(meta) = container {
        if meta.version != 1 {
            return Err(invalid("unsupported container metadata version"));
        }
        props_depth(&meta.properties)?;
        if meta.empty {
            if set.fonts.len() != 1 || !set.fonts[0].is_empty() {
                return Err(invalid("empty container has glyphs or additional fonts"));
            }
            set.fonts.clear();
        }
        set.source_format = meta.source_format;
        set.properties = meta.properties;
    }
    set.validate(limits)?;
    Ok((set, Report::default()))
}
fn json<T: for<'de> Deserialize<'de>>(
    s: &str,
    budget: &mut Budget,
    limits: &ResourceLimits,
) -> Result<T> {
    if s.len() > limits.max_table_bytes {
        return Err(limit("YAFF metadata record bytes", limits.max_table_bytes));
    }
    budget.charge(s.len().saturating_mul(4))?;
    serde_json::from_str(s).map_err(|e| invalid(format!("invalid native metadata: {e}")))
}
fn indented(s: &str) -> bool {
    s.starts_with([' ', '\t'])
}
fn split_property(s: &str) -> Result<(&str, &str)> {
    let (k, v) = s
        .split_once(':')
        .ok_or_else(|| invalid("expected a property or label"))?;
    if k.trim().is_empty()
        || !k
            .trim()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(invalid("invalid property key"));
    }
    Ok((k.trim(), v.trim()))
}
fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}
fn parse_section<'a>(
    lines: &[&'a str],
    budget: &mut Budget,
    limits: &ResourceLimits,
) -> Result<(Fields, Vec<RawGlyph<'a>>)> {
    let mut fields = Vec::new();
    let mut glyphs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.is_empty() {
            i += 1;
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            push(
                &mut fields,
                "yaff.comment",
                comment.strip_prefix([' ', '\t']).unwrap_or(comment),
                budget,
            )?;
            i += 1;
            continue;
        }
        if indented(line) {
            return Err(invalid("unexpected top-level indentation"));
        }
        if line.ends_with(':') {
            let mut end = i + 1;
            while end < lines.len()
                && (lines[end].trim().is_empty()
                    || indented(lines[end])
                    || lines[end].trim_end().ends_with(':'))
            {
                if !indented(lines[end])
                    && !lines[end].trim().is_empty()
                    && lines[end].trim_end().ends_with(':')
                    && end > i + 1
                    && lines[i + 1..end]
                        .iter()
                        .any(|l| indented(l) && !l.trim().is_empty())
                {
                    break;
                }
                end += 1;
            }
            let body = lines[i + 1..end]
                .iter()
                .find(|l| indented(l) && !l.trim().is_empty())
                .map(|l| l.trim())
                .unwrap_or("");
            let explicit = line.starts_with(['\'', '"', ':'])
                || line.as_bytes()[0].is_ascii_digit()
                || line.starts_with("u+")
                || line.starts_with("U+");
            let is_glyph = explicit || body.starts_with(['@', '.', '-']) && !body.contains(':');
            if is_glyph {
                if glyphs.len() >= limits.max_glyphs {
                    return Err(limit("YAFF glyph count", limits.max_glyphs));
                }
                budget.charge(
                    std::mem::size_of::<RawGlyph<'_>>() + std::mem::size_of::<Glyph>() + 128,
                )?;
                let mut glyph = RawGlyph {
                    labels: Vec::new(),
                    rows: Vec::new(),
                    fields: Vec::new(),
                };
                let mut j = i;
                let mut indent = None;
                let mut in_props = false;
                while j < end {
                    let line = lines[j].trim_end();
                    if line.is_empty() {
                        j += 1;
                        continue;
                    }
                    if !indented(line) {
                        if !glyph.rows.is_empty() {
                            break;
                        }
                        let label = line
                            .strip_suffix(':')
                            .ok_or_else(|| invalid("label missing colon"))?;
                        budget.charge(label.len() + 32)?;
                        glyph.labels.push(label.trim().into());
                        j += 1;
                        continue;
                    }
                    let n = line.len() - line.trim_start_matches([' ', '\t']).len();
                    let indent = *indent.get_or_insert(n);
                    if n != indent {
                        return Err(invalid("inconsistent glyph indentation"));
                    }
                    let value = &line[indent..];
                    if value.contains(':') {
                        in_props = true;
                        property(lines, &mut j, end, indent, &mut glyph.fields, budget)?;
                    } else {
                        if in_props {
                            return Err(invalid("pixel data follows glyph properties"));
                        }
                        budget.charge(std::mem::size_of::<&str>())?;
                        glyph.rows.push(value);
                        j += 1;
                    }
                }
                if glyph.rows.is_empty() {
                    return Err(invalid("missing glyph raster"));
                }
                glyphs.push(glyph);
                i = j;
                continue;
            }
        }
        if !glyphs.is_empty() {
            return Err(invalid("font property follows glyph definitions"));
        }
        property(lines, &mut i, lines.len(), 0, &mut fields, budget)?;
    }
    Ok((fields, glyphs))
}
fn push(fields: &mut Fields, k: &str, v: &str, budget: &mut Budget) -> Result<()> {
    if v.len() > budget.record_max {
        return Err(limit("YAFF property bytes", budget.record_max));
    }
    budget.charge(k.len().saturating_add(v.len()).saturating_add(96))?;
    fields.push((k.into(), v.into()));
    Ok(())
}
fn property(
    lines: &[&str],
    i: &mut usize,
    end: usize,
    indent: usize,
    fields: &mut Fields,
    budget: &mut Budget,
) -> Result<()> {
    let (k, v) = split_property(&lines[*i][indent..])?;
    *i += 1;
    if !v.is_empty() {
        return push(
            fields,
            k,
            if key(k) == "default-char" {
                v
            } else {
                unquote(v)
            },
            budget,
        );
    }
    let mut value = String::new();
    let mut has_line = false;
    while *i < end {
        let line = lines[*i];
        if line.trim().is_empty() {
            break;
        }
        let n = line.len() - line.trim_start_matches([' ', '\t']).len();
        if n <= indent {
            break;
        }
        let text = unquote(line.trim());
        if value.len().saturating_add(text.len()).saturating_add(1) > budget.record_max {
            return Err(limit("YAFF property bytes", budget.record_max));
        }
        budget.charge(text.len() + 1)?;
        if has_line {
            value.push('\n');
        }
        has_line = true;
        value.push_str(text);
        *i += 1;
    }
    if !has_line {
        return Err(invalid("property has no value"));
    }
    push(fields, k, &value, budget)
}
type ParsedLabels = (Vec<Codepoint>, Vec<Vec<u32>>, Option<String>, Properties);
fn parse_labels(labels: &[String]) -> Result<ParsedLabels> {
    let mut cp = Vec::new();
    let mut seq = Vec::new();
    let mut name = None;
    let mut extras = Properties::default();
    for label in labels {
        let s = label.trim();
        if s.is_empty() {
            continue;
        }
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            let tag = &s[1..s.len() - 1];
            if name.is_none() {
                name = Some(tag.into());
            } else {
                extras.push(Property::new("yaff.tag", tag));
            }
            continue;
        }
        if s.starts_with('u') && s.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("u+"))
            || s.starts_with("U+")
            || s.starts_with('\'')
        {
            let mut values = Vec::new();
            let mut rest = s;
            while !rest.is_empty() {
                rest = rest.trim_start();
                if let Some(quoted) = rest.strip_prefix('\'') {
                    // Closing quote is followed only by comma/end; quotes inside are literal.
                    let end = quoted
                        .char_indices()
                        .find(|&(n, c)| {
                            c == '\''
                                && (quoted[n + 1..].trim_start().starts_with(',')
                                    || quoted[n + 1..].trim().is_empty())
                        })
                        .map(|(n, _)| n)
                        .ok_or_else(|| invalid("unclosed character label"))?;
                    values.extend(quoted[..end].chars().map(u32::from));
                    rest = &quoted[end + 1..];
                } else {
                    let (part, next) = rest.split_once(',').unwrap_or((rest, ""));
                    if !part.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("u+")) {
                        return Err(invalid("mixed character label"));
                    }
                    let cp = u32::from_str_radix(part[2..].trim(), 16)
                        .map_err(|_| invalid("bad Unicode label"))?;
                    if char::from_u32(cp).is_none() {
                        return Err(invalid("non-scalar Unicode label"));
                    }
                    values.push(cp);
                    rest = next;
                    if rest.is_empty() {
                        break;
                    }
                    continue;
                }
                rest = rest.trim_start();
                if let Some(next) = rest.strip_prefix(',') {
                    rest = next;
                } else if !rest.is_empty() {
                    return Err(invalid("invalid character label separator"));
                }
            }
            match values.as_slice() {
                [] => return Err(invalid("empty character label")),
                [v] => cp.push(Codepoint::Unicode(*v)),
                _ => seq.push(values),
            }
            continue;
        }
        if s.as_bytes()[0].is_ascii_digit() {
            let mut values = s.split(',');
            let first = unsigned(values.next().unwrap_or(""))?;
            let mut value = first;

            for part in values {
                let v = unsigned(part)?;
                if first > 255 || v > 255 {
                    return Err(invalid("byte-list component exceeds 255"));
                }
                value = value
                    .checked_mul(256)
                    .and_then(|n| n.checked_add(v))
                    .ok_or_else(|| invalid("legacy code exceeds u32"))?;
            }

            cp.push(Codepoint::Legacy(value));
            continue;
        }
        // Pre-1.0 backward-compatible labels.
        if s.chars().count() == 1 || !s.is_ascii() {
            let values = s.chars().map(u32::from).collect::<Vec<_>>();
            if values.len() == 1 {
                cp.push(Codepoint::Unicode(values[0]));
            } else {
                seq.push(values);
            }
        } else if name.is_none() {
            name = Some(s.into());
        } else {
            extras.push(Property::new("yaff.tag", s));
        }
    }
    Ok((cp, seq, name, extras))
}
fn raster(
    rows: &[&str],
    levels: u32,
    budget: &mut Budget,
    limits: &ResourceLimits,
) -> Result<GlyphBitmap> {
    if !matches!(levels, 2 | 4 | 16 | 256) {
        return Err(invalid("levels must be 2, 4, 16 or 256"));
    }
    let empty = rows
        .iter()
        .all(|s| !s.is_empty() && s.bytes().all(|b| b == b'-'));
    let row_len = rows[0].len();
    if rows.iter().any(|r| r.len() != row_len) {
        return Err(invalid("ragged raster"));
    }
    let (w, h) = if empty {
        (row_len - 1, rows.len() - 1)
    } else {
        if levels == 256 && !row_len.is_multiple_of(2) {
            return Err(invalid("odd 256-level row"));
        }
        (row_len / if levels == 256 { 2 } else { 1 }, rows.len())
    };
    let width = u32::try_from(w).map_err(|_| invalid("raster width overflows"))?;
    let height = u32::try_from(h).map_err(|_| invalid("raster height overflows"))?;
    let pixels = w
        .checked_mul(h)
        .ok_or_else(|| limit("YAFF glyph pixels", limits.max_pixels_per_glyph))?;
    if pixels > limits.max_pixels_per_glyph {
        return Err(limit("YAFF glyph pixels", limits.max_pixels_per_glyph));
    }
    budget.charge(if levels == 2 {
        w.div_ceil(8).saturating_mul(h)
    } else {
        pixels
    })?;
    let b = BitmapLimits {
        max_pixels: limits.max_pixels_per_glyph,
        max_bytes: limits.max_total_bytes,
        ..BitmapLimits::default()
    };
    if levels == 2 {
        let mut bitmap = Bitmap::<Mono>::with_limits(width, height, b)?;
        if !empty {
            for (y, row) in rows.iter().enumerate() {
                for (x, v) in row.bytes().enumerate() {
                    match v {
                        b'.' => {}
                        b'@' => bitmap.set_pixel(x as u32, y as u32, 1)?,
                        _ => return Err(invalid("invalid monochrome pixel")),
                    }
                }
            }
        }
        Ok(bitmap.into())
    } else {
        let mut bitmap = Bitmap::<Gray8>::with_limits(width, height, b)?;
        if !empty {
            for (y, row) in rows.iter().enumerate() {
                for (x, v) in row
                    .as_bytes()
                    .chunks(if levels == 256 { 2 } else { 1 })
                    .enumerate()
                {
                    let value = if v.iter().all(|&b| b == b'.') {
                        0
                    } else if v.iter().all(|&b| b == b'@') {
                        levels - 1
                    } else {
                        u32::from_str_radix(
                            std::str::from_utf8(v).map_err(|_| invalid("invalid gray pixel"))?,
                            16,
                        )
                        .map_err(|_| invalid("invalid gray pixel"))?
                    };
                    if value >= levels {
                        return Err(invalid("gray sample exceeds levels"));
                    }
                    bitmap.set_pixel(x as u32, y as u32, (value * 255 / (levels - 1)) as u8)?;
                }
            }
        }
        Ok(bitmap.into())
    }
}
fn build_font(
    fields: Fields,
    raw: Vec<RawGlyph<'_>>,
    budget: &mut Budget,
    limits: &ResourceLimits,
) -> Result<Font> {
    let meta = field(&fields, "fontpixel.font")
        .map(|s| json::<FontMeta>(s, budget, limits))
        .transpose()?;
    let slots = meta.as_ref().map(|m| m.slots);
    let default_id = meta.as_ref().and_then(|m| m.metrics.default_glyph);
    if slots.is_some_and(|n| n > limits.max_glyphs) {
        return Err(limit("YAFF glyph slots", limits.max_glyphs));
    }
    let has_meta = meta.is_some();
    let mut font = if let Some(meta) = meta {
        meta.restore()?
    } else {
        let mut f = Font::default();
        f.source_format = Some("yaff".into());
        f
    };
    let expected = if has_meta {
        font_fields(&font)
    } else {
        Vec::new()
    };
    apply_font(&mut font, &fields, &expected, has_meta)?;
    let levels = field(&fields, "levels")
        .map(unsigned)
        .transpose()?
        .unwrap_or(2);
    let global_metrics = [
        metric(&fields, "left-bearing")?,
        metric(&fields, "shift-up")?,
        metric(&fields, "right-bearing")?,
    ];
    let mut next_slot = 0;
    for raw in raw {
        let gm = field(&raw.fields, "fontpixel.glyph")
            .map(|s| json::<GlyphMeta>(s, budget, limits))
            .transpose()?;
        let bitmap = raster(&raw.rows, levels, budget, limits)?;
        let (codes, sequences, name, extra) = parse_labels(&raw.labels)?;
        let mut glyph = Glyph::new(bitmap);
        let mut id = next_slot;
        let mut expected_fields = Vec::new();
        if let Some(meta) = gm {
            if meta.version != 1 {
                return Err(invalid("unsupported glyph metadata version"));
            }
            if meta.id < next_slot || meta.id >= slots.unwrap_or(limits.max_glyphs) {
                return Err(invalid("invalid glyph metadata slot"));
            }
            id = meta.id;
            props_depth(&meta.properties)?;
            glyph.codepoints = meta.codepoints;
            glyph.unicode_sequences = meta.unicode_sequences;
            glyph.name = meta.name;
            glyph.metrics = meta.metrics;
            glyph.properties = meta.properties;
            expected_fields = glyph_fields(&glyph, &font.metrics.glyph_defaults)?;
            let expected_labels = parse_labels(&labels(&glyph))?;
            if (
                codes.clone(),
                sequences.clone(),
                name.clone(),
                extra.clone(),
            ) != expected_labels
            {
                glyph.codepoints = codes;
                glyph.unicode_sequences = sequences;
                glyph.name = name;
                for p in extra.into_vec() {
                    glyph.properties.push(p);
                }
            }
            if !meta.gray
                && let GlyphBitmap::Gray8(gray) = &glyph.bitmap
            {
                if gray.as_bytes().iter().any(|&v| v != 0 && v != 255) {
                    return Err(invalid("gray pixels conflict with monochrome metadata"));
                }
                glyph.bitmap = gray.to_mono(128)?.into();
            }
        } else {
            glyph.codepoints = codes;
            glyph.unicode_sequences = sequences;
            glyph.name = name;
            glyph.properties = extra;
        }
        if glyph.metrics.bounds.width != glyph.bitmap.width()
            || glyph.metrics.bounds.height != glyph.bitmap.height()
        {
            expected_fields.clear();
        }
        glyph.metrics.bounds.width = glyph.bitmap.width();
        glyph.metrics.bounds.height = glyph.bitmap.height();
        apply_glyph(&mut glyph, &global_metrics, &raw.fields, &expected_fields)?;
        while next_slot < id {
            budget.charge(std::mem::size_of::<Glyph>() + 16)?;
            let hole = font.add_glyph(Glyph::default())?;
            font.remove_glyph(hole);
            next_slot += 1;
        }
        font.add_glyph(glyph)?;
        next_slot += 1;
    }
    if let Some(slots) = slots {
        while next_slot < slots {
            budget.charge(std::mem::size_of::<Glyph>() + 16)?;
            let hole = font.add_glyph(Glyph::default())?;
            font.remove_glyph(hole);
            next_slot += 1;
        }
    }
    if has_meta {
        font.metrics.default_glyph = default_id.filter(|id| font.glyph(*id).is_some());
        if default_id.is_some() && font.metrics.default_glyph.is_none() {
            return Err(invalid("default glyph refers to missing slot"));
        }
    }
    if !has_meta {
        font.update_bounds()?;
        if let Some(label) = field(&fields, "default-char") {
            let (c, _, name, _) = parse_labels(&[label.into()])?;
            font.metrics.default_character = c.first().copied();
            font.metrics.default_glyph = if let Some(cp) = c.first() {
                font.glyph_id(*cp)
            } else {
                font.glyphs()
                    .find(|(_, g)| g.name == name && name.is_some())
                    .map(|(id, _)| id)
            };
        }
    }
    font.validate(limits)?;
    Ok(font)
}
fn apply_font(f: &mut Font, fields: &Fields, expected: &Fields, has_meta: bool) -> Result<()> {
    let names = [
        "family",
        "subfamily",
        "name",
        "revision",
        "copyright",
        "notice",
        "foundry",
        "pixel-size",
        "ascent",
        "descent",
        "leading",
        "line-height",
        "underline-thickness",
        "underline-descent",
        "word-space",
        "point-size",
        "dpi",
        "encoding",
        "spacing",
        "default-char",
    ];
    for k in names {
        let v = field(fields, k);
        if has_meta && v == field(expected, k) {
            continue;
        }
        match k {
            "family" => f.names.family = v.map(str::to_owned),
            "subfamily" => f.names.style = v.map(str::to_owned),
            "name" => f.names.full = v.map(str::to_owned),
            "revision" => f.names.version = v.map(str::to_owned),
            "copyright" => f.names.copyright = v.map(str::to_owned),
            "notice" => f.names.license = v.map(str::to_owned),
            "foundry" => f.names.foundry = v.map(str::to_owned),
            "pixel-size" => f.metrics.pixel_size = v.map(unsigned).transpose()?,
            "ascent" => f.metrics.ascent = v.map(int).transpose()?,
            "descent" => f.metrics.descent = v.map(int).transpose()?,
            "leading" => f.metrics.line_gap = v.map(int).transpose()?,
            "line-height" => f.metrics.line_height = v.map(int).transpose()?,
            "underline-thickness" => f.metrics.underline_thickness = v.map(int).transpose()?,
            "word-space" => f.metrics.space_width = v.map(int).transpose()?,
            "underline-descent" => {
                f.metrics.underline_position = v
                    .map(|s| {
                        int(s)?
                            .checked_neg()
                            .ok_or_else(|| invalid("underline position overflow"))
                    })
                    .transpose()?
            }
            "point-size" => f.metrics.point_size = v.map(rational).transpose()?,
            "encoding" => f.charset.name = v.map(str::to_owned),
            "dpi" => {
                f.metrics.resolution = v
                    .map(|s| {
                        let parts = s
                            .split([' ', ',', 'x'])
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>();
                        match parts.as_slice() {
                            [a] => Ok(Vector::new(int(a)?, int(a)?)),
                            [a, b] => Ok(Vector::new(int(a)?, int(b)?)),
                            _ => Err(invalid("dpi requires one or two integers")),
                        }
                    })
                    .transpose()?
            }
            "spacing" => {
                f.metrics.spacing = v
                    .map(|s| match s {
                        "proportional" => Ok(Spacing::Proportional),
                        "monospace" => Ok(Spacing::Monospaced),
                        "character-cell" | "multi-cell" => Ok(Spacing::CharacterCell),
                        _ => Err(invalid("unknown spacing")),
                    })
                    .transpose()?
            }
            "default-char" => {
                f.metrics.default_character = v
                    .map(|s| parse_labels(&[s.into()]).map(|p| p.0.first().copied()))
                    .transpose()?
                    .flatten();
                f.metrics.default_glyph = None;
            }
            _ => {}
        }
    }
    for (k, v) in fields {
        if has_meta
            && matches!(
                key(k).as_str(),
                "levels" | "left-bearing" | "right-bearing" | "shift-up" | "offset" | "tracking"
            )
        {
            continue;
        }
        if (!has_meta || !names.contains(&key(k).as_str()))
            && !matches!(
                key(k).as_str(),
                "yaff" | "fontpixel.font" | "fontpixel.container"
            )
        {
            f.properties.push(Property::new(k.clone(), v.clone()));
        }
    }
    Ok(())
}
fn metric(fields: &Fields, k: &str) -> Result<i64> {
    if let Some(v) = field(fields, k) {
        return int(v);
    }
    if k == "right-bearing"
        && let Some(v) = field(fields, "tracking")
    {
        return int(v);
    }
    if matches!(k, "left-bearing" | "shift-up")
        && let Some(v) = field(fields, "offset")
    {
        let v = v
            .split([' ', ','])
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if v.len() != 2 {
            return Err(invalid("offset requires two integers"));
        }
        return int(v[usize::from(k == "shift-up")]);
    }
    Ok(0)
}
fn apply_glyph(g: &mut Glyph, global: &[i64; 3], local: &Fields, expected: &Fields) -> Result<()> {
    let mut changed = expected.is_empty();
    for (i, k) in ["left-bearing", "shift-up", "right-bearing"]
        .into_iter()
        .enumerate()
    {
        if global[i] != 0 || field(local, k) != field(expected, k) {
            changed = true;
        }
    }
    if changed {
        let x = add(global[0], metric(local, "left-bearing")?)?;
        let y = add(global[1], metric(local, "shift-up")?)?;
        let right = add(global[2], metric(local, "right-bearing")?)?;
        g.metrics.bounds.x = x;
        g.metrics.bounds.y = y;
        g.metrics.advance = Some(Vector::new(
            add(add(x, i64::from(g.bitmap.width()))?, right)?,
            g.metrics.advance.map_or(0, |a| a.y),
        ));
    }
    for (k, v) in local {
        if key(k) == "path" && g.bitmap.width() == 0 {
            return Err(invalid("path-only glyph rasterization is unsupported"));
        }
        if (expected.is_empty()
            || !matches!(
                key(k).as_str(),
                "left-bearing"
                    | "shift-up"
                    | "right-bearing"
                    | "offset"
                    | "tracking"
                    | "fontpixel.glyph"
            ))
            && key(k) != "fontpixel.glyph"
        {
            g.properties.push(Property::new(k.clone(), v.clone()));
        }
    }
    Ok(())
}
