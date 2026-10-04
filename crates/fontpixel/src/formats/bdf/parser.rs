use super::source::*;
use super::*;
use crate::bitmap::Bounds;
use crate::charset::{charset_table, detect_gl};
use crate::model::{Codepoint, GlyphMetrics, Properties, Property, Vector};

// Only these seven fields participate in first-value-wins glyph parsing.
// Unknown fields and duplicate records still retain their original properties.
#[derive(Default)]
struct UsedFields(u8);
impl UsedFields {
    fn bit(key: &str) -> u8 {
        match key {
            "ENCODING" => 1,
            "BBX" => 2,
            "SWIDTH" => 4,
            "DWIDTH" => 8,
            "SWIDTH1" => 16,
            "DWIDTH1" => 32,
            "VVECTOR" => 64,
            _ => 0,
        }
    }
    fn contains(&self, key: &str) -> bool {
        self.0 & Self::bit(key) != 0
    }
    fn insert(&mut self, key: &str) {
        self.0 |= Self::bit(key);
    }
}

#[derive(Default)]
struct Pending {
    name: String,
    code: Option<i32>,
    alt: Option<i32>,
    metrics: GlyphMetrics,
    used: UsedFields,
    rows: String,
    properties: Properties,
    start: usize,
}
struct Parser<'a> {
    font: Font,
    report: Report,
    bitmap_report: Report,
    limits: &'a ResourceLimits,
    state: u8,
    line: usize,
    chars: Option<i32>,
    propcount: Option<String>,
    propseen: usize,
    glyphs: usize,
    pending: Pending,
    budget: usize,
    bitmap_bytes: usize,
    // Once glyph parsing starts, no transition returns to headers/properties.
    // Memoize the projected charset only when the first encoded glyph needs it.
    unicode: Option<bool>,
}
impl<'a> Parser<'a> {
    fn new(limits: &'a ResourceLimits) -> Self {
        let mut font = Font::default();
        font.source_format = Some("bdf".into());
        Self {
            font,
            report: Report::default(),
            bitmap_report: Report::default(),
            limits,
            state: 0,
            line: 0,
            chars: None,
            propcount: None,
            propseen: 0,
            glyphs: 0,
            pending: Pending::default(),
            budget: 0,
            bitmap_bytes: 0,
            unicode: None,
        }
    }
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.budget = self.budget.checked_add(bytes).ok_or(Error::ResourceLimit {
            resource: "BDF model bytes",
            limit: self.limits.max_total_bytes,
        })?;
        if self.budget > self.limits.max_total_bytes {
            return Err(Error::ResourceLimit {
                resource: "BDF model bytes",
                limit: self.limits.max_total_bytes,
            });
        }
        Ok(())
    }
    fn warning(&mut self, code: &str, impact: Impact, message: String) -> Result<()> {
        self.charge(message.len() + 160)?;
        let glyph = if self.state == 4 || self.state == 5 {
            Some((
                self.glyphs - 1,
                self.pending.code.and_then(|n| n.try_into().ok()),
            ))
        } else {
            None
        };
        diagnostic(
            &mut self.report,
            Level::Warning,
            impact,
            code,
            glyph,
            message,
        );
        Ok(())
    }
    fn error(&self, message: String) -> Error {
        Error::BdfParse {
            line: self.line,
            message,
        }
    }
    fn num(&self, token: &str, key: &str, value: &str) -> Result<i32> {
        if !int_syntax(token) {
            return Err(self.error(format!("malformed number in {key} {}", py_repr(value))));
        }
        token.parse().map_err(|_| {
            self.error(format!(
                "number {token} in {key} is out of the 32-bit range"
            ))
        })
    }
    fn ints(&self, value: &str, n: usize, key: &str) -> Result<[i64; 4]> {
        // Every caller requests at most four numbers. Gather before parsing so
        // too few tokens still takes precedence over a malformed earlier token.
        debug_assert!(n <= 4);
        let mut parts = [""; 4];
        let mut count = 0;
        // VVECTOR strips whitespace but not commas; a leading/trailing comma is a token.
        let commas = key == "VVECTOR";
        for part in (commas && value.starts_with(','))
            .then_some("")
            .into_iter()
            .chain(
                value
                    .split(|c| ws(c) || commas && c == ',')
                    .filter(|s| !s.is_empty()),
            )
            .chain((commas && value.ends_with(',')).then_some(""))
            .take(n)
        {
            parts[count] = part;
            count += 1;
        }
        if count < n {
            return Err(self.error(format!("{key} needs {n} numbers, got {}", py_repr(value))));
        }
        let mut values = [0; 4];
        for (out, token) in values.iter_mut().zip(&parts).take(n) {
            *out = i64::from(self.num(token, key, value)?);
        }
        Ok(values)
    }
    fn header(&mut self, key: &str, value: PropertyValue, line: &str) -> Result<()> {
        self.charge(std::mem::size_of::<Property>() + key.len() + line.len() * 2 + 64)?;
        self.font.headers.push(Property {
            name: key.into(),
            value,
            raw: Some(line.as_bytes().to_vec()),
        });
        Ok(())
    }
    fn end_headers(&mut self) {
        if self.font.headers.get("METRICSSET").is_none() {
            self.font.headers.push(Property::new("METRICSSET", 0_i64));
        }
        project(&mut self.font);
    }
    fn start(&mut self, name: &str) -> Result<()> {
        if self.glyphs >= self.limits.max_glyphs {
            return Err(Error::ResourceLimit {
                resource: "glyph count",
                limit: self.limits.max_glyphs,
            });
        }
        self.charge(512 + name.len())?;
        self.glyphs += 1;
        self.pending = Pending {
            name: name.into(),
            start: self.line,
            ..Pending::default()
        };
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        if !self.pending.used.contains("BBX") {
            return Err(self.error(format!(
                "glyph '{}' (starting at line {}) has no BBX",
                self.pending.name, self.pending.start
            )));
        }
        if self.pending.code.is_none() {
            self.warning(
                "bdf.parse.missing_encoding",
                Impact::Repair,
                format!(
                    "line {}: glyph '{}' has no ENCODING, treated as unencoded",
                    self.pending.start, self.pending.name
                ),
            )?;
        } else if self.pending.code.is_some_and(|c| c < -1) {
            self.warning(
                "bdf.parse.negative_encoding",
                Impact::Repair,
                format!(
                    "line {}: glyph '{}' has negative ENCODING {}, treated as unencoded",
                    self.pending.start,
                    self.pending.name,
                    self.pending.code.unwrap_or(-1)
                ),
            )?;
        }
        let code = self.pending.code.filter(|&n| n >= 0).map(|n| n as u32);
        let cp = code.map(|n| {
            if *self
                .unicode
                .get_or_insert_with(|| known_unicode(&self.font))
            {
                Codepoint::Unicode(n)
            } else {
                Codepoint::Legacy(n)
            }
        });
        if let Some(cp) = cp
            && self.font.glyph_id(cp).is_some()
        {
            self.warning(
                "bdf.parse.duplicate_encoding",
                Impact::None,
                format!(
                    "line {}: duplicate codepoint {} (glyph '{}'), the later one wins",
                    self.pending.start,
                    cp.value(),
                    self.pending.name
                ),
            )?;
        }
        let b = self.pending.metrics.bounds;
        let stride = (b.width as usize).div_ceil(8);
        let size = stride
            .checked_mul(b.height as usize)
            .ok_or(Error::ResourceLimit {
                resource: "BDF bitmap bytes",
                limit: self.limits.max_total_bytes,
            })?;
        self.bitmap_bytes = self
            .bitmap_bytes
            .checked_add(size)
            .ok_or(Error::ResourceLimit {
                resource: "BDF bitmap bytes",
                limit: self.limits.max_total_bytes,
            })?;
        if self.bitmap_bytes > self.limits.max_total_bytes {
            return Err(Error::ResourceLimit {
                resource: "BDF bitmap bytes",
                limit: self.limits.max_total_bytes,
            });
        }
        self.charge(size.saturating_mul(3).saturating_add(256))?;
        let mut data = crate::bitmap::zeros(size, self.limits.max_table_bytes)?;
        let mut short = false;
        let mut long = false;
        let mut invalid = false;
        let mut dirty = false;
        let mut count = 0;
        for (r, row) in self.pending.rows.split_terminator('\n').enumerate() {
            count += 1;
            if !row.bytes().all(|b| b.is_ascii_hexdigit()) {
                invalid = true;
                continue;
            }
            if r >= b.height as usize {
                continue;
            }
            if row.len().saturating_mul(4) < b.width as usize {
                short = true;
            }
            if row.len() != stride * 2 {
                long |= row.len() > stride * 2;
            }
            for (j, digit) in row.bytes().take(stride * 2).enumerate() {
                let value = match digit {
                    b'0'..=b'9' => digit - b'0',
                    b'A'..=b'F' => digit - b'A' + 10,
                    _ => digit - b'a' + 10,
                };
                data[r * stride + j / 2] |= value << if j % 2 == 0 { 4 } else { 0 };
            }
            if !b.width.is_multiple_of(8) && stride > 0 {
                let mask = 0xff << (8 - b.width % 8);
                dirty |= data[r * stride + stride - 1] & !mask != 0;
            }
        }
        let bitmap =
            Bitmap::<Mono>::from_raw(b.width, b.height, &data, bitmap_limits(self.limits))?;
        let mut glyph = Glyph {
            codepoints: cp.into_iter().collect(),
            name: Some(self.pending.name.clone()),
            metrics: self.pending.metrics.clone(),
            bitmap: bitmap.into(),
            properties: std::mem::take(&mut self.pending.properties),
            ..Glyph::default()
        };
        if let Some(code) = self.pending.code.filter(|&v| v < 0) {
            glyph
                .properties
                .push(Property::new(ENCODING, i64::from(code)));
        }
        if let Some(alt) = self.pending.alt {
            glyph.properties.push(Property::new(ALT, i64::from(alt)));
        }
        save_bitmap(&mut glyph, std::mem::take(&mut self.pending.rows));
        let index = self.glyphs - 1;
        if count < b.height as usize {
            let message = format!(
                "Glyph \"{}\" (codepoint {}): {} source rows for height {}; zero-padded core view, original rows retained",
                self.pending.name,
                code.map_or(-1, i64::from),
                count,
                b.height
            );
            self.charge(message.len() + 160)?;
            diagnostic(
                &mut self.bitmap_report,
                Level::Info,
                Impact::Repair,
                "bdf.bitmap.missing_rows",
                Some((index, cp.map(Codepoint::value))),
                message,
            );
        }
        for (condition, code, message) in [
            (
                count > b.height as usize,
                "bdf.bitmap.extra_rows",
                "Extra bitmap rows excluded from the core rectangle; source rows retained",
            ),
            (
                short,
                "bdf.bitmap.short_row",
                "Short bitmap rows zero-padded in the core rectangle; source rows retained",
            ),
            (
                long,
                "bdf.bitmap.long_row",
                "Long bitmap rows clipped in the core rectangle; source rows retained",
            ),
            (
                dirty,
                "bdf.bitmap.padding",
                "Nonzero right padding cleared in the core rectangle; source rows retained",
            ),
            (
                invalid,
                "bdf.bitmap.invalid_hex",
                "Non-hex bitmap rows retained; core placeholder rows are zero. validate_bitmap rejects drawing this glyph",
            ),
        ] {
            if condition {
                self.charge(message.len() + 160)?;
                diagnostic(
                    &mut self.bitmap_report,
                    Level::Info,
                    Impact::Repair,
                    code,
                    Some((index, cp.map(Codepoint::value))),
                    message.into(),
                );
            }
        }
        self.font.add_glyph(glyph)?;
        Ok(())
    }
    fn row(&mut self, line: &str) -> Result<()> {
        self.charge(line.len() + 1)?;
        if self
            .pending
            .rows
            .len()
            .saturating_add(line.len())
            .saturating_add(1)
            > self.limits.max_table_bytes
        {
            return Err(Error::ResourceLimit {
                resource: "BDF glyph source rows",
                limit: self.limits.max_table_bytes,
            });
        }
        self.pending.rows.push_str(line);
        self.pending.rows.push('\n');
        Ok(())
    }
    fn prop_value(&mut self, value: &str) -> Result<PropertyValue> {
        if let Some(value) = value.strip_prefix('"') {
            let mut out = String::new();
            let mut chars = value.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        out.push('"');
                    } else {
                        return Ok(PropertyValue::Text(out));
                    }
                } else {
                    out.push(c);
                }
            }
            self.warning(
                "bdf.parse.unclosed_quote",
                Impact::Repair,
                format!("line {}: property value has no closing quote", self.line),
            )?;
            return Ok(PropertyValue::Text(out));
        }
        if int_syntax(value)
            && let Ok(v) = value.parse::<i64>()
            && (i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&v)
        {
            return Ok(PropertyValue::Integer(v));
        }
        Ok(PropertyValue::Text(value.into()))
    }
    fn line(&mut self, line: &str) -> Result<()> {
        let (key, value) = kv(line);
        if self.state == 5 {
            match key {
                "ENDCHAR" => {
                    self.finish()?;
                    self.state = 3;
                }
                "COMMENT" => {
                    self.charge(value.len() + 128)?;
                    self.pending
                        .properties
                        .push(Property::new("bdf.bitmap.comment", value));
                }
                "STARTCHAR" | "ENDFONT" => {
                    self.warning(
                        "bdf.parse.missing_endchar",
                        Impact::Repair,
                        format!(
                            "line {}: missing ENDCHAR for glyph '{}'",
                            self.line, self.pending.name
                        ),
                    )?;
                    self.finish()?;
                    if key == "STARTCHAR" {
                        self.start(value)?;
                        self.state = 4;
                    } else {
                        self.state = 6;
                    }
                }
                _ => self.row(line)?,
            }
            return Ok(());
        }
        if self.state == 4 {
            match key {
                "ENCODING" if !self.pending.used.contains(key) => {
                    let mut parts = value.split(ws).filter(|s| !s.is_empty());
                    let Some(first) = parts.next() else {
                        return Err(self.error("ENCODING needs a number".into()));
                    };
                    let code = self.num(first, key, value)?;
                    let alt = parts.next().map(|n| self.num(n, key, value)).transpose()?;
                    self.pending.code = Some(code);
                    self.pending.alt = if code < 0 { alt } else { None };
                    self.pending.used.insert(key);
                }
                "BBX" if !self.pending.used.contains(key) => {
                    let v = self.ints(value, 4, key)?;
                    if v[0] < 0 || v[1] < 0 {
                        return Err(self.error(format!(
                            "glyph '{}' has a negative BBX size {} {}",
                            self.pending.name, v[0], v[1]
                        )));
                    }
                    let bounds = Bounds::new(v[0] as u32, v[1] as u32, v[2], v[3]);
                    let size = (bounds.width as usize)
                        .div_ceil(8)
                        .checked_mul(bounds.height as usize)
                        .ok_or(Error::ResourceLimit {
                            resource: "BDF glyph bytes",
                            limit: self.limits.max_table_bytes,
                        })?;
                    bitmap_limits(self.limits).check(bounds.width, bounds.height, size)?;
                    self.pending.metrics.bounds = bounds;
                    self.pending.used.insert(key);
                }
                key if METRICS.contains(&key) && !self.pending.used.contains(key) => {
                    let v = self.ints(value, 2, key)?;
                    set_vector(&mut self.pending.metrics, key, Vector::new(v[0], v[1]));
                    self.pending.used.insert(key);
                }
                "ENCODING" | "BBX" | "SWIDTH" | "DWIDTH" | "SWIDTH1" | "DWIDTH1" | "VVECTOR" => {
                    self.charge(line.len() + 128)?;
                    self.pending
                        .properties
                        .push(Property::new(format!("bdf.duplicate.{key}"), value));
                }
                "BITMAP" => self.state = 5,
                "ENDCHAR" => {
                    self.finish()?;
                    self.state = 3;
                }
                "STARTCHAR" | "ENDFONT" => {
                    self.warning(
                        "bdf.parse.missing_endchar",
                        Impact::Repair,
                        format!(
                            "line {}: missing ENDCHAR for glyph '{}'",
                            self.line, self.pending.name
                        ),
                    )?;
                    self.finish()?;
                    if key == "STARTCHAR" {
                        self.start(value)?;
                    } else {
                        self.state = 6;
                    }
                }
                _ => {
                    self.warning(
                        "bdf.parse.unknown_glyph",
                        Impact::None,
                        format!(
                            "line {}: unknown keyword {} in glyph {} ignored",
                            self.line,
                            py_repr(key),
                            py_repr(&self.pending.name)
                        ),
                    )?;
                    self.charge(line.len() + 128)?;
                    self.pending.properties.push(Property {
                        name: key.into(),
                        value: PropertyValue::Text(value.into()),
                        raw: Some(line.as_bytes().to_vec()),
                    });
                }
            }
            return Ok(());
        }
        if self.state == 3 {
            match key {
                "STARTCHAR" => {
                    self.start(value)?;
                    self.state = 4;
                }
                "ENDFONT" => self.state = 6,
                "CHARS" if self.chars.is_none() => {
                    self.chars = Some(self.ints(value, 1, key)?[0] as i32)
                }
                _ => self.warning(
                    "bdf.parse.unexpected_line",
                    Impact::Loss,
                    format!(
                        "line {}: unexpected line {} ignored",
                        self.line,
                        py_repr(line)
                    ),
                )?,
            }
            return Ok(());
        }
        if self.state == 1 {
            if key == "ENDPROPERTIES" {
                if let Some(count) = &self.propcount
                    && *count != self.propseen.to_string()
                {
                    self.warning("bdf.parse.property_count", Impact::Repair, format!("The property count next to 'STARTPROPERTIES' keyword is {count}, which does not match the actual property count {}", self.propseen))?;
                }
                self.state = 2;
                project(&mut self.font);
            } else if key.eq_ignore_ascii_case("COMMENT") {
                if key != "COMMENT" {
                    self.warning(
                        "bdf.parse.comment_case",
                        Impact::Repair,
                        format!(
                            "line {}: property {} treated as a COMMENT line",
                            self.line,
                            py_repr(key)
                        ),
                    )?;
                }
                self.charge(line.len() * 2 + 128)?;
                self.font.properties.push(Property {
                    name: "COMMENT".into(),
                    value: PropertyValue::Text(comment(value).into()),
                    raw: Some(line.as_bytes().to_vec()),
                });
            } else if matches!(key, "CHARS" | "STARTCHAR" | "ENDFONT") {
                self.warning(
                    "bdf.parse.missing_endproperties",
                    Impact::Repair,
                    format!("line {}: missing 'ENDPROPERTIES'", self.line),
                )?;
                project(&mut self.font);
                if key == "CHARS" {
                    self.chars = Some(self.ints(value, 1, key)?[0] as i32);
                    self.state = 3;
                } else if key == "STARTCHAR" {
                    self.warning(
                        "bdf.parse.missing_chars",
                        Impact::Repair,
                        "Cannot find 'CHARS' line".into(),
                    )?;
                    self.start(value)?;
                    self.state = 4;
                } else {
                    self.warning(
                        "bdf.parse.no_glyphs",
                        Impact::None,
                        "This font does not have any glyphs".into(),
                    )?;
                    self.state = 6;
                }
            } else {
                self.propseen += 1;
                self.charge(line.len() * 2 + 128)?;
                let v = if value.is_empty() {
                    PropertyValue::Flag
                } else {
                    self.prop_value(value)?
                };
                self.font.properties.push(Property {
                    name: key.into(),
                    value: v,
                    raw: Some(line.as_bytes().to_vec()),
                });
            }
            return Ok(());
        }
        if self.state == 2 {
            match key {
                "CHARS" => {
                    self.chars = Some(self.ints(value, 1, key)?[0] as i32);
                    self.state = 3;
                }
                "STARTCHAR" => {
                    self.warning(
                        "bdf.parse.missing_chars",
                        Impact::Repair,
                        "Cannot find 'CHARS' line next to 'ENDPROPERTIES' line".into(),
                    )?;
                    self.start(value)?;
                    self.state = 4;
                }
                "ENDFONT" => {
                    self.warning(
                        "bdf.parse.no_glyphs",
                        Impact::None,
                        "This font does not have any glyphs".into(),
                    )?;
                    self.state = 6;
                }
                _ => self.warning(
                    "bdf.parse.unexpected_line",
                    Impact::Loss,
                    format!(
                        "line {}: unexpected line {} ignored",
                        self.line,
                        py_repr(line)
                    ),
                )?,
            }
            return Ok(());
        }
        match key {
            "STARTFONT" => {
                let mut parts = value.split('.');
                let valid = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
                if !parts.next().is_some_and(valid)
                    || parts.next().is_some_and(|p| !valid(p))
                    || parts.next().is_some()
                {
                    return Err(self.error(format!(
                        "malformed version number in STARTFONT {}",
                        py_repr(value)
                    )));
                }
                self.header(key, PropertyValue::Decimal(value.into()), line)?;
            }
            "FONT" => self.header(key, PropertyValue::Text(value.into()), line)?,
            "COMMENT" => self.header(key, PropertyValue::Text(comment(value).into()), line)?,
            "SIZE" | "FONTBOUNDINGBOX" | "SWIDTH" | "DWIDTH" | "SWIDTH1" | "DWIDTH1"
            | "VVECTOR" => {
                let n = match key {
                    "SIZE" => 3,
                    "FONTBOUNDINGBOX" => 4,
                    _ => 2,
                };
                let v = self.ints(value, n, key)?;
                if key == "FONTBOUNDINGBOX" && (v[0] < 0 || v[1] < 0) {
                    return Err(
                        self.error(format!("negative FONTBOUNDINGBOX size {} {}", v[0], v[1]))
                    );
                }
                self.header(
                    key,
                    PropertyValue::List(
                        v.into_iter().take(n).map(PropertyValue::Integer).collect(),
                    ),
                    line,
                )?;
            }
            "METRICSSET" | "CONTENTVERSION" => {
                let n = self.ints(value, 1, key)?[0];
                self.header(key, PropertyValue::Integer(n), line)?;
            }
            "STARTPROPERTIES" => {
                self.end_headers();
                if let Some(token) = value.split(ws).find(|s| !s.is_empty()) {
                    if int_syntax(token) {
                        self.propcount = Some(normalize_integer(token));
                    } else {
                        self.warning(
                            "bdf.parse.property_count_syntax",
                            Impact::Repair,
                            format!(
                                "line {}: malformed property count {}",
                                self.line,
                                py_repr(value)
                            ),
                        )?;
                    }
                }
                self.state = 1;
            }
            "CHARS" | "STARTCHAR" | "ENDFONT" => {
                self.warning("bdf.parse.missing_properties", Impact::Repair, "It looks like the font does not have property block beginning with 'STARTPROPERTIES' keyword".into())?;
                if key == "STARTCHAR" {
                    self.warning(
                        "bdf.parse.missing_chars",
                        Impact::Repair,
                        "Cannot find 'CHARS' line".into(),
                    )?;
                }
                if key == "ENDFONT" {
                    self.warning(
                        "bdf.parse.no_glyphs",
                        Impact::None,
                        "This font does not have any glyphs".into(),
                    )?;
                }
                self.end_headers();
                if key == "CHARS" {
                    self.chars = Some(self.ints(value, 1, key)?[0] as i32);
                    self.state = 3;
                } else if key == "STARTCHAR" {
                    self.start(value)?;
                    self.state = 4;
                } else {
                    self.state = 6;
                }
            }
            _ => {
                self.warning(
                    "bdf.parse.unknown_header",
                    Impact::None,
                    format!(
                        "line {}: unknown header keyword {} ignored",
                        self.line,
                        py_repr(key)
                    ),
                )?;
                self.header(key, PropertyValue::Text(value.into()), line)?;
            }
        }
        Ok(())
    }
}
fn comment(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}
fn normalize_integer(value: &str) -> String {
    let digits = value.trim_start_matches(['+', '-']).trim_start_matches('0');
    if digits.is_empty() {
        "0".into()
    } else if value.starts_with('-') {
        format!("-{digits}")
    } else {
        digits.into()
    }
}

