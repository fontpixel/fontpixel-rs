use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind<'a> {
    Ident(&'a str),
    Number(&'a str),
    Punct(u8),
    String(&'a str),
    Directive(&'a str),
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Token<'a> {
    pub kind: Kind<'a>,
    pub start: usize,
    pub end: usize,
}
pub(super) fn lex<'a>(s: &'a str, b: &mut Budget) -> Result<Vec<Token<'a>>> {
    let bytes = s.as_bytes();
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        let start = at;
        let c = bytes[at];
        if c.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            at += bytes[at..]
                .iter()
                .position(|&v| v == b'\n')
                .unwrap_or(bytes.len() - at);
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            at += bytes[at..]
                .windows(2)
                .position(|v| v == b"*/")
                .ok_or_else(|| invalid("unterminated C comment"))?
                + 2;
            continue;
        }
        let kind = if c == b'#' {
            at += bytes[at..]
                .iter()
                .position(|&v| v == b'\n')
                .unwrap_or(bytes.len() - at);
            Kind::Directive(&s[start..at])
        } else if c.is_ascii_alphabetic() || c == b'_' {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            Kind::Ident(&s[start..at])
        } else if c.is_ascii_digit() {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'.') {
                at += 1;
            }
            Kind::Number(&s[start..at])
        } else if c == b'"' || c == b'\'' {
            at += 1;
            loop {
                match bytes.get(at) {
                    None | Some(b'\n' | b'\r') => {
                        return Err(invalid("unterminated C string/character literal"));
                    }
                    Some(b'\\') => {
                        at += 2;
                    }
                    Some(v) if *v == c => {
                        at += 1;
                        break;
                    }
                    _ => at += 1,
                }
            }
            Kind::String(&s[start + 1..at - 1])
        } else if c.is_ascii() {
            at += 1;
            Kind::Punct(c)
        } else {
            return Err(invalid("non-ASCII C syntax outside comments/strings"));
        };
        b.add(std::mem::size_of::<Token>() * 2)?;
        out.push(Token {
            kind,
            start,
            end: at,
        });
    }
    Ok(out)
}
#[derive(Debug)]
pub(super) struct Declaration<'a> {
    pub name: &'a str,
    pub ty: &'a str,
    pub dimensions: Vec<Option<usize>>,
    pub start: usize,
    pub end: usize,
}
pub(super) fn declarations<'a>(t: &[Token<'a>], b: &mut Budget) -> Result<Vec<Declaration<'a>>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        if t[i].kind != Kind::Punct(b'=')
            || t.get(i + 1).is_none_or(|v| v.kind != Kind::Punct(b'{'))
        {
            i += 1;
            continue;
        }
        let begin = t[..i]
            .iter()
            .rposition(|v| matches!(v.kind, Kind::Punct(b';' | b'}') | Kind::Directive(_)))
            .map_or(0, |p| p + 1);
        let prefix = &t[begin..i];
        let ty = prefix.iter().find_map(|v| match v.kind {
            Kind::Ident(s @ ("char" | "uint8_t" | "GFXglyph" | "GFXfont")) => Some(s),
            _ => None,
        });
        let mut depth = 0usize;
        let mut end = i + 1;
        loop {
            match t.get(end).map(|v| v.kind) {
                Some(Kind::Punct(b'{')) => {
                    depth += 1;
                    if depth > 8 {
                        return Err(invalid("C initializer nesting exceeds 8"));
                    }
                }
                Some(Kind::Punct(b'}')) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                None => return Err(invalid("unterminated C initializer")),
                _ => {}
            }
            end += 1;
        }
        if t.get(end + 1).is_none_or(|v| v.kind != Kind::Punct(b';')) {
            return Err(invalid("C initializer needs a semicolon"));
        }
        if let Some(ty) = ty {
            let name_pos = prefix
                .iter()
                .position(|v| v.kind == Kind::Punct(b'['))
                .and_then(|p| p.checked_sub(1))
                .or_else(|| {
                    prefix
                        .iter()
                        .rposition(|v| matches!(v.kind, Kind::Ident(s) if s != "PROGMEM"))
                });
            let name_pos = name_pos.ok_or_else(|| invalid("C declaration lacks a name"))?;
            let Kind::Ident(name) = prefix[name_pos].kind else {
                return Err(invalid("C array name is not an identifier"));
            };
            let mut dimensions = Vec::new();
            let mut j = name_pos + 1;
            while j < prefix.len() {
                if prefix[j].kind == Kind::Ident("PROGMEM") {
                    j += 1;
                    continue;
                }
                if prefix[j].kind != Kind::Punct(b'[') {
                    return Err(invalid("unsupported C array declarator"));
                }
                j += 1;
                let n = if prefix.get(j).is_some_and(|v| v.kind == Kind::Punct(b']')) {
                    None
                } else {
                    let n = integer(prefix, &mut j)?;
                    Some(usize::try_from(n).map_err(|_| invalid("invalid C array dimension"))?)
                };
                if prefix.get(j).is_none_or(|v| v.kind != Kind::Punct(b']')) {
                    return Err(invalid("C dimension must be a literal"));
                }
                j += 1;
                dimensions.push(n);
                if dimensions.len() > 2 {
                    return Err(invalid("C array supports at most two dimensions"));
                }
            }
            b.add(std::mem::size_of::<Declaration>() * 2 + 64)?;
            out.push(Declaration {
                name,
                ty,
                dimensions,
                start: i + 1,
                end,
            });
        }
        i = end + 2;
    }
    Ok(out)
}
pub(super) fn integer(t: &[Token<'_>], i: &mut usize) -> Result<i64> {
    let negative = match t.get(*i).map(|v| v.kind) {
        Some(Kind::Punct(b'-')) => {
            *i += 1;
            true
        }
        Some(Kind::Punct(b'+')) => {
            *i += 1;
            false
        }
        _ => false,
    };
    let Some(Token {
        kind: Kind::Number(raw),
        ..
    }) = t.get(*i)
    else {
        return Err(invalid("C font fields require integer literals"));
    };
    *i += 1;
    let text = raw.trim_end_matches(['u', 'U', 'l', 'L']);
    let suffix = &raw[text.len()..];
    let long = suffix
        .strip_prefix(['u', 'U'])
        .or_else(|| suffix.strip_suffix(['u', 'U']))
        .unwrap_or(suffix);
    if !matches!(long, "" | "l" | "L" | "ll" | "LL") {
        return Err(invalid("invalid C integer suffix"));
    }
    let (digits, base) =
        if let Some(v) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (v, 16)
        } else if let Some(v) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            (v, 2)
        } else if text.len() > 1 && text.starts_with('0') {
            (&text[1..], 8)
        } else {
            (text, 10)
        };
    let value = i64::from_str_radix(digits, base)
        .map_err(|_| invalid("C literal is out of range or not an integer"))?;
    Ok(if negative { -value } else { value })
}
pub(super) struct Row {
    pub values: Vec<i64>,
    pub start: usize,
    pub end: usize,
}
fn row(t: &[Token<'_>], start: usize, end: usize, b: &mut Budget) -> Result<Row> {
    let mut i = start + 1;
    let mut values = Vec::new();
    while i < end {
        let n = integer(t, &mut i)?;
        b.add(16)?;
        values.push(n);
        if i != end {
            if t[i].kind != Kind::Punct(b',') {
                return Err(invalid("C font values must be comma separated literals"));
            }
            i += 1;
        }
    }
    Ok(Row {
        values,
        start: t[start].start,
        end: t[end].end,
    })
}
pub(super) fn rows(t: &[Token<'_>], d: &Declaration<'_>, b: &mut Budget) -> Result<Vec<Row>> {
    let mut out = Vec::new();
    if t.get(d.start + 1)
        .is_some_and(|v| v.kind == Kind::Punct(b'{'))
    {
        let mut i = d.start + 1;
        while i < d.end {
            if t[i].kind != Kind::Punct(b'{') {
                return Err(invalid("mixed flat and nested C array"));
            }
            let end = (i + 1..d.end)
                .find(|&n| t[n].kind == Kind::Punct(b'}'))
                .ok_or_else(|| invalid("unclosed C row"))?;
            b.add(128)?;
            out.push(row(t, i, end, b)?);
            i = end + 1;
            if i < d.end {
                if t[i].kind != Kind::Punct(b',') {
                    return Err(invalid("missing C row comma"));
                }
                i += 1;
            }
        }
    } else {
        b.add(128)?;
        out.push(row(t, d.start, d.end, b)?);
    }
    if d.dimensions.len() == 2 {
        if d.dimensions[0].is_some_and(|n| n != out.len())
            || d.dimensions[1].is_some_and(|n| out.iter().any(|r| r.values.len() != n))
        {
            return Err(invalid("C declared dimensions differ from initializer"));
        }
    } else if d.dimensions.first().copied().flatten().is_some_and(|n| {
        n != if t[d.start + 1].kind == Kind::Punct(b'{') {
            out.len()
        } else {
            out[0].values.len()
        }
    }) {
        return Err(invalid("C declared length differs from initializer"));
    }
    Ok(out)
}
