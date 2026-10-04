//! Jam3 JSON descriptors; bounded parsing preserves object-key uniqueness and types.
use super::*;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::io::Write;
#[derive(Debug)]
enum Value {
    String(String),
    Number(String),
    Bool(bool),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}
struct Seed<'a> {
    l: &'a ResourceLimits,
    budget: &'a mut Budget,
    depth: usize,
    array_max: usize,
    entry: Option<(usize, usize)>,
    failure: &'a mut Option<Error>,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<Value, D::Error> {
        resource(bound(self.depth, 8, "BMFont JSON nesting"), self.failure)?;
        if let Some((count, max)) = self.entry {
            resource(bound(count, max, "BMFont JSON array entries"), self.failure)?;
        }
        resource(self.budget.add(128), self.failure)?;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded BMFont JSON value")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        resource(self.budget.add(v.len().saturating_mul(2)), self.failure)?;
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        resource(self.budget.add(v.capacity()), self.failure)?;
        Ok(Value::String(v))
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.to_string()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.to_string()))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        loop {
            // Charge vector growth before deserializing the next value; the final
            // empty probe is conservative and prevents uncharged reallocation.
            resource(
                self.budget.add(2 * std::mem::size_of::<Value>()),
                self.failure,
            )?;
            let v = a.next_element_seed(Seed {
                l: self.l,
                budget: self.budget,
                depth: self.depth + 1,
                array_max: self.l.max_table_bytes / 8,
                entry: Some((values.len().saturating_add(1), self.array_max)),
                failure: self.failure,
            })?;
            let Some(v) = v else {
                break;
            };
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        let mut seen = BTreeSet::new();
        while let Some(k) = a.next_key::<String>()? {
            resource(
                bound(
                    values.len().saturating_add(1),
                    self.l.max_table_bytes / 16,
                    "BMFont JSON object fields",
                ),
                self.failure,
            )?;
            resource(
                self.budget
                    .add(256usize.saturating_add(k.len().saturating_mul(2))),
                self.failure,
            )?;
            if !seen.insert(k.clone()) {
                return Err(de::Error::custom("duplicate BMFont JSON key"));
            }
            let limit = match k.as_str() {
                "chars" => self.l.max_glyphs,
                "pages" => self.l.max_resources,
                "padding" => 4,
                "spacing" => 2,
                _ => self.l.max_table_bytes / 8,
            };
            let v = a.next_value_seed(Seed {
                l: self.l,
                budget: self.budget,
                depth: self.depth + 1,
                array_max: limit,
                entry: None,
                failure: self.failure,
            })?;
            values.push((k, v));
        }
        Ok(Value::Object(values))
    }
}
fn resource<E: de::Error>(
    result: Result<()>,
    failure: &mut Option<Error>,
) -> std::result::Result<(), E> {
    result.map_err(|e| {
        *failure = Some(e);
        E::custom("BMFont JSON resource budget exceeded")
    })
}
fn standard(tag: &str, key: &str) -> Option<&'static str> {
    if tag == "info" {
        match key {
            "face" | "charset" => Some("string"),
            "padding" | "spacing" => Some("numbers"),
            "size" | "bold" | "italic" | "unicode" | "stretchH" | "smooth" | "aa" | "outline"
            | "fixedHeight" => Some("number"),
            _ => None,
        }
    } else if tag == "common"
        && [
            "lineHeight",
            "base",
            "scaleW",
            "scaleH",
            "pages",
            "packed",
            "alphaChnl",
            "redChnl",
            "greenChnl",
            "blueChnl",
        ]
        .contains(&key)
        || tag == "char"
            && [
                "id", "x", "y", "width", "height", "xoffset", "yoffset", "xadvance", "page", "chnl",
            ]
            .contains(&key)
        || tag == "kerning" && ["first", "second", "amount"].contains(&key)
    {
        Some("number")
    } else {
        None
    }
}
fn object(tag: &str, v: Value, l: &ResourceLimits, budget: &mut Budget) -> Result<Record> {
    let Value::Object(fields) = v else {
        return Err(invalid(format!("JSON {tag} must be an object")));
    };
    budget.add(256)?;
    let mut r = Record::new(tag);
    for (key, v) in fields {
        let expected = standard(tag, &key);
        let (text, kind) = match v {
            Value::String(v) => (v, "string"),
            Value::Number(v) => (v, "number"),
            Value::Bool(v) => (u8::from(v).to_string(), "bool"),
            Value::Array(a) => {
                let size = a.iter().try_fold(0usize, |n, v| match v {
                    Value::Number(v) => Ok(n.saturating_add(v.len()).saturating_add(1)),
                    _ => Err(invalid("JSON field arrays must contain integers")),
                })?;
                budget.add(size)?;
                let mut text = String::new();
                text.try_reserve_exact(size).map_err(invalid)?;
                for (i, v) in a.into_iter().enumerate() {
                    let Value::Number(v) = v else {
                        return Err(invalid("JSON field arrays must contain integers"));
                    };
                    if i > 0 {
                        text.push(',');
                    }
                    text.push_str(&v);
                }
                (text, "numbers")
            }
            Value::Object(_) => {
                return Err(invalid(
                    "nested BMFont JSON extension objects are unsupported",
                ));
            }
        };
        bound(text.len(), l.max_table_bytes, "BMFont JSON field")?;
        if let Some(expected) = expected {
            if expected != kind
                && !(expected == "number"
                    && kind == "bool"
                    && [
                        "bold",
                        "italic",
                        "unicode",
                        "smooth",
                        "packed",
                        "fixedHeight",
                    ]
                    .contains(&key.as_str()))
            {
                return Err(invalid("BMFont JSON field type disagrees with schema"));
            }
            if expected == "numbers" {
                let n = text.split(',').count();
                if n != if key == "padding" { 4 } else { 2 } {
                    return Err(invalid("BMFont JSON padding/spacing length"));
                }
            }
        } else if kind != "string" {
            r.json_types.insert(key.clone(), kind.into());
        }
        budget
            .add(224usize.saturating_add(key.len().saturating_add(text.len()).saturating_mul(2)))?;
        if r.fields.insert(key, text).is_some() {
            return Err(invalid("duplicate BMFont JSON field"));
        }
    }
    Ok(r)
}
pub(super) fn parse(s: &str, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Record>> {
    bound(s.len(), l.max_table_bytes, "BMFont JSON descriptor")?;
    // serde_json can allocate escape buffers before invoking visitors. Reserving
    // input-proportional scratch bounds those allocations as well as key strings.
    budget.add(s.len().saturating_mul(4))?;
    let mut d = serde_json::Deserializer::from_str(s);
    let mut failure = None;
    let tree = Seed {
        l,
        budget,
        depth: 0,
        array_max: l.max_table_bytes / 8,
        entry: None,
        failure: &mut failure,
    }
    .deserialize(&mut d)
    .map_err(|e| failure.unwrap_or_else(|| invalid(e)))?;
    d.end().map_err(invalid)?;
    let Value::Object(root) = tree else {
        return Err(invalid("BMFont JSON root must be an object"));
    };
    let mut present = BTreeSet::new();
    let mut records = Vec::new();
    for (key, value) in root {
        present.insert(key.clone());
        match key.as_str() {
            "info" | "common" => records.push(object(&key, value, l, budget)?),
            "pages" => {
                let Value::Array(pages) = value else {
                    return Err(invalid("BMFont JSON pages must be an array"));
                };
                for (id, p) in pages.into_iter().enumerate() {
                    let Value::String(name) = p else {
                        return Err(invalid("BMFont JSON pages must contain filenames"));
                    };
                    budget.add(512usize.saturating_add(name.len()))?;
                    let mut r = Record::new("page");
                    r.set("id", id);
                    r.set("file", name);
                    records.push(r);
                }
            }
            "chars" | "kernings" => {
                let Value::Array(values) = value else {
                    return Err(invalid("BMFont JSON records must be arrays"));
                };
                budget.add(256)?;
                let mut count = Record::new(&key);
                count.set("count", values.len());
                records.push(count);
                let tag = if key == "chars" { "char" } else { "kerning" };
                for v in values {
                    records.push(object(tag, v, l, budget)?);
                }
            }
            _ => records.push(object(&key, value, l, budget)?),
        }
    }
    if !["info", "common", "pages", "chars"]
        .iter()
        .all(|k| present.contains(*k))
    {
        return Err(invalid("JSON lacks required BMFont sections"));
    }
    // Descriptor order is not JSON schema semantics; canonical native records
    // place metadata/pages before glyphs while preserving each array's order.
    budget.temporary(records.len().saturating_mul(std::mem::size_of::<Record>()))?;
    records.sort_by_key(|r| match r.tag.as_str() {
        "info" => 0,
        "common" => 1,
        "page" => 2,
        "chars" => 3,
        "char" => 4,
        "kernings" => 5,
        "kerning" => 6,
        _ => 7,
    });
    Ok(records)
}

struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("JSON output length overflow"))?;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn string(out: &mut dyn Write, s: &str) -> Result<()> {
    serde_json::to_writer(out, s).map_err(invalid)
}
fn literal(out: &mut dyn Write, s: &str) -> Result<()> {
    out.write_all(s.as_bytes()).map_err(invalid)
}
fn integer(out: &mut dyn Write, s: &str) -> Result<()> {
    if let Ok(v) = s.parse::<i64>() {
        serde_json::to_writer(out, &v).map_err(invalid)
    } else {
        let v = s.parse::<u64>().map_err(invalid)?;
        serde_json::to_writer(out, &v).map_err(invalid)
    }
}
fn emit_record(out: &mut dyn Write, r: &Record, l: &ResourceLimits) -> Result<()> {
    literal(out, "{")?;
    for (i, (k, v)) in r.fields.iter().enumerate() {
        bound(v.len(), l.max_table_bytes, "BMFont JSON field")?;
        if i > 0 {
            literal(out, ",")?;
        }
        string(out, k)?;
        literal(out, ":")?;
        let kind = r
            .json_types
            .get(k)
            .map(String::as_str)
            .or_else(|| standard(&r.tag, k))
            .unwrap_or("string");
        match kind {
            "number" => integer(out, v)?,
            "bool" => literal(
                out,
                match v.as_str() {
                    "1" => "true",
                    "0" => "false",
                    _ => return Err(invalid("invalid native JSON boolean")),
                },
            )?,
            "numbers" => {
                literal(out, "[")?;
                for (i, n) in v.split(',').filter(|_| !v.is_empty()).enumerate() {
                    if i > 0 {
                        literal(out, ",")?;
                    }
                    integer(out, n)?;
                }
                literal(out, "]")?;
            }
            _ => string(out, v)?,
        }
    }
    literal(out, "}")
}
fn emit(out: &mut dyn Write, records: &[Record], l: &ResourceLimits) -> Result<()> {
    literal(out, "{\"info\":")?;
    emit_record(
        out,
        records
            .iter()
            .find(|r| r.tag == "info")
            .ok_or_else(|| invalid("missing JSON info"))?,
        l,
    )?;
    literal(out, ",\"common\":")?;
    emit_record(
        out,
        records
            .iter()
            .find(|r| r.tag == "common")
            .ok_or_else(|| invalid("missing JSON common"))?,
        l,
    )?;
    literal(out, ",\"pages\":[")?;
    for (i, r) in records.iter().filter(|r| r.tag == "page").enumerate() {
        if r.num("id")? != i as i64 {
            return Err(invalid("JSON page IDs must follow array order"));
        }
        if i > 0 {
            literal(out, ",")?;
        }
        string(out, r.text("file")?)?;
    }
    literal(out, "],\"chars\":[")?;
    for (i, r) in records.iter().filter(|r| r.tag == "char").enumerate() {
        if i > 0 {
            literal(out, ",")?;
        }
        emit_record(out, r, l)?;
    }
    literal(out, "],\"kernings\":[")?;
    for (i, r) in records.iter().filter(|r| r.tag == "kerning").enumerate() {
        if i > 0 {
            literal(out, ",")?;
        }
        emit_record(out, r, l)?;
    }
    literal(out, "]}\n")
}
pub(super) fn encode(records: &[Record], l: &ResourceLimits) -> Result<Vec<u8>> {
    let mut count = Counter(0);
    emit(&mut count, records, l)?;
    bound(count.0, l.max_input_bytes, "BMFont JSON output")?;
    bound(count.0, l.max_table_bytes, "BMFont JSON descriptor")?;
    bound(
        count.0.saturating_mul(2),
        l.max_total_bytes,
        "BMFont JSON output scratch",
    )?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(count.0)
        .map_err(|_| invalid("BMFont JSON output allocation failed"))?;
    emit(&mut bytes, records, l)?;
    Ok(bytes)
}