pub(super) fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut parser = Parser::new(limits);
    let lines = text.split_inclusive(['\r', '\n']);
    let mut after_cr = false;
    for raw in lines {
        if after_cr && raw == "\n" {
            after_cr = false;
            continue;
        }
        after_cr = raw.ends_with('\r');
        parser.line += 1;
        if raw.len() > limits.max_table_bytes {
            return Err(Error::ResourceLimit {
                resource: "BDF line bytes",
                limit: limits.max_table_bytes,
            });
        }
        if parser.state == 5 {
            // Ordinary unindented hex rows need no keyword/whitespace scan.
            // CRLF bookkeeping and raw-line limits above still run first;
            // every control, empty, indented or malformed line falls through.
            let row = raw
                .strip_suffix('\n')
                .or_else(|| raw.strip_suffix('\r'))
                .unwrap_or(raw);
            if !row.is_empty() && row.bytes().all(|b| b.is_ascii_hexdigit()) {
                parser.row(row)?;
                continue;
            }
        }
        let line = raw.trim_matches(ws);
        if line.is_empty() {
            continue;
        }
        parser.line(line)?;
        if parser.state == 6 {
            break;
        }
    }
    if parser.state <= 1 {
        return Err(Error::BdfParse {
            line: parser.line + 1,
            message: "Unexpected end of file before the end of the headers".into(),
        });
    }
    if matches!(parser.state, 4 | 5) {
        parser.warning(
            "bdf.parse.missing_endchar",
            Impact::Repair,
            format!(
                "Unexpected end of file: missing ENDCHAR for glyph '{}'",
                parser.pending.name
            ),
        )?;
        parser.finish()?;
    }
    if parser.state != 6 {
        parser.warning(
            "bdf.parse.missing_endfont",
            Impact::Repair,
            "This font does not have 'ENDFONT' keyword".into(),
        )?;
    }
    if parser.state >= 3 && (parser.chars.is_some() || parser.glyphs != 0) {
        if let Some(chars) = parser.chars {
            if i64::from(chars) != parser.glyphs as i64 {
                parser.warning("bdf.parse.glyph_count", Impact::Repair, format!("The glyph count next to 'CHARS' keyword is {chars}, which does not match the actual glyph count {}", parser.glyphs))?;
            }
        } else {
            parser.warning(
                "bdf.parse.glyph_count_missing",
                Impact::Repair,
                "The glyph count next to 'CHARS' keyword does not exist".into(),
            )?;
        }
    }
    // Snapshot the source, then provide useful derived views without adding wire properties.
    project(&mut parser.font);
    save_snapshot(&mut parser.font);
    let key = parser.font.charset.canonical_name();
    parser.font.charset.gl = Some(detect_gl(
        &key,
        parser
            .font
            .charset
            .encoding
            .as_deref()
            .map(|s| s.trim_matches(ws)),
        parser.font.codepoints().map(|(c, _)| c.value()),
    ));
    parser.font.headers.push(Property::new(
        SOURCE_GL,
        i64::from(parser.font.charset.gl.unwrap_or(false)),
    ));
    if !crate::charset::is_unicode_charset(&key) && !charset_table().iter().any(|i| i.key == key) {
        diagnostic(
            &mut parser.bitmap_report,
            Level::Info,
            Impact::None,
            "bdf.metadata.unknown_charset",
            None,
            format!("Unknown charset '{key}', characters are mapped to codepoints as Unicode"),
        );
    }
    let missing_ascent =
        parser.font.metrics.ascent.is_none() || parser.font.metrics.descent.is_none();
    let missing_pixel = parser.font.metrics.pixel_size.is_none();
    derive(&mut parser.font);
    if missing_ascent && parser.font.metrics.bounds.is_some() {
        diagnostic(&mut parser.bitmap_report, Level::Info, Impact::Repair, "bdf.metadata.derived_metrics", None, "Absent ascent/descent derived from FONTBOUNDINGBOX in the core view; source absence retained".into());
    }
    if missing_pixel && parser.font.metrics.pixel_size.is_some() {
        diagnostic(&mut parser.bitmap_report, Level::Info, Impact::Repair, "bdf.metadata.derived_pixel_size", None, "Absent PIXEL_SIZE derived from XLFD, SIZE or FONTBOUNDINGBOX in the core view; source absence retained".into());
    }
    if std::str::from_utf8(bytes).is_err() {
        diagnostic(
            &mut parser.bitmap_report,
            Level::Warning,
            Impact::Loss,
            "bdf.encoding.invalid_utf8",
            None,
            "Invalid UTF-8 replaced with U+FFFD, as in SPEC load_bytes".into(),
        );
    }
    parser.report.extend(parser.bitmap_report);
    Ok((parser.font, parser.report))
}
