use super::*;
use crate::model::{Property, PropertyValue, Rational};
use crate::resources::ResourcePath;

pub(super) struct Range {
    pub first: u32,
    pub last: u32,
    pub path: ResourcePath,
    pub offset: usize,
}
pub(super) struct Header {
    pub font: Font,
    pub scheme: String,
    pub bitmap: Bounds,
    pub bounds: Bounds,
    pub chars: Option<usize>,
    pub space: Space,
    pub ranges: Vec<Range>,
}
/// Dense ranks skip forbidden low bytes without scanning the numeric code space.
pub(super) struct Space {
    pub second: Vec<u8>,
    pub third: Vec<u8>,
}
impl Space {
    pub fn rank(&self, code: u32) -> Result<usize> {
        let find = |bytes: &[u8], byte: u8| {
            bytes
                .binary_search(&byte)
                .map_err(|_| invalid("code endpoint lies outside byte ranges"))
        };
        if self.second.is_empty() {
            if code > 255 {
                return Err(invalid("multi-byte code requires byte ranges"));
            }
            return Ok(code as usize);
        }
        if self.third.is_empty() {
            if code > 65535 {
                return Err(invalid("code exceeds two-byte coding space"));
            }
            return Ok((code as usize >> 8) * self.second.len() + find(&self.second, code as u8)?);
        }
        if code > 0xffffff {
            return Err(invalid("code exceeds three-byte coding space"));
        }
        Ok(
            ((code as usize >> 16) * self.second.len() + find(&self.second, (code >> 8) as u8)?)
                * self.third.len()
                + find(&self.third, code as u8)?,
        )
    }
    pub fn count(&self, first: u32, last: u32) -> Result<usize> {
        self.rank(last)?
            .checked_sub(self.rank(first)?)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("reversed code range"))
    }
    pub fn code(&self, rank: usize) -> u32 {
        if self.second.is_empty() {
            return rank as u32;
        }
        if self.third.is_empty() {
            return (((rank / self.second.len()) as u32) << 8)
                | u32::from(self.second[rank % self.second.len()]);
        }
        let lower = self.third.len();
        let higher = rank / lower;
        (((higher / self.second.len()) as u32) << 16)
            | (u32::from(self.second[higher % self.second.len()]) << 8)
            | u32::from(self.third[rank % lower])
    }
}

