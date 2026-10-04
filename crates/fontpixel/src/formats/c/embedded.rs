//! Shared bounded literal-data helpers for the native embedded font families.
use super::*;
use std::fmt::Write as _;

pub(super) fn select<'a>(
    ds: &'a [Declaration<'a>],
    ty: &str,
    name: Option<&str>,
) -> Result<&'a Declaration<'a>> {
    let mut it = ds
        .iter()
        .filter(|d| d.ty == ty && name.is_none_or(|n| d.name == n));
    let d = it
        .next()
        .ok_or_else(|| invalid("missing C native declaration"))?;
    if it.next().is_some() {
        return Err(invalid("ambiguous C native declaration"));
    }
    Ok(d)
}
pub(super) fn named<'a>(ds: &'a [Declaration<'a>], name: &str) -> Result<&'a Declaration<'a>> {
    let mut it = ds.iter().filter(|d| d.name == name);
    let d = it
        .next()
        .ok_or_else(|| invalid(format!("missing C data array {name}")))?;
    if it.next().is_some() {
        return Err(invalid("duplicate C data declaration"));
    }
    Ok(d)
}
pub(super) fn blob(t: &[Token<'_>], d: &Declaration<'_>, b: &mut Budget) -> Result<Vec<u8>> {
    if matches!(t[d.start].kind, Kind::String(_)) {
        let mut out = Vec::new();
        for token in &t[d.start..=d.end] {
            let Kind::String(s) = token.kind else {
                return Err(invalid("non-string in C byte string"));
            };
            let mut i = 0;
            let src = s.as_bytes();
            while i < src.len() {
                let value = if src[i] != b'\\' {
                    let v = src[i];
                    i += 1;
                    v
                } else {
                    i += 1;
                    let first = *src.get(i).ok_or_else(|| invalid("truncated C escape"))?;
                    i += 1;
                    match first {
                        b'0'..=b'7' => {
                            let mut v = u16::from(first - b'0');
                            for _ in 0..2 {
                                if let Some(&c @ b'0'..=b'7') = src.get(i) {
                                    v = v * 8 + u16::from(c - b'0');
                                    i += 1;
                                } else {
                                    break;
                                }
                            }
                            u8::try_from(v).map_err(|_| invalid("C octal escape exceeds byte"))?
                        }
                        b'x' => {
                            let start = i;
                            let mut v = 0u16;
                            while let Some(&c) = src.get(i) {
                                let Some(n) = (c as char).to_digit(16) else {
                                    break;
                                };
                                v = v
                                    .checked_mul(16)
                                    .and_then(|v| v.checked_add(n as u16))
                                    .ok_or_else(|| invalid("C hex escape overflow"))?;
                                i += 1;
                            }
                            if i == start {
                                return Err(invalid("empty C hex escape"));
                            }
                            u8::try_from(v).map_err(|_| invalid("C hex escape exceeds byte"))?
                        }
                        b'\\' => b'\\',
                        b'"' => b'"',
                        b'\'' => b'\'',
                        b'?' => b'?',
                        b'a' => 7,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'v' => 11,
                        _ => return Err(invalid("unsupported C string escape")),
                    }
                };
                b.add(2)?;
                out.push(value);
            }
        }
        let n = d
            .dimensions
            .first()
            .copied()
            .flatten()
            .unwrap_or(out.len() + 1);
        if n == out.len() + 1 {
            b.add(2)?;
            out.push(0);
        } else if n != out.len() {
            return Err(invalid("C string length differs from declared size"));
        }
        Ok(out)
    } else {
        let rows = lexer::rows(t, d, b)?;
        let mut out = Vec::new();
        for row in rows {
            for v in row.values {
                b.add(2)?;
                out.push(u8::try_from(v).map_err(|_| invalid("C byte out of range"))?);
            }
        }
        Ok(out)
    }
}
/// Split a designated initializer into borrowed scalar value token slices.
pub(super) fn fields<'a>(
    t: &'a [Token<'a>],
    start: usize,
    end: usize,
) -> Result<BTreeMap<&'a str, &'a [Token<'a>]>> {
    let mut out = BTreeMap::new();
    let mut i = start + 1;
    while i < end {
        if matches!(t[i].kind, Kind::Directive(_)) {
            i += 1;
            continue;
        }
        if t[i].kind != Kind::Punct(b'.') {
            return Err(invalid("LVGL structures require designated literal fields"));
        }
        let Some(Token {
            kind: Kind::Ident(name),
            ..
        }) = t.get(i + 1)
        else {
            return Err(invalid("missing C field name"));
        };
        if t.get(i + 2).is_none_or(|v| v.kind != Kind::Punct(b'=')) {
            return Err(invalid("missing C field equals"));
        }
        i += 3;
        let begin = i;
        while i < end && !matches!(t[i].kind, Kind::Punct(b',')) {
            i += 1;
        }
        if out.insert(*name, &t[begin..i]).is_some() {
            return Err(invalid("duplicate C designated field"));
        }
        if i < end {
            i += 1;
        }
    }
    Ok(out)
}
pub(super) fn scalar(t: &[Token<'_>]) -> Result<i64> {
    let mut i = 0;
    let n = lexer::integer(t, &mut i)?;
    if i != t.len() {
        return Err(invalid("C scalar is not a literal"));
    }
    Ok(n)
}
pub(super) fn number(
    f: &BTreeMap<&str, &[Token<'_>]>,
    name: &str,
    default: Option<i64>,
) -> Result<i64> {
    f.get(name).map_or_else(
        || default.ok_or_else(|| invalid(format!("missing LVGL field {name}"))),
        |t| scalar(t),
    )
}
pub(super) fn reference<'a>(t: &[Token<'a>]) -> Result<Option<&'a str>> {
    match t {
        [
            Token {
                kind: Kind::Ident("NULL"),
                ..
            },
        ]
        | [
            Token {
                kind: Kind::Number("0"),
                ..
            },
        ] => Ok(None),
        [
            Token {
                kind: Kind::Ident(s),
                ..
            },
        ]
        | [
            Token {
                kind: Kind::Punct(b'&'),
                ..
            },
            Token {
                kind: Kind::Ident(s),
                ..
            },
        ] => Ok(Some(s)),
        _ => Err(invalid("C pointer must be a literal data symbol")),
    }
}
pub(super) fn ref_field<'a>(f: &BTreeMap<&str, &[Token<'a>]>, name: &str) -> Result<&'a str> {
    reference(
        f.get(name)
            .ok_or_else(|| invalid(format!("missing LVGL pointer {name}")))?,
    )?
    .ok_or_else(|| invalid("required LVGL pointer is null"))
}
pub(super) fn records(t: &[Token<'_>], d: &Declaration<'_>) -> Result<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    let mut i = d.start + 1;
    while i < d.end {
        if t[i].kind != Kind::Punct(b'{') {
            return Err(invalid("LVGL array requires structure records"));
        }
        let end = (i + 1..d.end)
            .find(|&j| t[j].kind == Kind::Punct(b'}'))
            .ok_or_else(|| invalid("unterminated LVGL record"))?;
        out.push((i, end));
        i = end + 1;
        if i < d.end {
            if t[i].kind != Kind::Punct(b',') {
                return Err(invalid("missing LVGL record comma"));
            }
            i += 1;
        }
    }
    if d.dimensions
        .first()
        .copied()
        .flatten()
        .is_some_and(|n| n != out.len())
    {
        return Err(invalid("LVGL record count mismatch"));
    }
    Ok(out)
}
pub(super) fn unsigned(v: i64, max: u32) -> Result<u32> {
    u32::try_from(v)
        .ok()
        .filter(|v| *v <= max)
        .ok_or_else(|| invalid("native C field outside range"))
}
pub(super) fn emit_bytes(out: &mut String, name: &str, bytes: &[u8]) {
    writeln!(out, "static const uint8_t {name}[] = {{").expect("String write");
    if bytes.is_empty() {
        out.push('0');
    }
    for chunk in bytes.chunks(24) {
        for v in chunk {
            write!(out, "0x{v:02x},").expect("String write");
        }
        out.push('\n');
    }
    out.push_str("};\n");
}
pub(super) fn write(set: &FontSet, o: &CWriteOptions) -> Result<(Vec<u8>, Report)> {
    let l = &o.limits;
    set.validate(l)?;
    let f = set.single()?;
    if !writer::symbol(&o.array_name) {
        return Err(invalid("invalid C output symbol"));
    }
    check(o.array_name.len(), l.max_table_bytes, "C symbol bytes")?;
    let mut b = Budget::new(0, l)?;
    source_cost(set, &mut b)?;
    let mut selected = BTreeMap::new();
    let mut report = Report::default();
    for (id, g) in f.glyphs() {
        b.glyph(g.bitmap.as_bytes().len(), id.index() + 1, l)?;
        b.add(g.codepoints.len().saturating_mul(48) + 8192)?;
        let legacy = matches!(o.family, Family::U8x8 | Family::U8glib | Family::Netbsd)
            && o.generic.as_ref().is_some_and(|v| v.charset != "unicode");
        if let Some(cp) = g.codepoints.iter().find_map(|cp| match (legacy, cp) {
            (false, Codepoint::Unicode(v)) | (true, Codepoint::Legacy(v)) => Some(*v),
            _ => None,
        }) {
            if o.range.is_some_and(|(a, z)| cp < a || cp > z) {
                loss(
                    &mut report,
                    "write.outside_range",
                    Some(id.index()),
                    "Glyph outside native output range omitted",
                );
            } else if selected.insert(cp, (id.index(), g)).is_some() {
                return Err(invalid("duplicate native C mapping"));
            }
        } else {
            loss(
                &mut report,
                "write.unmapped",
                Some(id.index()),
                "Native C font requires Unicode mapping",
            );
        }
    }
    if selected.is_empty() {
        report.push(ReportEntry {
            level: Level::Error,
            impact: Impact::None,
            code: "c.write.no_representable_glyphs".into(),
            location: Location {
                font_index: Some(0),
                ..Default::default()
            },
            message: "Native C output has no encodable glyphs".into(),
        });
        return Err(Error::ReportedErrors(report));
    }
    if o.container == CContainer::LvglBinary && o.family != Family::Lvgl {
        return Err(invalid("LVGL binary container requires LVGL family"));
    }
    let out = match o.family {
        Family::U8g2 => super::u8g2::write(f, &selected, o, &mut b)?,
        Family::Lvgl => super::lvgl::write(f, &selected, o, &mut b)?,
        _ => super::legacy_embedded::write(f, &selected, o, &mut b)?,
    };
    check(
        out.len(),
        l.max_input_bytes.min(l.max_table_bytes),
        "C output bytes",
    )?;
    b.add(out.len().saturating_mul(2))?;
    let output = if o.container == CContainer::LvglBinary {
        super::lvgl_binary::encode_source(&out, o, &mut b)?
    } else {
        out.into_bytes()
    };
    let input = crate::Input::new(&output);
    let ro = ReadOptions {
        limits: ResourceLimits {
            max_total_bytes: l.max_total_bytes.saturating_sub(b.used),
            ..l.clone()
        },
        ..Default::default()
    };
    let mut ctx = ReadContext::new(&input, &ro.limits)?;
    let codec = CCodec {
        read_options: CReadOptions {
            family: o.family,
            array_name: Some(o.array_name.clone()),
            generic: o.generic.clone(),
            u8x8_legacy: o.u8x8_legacy,
            container: Some(o.container),
        },
        ..Default::default()
    };
    let (back, r) = codec.read(&mut ctx, &ro)?;
    report.entries.extend(r.entries);
    writer::compare(f, back.single()?, &selected, 0, u32::MAX, &mut report);
    if f.source_format != back.single()?.source_format || set.source_format != back.source_format {
        loss(
            &mut report,
            "write.source_format",
            None,
            "Native C target changes source provenance",
        );
    }
    if !set.properties.is_empty() {
        loss(
            &mut report,
            "write.container_properties",
            None,
            "Native C output cannot retain container metadata",
        );
    }
    Ok((output, report))
}

