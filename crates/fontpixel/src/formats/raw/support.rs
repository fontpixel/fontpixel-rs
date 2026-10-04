//! Shared W4.1 diagnostics. Kept private to each independently compiled codec.
use crate::model::{Font, Glyph, Properties};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, FontSet, Report, Result};
use std::mem::size_of;
#[allow(clippy::too_many_arguments)]
pub(crate) fn note(
    r: &mut impl Sink,
    f: &str,
    code: &str,
    fi: Option<usize>,
    gi: Option<usize>,
    cp: Option<u32>,
    impact: Impact,
    message: impl Into<String>,
) {
    r.push_entry(ReportEntry {
        level: Level::Warning,
        impact,
        code: format!("{f}.{code}"),
        location: Location {
            font_index: fi,
            glyph_index: gi,
            codepoint: cp,
            asset: None,
        },
        message: message.into(),
    });
}
pub(crate) fn fail<T>(mut r: BoundedReport, f: &str, e: Error) -> Result<T> {
    if r.report.entries.is_empty() && !r.exhausted {
        return Err(e);
    }
    r.push_entry(ReportEntry {
        level: Level::Error,
        impact: Impact::None,
        code: format!("{f}.write.failed"),
        location: Location::default(),
        message: e.to_string(),
    });
    Err(Error::ReportedErrors(r.finish()?))
}

