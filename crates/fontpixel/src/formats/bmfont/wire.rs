use super::*;
use std::fmt::Write;
pub(super) fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Record>> {
    bound(b.len(), l.max_input_bytes, "BMFont descriptor")?;
    let r = if b.starts_with(b"BMF") {
        binary(b, l, budget)?
    } else {
        let s = std::str::from_utf8(b)
            .map_err(invalid)?
            .trim_start_matches('\u{feff}');
        if s.trim_start().starts_with('<') {
            xml(s, l, budget)?
        } else {
            text(s, l, budget)?
        }
    };
    bound(r.len(), l.max_total_bytes / 128, "BMFont records")?;
    Ok(r)
}
fn text(s: &str, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    for line in s.lines() {
        bound(line.len(), l.max_table_bytes, "BMFont record")?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let end = line.find(char::is_whitespace).unwrap_or(line.len());
        budget.add(256 + end)?;
        let mut r = Record::new(&line[..end]);
        let mut rest = &line[end..];
        while !rest.trim_start().is_empty() {
            rest = rest.trim_start();
            let eq = rest
                .find('=')
                .ok_or_else(|| invalid("missing equals sign"))?;
            let key = &rest[..eq];
            if key.is_empty() || key.chars().any(char::is_whitespace) {
                return Err(invalid("invalid field name"));
            }
            rest = &rest[eq + 1..];
            let value;
            if let Some(s) = rest.strip_prefix('"') {
                let end = s.find('"').ok_or_else(|| invalid("unclosed quote"))?;
                value = &s[..end];
                rest = &s[end + 1..];
                if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
                    return Err(invalid("missing field separator"));
                }
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                value = &rest[..end];
                rest = &rest[end..];
            }
            budget.add(
                224usize.saturating_add(key.len().saturating_add(value.len()).saturating_mul(2)),
            )?;
            if r.fields.insert(key.into(), value.into()).is_some() {
                return Err(invalid("duplicate field"));
            }
        }
        out.push(r);
    }
    Ok(out)
}
fn unescape(s: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let end = rest
            .find(';')
            .ok_or_else(|| invalid("unterminated XML entity"))?;
        let entity = &rest[..end];
        out.push(match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let cp = if let Some(n) = entity.strip_prefix("#x") {
                    u32::from_str_radix(n, 16).map_err(invalid)?
                } else if let Some(n) = entity.strip_prefix('#') {
                    n.parse().map_err(invalid)?
                } else {
                    return Err(invalid("custom XML entities forbidden"));
                };
                char::from_u32(cp)
                    .filter(|c| !c.is_control() || ['\t', '\n', '\r'].contains(c))
                    .ok_or_else(|| invalid("invalid XML character"))?
            }
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
fn xml(s: &str, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Record>> {
    use xmlparser::{ElementEnd, Token};
    let mut stack = Vec::<String>::new();
    let mut pending = None::<Record>;
    let mut out = Vec::new();
    let mut root = false;
    for token in xmlparser::Tokenizer::from(s) {
        match token.map_err(invalid)? {
            Token::ElementStart { prefix, local, .. } => {
                if !prefix.is_empty() {
                    return Err(invalid("XML namespaces unsupported"));
                }
                if stack.len() >= 4 {
                    return Err(invalid("XML nesting limit"));
                }
                let tag = local.as_str();
                let valid = match stack.last().map(String::as_str) {
                    None => !root && tag == "font",
                    Some("font") => ["info", "common", "pages", "chars", "kernings"].contains(&tag),
                    Some("pages") => tag == "page",
                    Some("chars") => tag == "char",
                    Some("kernings") => tag == "kerning",
                    _ => false,
                };
                if !valid {
                    return Err(invalid("invalid XML hierarchy"));
                }
                root = true;
                budget.add(256 + tag.len())?;
                pending = Some(Record::new(tag));
            }
            Token::Attribute {
                prefix,
                local,
                value,
                ..
            } => {
                if !prefix.is_empty() {
                    return Err(invalid("namespaced attribute"));
                }
                bound(value.len(), l.max_table_bytes, "BMFont XML attribute")?;
                budget
                    .add(224usize.saturating_add(
                        local.len().saturating_add(value.len()).saturating_mul(2),
                    ))?;
                let r = pending
                    .as_mut()
                    .ok_or_else(|| invalid("attribute outside element"))?;
                bound(r.fields.len() + 1, 64, "BMFont XML attributes")?;
                if r.fields
                    .insert(local.as_str().into(), unescape(value.as_str())?)
                    .is_some()
                {
                    return Err(invalid("duplicate XML attribute"));
                }
            }
            Token::ElementEnd { end, .. } => match end {
                ElementEnd::Open | ElementEnd::Empty => {
                    let r = pending
                        .take()
                        .ok_or_else(|| invalid("missing element start"))?;
                    if end == ElementEnd::Open {
                        stack.push(r.tag.clone());
                    }
                    if !["font", "pages"].contains(&r.tag.as_str()) {
                        out.push(r);
                    }
                }
                ElementEnd::Close(prefix, local) => {
                    if !prefix.is_empty() || stack.pop().as_deref() != Some(local.as_str()) {
                        return Err(invalid("mismatched XML close"));
                    }
                }
            },
            Token::Text { text } if !text.as_str().trim().is_empty() => {
                return Err(invalid("unexpected XML text"));
            }
            Token::DtdStart { .. }
            | Token::EmptyDtd { .. }
            | Token::EntityDeclaration { .. }
            | Token::DtdEnd { .. }
            | Token::Cdata { .. }
            | Token::ProcessingInstruction { .. } => {
                return Err(invalid(
                    "XML DTD, entity, CDATA or processing instruction forbidden",
                ));
            }
            Token::Declaration {
                encoding: Some(v), ..
            } if !v.as_str().eq_ignore_ascii_case("utf-8") => {
                return Err(invalid("only UTF-8 XML supported"));
            }
            _ => {}
        }
    }
    if !root || !stack.is_empty() || pending.is_some() {
        return Err(invalid("incomplete XML"));
    }
    Ok(out)
}
fn u16_at(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
fn i16_at(b: &[u8], p: usize) -> i16 {
    i16::from_le_bytes([b[p], b[p + 1]])
}
fn u32_at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}
fn binary(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Record>> {
    if b.get(3) != Some(&3) {
        return Err(invalid("only binary version 3 supported"));
    }
    let mut pos = 4;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    while pos < b.len() {
        let head = b
            .get(pos..pos + 5)
            .ok_or_else(|| invalid("truncated block header"))?;
        let kind = head[0];
        let size = u32_at(head, 1) as usize;
        bound(size, l.max_table_bytes, "BMFont binary block")?;
        pos += 5;
        let end = pos
            .checked_add(size)
            .ok_or_else(|| invalid("block overflow"))?;
        let d = b.get(pos..end).ok_or_else(|| invalid("truncated block"))?;
        pos = end;
        if !seen.insert(kind) {
            return Err(invalid("duplicate binary block"));
        }
        let reserve = match kind {
            4 => (size / 20).saturating_mul(3000),
            5 => (size / 10).saturating_mul(2048),
            1 | 2 => size.saturating_mul(4).saturating_add(8192),
            3 => d
                .iter()
                .filter(|&&v| v == 0)
                .count()
                .saturating_mul(1536)
                .saturating_add(size.saturating_mul(2)),
            _ => 0,
        };
        budget.add(reserve)?;
        match kind {
            1 => {
                if d.len() < 15 || d.last() != Some(&0) || d[14..d.len() - 1].contains(&0) {
                    return Err(invalid("invalid info block"));
                }
                let mut r = Record::new("info");
                r.set(
                    "face",
                    std::str::from_utf8(&d[14..d.len() - 1]).map_err(invalid)?,
                );
                r.set("size", i16_at(d, 0));
                for (k, shift) in [
                    ("smooth", 0),
                    ("unicode", 1),
                    ("italic", 2),
                    ("bold", 3),
                    ("fixedHeight", 4),
                ] {
                    r.set(k, (d[2] >> shift) & 1);
                }
                r.set("charset", d[3]);
                r.set("stretchH", u16_at(d, 4));
                r.set("aa", d[6]);
                r.set("padding", format!("{},{},{},{}", d[7], d[8], d[9], d[10]));
                r.set("spacing", format!("{},{}", d[11], d[12]));
                r.set("outline", d[13]);
                if d[2] & 0xe0 != 0 {
                    r.set("reserved", d[2] & 0xe0);
                }
                out.push(r);
            }
            2 => {
                if d.len() != 15 {
                    return Err(invalid("common block size"));
                }
                let mut r = Record::new("common");
                for (k, p) in [
                    ("lineHeight", 0),
                    ("base", 2),
                    ("scaleW", 4),
                    ("scaleH", 6),
                    ("pages", 8),
                ] {
                    r.set(k, u16_at(d, p));
                }
                r.set("packed", d[10] >> 7);
                for (k, p) in [
                    ("alphaChnl", 11),
                    ("redChnl", 12),
                    ("greenChnl", 13),
                    ("blueChnl", 14),
                ] {
                    r.set(k, d[p]);
                }
                if d[10] & 127 != 0 {
                    r.set("reserved", d[10] & 127);
                }
                out.push(r);
            }
            3 => {
                if d.is_empty() || d.last() != Some(&0) {
                    return Err(invalid("pages need NUL terminators"));
                }
                for (id, name) in d[..d.len() - 1].split(|&v| v == 0).enumerate() {
                    if name.is_empty() {
                        return Err(invalid("empty page name"));
                    }
                    let mut r = Record::new("page");
                    r.set("id", id);
                    r.set("file", std::str::from_utf8(name).map_err(invalid)?);
                    out.push(r);
                }
            }
            4 => {
                if d.len() % 20 != 0 {
                    return Err(invalid("char block alignment"));
                }
                bound(d.len() / 20, l.max_glyphs, "BMFont glyphs")?;
                for d in d.as_chunks::<20>().0 {
                    let mut r = Record::new("char");
                    r.set("id", u32_at(d, 0));
                    for (k, p) in [("x", 4), ("y", 6), ("width", 8), ("height", 10)] {
                        r.set(k, u16_at(d, p));
                    }
                    for (k, p) in [("xoffset", 12), ("yoffset", 14), ("xadvance", 16)] {
                        r.set(k, i16_at(d, p));
                    }
                    r.set("page", d[18]);
                    r.set("chnl", d[19]);
                    out.push(r);
                }
            }
            5 => {
                if d.len() % 10 != 0 {
                    return Err(invalid("kerning block alignment"));
                }
                for d in d.as_chunks::<10>().0 {
                    let mut r = Record::new("kerning");
                    r.set("first", u32_at(d, 0));
                    r.set("second", u32_at(d, 4));
                    r.set("amount", i16_at(d, 8));
                    out.push(r);
                }
            }
            _ => return Err(invalid("unknown binary block")),
        }
    }
    Ok(out)
}
fn quoted(s: &str, xml: bool) -> Result<String> {
    if s.chars().any(char::is_control) || s.contains('\0') {
        return Err(invalid("control in attribute"));
    }
    if xml {
        Ok(s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;"))
    } else if s.contains('"') {
        Err(invalid("quote in text field; use XML encoding"))
    } else {
        Ok(s.into())
    }
}
pub(super) fn encode(
    records: &[Record],
    encoding: BmfontEncoding,
    l: &ResourceLimits,
) -> Result<Vec<u8>> {
    if encoding == BmfontEncoding::Binary {
        return encode_binary(records, l);
    }
    let mut capacity = 64usize;
    let mut largest = 0usize;
    for r in records {
        let mut size = r.tag.len().saturating_add(128);
        for (key, value) in &r.fields {
            let escaped = if encoding == BmfontEncoding::Xml {
                value
                    .chars()
                    .map(|c| match c {
                        '&' => 5,
                        '<' | '>' => 4,
                        '"' => 6,
                        _ => c.len_utf8(),
                    })
                    .sum::<usize>()
            } else {
                value.len()
            };
            size = size
                .saturating_add(key.len())
                .saturating_add(escaped)
                .saturating_add(8);
        }
        capacity = capacity.saturating_add(size);
        largest = largest.max(size);
    }
    bound(
        capacity.saturating_add(largest.saturating_mul(2)),
        l.max_total_bytes,
        "BMFont escaped output scratch",
    )?;
    let mut s = String::with_capacity(capacity);
    if encoding == BmfontEncoding::Xml {
        s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<font>\n");
    }
    let mut group = "";
    for r in records {
        if encoding == BmfontEncoding::Xml {
            let next = match r.tag.as_str() {
                "page" => "pages",
                "char" | "chars" => "chars",
                "kerning" | "kernings" => "kernings",
                _ => "",
            };
            if group != next {
                if !group.is_empty() {
                    writeln!(s, "</{group}>").map_err(invalid)?;
                }
                if !next.is_empty() {
                    write!(s, "<{next}").map_err(invalid)?;
                    if r.tag == next {
                        for (k, v) in &r.fields {
                            write!(s, " {k}=\"{}\"", quoted(v, true)?).map_err(invalid)?;
                        }
                    }
                    s.push_str(">\n");
                }
                group = next;
            }
            if r.tag == "chars" || r.tag == "kernings" {
                continue;
            }
            write!(s, "<{}", r.tag).map_err(invalid)?;
            for (k, v) in &r.fields {
                write!(s, " {k}=\"{}\"", quoted(v, true)?).map_err(invalid)?;
            }
            s.push_str("/>\n");
        } else {
            write!(s, "{}", r.tag).map_err(invalid)?;
            for (k, v) in &r.fields {
                let v = quoted(v, false)?;
                if ["face", "file", "charset"].contains(&k.as_str())
                    || v.chars().any(char::is_whitespace)
                {
                    write!(s, " {k}=\"{v}\"").map_err(invalid)?;
                } else {
                    write!(s, " {k}={v}").map_err(invalid)?;
                }
            }
            s.push('\n');
        }
        bound(s.len(), l.max_input_bytes, "BMFont output")?;
    }
    if encoding == BmfontEncoding::Xml {
        if !group.is_empty() {
            writeln!(s, "</{group}>").map_err(invalid)?;
        }
        s.push_str("</font>\n");
    }
    Ok(s.into_bytes())
}
fn block(out: &mut Vec<u8>, tag: u8, data: &[u8], l: &ResourceLimits) -> Result<()> {
    bound(data.len(), l.max_table_bytes, "BMFont output block")?;
    bound(
        out.len().saturating_add(5).saturating_add(data.len()),
        l.max_input_bytes,
        "BMFont output",
    )?;
    out.push(tag);
    out.extend_from_slice(&u32::try_from(data.len()).map_err(invalid)?.to_le_bytes());
    out.extend_from_slice(data);
    Ok(())
}
fn encode_binary(r: &[Record], l: &ResourceLimits) -> Result<Vec<u8>> {
    let info = r
        .iter()
        .find(|r| r.tag == "info")
        .ok_or_else(|| invalid("missing info"))?;
    let common = r
        .iter()
        .find(|r| r.tag == "common")
        .ok_or_else(|| invalid("missing common"))?;
    let name_size = info.text("face")?.len().saturating_add(15);
    let page_size = r
        .iter()
        .filter(|r| r.tag == "page")
        .try_fold(0usize, |n, r| {
            Ok::<_, Error>(n.saturating_add(r.text("file")?.len()).saturating_add(1))
        })?;
    let char_size = r
        .iter()
        .filter(|r| r.tag == "char")
        .count()
        .saturating_mul(20);
    let kern_size = r
        .iter()
        .filter(|r| r.tag == "kerning")
        .count()
        .saturating_mul(10);
    let max_block = name_size
        .max(page_size)
        .max(char_size)
        .max(kern_size)
        .max(15);
    let total = name_size
        .saturating_add(page_size)
        .saturating_add(char_size)
        .saturating_add(kern_size)
        .saturating_add(44);
    bound(max_block, l.max_table_bytes, "BMFont binary output block")?;
    bound(total, l.max_input_bytes, "BMFont binary output")?;
    bound(
        total.saturating_add(max_block),
        l.max_total_bytes,
        "BMFont binary output scratch",
    )?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"BMF\x03");
    let mut d = Vec::with_capacity(max_block);
    d.extend_from_slice(
        &i16::try_from(info.num("size")?)
            .map_err(invalid)?
            .to_le_bytes(),
    );
    let mut flags = 0;
    for (k, shift) in [
        ("smooth", 0),
        ("unicode", 1),
        ("italic", 2),
        ("bold", 3),
        ("fixedHeight", 4),
    ] {
        flags |= u8::try_from(info.number(k, 0)?).map_err(invalid)? << shift;
    }
    d.push(flags);
    let charset = info
        .fields
        .get("charset")
        .filter(|v| !v.is_empty())
        .map_or(Ok(0), |s| s.parse::<u8>().map_err(invalid))?;
    d.push(charset);
    d.extend_from_slice(
        &u16::try_from(info.number("stretchH", 100)?)
            .map_err(invalid)?
            .to_le_bytes(),
    );
    d.push(u8::try_from(info.number("aa", 1)?).map_err(invalid)?);
    for (k, n) in [("padding", 4), ("spacing", 2)] {
        let v = info.fields.get(k).map(String::as_str).unwrap_or(if n == 4 {
            "0,0,0,0"
        } else {
            "0,0"
        });
        let parts = v
            .split(',')
            .map(|v| v.parse::<u8>().map_err(invalid))
            .collect::<Result<Vec<_>>>()?;
        if parts.len() != n {
            return Err(invalid("padding/spacing count"));
        }
        d.extend_from_slice(&parts);
    }
    d.push(u8::try_from(info.number("outline", 0)?).map_err(invalid)?);
    let name = info.text("face")?;
    if name.contains('\0') {
        return Err(invalid("NUL in font name"));
    }
    d.extend_from_slice(name.as_bytes());
    d.push(0);
    block(&mut out, 1, &d, l)?;
    d.clear();
    for k in ["lineHeight", "base", "scaleW", "scaleH", "pages"] {
        d.extend_from_slice(
            &u16::try_from(common.num(k)?)
                .map_err(invalid)?
                .to_le_bytes(),
        );
    }
    d.push(u8::try_from(common.number("packed", 0)?).map_err(invalid)? << 7);
    for k in ["alphaChnl", "redChnl", "greenChnl", "blueChnl"] {
        d.push(u8::try_from(common.number(k, 0)?).map_err(invalid)?);
    }
    block(&mut out, 2, &d, l)?;
    d.clear();
    for page in r.iter().filter(|r| r.tag == "page") {
        let name = page.text("file")?;
        if name.contains('\0') {
            return Err(invalid("NUL in page name"));
        }
        d.extend_from_slice(name.as_bytes());
        d.push(0);
    }
    block(&mut out, 3, &d, l)?;
    d.clear();
    for ch in r.iter().filter(|r| r.tag == "char") {
        d.extend_from_slice(&u32::try_from(ch.num("id")?).map_err(invalid)?.to_le_bytes());
        for k in ["x", "y", "width", "height"] {
            d.extend_from_slice(&u16::try_from(ch.num(k)?).map_err(invalid)?.to_le_bytes());
        }
        for k in ["xoffset", "yoffset", "xadvance"] {
            d.extend_from_slice(&i16::try_from(ch.num(k)?).map_err(invalid)?.to_le_bytes());
        }
        d.push(u8::try_from(ch.num("page")?).map_err(invalid)?);
        d.push(u8::try_from(ch.num("chnl")?).map_err(invalid)?);
    }
    block(&mut out, 4, &d, l)?;
    d.clear();
    for k in r.iter().filter(|r| r.tag == "kerning") {
        for field in ["first", "second"] {
            d.extend_from_slice(&u32::try_from(k.num(field)?).map_err(invalid)?.to_le_bytes());
        }
        d.extend_from_slice(
            &i16::try_from(k.num("amount")?)
                .map_err(invalid)?
                .to_le_bytes(),
        );
    }
    if !d.is_empty() {
        block(&mut out, 5, &d, l)?;
    }
    Ok(out)
}
