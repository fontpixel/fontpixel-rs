use super::*;
use crate::model::{Codepoint, FontNames, Glyph, Property, PropertyValue, Vector};
use roxmltree::{Document, Node};
use std::collections::BTreeSet;

pub(super) fn preflight(s: &str, l: &ResourceLimits, budget: &mut Budget) -> Result<u32> {
    use xmlparser::{ElementEnd, Token};
    let (mut depth, mut attrs, mut nodes) = (0usize, 0usize, 1u32);
    for token in xmlparser::Tokenizer::from(s) {
        match token.map_err(invalid)? {
            Token::DtdStart { .. } | Token::EntityDeclaration { .. } | Token::DtdEnd { .. } => {
                return Err(invalid(
                    "internal DTD and entity declarations are forbidden",
                ));
            }
            Token::EmptyDtd { name, .. } if name.as_str() != "kbits" => {
                return Err(invalid("DOCTYPE root is not kbits"));
            }
            Token::ProcessingInstruction { .. } => {
                return Err(invalid("processing instructions are forbidden"));
            }
            Token::Declaration {
                encoding: Some(e), ..
            } if !e.as_str().eq_ignore_ascii_case("utf-8") => {
                return Err(invalid("only UTF-8 XML is supported"));
            }
            Token::ElementStart { .. } => {
                depth += 1;
                attrs = 0;
                bound(depth, 32, "KBITX XML depth")?;
                nodes = nodes
                    .checked_add(1)
                    .ok_or_else(|| invalid("too many XML nodes"))?;
                budget.add(192)?;
            }
            Token::ElementEnd {
                end: ElementEnd::Close(..) | ElementEnd::Empty,
                ..
            } => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| invalid("unbalanced XML"))?;
            }
            Token::Attribute { value, .. } => {
                attrs += 1;
                bound(attrs, 32, "KBITX XML attributes")?;
                bound(value.len(), l.max_table_bytes, "KBITX XML attribute bytes")?;
                budget.add(128usize.saturating_add(value.len().saturating_mul(2)))?;
            }
            Token::Text { text } | Token::Cdata { text, .. } | Token::Comment { text, .. } => {
                nodes = nodes
                    .checked_add(1)
                    .ok_or_else(|| invalid("too many XML nodes"))?;
                bound(text.len(), l.max_table_bytes, "KBITX XML text bytes")?;
                budget.add(192usize.saturating_add(text.len().saturating_mul(2)))?;
            }
            _ => {}
        }
    }
    Ok(nodes)
}
pub(super) fn document<'a>(
    s: &'a str,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Document<'a>> {
    let nodes = preflight(s, l, budget)?;
    Document::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: nodes.saturating_add(1),
            entity_resolver: None,
        },
    )
    .map_err(invalid)
}
fn int(s: &str) -> Result<i64> {
    checked_i32(s.parse::<i64>().map_err(invalid)?)
}
fn num(n: Node<'_, '_>, name: &str) -> Result<i64> {
    n.attribute(name).map_or(Ok(0), int)
}
fn attrs(
    n: Node<'_, '_>,
    allowed: &[&str],
    report: &mut Report,
    budget: &mut Budget,
    index: Option<usize>,
) -> Result<()> {
    for a in n.attributes() {
        if a.namespace().is_some() || !allowed.contains(&a.name()) {
            budget.add(256 + a.name().len())?;
            diag(
                report,
                "read.attribute",
                Impact::Loss,
                index,
                format!(
                    "Unsupported attribute {:?} on {} omitted",
                    a.name(),
                    n.tag_name().name()
                ),
            );
        }
    }
    Ok(())
}
fn empty(n: Node<'_, '_>) -> Result<()> {
    if n.children()
        .any(|v| v.is_element() || v.is_text() && !v.text().unwrap_or("").trim().is_empty())
    {
        Err(invalid("scalar/glyph element must be empty"))
    } else {
        Ok(())
    }
}
pub(super) fn name_mut<'a>(names: &'a mut FontNames, id: &str) -> Option<&'a mut Option<String>> {
    match id {
        "0" => Some(&mut names.copyright),
        "1" => Some(&mut names.family),
        "2" => Some(&mut names.style),
        "4" => Some(&mut names.full),
        "5" => Some(&mut names.version),
        "6" => Some(&mut names.postscript),
        "8" => Some(&mut names.foundry),
        "13" => Some(&mut names.license),
        "14" => Some(&mut names.license_url),
        _ => None,
    }
}
pub(super) fn prop(font: &Font, key: &str) -> Option<i64> {
    match font.properties.get(&format!("kbitx.prop.{key}")) {
        Some(PropertyValue::Integer(v)) => Some(*v),
        _ => None,
    }
}
pub(super) fn normalize(font: &mut Font) -> Result<()> {
    font.metrics.ascent = prop(font, "emAscent").or_else(|| prop(font, "lineAscent"));
    font.metrics.descent = prop(font, "emDescent").or_else(|| prop(font, "lineDescent"));
    font.metrics.line_gap = prop(font, "lineGap");
    font.metrics.pixel_size = match (font.metrics.ascent, font.metrics.descent) {
        (Some(a), Some(d)) => Some(
            u32::try_from(
                a.checked_add(d)
                    .ok_or_else(|| invalid("em height overflow"))?,
            )
            .map_err(invalid)?,
        ),
        _ => None,
    };
    font.metrics.point_size = Some(crate::model::Rational::integer(i64::from(
        font.metrics.pixel_size.unwrap_or(0),
    )));
    font.metrics.resolution = Some(Vector::new(72, 72));
    font.update_bounds()?;
    Ok(())
}
/// Retained metadata is an inert standard subtree, never executable XML.
pub(super) fn auxiliary(n: Node<'_, '_>) -> Result<()> {
    if n.tag_name().namespace().is_some() {
        return Err(invalid("namespaced auxiliary XML is unsupported"));
    }
    let (allowed, children): (&[&str], &[&str]) = match n.tag_name().name() {
        "k" => (&["lu", "ln", "ru", "rn", "o"], &[]),
        "feat" => (&["name"], &[]),
        "puaa" => (&[], &["pt"]),
        "pt" => (&["prop"], &["pv"]),
        "pv" => (&["range", "value"], &[]),
        _ => return Err(invalid("unknown auxiliary element")),
    };
    if n.attributes()
        .any(|a| a.namespace().is_some() || !allowed.contains(&a.name()))
    {
        return Err(invalid("unknown auxiliary attribute"));
    }
    for child in n.children() {
        if child.is_element() {
            if !children.contains(&child.tag_name().name()) {
                return Err(invalid("invalid auxiliary child"));
            }
            auxiliary(child)?;
        } else if child.is_text()
            && n.tag_name().name() != "feat"
            && !child.text().unwrap_or("").trim().is_empty()
        {
            return Err(invalid("unexpected auxiliary text"));
        }
    }
    match n.tag_name().name() {
        "k" => {
            num(n, "o")?;
            if n.attribute("o").is_none()
                || (n.attribute("lu").is_none() && n.attribute("ln").is_none())
                || (n.attribute("ru").is_none() && n.attribute("rn").is_none())
            {
                return Err(invalid("incomplete kerning record"));
            }
            for key in ["lu", "ru"] {
                if let Some(s) = n.attribute(key) {
                    let cp = u32::try_from(int(s)?).map_err(invalid)?;
                    if char::from_u32(cp).is_none() {
                        return Err(invalid("invalid kerning Unicode scalar"));
                    }
                }
            }
        }
        "pt" if n.attribute("prop").is_none() => {
            return Err(invalid("PUAA property name is missing"));
        }
        "pv" if n.attribute("range").is_none() || n.attribute("value").is_none() => {
            return Err(invalid("PUAA range/value is missing"));
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn read(
    bytes: &[u8],
    options: &KbitxReadOptions,
    l: &ResourceLimits,
    retained: usize,
) -> Result<(Font, Report)> {
    bound(bytes.len(), l.max_input_bytes, "KBITX input bytes")?;
    let s = std::str::from_utf8(bytes)
        .map_err(invalid)?
        .trim_start_matches('\u{feff}');
    let mut budget = Budget::new(retained, l.max_total_bytes)?;
    budget.add(2048)?;
    let doc = document(s, l, &mut budget)?;
    let root = doc.root_element();
    if root.tag_name().name() != "kbits" || root.tag_name().namespace().is_some() {
        return Err(invalid("root is not kbits"));
    }
    let mut font = Font::default();
    font.source_format = Some("kbitx".into());
    font.charset.name = Some("unicode".into());
    let mut report = Report::default();
    let mut scalar_ids = BTreeSet::new();
    let mut seen_puaa = false;
    attrs(root, &[], &mut report, &mut budget, None)?;
    for n in root.children() {
        if !n.is_element() {
            if n.is_text() && !n.text().unwrap_or("").trim().is_empty() {
                budget.add(256)?;
                diag(
                    &mut report,
                    "read.text",
                    Impact::Loss,
                    None,
                    "Text outside native records omitted",
                );
            }
            continue;
        }
        if n.tag_name().namespace().is_some() {
            budget.add(256)?;
            diag(
                &mut report,
                "read.element",
                Impact::Loss,
                None,
                "Namespaced element omitted",
            );
            continue;
        }
        match n.tag_name().name() {
            "prop" | "name" => {
                attrs(n, &["id", "value"], &mut report, &mut budget, None)?;
                empty(n)?;
                let id = n
                    .attribute("id")
                    .ok_or_else(|| invalid("scalar record has no ID"))?;
                let value = n
                    .attribute("value")
                    .ok_or_else(|| invalid("scalar record has no value"))?;
                let kind = n.tag_name().name();
                budget.add(
                    640usize
                        .saturating_add(id.len().saturating_mul(2))
                        .saturating_add(value.len().saturating_mul(2)),
                )?;
                let canonical;
                let id = if kind == "name" {
                    canonical = int(id)?.to_string();
                    canonical.as_str()
                } else {
                    id
                };
                let key = format!("kbitx.{kind}.{id}");
                if !scalar_ids.insert(key.clone()) {
                    diag(
                        &mut report,
                        "read.duplicate_scalar",
                        Impact::None,
                        None,
                        "Duplicate scalar retained; last value supplies normalized metadata",
                    );
                }
                let value = if kind == "prop" {
                    PropertyValue::Integer(int(value)?)
                } else {
                    int(id)?;
                    if let Some(slot) = name_mut(&mut font.names, id) {
                        *slot = Some(value.to_owned());
                    }
                    PropertyValue::Text(value.into())
                };
                font.properties.push(Property::new(key, value));
            }
            "g" => {
                bound(
                    font.len().saturating_add(1),
                    l.max_glyphs,
                    "KBITX glyph count",
                )?;
                let index = font.len();
                budget.add(1536)?;
                attrs(
                    n,
                    &["u", "n", "x", "y", "w", "d"],
                    &mut report,
                    &mut budget,
                    Some(index),
                )?;
                empty(n)?;
                let cp = n
                    .attribute("u")
                    .map(|v| v.parse::<u32>().map_err(invalid))
                    .transpose()?;
                if cp.is_some_and(|v| char::from_u32(v).is_none()) {
                    return Err(invalid("glyph code is not a Unicode scalar"));
                }
                let name = n.attribute("n");
                if cp.is_none() && name.is_none() {
                    return Err(invalid("glyph requires Unicode or a name"));
                }
                let data = n
                    .attribute("d")
                    .ok_or_else(|| invalid("glyph bitmap is missing"))?;
                let (bitmap, loss) = wire::decode(data, options.gray_threshold, l, &mut budget)?;
                let mut g = Glyph::new(bitmap);
                g.metrics.bounds.x = num(n, "x")?;
                g.metrics.bounds.y = num(n, "y")?
                    .checked_sub(i64::from(g.bitmap.height()))
                    .ok_or_else(|| invalid("bottom bearing overflow"))?;
                g.metrics.advance = Some(Vector::new(num(n, "w")?, 0));
                if let Some(cp) = cp {
                    g.codepoints.push(Codepoint::Unicode(cp));
                    if font.glyph_id(Codepoint::Unicode(cp)).is_some() {
                        diag(
                            &mut report,
                            "read.duplicate_mapping",
                            Impact::None,
                            Some(index),
                            "Duplicate Unicode label retained; last glyph wins lookup",
                        );
                    }
                }
                if let Some(name) = name {
                    budget.add(name.len())?;
                    g.name = Some(name.into());
                }
                if loss {
                    diag(
                        &mut report,
                        "read.gray",
                        Impact::Loss,
                        Some(index),
                        "Raster samples thresholded to monochrome",
                    );
                }
                font.add_glyph(g)?;
            }
            "k" | "feat" | "puaa" => {
                if n.tag_name().name() == "puaa" {
                    if seen_puaa {
                        return Err(invalid("only one PUAA element is permitted"));
                    }
                    seen_puaa = true;
                }
                auxiliary(n)?;
                let raw = &s[n.range()];
                budget.add(raw.len().saturating_add(256))?;
                font.properties.push(Property::new("kbitx.element", raw));
            }
            _ => {
                budget.add(256usize.saturating_add(n.tag_name().name().len()))?;
                diag(
                    &mut report,
                    "read.element",
                    Impact::Loss,
                    None,
                    format!("Unsupported element {:?} omitted", n.tag_name().name()),
                );
            }
        }
    }
    normalize(&mut font)?;
    Ok((font, report))
}