// Allocation-free JSON syntax probe. It records only the four required root
// field names and their container types; no descriptor records/pages are built.
struct Probe<'a> {
    b: &'a [u8],
    at: usize,
}
impl<'a> Probe<'a> {
    fn ws(&mut self) {
        while self
            .b
            .get(self.at)
            .is_some_and(|v| matches!(v, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.at += 1;
        }
    }
    fn token(&mut self, v: u8) -> Option<()> {
        self.ws();
        if self.b.get(self.at) != Some(&v) {
            return None;
        }
        self.at += 1;
        Some(())
    }
    fn string(&mut self) -> Option<&'a [u8]> {
        self.token(b'"')?;
        let start = self.at;
        while let Some(&v) = self.b.get(self.at) {
            self.at += 1;
            match v {
                b'"' => return Some(&self.b[start..self.at - 1]),
                0..=31 => return None,
                b'\\' => {
                    let escape = *self.b.get(self.at)?;
                    self.at += 1;
                    if escape == b'u' {
                        for _ in 0..4 {
                            if !self.b.get(self.at)?.is_ascii_hexdigit() {
                                return None;
                            }
                            self.at += 1;
                        }
                    } else if !b"\"\\/bfnrt".contains(&escape) {
                        return None;
                    }
                }
                _ => {}
            }
        }
        None
    }
    fn value(&mut self, depth: usize) -> Option<()> {
        if depth > 16 {
            return None;
        }
        self.ws();
        match *self.b.get(self.at)? {
            b'"' => {
                self.string()?;
                Some(())
            }
            b'{' => {
                self.at += 1;
                self.ws();
                if self.b.get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Some(());
                }
                loop {
                    self.string()?;
                    self.token(b':')?;
                    self.value(depth + 1)?;
                    self.ws();
                    match self.b.get(self.at)? {
                        b'}' => {
                            self.at += 1;
                            return Some(());
                        }
                        b',' => self.at += 1,
                        _ => return None,
                    }
                }
            }
            b'[' => {
                self.at += 1;
                self.ws();
                if self.b.get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Some(());
                }
                loop {
                    self.value(depth + 1)?;
                    self.ws();
                    match self.b.get(self.at)? {
                        b']' => {
                            self.at += 1;
                            return Some(());
                        }
                        b',' => self.at += 1,
                        _ => return None,
                    }
                }
            }
            b't' | b'f' | b'n' => {
                let word = match self.b[self.at] {
                    b't' => b"true".as_slice(),
                    b'f' => b"false".as_slice(),
                    _ => b"null".as_slice(),
                };
                if self.b.get(self.at..self.at + word.len()) != Some(word) {
                    return None;
                }
                self.at += word.len();
                Some(())
            }
            _ => {
                if self.b.get(self.at) == Some(&b'-') {
                    self.at += 1;
                }
                match self.b.get(self.at)? {
                    b'0' => self.at += 1,
                    b'1'..=b'9' => {
                        while self.b.get(self.at).is_some_and(u8::is_ascii_digit) {
                            self.at += 1;
                        }
                    }
                    _ => return None,
                }
                if self.b.get(self.at) == Some(&b'.') {
                    self.at += 1;
                    let start = self.at;
                    while self.b.get(self.at).is_some_and(u8::is_ascii_digit) {
                        self.at += 1;
                    }
                    if self.at == start {
                        return None;
                    }
                }
                if self
                    .b
                    .get(self.at)
                    .is_some_and(|v| matches!(v, b'e' | b'E'))
                {
                    self.at += 1;
                    if self
                        .b
                        .get(self.at)
                        .is_some_and(|v| matches!(v, b'+' | b'-'))
                    {
                        self.at += 1;
                    }
                    let start = self.at;
                    while self.b.get(self.at).is_some_and(u8::is_ascii_digit) {
                        self.at += 1;
                    }
                    if self.at == start {
                        return None;
                    }
                }
                Some(())
            }
        }
    }
    fn root(&mut self) -> Option<()> {
        self.token(b'{')?;
        let mut mask = 0u8;
        loop {
            let key = self.string()?;
            self.token(b':')?;
            self.ws();
            let expected = match key {
                b"info" => (1, b'{'),
                b"common" => (2, b'{'),
                b"pages" => (4, b'['),
                b"chars" => (8, b'['),
                _ => (0, 0),
            };
            if expected.0 != 0 {
                if mask & expected.0 != 0 || self.b.get(self.at) != Some(&expected.1) {
                    return None;
                }
                mask |= expected.0;
            }
            self.value(1)?;
            self.ws();
            match self.b.get(self.at)? {
                b'}' => {
                    self.at += 1;
                    break;
                }
                b',' => self.at += 1,
                _ => return None,
            }
        }
        self.ws();
        if mask == 15 && self.at == self.b.len() {
            Some(())
        } else {
            None
        }
    }
}
pub(super) fn detect(b: &[u8]) -> Confidence {
    let b = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b);
    if std::str::from_utf8(b).is_ok() && (Probe { b, at: 0 }).root().is_some() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
