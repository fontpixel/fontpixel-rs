use crate::model::Font;
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};
pub(crate) fn loss(
    r: &mut Report,
    format: &str,
    code: &str,
    fi: usize,
    gi: Option<usize>,
    text: impl Into<String>,
) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("{format}.{code}"),
        location: Location {
            font_index: Some(fi),
            glyph_index: gi,
            ..Default::default()
        },
        message: text.into(),
    });
}
pub(crate) fn audit(
    a: &Font,
    b: &Font,
    format: &str,
    fi: usize,
    r: &mut Report,
    max_bytes: usize,
) -> Result<()> {
    let mut emit = |code: &str, gi: Option<usize>, message: String| -> Result<()> {
        bound(
            r.entries.len().saturating_add(1).saturating_mul(512),
            max_bytes,
            "editor codec report",
        )?;
        loss(r, format, code, fi, gi, message);
        Ok(())
    };
    for (changed, field) in [
        (a.metrics != b.metrics, "font_metrics"),
        (a.names != b.names, "font_names"),
        (a.charset != b.charset, "charset"),
        (a.headers != b.headers, "headers"),
        (a.properties != b.properties, "properties"),
        (a.source_format != b.source_format, "source_format"),
    ] {
        if changed {
            emit(
                field,
                None,
                format!("{field} changed to the native representation"),
            )?;
        }
    }
    if a.slot_count() != b.slot_count() {
        emit(
            "glyph_slots",
            None,
            "Stable glyph-table slots changed".into(),
        )?;
    }
    if a.len() != b.len() {
        emit("glyph_count", None, "Glyph count changed".into())?;
    }
    for ((id, x), (out_id, y)) in a.glyphs().zip(b.glyphs()) {
        for (changed, field) in [
            (id != out_id, "glyph_slot"),
            (x.codepoints != y.codepoints, "codepoints"),
            (
                x.unicode_sequences != y.unicode_sequences,
                "unicode_sequences",
            ),
            (x.name != y.name, "glyph_name"),
            (x.metrics != y.metrics, "glyph_metrics"),
            (x.bitmap != y.bitmap, "bitmap"),
            (x.properties != y.properties, "glyph_properties"),
        ] {
            if changed {
                emit(
                    field,
                    Some(id.index()),
                    format!("{field} changed to the native representation"),
                )?;
            }
        }
    }
    Ok(())
}
pub(crate) fn bound(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
pub(crate) struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    pub(crate) fn new(used: usize, l: &ResourceLimits) -> Result<Self> {
        bound(used, l.max_total_bytes, "editor codec bytes")?;
        Ok(Self {
            used,
            max: l.max_total_bytes,
        })
    }
    pub(crate) fn add(&mut self, n: usize) -> Result<()> {
        self.used = self.used.checked_add(n).ok_or(Error::ResourceLimit {
            resource: "editor codec bytes",
            limit: self.max,
        })?;
        bound(self.used, self.max, "editor codec bytes")
    }
    pub(crate) fn remaining(&self) -> usize {
        self.max - self.used
    }
    pub(crate) fn temporary(&self, n: usize) -> Result<()> {
        bound(n, self.remaining(), "editor codec scratch")
    }
}

pub(crate) fn properties_bytes(p: &crate::model::Properties) -> Result<usize> {
    fn value(v: &crate::model::PropertyValue, depth: usize) -> Result<usize> {
        bound(depth, 64, "editor property nesting")?;
        use crate::model::PropertyValue::*;
        Ok(match v {
            Text(s) | Decimal(s) => s.capacity(),
            Bytes(b) => b.capacity(),
            List(v) => v.iter().try_fold(
                v.capacity()
                    .saturating_mul(std::mem::size_of::<crate::model::PropertyValue>()),
                |n, v| Ok::<_, Error>(n.saturating_add(value(v, depth + 1)?)),
            )?,
            _ => 0,
        })
    }
    p.iter().try_fold(0usize, |n, p| {
        Ok(n.saturating_add(192)
            .saturating_add(p.name.capacity())
            .saturating_add(p.raw.as_ref().map_or(0, Vec::capacity))
            .saturating_add(value(&p.value, 0)?))
    })
}
/// Container-level part of a source model estimate: its properties,
/// `source_format` and font array.
pub(crate) fn container_bytes(s: &crate::model::FontSet) -> Result<usize> {
    Ok(properties_bytes(&s.properties)?
        .saturating_add(s.source_format.as_ref().map_or(0, String::capacity))
        .saturating_add(
            s.fonts
                .capacity()
                .saturating_mul(std::mem::size_of::<Font>()),
        ))
}
pub(crate) fn model_bytes(f: &Font) -> Result<usize> {
    let mut n = 4096usize
        .saturating_add(f.slot_count().saturating_mul(1024))
        .saturating_add(properties_bytes(&f.headers)?)
        .saturating_add(properties_bytes(&f.properties)?)
        .saturating_add(properties_bytes(&f.names.localized)?);
    for s in [
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
        &f.charset.registry,
        &f.charset.encoding,
        &f.charset.name,
        &f.source_format,
    ] {
        n = n.saturating_add(s.as_ref().map_or(0, String::capacity));
    }
    n = n
        .saturating_add(f.names.xlfd_fields.capacity().saturating_mul(24))
        .saturating_add(f.charset.code_ranges.capacity().saturating_mul(8));
    for field in &f.names.xlfd_fields {
        n = n.saturating_add(field.capacity());
    }
    for (_, g) in f.glyphs() {
        n = n
            .saturating_add(g.bitmap.as_bytes().len())
            .saturating_add(properties_bytes(&g.properties)?)
            .saturating_add(g.name.as_ref().map_or(0, String::capacity))
            .saturating_add(g.codepoints.capacity().saturating_mul(128))
            .saturating_add(g.unicode_sequences.capacity().saturating_mul(24));
        for sequence in &g.unicode_sequences {
            n = n
                .saturating_add(64)
                .saturating_add(sequence.capacity().saturating_mul(8));
        }
    }
    Ok(n)
}