fn properties_cost(p: &crate::model::Properties, b: &mut Budget) -> Result<()> {
    use crate::model::PropertyValue;
    let mut pending = Vec::new();
    for p in p.iter() {
        b.add(
            p.name
                .len()
                .saturating_add(p.raw.as_ref().map_or(0, Vec::len))
                .saturating_add(128),
        )?;
        pending.push(&p.value);
    }
    while let Some(value) = pending.pop() {
        b.add(64)?;
        match value {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => b.add(s.len())?,
            PropertyValue::Bytes(v) => b.add(v.len())?,
            PropertyValue::List(v) => {
                b.add(v.len().saturating_mul(16))?;
                pending.extend(v);
            }
            _ => {}
        }
    }
    Ok(())
}
fn source_cost(set: &FontSet, b: &mut Budget) -> Result<()> {
    properties_cost(&set.properties, b)?;
    if let Some(v) = &set.source_format {
        b.add(v.len())?;
    }
    for f in &set.fonts {
        let n = &f.names;
        for v in [
            &n.family,
            &n.style,
            &n.full,
            &n.postscript,
            &n.version,
            &n.copyright,
            &n.license,
            &n.license_url,
            &n.foundry,
            &n.xlfd,
            &f.charset.name,
            &f.charset.registry,
            &f.charset.encoding,
            &f.source_format,
        ]
        .into_iter()
        .flatten()
        {
            b.add(v.len())?;
        }
        for v in &n.xlfd_fields {
            b.add(v.len().saturating_add(64))?;
        }
        b.add(f.charset.code_ranges.len().saturating_mul(8))?;
        for props in [&n.localized, &f.headers, &f.properties] {
            properties_cost(props, b)?;
        }
        for (_, g) in f.glyphs() {
            properties_cost(&g.properties, b)?;
            if let Some(v) = &g.name {
                b.add(v.len())?;
            }
            for sequence in &g.unicode_sequences {
                b.add(sequence.len().saturating_mul(4).saturating_add(64))?;
            }
        }
    }
    Ok(())
}
