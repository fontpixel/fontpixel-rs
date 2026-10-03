//! Native OpenType projection: exhaustive grid recovery, exact center filling,
//! matching embedded-strike preference, web wrappers and ordered collection faces.
use super::embedded::{self, EmbeddedReadOptions};
use crate::{
    Bitmap, Error, Font, FontSet, Glyph, Report, ReportEntry, Result,
    bitmap::Bounds,
    model::{Codepoint, Property, PropertyValue, Spacing, Vector},
    outline::{
        self,
        path::{Edge, Point},
    },
    report::{Impact, Level},
    sfnt::{self, Sfnt, cff, cmap, glyf, metrics, post},
};
use std::collections::BTreeMap;

/// Limits shared by all vector-reader paths and accumulated model output.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Input, sfnt table and glyph limits.
    pub sfnt: sfnt::Limits,
    /// Per-glyph raster and expanded geometry budgets.
    pub geometry: outline::Limits,
    /// Cumulative expanded edges across a complete face pass.
    pub max_total_edges: usize,
    /// Cumulative graph visits or CFF executed bytes across a face pass.
    pub max_total_work: usize,
    /// Maximum collection faces and retained strikes.
    pub max_fonts: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sfnt: Default::default(),
            geometry: Default::default(),
            max_total_edges: 32 * 1024 * 1024,
            max_total_work: 256 * 1024 * 1024,
            max_fonts: 1024,
        }
    }
}
/// Explicit overrides distinguish rasterization from automatic grid recovery.
#[derive(Clone, Debug)]
pub struct Options {
    /// None detects a native grid or extracts every physical embedded size.
    pub ppem: Option<u16>,
    /// Face index for TTC/OTC; None reads every face.
    pub face_index: Option<usize>,
    /// Matching physical embedded images take precedence, by default.
    pub prefer_embedded: bool,
    /// Explicit design units per pixel; requires ppem (for nondivisible ems).
    pub grid_units: Option<u16>,
    /// Reviewed translation in pixel units, applied to outlines only.
    pub pixel_offset: Point,
    /// Optional explicit gray threshold, otherwise exact gray coverage is retained.
    pub gray_threshold: Option<u8>,
    /// Alpha-weighted darkness instead of alpha for color images.
    pub color_darkness: bool,
    /// Complete bounded-resource policy.
    pub limits: Limits,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            ppem: None,
            face_index: None,
            prefer_embedded: true,
            grid_units: None,
            pixel_offset: (0., 0.),
            gray_threshold: None,
            color_darkness: false,
            limits: Default::default(),
        }
    }
}
fn invalid(s: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("vector-raster: {s}"))
}
fn required<'a>(f: &Sfnt<'a>, tag: [u8; 4]) -> Result<&'a [u8]> {
    f.table(tag)
        .ok_or_else(|| invalid(format!("missing {}", String::from_utf8_lossy(&tag))))
}
fn note(r: &mut Report, code: &str, message: impl Into<String>) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("vector-raster.{code}"),
        location: Default::default(),
        message: message.into(),
    });
}
fn bound(n: usize, limit: usize, resource: &'static str) -> Result<()> {
    if n > limit {
        Err(Error::ResourceLimit { resource, limit })
    } else {
        Ok(())
    }
}
fn charge(n: &mut usize, amount: usize, l: usize, label: &'static str) -> Result<()> {
    *n = n
        .checked_add(amount)
        .ok_or_else(|| invalid("budget overflow"))?;
    bound(*n, l, label)
}
fn validate(o: &Options) -> Result<()> {
    if o.ppem.is_some_and(|p| p == 0 || p > 4096)
        || o.grid_units == Some(0)
        || o.grid_units.is_some() && o.ppem.is_none()
        || !o.pixel_offset.0.is_finite()
        || !o.pixel_offset.1.is_finite()
        || o.pixel_offset.0.abs() > 32768.
        || o.pixel_offset.1.abs() > 32768.
    {
        return Err(invalid("invalid ppem/grid units/pixel offset"));
    }
    Ok(())
}
enum Outlines<'a> {
    TrueType(glyf::GlyfTable),
    Cff(cff::Cff<'a>),
}
impl<'a> Outlines<'a> {
    fn new(f: &Sfnt<'a>, l: &Limits) -> Result<Self> {
        for tag in [*b"fvar", *b"gvar", *b"CFF2"] {
            if f.table(tag).is_some() {
                return Err(invalid(
                    "variable/CFF2 outlines require prior external instantiation",
                ));
            }
        }
        if f.table(*b"glyf").is_some() {
            Ok(Self::TrueType(glyf::GlyfTable::from_sfnt(f, &gl(l))?))
        } else if f.table(*b"CFF ").is_some() {
            Ok(Self::Cff(cff::Cff::from_sfnt(f, &cl(l))?))
        } else {
            Err(invalid(
                "no supported outlines or matching embedded bitmaps",
            ))
        }
    }
    fn walk(
        &self,
        count: u16,
        l: &Limits,
        mut visit: impl FnMut(u16, Vec<Edge>) -> Result<()>,
    ) -> Result<()> {
        let (mut work, mut commands, mut edges) = (0usize, 0usize, 0usize);
        for gid in 0..count {
            let path = match self {
                Self::TrueType(table) => {
                    let cs = table.contours_cumulative(gid, &gl(l), &mut work)?;
                    let mut out = Vec::new();
                    for c in cs {
                        for s in outline::segments(&c)? {
                            out.push(match s {
                                outline::Segment::Line(a, b) => Edge::Line(a, b),
                                outline::Segment::Quad(a, c, b) => Edge::Quad(a, c, b),
                            });
                        }
                    }
                    out
                }
                Self::Cff(table) => {
                    let glyph = table.decode(gid, &cl(l), &mut work, &mut commands)?;
                    let (mut start, mut cur) = (None, None);
                    let mut out = Vec::new();
                    for c in glyph.commands {
                        match c {
                            cff::Command::MoveTo(p) => {
                                if let (Some(a), Some(b)) = (cur, start) {
                                    if a != b {
                                        out.push(Edge::Line(a, b));
                                    }
                                }
                                start = Some(p);
                                cur = Some(p);
                            }
                            cff::Command::LineTo(p) => {
                                out.push(Edge::Line(
                                    cur.ok_or_else(|| invalid("CFF line before move"))?,
                                    p,
                                ));
                                cur = Some(p);
                            }
                            cff::Command::CurveTo(a, b, p) => {
                                out.push(Edge::Cubic(
                                    cur.ok_or_else(|| invalid("CFF curve before move"))?,
                                    a,
                                    b,
                                    p,
                                ));
                                cur = Some(p);
                            }
                            cff::Command::Close => {
                                if let (Some(a), Some(b)) = (cur, start) {
                                    if a != b {
                                        out.push(Edge::Line(a, b));
                                    }
                                }
                                cur = None;
                                start = None;
                            }
                        }
                    }
                    if let (Some(a), Some(b)) = (cur, start) {
                        if a != b {
                            out.push(Edge::Line(a, b));
                        }
                    }
                    out
                }
            };
            charge(
                &mut edges,
                path.len(),
                l.max_total_edges,
                "raster total edges",
            )?;
            bound(path.len(), l.geometry.max_edges, "raster glyph edges")?;
            visit(gid, path)?;
        }
        Ok(())
    }
}
fn gl(l: &Limits) -> glyf::Limits {
    glyf::Limits {
        sfnt: l.sfnt.clone(),
        geometry: l.geometry.clone(),
        max_total_visits: l.max_total_work,
        ..Default::default()
    }
}
fn cl(l: &Limits) -> cff::Limits {
    cff::Limits {
        sfnt: l.sfnt.clone(),
        geometry: l.geometry.clone(),
        max_total_program_bytes: l.max_total_work,
        max_total_commands: l.max_total_edges,
        ..Default::default()
    }
}
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn grid(paths: &Outlines<'_>, count: u16, upem: u16, l: &Limits) -> Result<Option<u16>> {
    let (mut divisor, mut integral) = (0u64, true);
    paths.walk(count, l, |_, edges| {
        for e in edges {
            for (x, y) in e.points() {
                for v in [x, y] {
                    if !v.is_finite() || v.fract() != 0. || v.abs() > 1e9 {
                        integral = false;
                    } else {
                        divisor = gcd(divisor, v.abs() as u64);
                    }
                }
            }
        }
        Ok(())
    })?;
    if !integral || divisor <= 1 || u64::from(upem) % divisor != 0 {
        return Ok(None);
    }
    let ppem = u64::from(upem) / divisor;
    Ok((4..=64).contains(&ppem).then_some(ppem as u16))
}
/// Exhaustively prove a standalone sfnt's native integral grid (no sampling).
/// Unsupported outlines/malformed inputs return errors; a valid nongrid returns None.
pub fn detect_native_ppem(data: &[u8], l: &Limits) -> Result<Option<u16>> {
    let f = Sfnt::parse(data, &l.sfnt)?;
    let n = sfnt::u16_at(required(&f, *b"maxp")?, 4)?;
    l.sfnt.glyphs(n as usize)?;
    let upem = sfnt::u16_at(required(&f, *b"head")?, 18)?;
    if !(16..=16384).contains(&upem) {
        return Err(invalid("invalid unitsPerEm"));
    }
    grid(&Outlines::new(&f, l)?, n, upem, l)
}
fn raster(edges: &[Edge], scale: f64, offset: Point, l: &Limits) -> Result<(Bitmap, Bounds, f64)> {
    let (mut left, mut bottom, mut right, mut top) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for e in edges {
        for p in e.points() {
            left = left.min(p.0);
            bottom = bottom.min(p.1);
            right = right.max(p.0);
            top = top.max(p.1);
        }
    }
    if edges.is_empty() {
        return Ok((Bitmap::default(), Bounds::default(), 0.));
    }
    let design_top = top;
    let x = (left / scale + offset.0).floor();
    let y = (bottom / scale + offset.1).floor();
    let w = (right / scale + offset.0).ceil() - x;
    let h = (top / scale + offset.1).ceil() - y;
    if [x, y, w, h]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > i32::MAX as f64)
    {
        return Err(invalid("raster bounds outside integer range"));
    }
    let mut geometry = l.geometry.clone();
    geometry.max_pixels = geometry
        .max_pixels
        .min(l.sfnt.resources.max_pixels_per_glyph);
    let b = outline::path::rasterize(
        edges,
        scale,
        (x - offset.0, y - offset.1),
        w as u32,
        h as u32,
        &geometry,
    )?;
    let mut bb = b.bbox();
    let cropped = b.crop_bounds(bb)?;
    if bb.width > 0 && bb.height > 0 {
        bb.x += x as i64;
        bb.y += y as i64;
    }
    Ok((cropped, bb, design_top))
}
fn scale_metric(v: f64, scale: f64, rounded: &mut bool) -> i64 {
    let p = v / scale;
    *rounded |= (p - p.round_ties_even()).abs() > 1e-10;
    p.round_ties_even() as i64
}
fn model_charge(f: &Font, total: &mut usize, l: &Limits) -> Result<()> {
    charge(
        total,
        2048,
        l.sfnt.resources.max_total_bytes,
        "raster model bytes",
    )?;
    for (_, g) in f.glyphs() {
        charge(
            total,
            1024 + g.bitmap.as_bytes().len()
                + g.codepoints.len() * 32
                + g.name.as_ref().map_or(0, String::len),
            l.sfnt.resources.max_total_bytes,
            "raster model bytes",
        )?;
    }
    Ok(())
}
/// Decode any enabled sfnt/webfont/collection container into ordered native fonts.
pub fn read(data: &[u8], o: &Options) -> Result<(FontSet, Report)> {
    validate(o)?;
    read_inner(data, o, 0)
}
fn read_inner(data: &[u8], o: &Options, depth: u8) -> Result<(FontSet, Report)> {
    if depth > 2 {
        return Err(invalid("nested font wrappers"));
    }
    bound(
        data.len(),
        o.limits.sfnt.resources.max_input_bytes,
        "raster input bytes",
    )?;
    match data.get(..4) {
        Some(b"ttcf") => {
            #[cfg(feature = "ttc")]
            {
                let l = sfnt::collection::Limits {
                    sfnt: o.limits.sfnt.clone(),
                    max_faces: o.limits.max_fonts,
                    ..Default::default()
                };
                let c = sfnt::collection::Collection::parse(data, &l)?;
                let indices = if let Some(i) = o.face_index {
                    if i >= c.faces.len() {
                        return Err(invalid("face index out of range"));
                    }
                    vec![i]
                } else {
                    (0..c.faces.len()).collect()
                };
                let mut out = FontSet::default();
                let mut report = Report::default();
                let (mut bytes, mut glyphs) = (0usize, 0usize);
                for i in indices {
                    let (f, mut r) = c.extract(i, &l)?;
                    let mut inner = o.clone();
                    inner.face_index = None;
                    let (mut set, rr) = read_inner(&f, &inner, depth + 1)?;
                    r.extend(rr);
                    for f in &mut set.fonts {
                        f.properties
                            .push(Property::new("sfnt.face_index", i as i64));
                        model_charge(f, &mut bytes, &o.limits)?;
                        charge(
                            &mut glyphs,
                            f.len(),
                            o.limits.sfnt.resources.max_glyphs,
                            "raster collection glyphs",
                        )?;
                    }
                    bound(
                        out.fonts.len() + set.fonts.len(),
                        o.limits.max_fonts,
                        "raster fonts",
                    )?;
                    out.fonts.extend(set.fonts);
                    report.extend(r);
                }
                out.source_format = Some("ttc".into());
                return Ok((out, report));
            }
            #[cfg(not(feature = "ttc"))]
            return Err(invalid("TTC feature is disabled"));
        }
        Some(b"wOFF") | Some(b"wOF2") => {
            #[cfg(any(feature = "woff", feature = "woff2"))]
            {
                let l = crate::woff::Limits {
                    sfnt: o.limits.sfnt.clone(),
                    ..Default::default()
                };
                let decoded = if data.starts_with(b"wOFF") {
                    #[cfg(feature = "woff")]
                    {
                        crate::woff::woff1::decode(data, &l)?
                    }
                    #[cfg(not(feature = "woff"))]
                    return Err(invalid("WOFF feature is disabled"));
                } else {
                    #[cfg(feature = "woff2")]
                    {
                        crate::woff::woff2::decode(data, &l)?
                    }
                    #[cfg(not(feature = "woff2"))]
                    return Err(invalid("WOFF2 feature is disabled"));
                };
                let (mut out, mut report) = read_inner(&decoded.sfnt, o, depth + 1)?;
                report.extend(decoded.report);
                if decoded.auxiliary.metadata.is_some() || decoded.auxiliary.private_data.is_some()
                {
                    note(
                        &mut report,
                        "web_auxiliary",
                        "Webfont metadata/private blocks are not bitmap model fields",
                    );
                }
                out.source_format = Some(
                    if data.starts_with(b"wOFF") {
                        "woff"
                    } else {
                        "woff2"
                    }
                    .into(),
                );
                return Ok((out, report));
            }
            #[cfg(not(any(feature = "woff", feature = "woff2")))]
            return Err(invalid("webfont features are disabled"));
        }
        _ => {}
    }
    if o.face_index.is_some_and(|i| i != 0) {
        return Err(invalid("standalone font has only face zero"));
    }
    read_sfnt(data, o)
}
fn read_sfnt(data: &[u8], o: &Options) -> Result<(FontSet, Report)> {
    let l = &o.limits;
    let f = Sfnt::parse(data, &l.sfnt)?;
    let count = sfnt::u16_at(required(&f, *b"maxp")?, 4)?;
    l.sfnt.glyphs(count as usize)?;
    let has_outline =
        f.table(*b"glyf").is_some() || f.table(*b"CFF ").is_some() || f.table(*b"CFF2").is_some();
    let mut embedded_set = FontSet::default();
    let mut report = Report::default();
    if o.prefer_embedded {
        // Stable priority: OpenType mono/gray, Apple mono/gray, then color.
        for kind in [
            sfnt::bitmap::Kind::OpenType,
            sfnt::bitmap::Kind::Apple,
            sfnt::bitmap::Kind::Color,
        ] {
            let (dt, lt) = kind.tags();
            if f.table(dt).is_some()
                || f.table(lt).is_some()
                || kind == sfnt::bitmap::Kind::Color && f.table(*b"sbix").is_some()
            {
                let eo = EmbeddedReadOptions {
                    limits: sfnt::bitmap::Limits {
                        sfnt: l.sfnt.clone(),
                        ..Default::default()
                    },
                    gray_threshold: o.gray_threshold,
                    color_darkness: o.color_darkness,
                };
                let (mut set, r) = embedded::read(data, kind, &eo)?;
                if let Some(p) = o.ppem {
                    set.fonts.retain(|s| {
                        s.metrics.ppem_x == Some(p as u32) && s.metrics.ppem_y == Some(p as u32)
                    });
                }
                for strike in set.fonts {
                    if !embedded_set.fonts.iter().any(|s| {
                        s.metrics.ppem_x == strike.metrics.ppem_x
                            && s.metrics.ppem_y == strike.metrics.ppem_y
                    }) {
                        embedded_set.fonts.push(strike);
                    }
                }
                report.extend(r);
            }
        }
    }
    bound(embedded_set.fonts.len(), l.max_fonts, "raster fonts")?;
    if !embedded_set.fonts.is_empty()
        && (!has_outline || embedded_set.fonts.iter().all(|s| s.len() == count as usize))
    {
        report
            .entries
            .retain(|e| !e.code.ends_with("missing_image"));
        return Ok((embedded_set, report));
    }
    if !has_outline {
        if !embedded_set.fonts.is_empty() {
            return Ok((embedded_set, report));
        }
        return Err(invalid("no matching physical strike and no outlines"));
    }
    let upem = sfnt::u16_at(required(&f, *b"head")?, 18)?;
    if !(16..=16384).contains(&upem) {
        return Err(invalid("invalid unitsPerEm"));
    }
    let paths = Outlines::new(&f, l)?;
    let sizes: Vec<u16> = if embedded_set.fonts.is_empty() {
        vec![match o.ppem {
            Some(p) => p,
            None => grid(&paths, count, upem, l)?.ok_or_else(|| {
                invalid("no integral native pixel grid in 4..=64; specify ppem explicitly")
            })?,
        }]
    } else {
        embedded_set
            .fonts
            .iter()
            .map(|s| s.metrics.ppem_y.unwrap_or(0) as u16)
            .collect()
    };
    let mapping = cmap::decode(required(&f, *b"cmap")?, count, &l.sfnt)?;
    let mut reverse = BTreeMap::<u16, Vec<u32>>::new();
    for (cp, gid) in mapping {
        reverse.entry(gid).or_default().push(cp);
    }
    let names = embedded::names(f.table(*b"name"), &l.sfnt)?;
    let post = f
        .table(*b"post")
        .map(|t| post::PostTable::decode(t, count, &l.sfnt))
        .transpose()?;
    let (hhea, hmtx) = metrics::decode(
        metrics::Axis::Horizontal,
        required(&f, *b"hhea")?,
        required(&f, *b"hmtx")?,
        count,
        &l.sfnt,
    )?;
    let vertical = match (f.table(*b"vhea"), f.table(*b"vmtx")) {
        (Some(h), Some(m)) => Some(metrics::decode(
            metrics::Axis::Vertical,
            h,
            m,
            count,
            &l.sfnt,
        )?),
        (None, None) => None,
        _ => return Err(invalid("unpaired vertical metrics")),
    };
    let mut out = FontSet::default();
    let (mut native_bytes, mut total_glyphs) = (0usize, 0usize);
    let mut rounded = false;
    for ppem in sizes {
        let scale = o
            .grid_units
            .map(f64::from)
            .unwrap_or(f64::from(upem) / f64::from(ppem));
        let mut font = Font::default();
        font.names = names.clone();
        font.source_format = Some("vector-raster".into());
        let embedded = embedded_set
            .fonts
            .iter()
            .find(|s| s.metrics.ppem_y == Some(ppem as u32));
        let by_gid: BTreeMap<_, _> = embedded
            .into_iter()
            .flat_map(|s| s.glyphs())
            .filter_map(|(_, g)| match g.properties.get("sfnt.glyph_id") {
                Some(PropertyValue::Integer(id)) => Some((*id as u16, g)),
                _ => None,
            })
            .collect();
        font.metrics.pixel_size = Some(ppem as u32);
        font.metrics.ppem_x = Some(ppem as u32);
        font.metrics.ppem_y = Some(ppem as u32);
        font.metrics.ascent = Some(scale_metric(hhea.ascender as f64, scale, &mut rounded));
        font.metrics.descent = Some(-scale_metric(hhea.descender as f64, scale, &mut rounded));
        font.metrics.line_gap = Some(scale_metric(hhea.line_gap as f64, scale, &mut rounded));
        if let Some(p) = &post {
            font.metrics.underline_position = Some(scale_metric(
                p.header.underline_position as f64,
                scale,
                &mut rounded,
            ));
            font.metrics.underline_thickness = Some(scale_metric(
                p.header.underline_thickness as f64,
                scale,
                &mut rounded,
            ));
            font.metrics.spacing = Some(if p.header.is_fixed_pitch != 0 {
                Spacing::Monospaced
            } else {
                Spacing::Proportional
            });
        }
        paths.walk(count, l, |gid, edges| {
            let g = if let Some(g) = by_gid.get(&gid) {
                (*g).clone()
            } else {
                let (b, bounds, design_top) = raster(&edges, scale, o.pixel_offset, l)?;
                let mut g = Glyph::new(b);
                g.metrics.bounds = bounds;
                g.metrics.advance = Some(Vector::new(
                    scale_metric(hmtx[gid as usize].advance as f64, scale, &mut rounded),
                    0,
                ));
                if let Some((_, v)) = &vertical {
                    g.metrics.vertical_advance = Some(Vector::new(
                        0,
                        -scale_metric(v[gid as usize].advance as f64, scale, &mut rounded),
                    ));
                    g.metrics.vertical_origin = Some(Vector::new(
                        scale_metric(hmtx[gid as usize].advance as f64 / 2., scale, &mut rounded),
                        scale_metric(
                            design_top + v[gid as usize].bearing as f64,
                            scale,
                            &mut rounded,
                        ),
                    ));
                }
                g.codepoints = reverse
                    .get(&gid)
                    .into_iter()
                    .flatten()
                    .copied()
                    .map(Codepoint::Unicode)
                    .collect();
                g.name = post
                    .as_ref()
                    .and_then(|p| p.names.as_ref())
                    .and_then(|p| p.get(gid as usize))
                    .cloned()
                    .or_else(|| match &paths {
                        Outlines::Cff(c) => c
                            .glyph_name(gid)
                            .map(|b| String::from_utf8_lossy(b).into_owned()),
                        _ => None,
                    })
                    .or_else(|| (gid == 0).then(|| ".notdef".into()));
                g.properties
                    .push(Property::new("sfnt.glyph_id", gid as i64));
                g
            };
            charge(
                &mut native_bytes,
                1024 + g.bitmap.as_bytes().len()
                    + g.codepoints.len() * 32
                    + g.name.as_ref().map_or(0, String::len),
                l.sfnt.resources.max_total_bytes,
                "raster model bytes",
            )?;
            charge(
                &mut total_glyphs,
                1,
                l.sfnt.resources.max_glyphs,
                "raster retained glyphs",
            )?;
            font.add_glyph(g)?;
            Ok(())
        })?;
        embedded::finish_font(&mut font)?;
        out.fonts.push(font);
    }
    report
        .entries
        .retain(|e| !e.code.ends_with("missing_image"));
    if rounded {
        note(
            &mut report,
            "metric_rounding",
            "Fractional design-space metrics rounded to nearest pixel, ties to even",
        );
    }
    note(
        &mut report,
        "outline_projection",
        "Outlines and hint programs projected to unhinted pixel-center coverage; original geometry is not editable in the bitmap model",
    );
    for table in f.tables() {
        if ![
            *b"head", *b"maxp", *b"cmap", *b"name", *b"post", *b"hhea", *b"hmtx", *b"vhea",
            *b"vmtx", *b"glyf", *b"loca", *b"CFF ", *b"EBDT", *b"EBLC", *b"bdat", *b"bloc",
            *b"CBDT", *b"CBLC", *b"sbix",
        ]
        .contains(&table.tag)
        {
            note(
                &mut report,
                "unprojected_table",
                format!(
                    "{} is not represented as bitmap semantics",
                    String::from_utf8_lossy(&table.tag)
                ),
            );
        }
    }
    out.source_format = Some("vector-raster".into());
    Ok((out, report))
}