pub(crate) fn attach_asset(
    r: &mut Report,
    name: Option<&str>,
    retained: usize,
    model: usize,
    l: &ResourceLimits,
) -> Result<()> {
    let count = r.entries.len();
    let names = count.saturating_mul(name.map_or(0, str::len));
    bound(
        retained
            .saturating_add(model)
            .saturating_add(count.saturating_mul(512))
            .saturating_add(names),
        l.max_total_bytes,
        "editor diagnostics and asset names",
    )?;
    for e in &mut r.entries {
        e.location.asset = name.map(str::to_owned);
    }
    Ok(())
}

/// Preserve every affected source slot/code when a representability failure
/// prevents any output. Resource failures retain their original typed category.
/// `extra_bytes` is charged as given; the fonts' model estimate only when the
/// writer charges its source (`charge`).
pub(crate) fn failed_write<'a>(
    error: Error,
    format: &str,
    fonts: impl Iterator<Item = (usize, &'a Font)> + Clone,
    extra_bytes: usize,
    limits: &ResourceLimits,
    charge: super::ModelCharge,
) -> Error {
    let Error::InvalidInput(message) = error else {
        return error;
    };
    let build = || -> Result<Report> {
        let mut source = 0usize;
        let mut count = 1usize;
        for (_, font) in fonts.clone() {
            source = source.saturating_add(model_bytes(font)?);
            for (_, glyph) in font.glyphs() {
                count = count.saturating_add(glyph.codepoints.len().max(1));
            }
        }
        let source = extra_bytes.saturating_add(charge.of(source));
        let mut budget = Budget::new(source, limits)?;
        budget.add(count.saturating_mul(512).saturating_add(message.capacity()))?;
        let mut report = Report::default();
        report.push(ReportEntry {
            level: Level::Error,
            impact: Impact::None,
            code: format!("{format}.write_failed"),
            location: Location::default(),
            message,
        });
        for (fi, font) in fonts {
            for (id, glyph) in font.glyphs() {
                let mut emit = |codepoint| {
                    report.push(ReportEntry {
                        level: Level::Warning,
                        impact: Impact::Loss,
                        code: format!("{format}.glyph_omitted"),
                        location: Location {
                            font_index: Some(fi),
                            glyph_index: Some(id.index()),
                            codepoint,
                            asset: None,
                        },
                        message:
                            "Glyph mapping omitted because the write failed; no output returned"
                                .into(),
                    });
                };
                if glyph.codepoints.is_empty() {
                    emit(None);
                } else {
                    for code in &glyph.codepoints {
                        emit(Some(code.value()));
                    }
                }
            }
        }
        Ok(report)
    };
    match build() {
        Ok(report) => Error::ReportedErrors(report),
        Err(error) => error,
    }
}
/// Allow a bounded sink to refuse compression output before Vec growth.
#[cfg(feature = "bmfont")]
pub(crate) struct Sink {
    pub(crate) bytes: Vec<u8>,
    pub(crate) max: usize,
}
#[cfg(feature = "bmfont")]
impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.max)
            .ok_or_else(|| std::io::Error::other("PNG output budget exhausted"))?;
        if end > self.bytes.capacity() {
            self.bytes
                .try_reserve_exact(end - self.bytes.len())
                .map_err(std::io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
