use super::*;
use crate::{
    Font,
    bitmap::Bounds,
    model::{Codepoint, Spacing, Vector},
    outline::path::Edge,
};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;
fn svg(n: Node<'_, '_>, name: &str) -> bool {
    n.is_element()
        && n.tag_name().name() == name
        && matches!(n.tag_name().namespace(), None | Some(NS))
}
fn fp<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute((FP, name))
}
fn number(s: &str) -> Result<f64> {
    let n: f64 = s.trim().parse().map_err(|_| invalid("invalid number"))?;
    if !n.is_finite() || n.abs() > 1e9 {
        return Err(invalid("nonfinite/excessive number"));
    }
    Ok(n)
}
fn num(n: Node<'_, '_>, key: &str, default: f64) -> Result<f64> {
    n.attribute(key)
        .map(number)
        .transpose()
        .map(|v| v.unwrap_or(default))
}
fn integer(s: &str) -> Result<i64> {
    s.parse().map_err(|_| invalid("invalid native integer"))
}
fn native_bounds(s: &str) -> Result<Bounds> {
    let v = s
        .split_whitespace()
        .map(integer)
        .collect::<Result<Vec<_>>>()?;
    if v.len() != 4 {
        return Err(invalid("native bounds need width height x y"));
    }
    let b = Bounds::new(
        v[0].try_into().map_err(|_| invalid("bounds width"))?,
        v[1].try_into().map_err(|_| invalid("bounds height"))?,
        v[2],
        v[3],
    );
    b.x.checked_add(i64::from(b.width))
        .ok_or_else(|| invalid("bounds right overflow"))?;
    b.y.checked_add(i64::from(b.height))
        .ok_or_else(|| invalid("bounds top overflow"))?;
    Ok(b)
}
fn vector(s: &str) -> Result<Vector> {
    let v = s
        .split_whitespace()
        .map(integer)
        .collect::<Result<Vec<_>>>()?;
    if v.len() != 2 {
        return Err(invalid("native vector needs two integers"));
    }
    Ok(Vector::new(v[0], v[1]))
}
fn document<'a>(bytes: &'a [u8], l: &Limits) -> Result<Document<'a>> {
    bound(
        bytes.len(),
        l.resources.max_input_bytes.min(l.resources.max_total_bytes),
        "SVG input bytes",
    )?;
    let s = std::str::from_utf8(bytes).map_err(invalid)?;
    // roxmltree allows an empty DTD even with allow_dtd=false; reject all DTDs.
    if s.contains("<!DOCTYPE") {
        return Err(invalid("DTD is forbidden"));
    }
    let d = Document::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: l.max_nodes,
            entity_resolver: None,
        },
    )
    .map_err(invalid)?;
    if !svg(d.root_element(), "svg") {
        return Err(invalid("root is not SVG"));
    }
    for n in d.descendants() {
        bound(
            n.ancestors().take(l.max_depth.saturating_add(2)).count(),
            l.max_depth.saturating_add(1),
            "SVG XML depth",
        )?;
        if n.is_pi() {
            return Err(invalid("processing instructions are unsupported"));
        }
        if let Some(text) = n.text() {
            bound(text.len(), l.resources.max_table_bytes, "SVG text bytes")?;
        }
        bound(n.attributes().len(), l.max_attributes, "SVG XML attributes")?;
        for a in n.attributes() {
            bound(
                a.value().len(),
                l.resources.max_table_bytes,
                "SVG attribute bytes",
            )?;
        }
    }
    Ok(d)
}
pub(super) fn detect(bytes: &[u8], l: &Limits) -> Confidence {
    match document(bytes, l) {
        Ok(d) if d.descendants().any(|n| svg(n, "font")) => Confidence::Certain,
        _ => Confidence::NoMatch,
    }
}
fn semantics(n: Node<'_, '_>) -> Result<()> {
    for a in n.attributes() {
        if a.namespace() == Some(FP) {
            continue;
        }
        if matches!(
            a.name(),
            "style"
                | "class"
                | "transform"
                | "clip-path"
                | "mask"
                | "filter"
                | "href"
                | "lang"
                | "arabic-form"
                | "orientation"
        ) || a.name().starts_with("on")
            || a.name() == "fill-rule" && a.value() != "nonzero"
            || a.name() == "fill" && !matches!(a.value(), "black" | "#000" | "#000000")
            || a.name() == "stroke" && a.value() != "none"
            || matches!(a.name(), "opacity" | "fill-opacity") && a.value() != "1"
            || matches!(a.name(), "display" | "visibility")
        {
            return Err(invalid(format!(
                "unsupported rendering/selection attribute {}",
                a.name()
            )));
        }
    }
    Ok(())
}
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn grid(paths: &[Record<'_, '_>], upem: f64) -> Result<u16> {
    if upem.fract() != 0. {
        return Err(invalid("nonintegral em requires ppem"));
    }
    let mut div = 0;
    for record in paths {
        for e in &record.edges {
            for (x, y) in e.points() {
                for v in [x, y] {
                    if !v.is_finite() || v.fract() != 0. || v.abs() > 1e9 {
                        return Err(invalid("off-grid path requires explicit ppem"));
                    }
                    div = gcd(div, v.abs() as u64);
                }
            }
        }
    }
    if div == 0 || upem as u64 % div != 0 {
        return Err(invalid("no native grid; specify ppem"));
    }
    let ppem = upem as u64 / div;
    if !(4..=64).contains(&ppem) {
        return Err(invalid("native grid outside 4..64; specify ppem"));
    }
    Ok(ppem as u16)
}
struct Record<'a, 'b> {
    node: Node<'a, 'b>,
    edges: Vec<Edge>,
    signature: Vec<(String, String)>,
}
fn record<'a, 'b>(n: Node<'a, 'b>, origin: (f64, f64), l: &Limits) -> Result<Record<'a, 'b>> {
    semantics(n)?;
    let mut edges = path::parse(n.attribute("d").unwrap_or(""), l)?;
    let mut signature = n
        .attributes()
        .filter(|a| a.name() != "unicode" && !(a.namespace() == Some(FP) && a.name() == "id"))
        .map(|a| {
            (
                format!("{}:{}", a.namespace().unwrap_or(""), a.name()),
                a.value().to_string(),
            )
        })
        .collect::<Vec<_>>();
    for child in n.children().filter(Node::is_element) {
        if svg(child, "path") {
            semantics(child)?;
            if n.attribute("d").is_some_and(|s| !s.is_empty()) {
                return Err(invalid("glyph d plus graphical children is ambiguous"));
            }
            for a in child.attributes() {
                if !matches!(a.name(), "d" | "id" | "fill" | "fill-rule" | "stroke") {
                    return Err(invalid("unsupported child path attribute"));
                }
            }
            if child.children().any(|n| n.is_element()) {
                return Err(invalid("nested graphical glyph content"));
            }
            let d = child.attribute("d").unwrap_or("");
            signature.push(("child-path".into(), d.to_owned()));
            let e = path::parse(d, l)?;
            bound(
                edges.len().saturating_add(e.len()),
                l.geometry.max_edges,
                "SVG glyph edges",
            )?;
            edges.extend(e);
        } else if !svg(child, "title") && !svg(child, "desc") && !svg(child, "metadata") {
            return Err(invalid("unsupported graphical glyph child"));
        }
    }
    signature.sort();
    path::shift(&mut edges, origin);
    Ok(Record {
        node: n,
        edges,
        signature,
    })
}
fn project(v: f64, scale: f64, rounded: &mut bool) -> Result<i64> {
    let v = v / scale;
    if !v.is_finite() || v.abs() > 1e9 {
        return Err(invalid("projected metric out of range"));
    }
    *rounded |= (v - v.round()).abs() > 1e-9;
    Ok(v.round() as i64)
}
/// Parse SVG 1.1 fonts without resolving any external resource.
pub fn read(bytes: &[u8], o: &ReadOptions) -> Result<(FontSet, Report)> {
    if o.ppem.is_some_and(|p| p == 0 || p > 4096) {
        return Err(invalid("ppem must be 1..4096"));
    }
    let l = &o.limits;
    let d = document(bytes, l)?;
    let nodes = d
        .descendants()
        .filter(|n| svg(*n, "font"))
        .collect::<Vec<_>>();
    bound(nodes.len(), l.max_fonts, "SVG fonts")?;
    if nodes.is_empty() {
        return Err(invalid("document contains no SVG font"));
    }
    if o.font_index.is_some_and(|i| i >= nodes.len()) {
        return Err(invalid("font index out of range"));
    }
    let mut report = Report::default();
    let mut set = FontSet::default();
    set.source_format = Some("svg".into());
    let (mut total_edges, mut total_pixels, mut total_glyphs) = (0usize, 0usize, 0usize);
    // Active content cannot influence even a selected face.
    for n in d.descendants().filter(Node::is_element) {
        if matches!(
            n.tag_name().name(),
            "script" | "style" | "use" | "image" | "foreignObject" | "font-face-uri"
        ) {
            return Err(invalid("active/external SVG content is unsupported"));
        }
    }
    for (index, n) in nodes.into_iter().enumerate() {
        if o.font_index.is_some_and(|i| i != index) {
            continue;
        }
        let fi = set.fonts.len();
        for a in n.ancestors().filter(Node::is_element) {
            semantics(a)?;
            if a != n && svg(a, "font") {
                return Err(invalid("nested fonts"));
            }
        }
        let faces = n
            .children()
            .filter(|n| svg(*n, "font-face"))
            .collect::<Vec<_>>();
        if faces.len() != 1 {
            return Err(invalid("exactly one font-face required per font"));
        }
        let face = faces[0];
        semantics(face)?;
        let upem = num(face, "units-per-em", 1000.)?;
        if upem <= 0. {
            return Err(invalid("units-per-em must be positive"));
        }
        let origin = (num(n, "horiz-origin-x", 0.)?, num(n, "horiz-origin-y", 0.)?);
        let mut records = Vec::new();
        let mut ignored = false;
        for child in n.children().filter(Node::is_element) {
            if svg(child, "glyph") || svg(child, "missing-glyph") {
                charge(
                    &mut total_glyphs,
                    1,
                    l.resources.max_glyphs,
                    "SVG glyph records",
                )?;
                let rec = record(child, origin, l)?;
                charge(
                    &mut total_edges,
                    rec.edges.len(),
                    l.max_total_edges,
                    "SVG total edges",
                )?;
                records.push(rec);
            } else if !svg(child, "font-face") {
                ignored = true;
            }
        }
        if ignored {
            loss(
                &mut report,
                "unsupported-font-content",
                "Kerning/descriptive or unknown font content is omitted",
                Some(fi),
                None,
            );
        }
        let native = fp(n, "ppem")
            .map(integer)
            .transpose()?
            .map(|p| u16::try_from(p).map_err(|_| invalid("native ppem out of range")))
            .transpose()?;
        if native.is_some_and(|p| p == 0 || p > 4096) {
            return Err(invalid("native ppem must be 1..4096"));
        }
        let ppem = match o.ppem.or(native) {
            Some(p) => p,
            None => grid(&records, upem)?,
        };
        let scale = upem / f64::from(ppem);
        let native_exact = native == Some(ppem);
        let mut rounded = false;
        let mut f = Font::default();
        f.source_format = Some("svg".into());
        f.metrics.pixel_size = Some(u32::from(ppem));
        f.names.family = face.attribute("font-family").map(str::to_owned);
        f.names.style = face.attribute("font-style").map(str::to_owned);
        for (key, target) in [
            ("full", &mut f.names.full),
            ("postscript", &mut f.names.postscript),
            ("version", &mut f.names.version),
            ("copyright", &mut f.names.copyright),
            ("license", &mut f.names.license),
            ("license-url", &mut f.names.license_url),
            ("foundry", &mut f.names.foundry),
            ("xlfd", &mut f.names.xlfd),
        ] {
            *target = fp(face, key).map(str::to_owned);
        }
        for (key, target) in [
            ("ascent", &mut f.metrics.ascent),
            ("descent", &mut f.metrics.descent),
            ("underline-position", &mut f.metrics.underline_position),
            ("underline-thickness", &mut f.metrics.underline_thickness),
        ] {
            if let Some(s) = face.attribute(key) {
                let mut v = project(number(s)?, scale, &mut rounded)?;
                if key == "descent" {
                    v = v.abs();
                }
                *target = Some(v);
            }
        }
        if native_exact {
            f.metrics.bounds = fp(n, "bounds").map(native_bounds).transpose()?;
            for (key, target) in [
                ("line-gap", &mut f.metrics.line_gap),
                ("line-height", &mut f.metrics.line_height),
                ("space-width", &mut f.metrics.space_width),
            ] {
                *target = fp(n, key).map(integer).transpose()?;
            }
            f.metrics.spacing = fp(n, "spacing")
                .map(|s| match s {
                    "p" => Ok(Spacing::Proportional),
                    "m" => Ok(Spacing::Monospaced),
                    "c" => Ok(Spacing::CharacterCell),
                    _ => Err(invalid("invalid native spacing")),
                })
                .transpose()?;
        }
        let adv = num(n, "horiz-adv-x", 0.)?;
        let asc = num(face, "ascent", upem)?;
        let mut groups: BTreeMap<usize, (crate::model::GlyphId, Vec<(String, String)>)> =
            BTreeMap::new();
        let mut missing = false;
        for rec in records {
            let node = rec.node;
            let gi = f.len();
            let mut b = if native_exact {
                fp(node, "bounds").map(native_bounds).transpose()?
            } else {
                None
            };
            let fixed = b.is_some();
            if b.is_none() {
                if rec.edges.is_empty() {
                    b = Some(Bounds::default());
                } else {
                    let (mut x0, mut y0, mut x1, mut y1) = (
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    );
                    for e in &rec.edges {
                        for (x, y) in e.points() {
                            x0 = x0.min(x / scale);
                            y0 = y0.min(y / scale);
                            x1 = x1.max(x / scale);
                            y1 = y1.max(y / scale);
                        }
                    }
                    if [x0, y0, x1, y1]
                        .iter()
                        .any(|v| !v.is_finite() || v.abs() > 1e9)
                    {
                        return Err(invalid("projected bounds out of range"));
                    }
                    b = Some(Bounds::new(
                        (x1.ceil() - x0.floor()) as u32,
                        (y1.ceil() - y0.floor()) as u32,
                        x0.floor() as i64,
                        y0.floor() as i64,
                    ));
                }
            }
            let b = b.ok_or_else(|| invalid("missing raster bounds"))?;
            let pixels = (b.width as usize)
                .saturating_mul(b.height as usize)
                .saturating_add(b.height as usize);
            bound(
                pixels,
                l.resources.max_pixels_per_glyph.min(l.geometry.max_pixels),
                "SVG glyph pixels and rows",
            )?;
            charge(
                &mut total_pixels,
                pixels,
                l.max_total_pixels,
                "SVG total raster pixels",
            )?;
            let bitmap = outline::path::rasterize(
                &rec.edges,
                scale,
                (b.x as f64, b.y as f64),
                b.width,
                b.height,
                &l.geometry,
            )?;
            let mut g = Glyph::new(bitmap);
            g.metrics.bounds = b;
            if !fixed {
                let bitmap = g.bitmap.clone();
                g.set_bitmap(bitmap, b.x, b.y, true)?;
            }
            g.name = node.attribute("glyph-name").map(str::to_owned);
            let ax = project(num(node, "horiz-adv-x", adv)?, scale, &mut rounded)?;
            let ay = if native_exact {
                fp(node, "advance-y").map(integer).transpose()?.unwrap_or(0)
            } else {
                0
            };
            g.metrics.advance = Some(Vector::new(ax, ay));
            let vertical_present =
                node.attribute("vert-adv-y").is_some() || n.attribute("vert-adv-y").is_some();
            if vertical_present {
                g.metrics.vertical_advance = Some(Vector::new(
                    0,
                    -project(
                        num(node, "vert-adv-y", num(n, "vert-adv-y", upem)?)?,
                        scale,
                        &mut rounded,
                    )?,
                ));
            }
            if native_exact && let Some(v) = fp(node, "vertical-advance") {
                let v = vector(v)?;
                if g.metrics.vertical_advance.is_some_and(|s| s.y != v.y) {
                    return Err(invalid("inconsistent native vertical metric"));
                }
                g.metrics.vertical_advance = Some(v);
            }
            if vertical_present
                || node.attribute("vert-origin-x").is_some()
                || node.attribute("vert-origin-y").is_some()
                || n.attribute("vert-origin-x").is_some()
                || n.attribute("vert-origin-y").is_some()
            {
                let x = num(
                    node,
                    "vert-origin-x",
                    num(n, "vert-origin-x", num(node, "horiz-adv-x", adv)? / 2.)?,
                )? - origin.0;
                let y = num(node, "vert-origin-y", num(n, "vert-origin-y", asc)?)? - origin.1;
                g.metrics.vertical_origin = Some(Vector::new(
                    project(x, scale, &mut rounded)?,
                    project(y, scale, &mut rounded)?,
                ));
            }
            let is_missing = svg(node, "missing-glyph");
            if is_missing {
                if missing {
                    return Err(invalid("multiple missing-glyph elements"));
                }
                missing = true;
            }
            let mapping = node
                .attribute("unicode")
                .map(|s| s.chars().map(|c| c as u32).collect::<Vec<_>>());
            if mapping.as_ref().is_some_and(Vec::is_empty) {
                return Err(invalid("empty unicode mapping"));
            }
            if is_missing && mapping.is_some() {
                return Err(invalid("missing-glyph cannot carry unicode"));
            }
            let group = fp(node, "id")
                .map(|s| {
                    s.parse::<usize>()
                        .map_err(|_| invalid("invalid glyph identity"))
                })
                .transpose()?;
            let append_mapping = |g: &mut Glyph| {
                if let Some(s) = &mapping {
                    if s.len() == 1 {
                        g.codepoints.push(Codepoint::Unicode(s[0]));
                    } else {
                        g.unicode_sequences.push(s.clone());
                    }
                }
            };
            let id = if let Some((id, signature)) = group.and_then(|v| groups.get(&v)) {
                let old = f
                    .glyph(*id)
                    .ok_or_else(|| invalid("invalid alias reference"))?;
                if old.bitmap != g.bitmap
                    || old.metrics != g.metrics
                    || old.name != g.name
                    || *signature != rec.signature
                {
                    return Err(invalid("inconsistent native alias identity"));
                }
                let id = *id;
                f.edit_glyph(id, |g| {
                    append_mapping(g);
                    Ok(())
                })?;
                id
            } else {
                append_mapping(&mut g);
                let id = f.add_glyph(g)?;
                if let Some(group) = group {
                    groups.insert(group, (id, rec.signature));
                }
                id
            };
            if is_missing {
                f.metrics.default_glyph = Some(id);
            }
            let allowed = [
                "id",
                "glyph-name",
                "unicode",
                "d",
                "horiz-adv-x",
                "vert-adv-y",
                "vert-origin-x",
                "vert-origin-y",
                "fill",
                "fill-rule",
                "stroke",
            ];
            if node
                .attributes()
                .any(|a| a.namespace() != Some(FP) && !allowed.contains(&a.name()))
            {
                loss(
                    &mut report,
                    "glyph-attributes",
                    "Unsupported glyph attributes omitted",
                    Some(fi),
                    Some(gi),
                );
            }
        }
        if f.metrics.bounds.is_none() {
            f.update_bounds()?;
        }
        if rounded || o.ppem.is_some() && native.is_some() && !native_exact {
            loss(
                &mut report,
                "raster-projection",
                "Explicit raster projection rounds design metrics or changes native size",
                Some(fi),
                None,
            );
        }
        if !native_exact {
            loss(
                &mut report,
                "outline-projection",
                "SVG vector outlines projected to monochrome pixel centers; original paths are not retained",
                Some(fi),
                None,
            );
        }
        f.validate(&l.resources)?;
        set.fonts.push(f);
    }
    Ok((set, report))
}