/// Container/output choices for native vectorization; outline generation reuses
/// the independently verified variants writer without inventing language routes.
#[derive(Clone, Debug, Default)]
pub struct WriteOptions {
    /// Shape, units, names, bitmap attachment and resource limits.
    pub outline: crate::variants::Options,
    /// TTC version: false=v1, true=v2 with absent signature fields.
    pub collection_v2: bool,
    /// WOFF1 compression and auxiliary options.
    #[cfg(feature = "woff")]
    pub woff: crate::woff::woff1::Options,
    /// WOFF2 compression, transforms and auxiliary options.
    #[cfg(feature = "woff2")]
    pub woff2: crate::woff::woff2::Options,
}
/// Encode a native FontSet as one outline font, a webfont, or one TTC face per Font.
pub fn write(fonts: &FontSet, id: &str, o: &WriteOptions) -> Result<(Vec<u8>, Report)> {
    if fonts.fonts.is_empty() || id != "ttc" && fonts.fonts.len() != 1 {
        return Err(invalid(
            "standalone outline output requires one Font; use TTC for multiple faces",
        ));
    }
    let mut report = Report::default();
    let mut encoded = Vec::new();
    let mut total = 0usize;
    if !fonts.properties.is_empty() {
        note(
            &mut report,
            "container_properties",
            "Source container properties are not copied into regenerated outline tables",
        );
    }
    for f in &fonts.fonts {
        let mut options = o.outline.clone();
        if id == "otf" {
            options.flavor = crate::variants::OutlineFlavor::Cff;
        } else if id == "ttf" {
            options.flavor = crate::variants::OutlineFlavor::TrueType;
        }
        if options.style.is_none() {
            options.style = f.names.style.clone();
        }
        let e = crate::variants::encode_single(f, &options)?;
        report.extend(e.report);
        charge(
            &mut total,
            e.bytes.len(),
            options.limits.sfnt.resources.max_total_bytes,
            "native OpenType output bytes",
        )?;
        encoded.push(e.bytes);
        if !f.headers.is_empty()
            || !f.properties.is_empty()
            || f.charset != Default::default()
            || f.names.xlfd.is_some()
            || !f.names.localized.is_empty()
        {
            note(
                &mut report,
                "source_metadata",
                "Source properties, charset and localized/XLFD names are not copied into regenerated outline tables",
            );
        }
        for (_, g) in f.glyphs() {
            if !g.unicode_sequences.is_empty()
                || !g.properties.is_empty()
                || g.metrics.scalable_advance.is_some()
                || g.metrics.vertical_scalable_advance.is_some()
                || g.metrics.abc.is_some()
                || g.metrics.left_bearing.is_some()
                || g.metrics.right_bearing.is_some()
                || g.metrics.image_offset.is_some()
            {
                note(
                    &mut report,
                    "glyph_metadata",
                    "Source sequence/property/scalable or auxiliary spacing fields are not represented in generated outlines",
                );
                break;
            }
        }
    }
    if id == "ttc" {
        #[cfg(feature = "ttc")]
        {
            let refs = encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
            let (bytes, r) = sfnt::collection::encode(
                &refs,
                if o.collection_v2 { 0x20000 } else { 0x10000 },
                &sfnt::collection::Limits {
                    sfnt: o.outline.limits.sfnt.clone(),
                    ..Default::default()
                },
            )?;
            report.extend(r);
            return Ok((bytes, report));
        }
        #[cfg(not(feature = "ttc"))]
        return Err(invalid("TTC feature is disabled"));
    }
    let bytes = encoded.remove(0);
    #[cfg(any(feature = "woff", feature = "woff2"))]
    let l = crate::woff::Limits {
        sfnt: o.outline.limits.sfnt.clone(),
        ..Default::default()
    };
    match id {
        "ttf" | "otf" | "vector-raster" => Ok((bytes, report)),
        #[cfg(feature = "woff")]
        "woff" => {
            let e = crate::woff::woff1::encode(&bytes, &o.woff, &l)?;
            report.extend(e.report);
            Ok((e.bytes, report))
        }
        #[cfg(feature = "woff2")]
        "woff2" => {
            let e = crate::woff::woff2::encode(&bytes, &o.woff2, &l)?;
            report.extend(e.report);
            Ok((e.bytes, report))
        }
        _ => Err(invalid("unsupported output container")),
    }
}
/// Configurable registry codec. Canonical container detectors never compete with
/// the explicit, format-agnostic vector-raster alias.
#[derive(Clone, Debug)]
pub struct Codec {
    /// Canonical registered format ID.
    pub id: &'static str,
    /// Read options.
    pub read_options: Options,
    /// Write options.
    pub write_options: WriteOptions,
}
impl Codec {
    /// Default codec for one canonical ID.
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            read_options: Default::default(),
            write_options: Default::default(),
        }
    }
}
impl super::FormatDetector for Codec {
    fn detect(&self, input: &mut crate::resources::ReadContext<'_>) -> Result<super::Confidence> {
        let d = input.bytes();
        let matched = match self.id {
            "ttc" => d.starts_with(b"ttcf"),
            "woff" => d.starts_with(b"wOFF"),
            "woff2" => d.starts_with(b"wOF2"),
            "ttf" | "otf" => {
                let l = sfnt::Limits {
                    resources: input.limits().clone(),
                    ..Default::default()
                };
                match Sfnt::parse(d, &l) {
                    Ok(f) => {
                        if self.id == "ttf" {
                            f.table(*b"glyf").is_some()
                        } else {
                            f.table(*b"CFF ").is_some() || f.table(*b"CFF2").is_some()
                        }
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        };
        Ok(if matched {
            super::Confidence::Certain
        } else {
            super::Confidence::NoMatch
        })
    }
}
impl super::FormatReader<FontSet> for Codec {
    fn read(
        &self,
        input: &mut crate::resources::ReadContext<'_>,
        _: &super::ReadOptions,
    ) -> Result<(FontSet, Report)> {
        let mut o = self.read_options.clone();
        o.limits.sfnt.resources = input.limits().clone();
        read(input.bytes(), &o)
    }
}
impl super::FormatWriter<FontSet> for Codec {
    fn write(&self, fonts: &FontSet, o: &super::WriteOptions) -> Result<crate::resources::Written> {
        let (bytes, report) = write(fonts, self.id, &self.write_options)?;
        Ok(crate::resources::Written {
            output: crate::OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