fn props(
    a: &Properties,
    b: &Properties,
    r: &mut impl Sink,
    f: &str,
    fi: Option<usize>,
    gi: Option<usize>,
) {
    if b.len() > a.len() {
        note(
            r,
            f,
            "write.property",
            fi,
            gi,
            None,
            Impact::Loss,
            "Target supplies additional properties",
        );
    }
    let mut other = b.iter();
    for p in a.iter() {
        if r.full() {
            return;
        }
        if other.next() != Some(p) {
            note(
                r,
                f,
                "write.property",
                fi,
                gi,
                None,
                Impact::Loss,
                format!(
                    "Property {:?} (including raw provenance) is changed or omitted",
                    p.name
                ),
            );
        }
    }
}
pub(crate) fn compare(a: &FontSet, b: &FontSet, f: &str, r: &mut impl Sink) {
    if a.source_format != b.source_format {
        note(
            r,
            f,
            "write.source_format",
            None,
            None,
            None,
            Impact::Loss,
            "Container source format changes",
        );
    }
    props(&a.properties, &b.properties, r, f, None, None);
    for (fi, font) in a.fonts.iter().enumerate() {
        if r.full() {
            return;
        }
        if let Some(out) = b.fonts.get(fi) {
            font_compare(font, out, f, fi, r)
        } else {
            font_compare(font, &Font::default(), f, fi, r);
            note(
                r,
                f,
                "write.font",
                Some(fi),
                None,
                None,
                Impact::Loss,
                "Strike omitted",
            );
        }
    }
    if a.fonts.len() != b.fonts.len() {
        note(
            r,
            f,
            "write.font_count",
            None,
            None,
            None,
            Impact::Loss,
            "Strike count changes",
        );
    }
}
fn font_compare(a: &Font, b: &Font, f: &str, fi: usize, r: &mut impl Sink) {
    if a.names != b.names {
        note(
            r,
            f,
            "write.names",
            Some(fi),
            None,
            None,
            Impact::Loss,
            "Font naming and licensing metadata changes",
        );
    }
    if a.charset != b.charset {
        note(
            r,
            f,
            "write.charset",
            Some(fi),
            None,
            None,
            Impact::Loss,
            "Charset metadata changes; no implicit code conversion is performed",
        );
    }
    if a.source_format != b.source_format {
        note(
            r,
            f,
            "write.source_format",
            Some(fi),
            None,
            None,
            Impact::Loss,
            "Strike source format changes",
        );
    }
    props(&a.headers, &b.headers, r, f, Some(fi), None);
    props(&a.properties, &b.properties, r, f, Some(fi), None);
    macro_rules! fields {($($field:ident),*)=>{$(if a.metrics.$field!=b.metrics.$field{note(r,f,"write.font_metric",Some(fi),None,None,Impact::Loss,concat!("Font metric ",stringify!($field)," changes"));})*};}
    fields!(
        pixel_size,
        ppem_x,
        ppem_y,
        point_size,
        resolution,
        ascent,
        descent,
        bounds,
        line_gap,
        line_height,
        underline_position,
        underline_thickness,
        default_glyph,
        default_character,
        space_width,
        spacing,
        metrics_set,
        glyph_defaults
    );
    let mut other = b.glyphs();
    for (id, g) in a.glyphs() {
        if r.full() {
            return;
        }
        if let Some((oid, o)) = other.next() {
            if id != oid {
                note(
                    r,
                    f,
                    "write.glyph_id",
                    Some(fi),
                    Some(id.index()),
                    None,
                    Impact::Loss,
                    "Stable glyph index changes",
                );
            }
            glyph_compare(g, o, f, fi, id.index(), r)
        } else {
            glyph_compare(g, &Glyph::default(), f, fi, id.index(), r);
            note(
                r,
                f,
                "write.glyph",
                Some(fi),
                Some(id.index()),
                None,
                Impact::Loss,
                "Glyph omitted",
            );
            for cp in &g.codepoints {
                if r.full() {
                    return;
                }
                note(
                    r,
                    f,
                    "write.mapping",
                    Some(fi),
                    Some(id.index()),
                    Some(cp.value()),
                    Impact::Loss,
                    "Mapping omitted with glyph",
                );
            }
        }
    }
    if a.slot_count() != b.slot_count() {
        note(
            r,
            f,
            "write.glyph_slots",
            Some(fi),
            None,
            None,
            Impact::Loss,
            "Stable glyph table holes or length change",
        );
    }
    if a.len() != b.len() {
        note(
            r,
            f,
            "write.glyph_count",
            Some(fi),
            None,
            None,
            Impact::Loss,
            "Glyph count changes",
        );
    }
}
fn glyph_compare(a: &Glyph, b: &Glyph, f: &str, fi: usize, gi: usize, r: &mut impl Sink) {
    for cp in &a.codepoints {
        if r.full() {
            return;
        }
        if !b.codepoints.contains(cp) {
            note(
                r,
                f,
                "write.mapping",
                Some(fi),
                Some(gi),
                Some(cp.value()),
                Impact::Loss,
                "Mapping is absent at its original glyph position",
            );
        }
    }
    if a.codepoints != b.codepoints && a.codepoints.iter().all(|cp| b.codepoints.contains(cp)) {
        note(
            r,
            f,
            "write.mapping_order",
            Some(fi),
            Some(gi),
            None,
            Impact::Loss,
            "Mapping order, multiplicity, or positional code changes",
        );
    }
    for seq in &a.unicode_sequences {
        if r.full() {
            return;
        }
        if !b.unicode_sequences.contains(seq) {
            note(
                r,
                f,
                "write.sequence",
                Some(fi),
                Some(gi),
                None,
                Impact::Loss,
                format!("Unicode sequence {seq:?} omitted"),
            );
        }
    }
    if a.name != b.name {
        note(
            r,
            f,
            "write.glyph_name",
            Some(fi),
            Some(gi),
            None,
            Impact::Loss,
            "Glyph name changes",
        );
    }
    props(&a.properties, &b.properties, r, f, Some(fi), Some(gi));
    macro_rules! fields {($($field:ident),*)=>{$(if a.metrics.$field!=b.metrics.$field{note(r,f,"write.glyph_metric",Some(fi),Some(gi),None,Impact::Loss,concat!("Glyph metric ",stringify!($field)," changes"));})*};}
    fields!(
        bounds,
        advance,
        scalable_advance,
        vertical_advance,
        vertical_scalable_advance,
        vertical_origin,
        abc,
        left_bearing,
        right_bearing,
        image_offset
    );
    if a.bitmap != b.bitmap {
        note(
            r,
            f,
            "write.bitmap",
            Some(fi),
            Some(gi),
            None,
            Impact::Loss,
            "Bitmap geometry, pixels, or depth changes",
        );
    }
}