pub(super) fn unsigned(s: &str) -> Result<u64> {
    if s.is_empty() || s.starts_with(['+', '-']) {
        return Err(invalid("invalid unsigned integer"));
    }
    let (radix, digits) = if let Some(t) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, t)
    } else if s.len() > 1 && s.starts_with('0') {
        (8, &s[1..])
    } else {
        (10, s)
    };
    u64::from_str_radix(digits, radix).map_err(|_| invalid("invalid or overflowing integer"))
}
pub(super) fn integer(s: &str) -> Result<i64> {
    if let Some(t) = s.strip_prefix('-') {
        let n = unsigned(t)?;
        if n == 1u64 << 63 {
            Ok(i64::MIN)
        } else {
            i64::try_from(n)
                .ok()
                .and_then(i64::checked_neg)
                .ok_or_else(|| invalid("signed integer overflow"))
        }
    } else {
        i64::try_from(unsigned(s.strip_prefix('+').unwrap_or(s))?)
            .map_err(|_| invalid("signed integer overflow"))
    }
}
fn pair(s: &str) -> Result<(u32, u32)> {
    let (a, b) = s
        .split_once('-')
        .ok_or_else(|| invalid("range requires '-' separator"))?;
    let a = u32::try_from(unsigned(a.trim())?).map_err(|_| invalid("range overflow"))?;
    let b = u32::try_from(unsigned(b.trim())?).map_err(|_| invalid("range overflow"))?;
    if a > b {
        return Err(invalid("reversed range"));
    }
    Ok((a, b))
}
fn bounds(s: &str) -> Result<Bounds> {
    let mut t = s.split_ascii_whitespace();
    let mut next = || {
        t.next()
            .ok_or_else(|| invalid("bounding box needs four integers"))
    };
    let w = u32::try_from(unsigned(next()?)?).map_err(|_| invalid("width overflow"))?;
    let h = u32::try_from(unsigned(next()?)?).map_err(|_| invalid("height overflow"))?;
    let x = integer(next()?)?;
    let y = integer(next()?)?;
    if t.next().is_some() {
        return Err(invalid("trailing bounding-box tokens"));
    }
    Ok(Bounds::new(w, h, x, y))
}
fn property(s: &str) -> Result<PropertyValue> {
    if s.bytes().any(|b| b.is_ascii_control()) {
        return Err(invalid("control character inside property value"));
    }
    if !s.starts_with('"') {
        return integer(s)
            .map(PropertyValue::Integer)
            .or_else(|_| unsigned(s).map(PropertyValue::Unsigned));
    }
    let b = s.as_bytes();
    let mut text = String::new();
    let mut i = 1;
    while i < b.len() {
        if b[i] == b'"' {
            if i + 1 == b.len() {
                return Ok(PropertyValue::Text(text));
            }
            if b[i + 1] != b'"' {
                return Err(invalid("trailing quoted property text"));
            }
            i += 1;
        }
        text.push(char::from(b[i]));
        i += 1;
    }
    Err(invalid("unterminated quoted property"))
}
struct Lines<'a> {
    lines: std::str::Lines<'a>,
    pending: Option<(&'a str, &'a str)>,
    comments: Vec<Property>,
    max_line: usize,
}
impl<'a> Lines<'a> {
    fn next(&mut self) -> Result<Option<(&'a str, &'a str)>> {
        if self.pending.is_some() {
            return Ok(self.pending.take());
        }
        for line in self.lines.by_ref() {
            check(line.len(), self.max_line, "HBF record bytes")?;
            let line = line.trim_matches([' ', '\t', '\r']);
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once([' ', '\t']).unwrap_or((line, ""));
            let value = value.trim_matches([' ', '\t']);
            if key == "COMMENT" {
                if value.bytes().any(|b| b.is_ascii_control()) {
                    return Err(invalid("control character inside comment"));
                }
                self.comments.push(Property::new("hbf.comment", value));
                continue;
            }
            if !key
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(invalid("invalid structural keyword"));
            }
            return Ok(Some((key, value)));
        }
        Ok(None)
    }
    fn expect(&mut self, key: &str) -> Result<&'a str> {
        match self.next()? {
            Some((k, v)) if k == key => Ok(v),
            _ => Err(invalid(format!("expected {key}"))),
        }
    }
    fn end(&mut self, key: &str) -> Result<()> {
        if !self.expect(key)?.is_empty() {
            return Err(invalid(format!("unexpected arguments after {key}")));
        }
        Ok(())
    }
    fn count(&mut self, key: &str, max: usize) -> Result<usize> {
        let n = usize::try_from(unsigned(self.expect(key)?)?)
            .map_err(|_| invalid("record count overflow"))?;
        check(n, max, "HBF record count")?;
        Ok(n)
    }
}
fn bytes_section(lines: &mut Lines<'_>, n: u8, font: &mut Font) -> Result<Vec<u8>> {
    let count = lines.count(&format!("HBF_START_BYTE_{n}_RANGES"), 256)?;
    if count == 0 {
        return Err(invalid("empty byte-range section"));
    }
    let mut out = Vec::new();
    let mut properties = Vec::new();
    let mut prev = None;
    for _ in 0..count {
        let (a, b) = pair(lines.expect(&format!("HBF_BYTE_{n}_RANGE"))?)?;
        if b > 255 || prev.is_some_and(|p| a <= p) {
            return Err(invalid("byte ranges overlap or exceed one byte"));
        }
        out.extend((a..=b).map(|n| n as u8));
        prev = Some(b);
        properties.push(PropertyValue::List(vec![
            PropertyValue::Integer(i64::from(a)),
            PropertyValue::Integer(i64::from(b)),
        ]));
    }
    lines.end(&format!("HBF_END_BYTE_{n}_RANGES"))?;
    font.headers.push(Property::new(
        format!("hbf.byte{n}"),
        PropertyValue::List(properties),
    ));
    Ok(out)
}
pub(super) fn parse(bytes: &[u8], limits: &ResourceLimits) -> Result<Header> {
    if bytes
        .iter()
        .any(|&b| !b.is_ascii() || (b.is_ascii_control() && !matches!(b, b'\n' | b'\r' | b'\t')))
    {
        return Err(invalid("header must be printable ASCII"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("header is not ASCII"))?;
    let mut lines = Lines {
        lines: text.lines(),
        pending: None,
        comments: Vec::new(),
        max_line: limits.max_table_bytes,
    };
    if lines.expect("HBF_START_FONT")? != "1.1" {
        return Err(invalid("only HBF version 1.1 is supported"));
    }
    let scheme = lines.expect("HBF_CODE_SCHEME")?.to_owned();
    if scheme.is_empty() || scheme.bytes().any(|b| b.is_ascii_control()) {
        return Err(invalid("empty code scheme"));
    }
    let mut font = Font::default();
    font.source_format = Some("hbf".into());
    font.headers
        .push(Property::new("hbf.code_scheme", scheme.as_str()));
    let name = lines.expect("FONT")?;
    if name.is_empty() || name.bytes().any(|b| b.is_ascii_control()) {
        return Err(invalid("empty FONT name"));
    }
    font.names.xlfd = Some(name.into());
    let mut size = lines.expect("SIZE")?.split_ascii_whitespace();
    let mut next = || {
        size.next()
            .ok_or_else(|| invalid("SIZE needs three positive integers"))
            .and_then(unsigned)
    };
    let p = next()?;
    let x = next()?;
    let y = next()?;
    if p == 0 || x == 0 || y == 0 || size.next().is_some() {
        return Err(invalid("invalid SIZE"));
    }
    font.metrics.point_size = Some(Rational::integer(
        i64::try_from(p).map_err(|_| invalid("point size overflow"))?,
    ));
    font.metrics.resolution = Some(Vector::new(
        i64::try_from(x).map_err(|_| invalid("resolution overflow"))?,
        i64::try_from(y).map_err(|_| invalid("resolution overflow"))?,
    ));
    let bitmap = bounds(lines.expect("HBF_BITMAP_BOUNDING_BOX")?)?;
    let bounds = bounds(lines.expect("FONTBOUNDINGBOX")?)?;
    font.metrics.bounds = Some(bounds);
    let props = lines.count("STARTPROPERTIES", bytes.len() / 2)?;
    for _ in 0..props {
        let (key, value) = lines
            .next()?
            .ok_or_else(|| invalid("truncated properties"))?;
        if matches!(key, "STARTPROPERTIES" | "ENDPROPERTIES") || key.starts_with("HBF_") {
            return Err(invalid("invalid property record"));
        }
        font.properties.push(Property::new(key, property(value)?));
    }
    lines.end("ENDPROPERTIES")?;
    if font.properties.get("DEFAULT_CHAR").is_none() {
        return Err(invalid("DEFAULT_CHAR property is required"));
    }
    let next = lines.next()?;
    let chars = if let Some(("CHARS", s)) = next {
        Some(usize::try_from(unsigned(s)?).map_err(|_| invalid("CHARS overflow"))?)
    } else {
        lines.pending = next;
        None
    };
    let next = lines.next()?;
    lines.pending = next;
    let second = if next.is_some_and(|(k, _)| k == "HBF_START_BYTE_2_RANGES") {
        bytes_section(&mut lines, 2, &mut font)?
    } else {
        Vec::new()
    };
    let next = lines.next()?;
    lines.pending = next;
    let third = if next.is_some_and(|(k, _)| k == "HBF_START_BYTE_3_RANGES") {
        if second.is_empty() {
            return Err(invalid("byte 3 ranges require byte 2 ranges"));
        }
        bytes_section(&mut lines, 3, &mut font)?
    } else {
        Vec::new()
    };
    let space = Space { second, third };
    let count = lines.count("HBF_START_CODE_RANGES", limits.max_glyphs)?;
    if count == 0 {
        return Err(invalid("empty code ranges"));
    }
    let mut ranges = Vec::new();
    let mut prev = None;
    for _ in 0..count {
        let s = lines.expect("HBF_CODE_RANGE")?;
        // Whitespace around the range separator is accepted.
        let tokens: Vec<_> = s.split_ascii_whitespace().collect();
        if tokens.len() < 3 || tokens.len() > 5 {
            return Err(invalid("invalid code range record"));
        }
        let offset = usize::try_from(unsigned(tokens[tokens.len() - 1])?)
            .map_err(|_| invalid("bitmap offset overflow"))?;
        let path = ResourcePath::new(tokens[tokens.len() - 2])?;
        let without_offset = s
            .strip_suffix(tokens[tokens.len() - 1])
            .ok_or_else(|| invalid("missing offset"))?
            .trim_end();
        let range_text = without_offset
            .strip_suffix(tokens[tokens.len() - 2])
            .ok_or_else(|| invalid("missing bitmap path"))?
            .trim_end();
        let (first, last) = pair(range_text)?;
        if prev.is_some_and(|p| first <= p) {
            return Err(invalid("code ranges overlap or are unordered"));
        }
        space.count(first, last)?;
        prev = Some(last);
        ranges.push(Range {
            first,
            last,
            path,
            offset,
        });
    }
    lines.end("HBF_END_CODE_RANGES")?;
    lines.end("HBF_END_FONT")?;
    if lines.next()?.is_some() {
        return Err(invalid("trailing records after HBF_END_FONT"));
    }
    for p in lines.comments {
        font.headers.push(p);
    }
    Ok(Header {
        font,
        scheme,
        bitmap,
        bounds,
        chars,
        space,
        ranges,
    })
}