/// Conservative retained model cost, including structured properties and indexes.
pub(crate) fn model_cost(s: &FontSet, l: &crate::ResourceLimits) -> Result<usize> {
    let mut n = 2048usize;
    fn add(n: &mut usize, value: usize, l: &crate::ResourceLimits) -> Result<()> {
        *n = n.checked_add(value).ok_or(Error::ResourceLimit {
            resource: "W4.1 model bytes",
            limit: l.max_total_bytes,
        })?;
        super::super::console::check(*n, l.max_total_bytes, "W4.1 model bytes")
    }
    fn properties(n: &mut usize, p: &Properties, l: &crate::ResourceLimits) -> Result<()> {
        for p in p.iter() {
            add(
                n,
                128usize
                    .saturating_add(p.name.len().saturating_mul(16))
                    .saturating_add(p.raw.as_ref().map_or(0, Vec::len)),
                l,
            )?;
            let mut values = vec![&p.value];
            while let Some(value) = values.pop() {
                use crate::model::PropertyValue as V;
                add(n, 64, l)?;
                match value {
                    V::Bytes(b) => add(n, b.len(), l)?,
                    V::Text(s) | V::Decimal(s) => add(n, s.len(), l)?,
                    V::List(items) => {
                        add(n, items.len().saturating_mul(16), l)?;
                        values.extend(items)
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    properties(&mut n, &s.properties, l)?;
    add(
        &mut n,
        256 + s.source_format.as_ref().map_or(0, String::len),
        l,
    )?;
    for f in &s.fonts {
        add(
            &mut n,
            size_of::<Font>()
                .saturating_mul(2)
                .saturating_add(512)
                .saturating_add(f.slot_count().saturating_mul(16)),
            l,
        )?;
        for text in [
            &f.source_format,
            &f.charset.name,
            &f.charset.registry,
            &f.charset.encoding,
            &f.names.family,
            &f.names.style,
            &f.names.full,
            &f.names.postscript,
            &f.names.version,
            &f.names.copyright,
            &f.names.license,
            &f.names.license_url,
            &f.names.foundry,
            &f.names.xlfd,
        ]
        .into_iter()
        .flatten()
        {
            add(&mut n, text.len(), l)?;
        }
        for text in &f.names.xlfd_fields {
            add(&mut n, text.len().saturating_add(32), l)?;
        }
        add(&mut n, f.charset.code_ranges.len().saturating_mul(8), l)?;
        properties(&mut n, &f.names.localized, l)?;
        properties(&mut n, &f.headers, l)?;
        properties(&mut n, &f.properties, l)?;
        for (_, g) in f.glyphs() {
            add(
                &mut n,
                1024usize
                    .saturating_add(g.bitmap.as_bytes().len())
                    .saturating_add(g.codepoints.len().saturating_mul(128))
                    .saturating_add(g.name.as_ref().map_or(0, String::len)),
                l,
            )?;
            for seq in &g.unicode_sequences {
                add(
                    &mut n,
                    128usize.saturating_add(seq.len().saturating_mul(48)),
                    l,
                )?;
            }
            properties(&mut n, &g.properties, l)?;
        }
    }
    Ok(n)
}
/// A failed conversion emits no assets; keep every source mapping/property in its report.
pub(crate) fn failed_projection(s: &FontSet, f: &str, r: &mut impl Sink) {
    props(&s.properties, &Properties::default(), r, f, None, None);
    if s.source_format.is_some() {
        note(
            r,
            f,
            "write.source_format",
            None,
            None,
            None,
            Impact::Loss,
            "No container provenance was emitted because conversion failed",
        );
    }
    for (fi, font) in s.fonts.iter().enumerate() {
        if r.full() {
            return;
        }
        props(&font.headers, &Properties::default(), r, f, Some(fi), None);
        props(
            &font.properties,
            &Properties::default(),
            r,
            f,
            Some(fi),
            None,
        );
        if font.source_format.is_some() {
            note(
                r,
                f,
                "write.source_format",
                Some(fi),
                None,
                None,
                Impact::Loss,
                "No source-format provenance was emitted",
            );
        }
        if font.names != Default::default() {
            note(
                r,
                f,
                "write.names",
                Some(fi),
                None,
                None,
                Impact::Loss,
                "No naming metadata was emitted",
            );
        }
        if font.metrics != Default::default() {
            note(
                r,
                f,
                "write.font_metric",
                Some(fi),
                None,
                None,
                Impact::Loss,
                "No strike metrics were emitted",
            );
        }
        for (id, g) in font.glyphs() {
            if r.full() {
                return;
            }
            for cp in &g.codepoints {
                if r.full() {
                    return;
                }
                note(
                    r,
                    f,
                    "write.mapping",
                    Some(fi),
                    Some(id.index()),
                    Some(cp.value()),
                    Impact::Loss,
                    "No mapping was emitted because conversion failed",
                );
            }
            for seq in &g.unicode_sequences {
                if r.full() {
                    return;
                }
                note(
                    r,
                    f,
                    "write.sequence",
                    Some(fi),
                    Some(id.index()),
                    None,
                    Impact::Loss,
                    format!("No Unicode sequence {seq:?} was emitted"),
                );
            }
            props(
                &g.properties,
                &Properties::default(),
                r,
                f,
                Some(fi),
                Some(id.index()),
            );
            if g.metrics != Default::default() {
                note(
                    r,
                    f,
                    "write.glyph_metric",
                    Some(fi),
                    Some(id.index()),
                    None,
                    Impact::Loss,
                    "No glyph metrics were emitted",
                );
            }
        }
    }
}

/// Internal sink allowing readers' pre-budgeted notes and dynamically bounded writers.
pub(crate) trait Sink {
    fn push_entry(&mut self, entry: ReportEntry);
    fn full(&self) -> bool {
        false
    }
}
impl Sink for Report {
    fn push_entry(&mut self, entry: ReportEntry) {
        self.push(entry);
    }
}
/// Owns a report and accounts retained objects before allocating report entries.
pub(crate) struct BoundedReport {
    report: Report,
    used: usize,
    max: usize,
    exhausted: bool,
}
impl BoundedReport {
    pub(crate) fn new(retained: usize, l: &crate::ResourceLimits) -> Result<Self> {
        super::super::console::check(retained, l.max_total_bytes, "W4.1 write peak bytes")?;
        Ok(Self {
            report: Report::default(),
            used: retained,
            max: l.max_total_bytes,
            exhausted: false,
        })
    }
    pub(crate) fn reserve(&mut self, bytes: usize) -> Result<()> {
        if self.exhausted {
            return Err(self.error());
        }
        let Some(next) = self.used.checked_add(bytes) else {
            self.exhausted = true;
            return Err(self.error());
        };
        if next > self.max {
            self.exhausted = true;
            return Err(self.error());
        }
        self.used = next;
        Ok(())
    }
    pub(crate) fn used(&self) -> usize {
        self.used
    }
    fn error(&self) -> Error {
        Error::ResourceLimit {
            resource: "W4.1 write peak bytes",
            limit: self.max,
        }
    }
    pub(crate) fn finish(self) -> Result<Report> {
        if self.exhausted {
            Err(self.error())
        } else {
            Ok(self.report)
        }
    }
}
impl Sink for BoundedReport {
    fn full(&self) -> bool {
        self.exhausted
    }
    fn push_entry(&mut self, entry: ReportEntry) {
        if self.exhausted {
            return;
        }
        let strings = entry
            .code
            .capacity()
            .saturating_add(entry.message.capacity())
            .saturating_add(entry.location.asset.as_ref().map_or(0, String::capacity));
        let grow = if self.report.entries.len() == self.report.entries.capacity() {
            64
        } else {
            0
        };
        if self
            .reserve(strings.saturating_add(grow * size_of::<ReportEntry>()))
            .is_err()
        {
            return;
        }
        if grow > 0 && self.report.entries.try_reserve_exact(grow).is_err() {
            self.exhausted = true;
            return;
        }
        self.report.entries.push(entry);
    }
}
