//! Native Bits'N'Picas 1.x binary fonts, including modified UTF-8 names.
use super::editor_fidelity as fidelity;
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ModelCharge, ReadOptions, WriteOptions,
};
use crate::bitmap::{Bitmap, BitmapLimits, Gray8};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
use fidelity::{Budget, audit, bound, loss};
mod wire;
/// Bounded native output options.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct KbitsWriteOptions {
    /// Input/model and output allocation limits.
    pub limits: ResourceLimits,
}
/// Native Kbits registry adapter.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct KbitsCodec {
    /// Native output controls.
    pub write_options: KbitsWriteOptions,
}
impl FormatDetector for KbitsCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for KbitsCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (f, mut r) = parse(c.bytes(), c.limits(), c.retained_input_bytes())?;
        fidelity::attach_asset(
            &mut r,
            c.name(),
            c.retained_input_bytes(),
            fidelity::model_bytes(&f)?,
            c.limits(),
        )?;
        let mut set = FontSet::from(f);
        set.source_format = Some("kbits".into());
        Ok((set, r))
    }
}
impl FormatWriter<FontSet> for KbitsCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(s, o, ModelCharge::Estimate)
    }
}
impl KbitsCodec {
    /// [`write`](FormatWriter::write) with the model reserved by the caller.
    pub(crate) fn write_reserved(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(s, o, ModelCharge::Reserved)
    }
    fn write_charged(&self, s: &FontSet, o: &WriteOptions, charge: ModelCharge) -> Result<Written> {
        let changed = !s.properties.is_empty() || s.source_format.as_deref() != Some("kbits");
        let reserve = charge
            .of(fidelity::container_bytes(s)?)
            .saturating_add(if changed { 512 } else { 0 });
        let mut options = self.write_options.clone();
        options.limits.max_total_bytes = Budget::new(reserve, &options.limits)?.remaining();
        let font = s.single().map_err(|e| {
            fidelity::failed_write(
                e,
                "kbits",
                s.fonts.iter().enumerate(),
                reserve,
                &self.write_options.limits,
                charge,
            )
        })?;
        let (b, mut r) = write_charged(font, &options, charge)?;
        if changed {
            loss(
                &mut r,
                "kbits",
                "container_metadata",
                0,
                None,
                "Container metadata or source format changed",
            );
        }
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), b),
            report: r,
        })
    }
}
/// Detect the eight-byte Kbits magic.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"KBnPbits") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn invalid(m: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("KBITS: {m}"))
}
fn bl(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: i32::MAX as u32,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}
/// Read one font with exact grayscale samples and ordered native metadata.
pub fn read(b: &[u8], l: &ResourceLimits) -> Result<(Font, Report)> {
    parse(b, l, b.len())
}
fn parse(b: &[u8], l: &ResourceLimits, retained: usize) -> Result<(Font, Report)> {
    bound(b.len(), l.max_input_bytes, "KBITS input")?;
    let mut budget = Budget::new(retained, l)?;
    budget.add(4096)?;
    let mut c = wire::Cursor::new(b);
    if c.take(8)? != b"KBnPbits" || c.int()? != 1 {
        return Err(invalid("invalid magic or version"));
    }
    let mut f = Font::default();
    f.source_format = Some("kbits".into());
    let mut r = Report::default();
    let keys = [
        "em_ascent",
        "em_descent",
        "line_ascent",
        "line_descent",
        "line_gap",
        "x_height",
    ];
    let mut metrics = [0i64; 6];
    for (i, k) in keys.iter().enumerate() {
        metrics[i] = i64::from(c.int()?);
        budget.add(256)?;
        f.properties
            .push(Property::new(format!("kbits.{k}"), metrics[i]));
    }
    f.metrics.ascent = Some(metrics[0]);
    f.metrics.descent = Some(metrics[1]);
    f.metrics.pixel_size = u32::try_from(metrics[0] + metrics[1]).ok();
    f.metrics.line_height = Some(metrics[2] + metrics[3] + metrics[4]);
    f.metrics.line_gap = Some(metrics[4]);
    f.charset.name = Some("unicode".into());
    loop {
        match c.take(4)? {
            b"fin." => {
                if !c.rest().is_empty() {
                    return Err(invalid("trailing bytes after fin."));
                }
                break;
            }
            b"name" => {
                if c.int()? != 1 {
                    return Err(invalid("name version"));
                }
                let id = c.int()?;
                let n = usize::from(c.short()?);
                bound(n, l.max_table_bytes, "KBITS name bytes")?;
                budget.add(n.saturating_mul(4).saturating_add(256))?;
                let name = wire::decode(c.take(n)?)?;
                match id {
                    0 => f.names.copyright = Some(name.clone()),
                    1 => f.names.family = Some(name.clone()),
                    2 => f.names.style = Some(name.clone()),
                    4 => f.names.full = Some(name.clone()),
                    5 => f.names.version = Some(name.clone()),
                    6 => f.names.postscript = Some(name.clone()),
                    8 => f.names.foundry = Some(name.clone()),
                    13 => f.names.license = Some(name.clone()),
                    14 => f.names.license_url = Some(name.clone()),
                    _ => {}
                };
                f.properties
                    .push(Property::new(format!("kbits.name.{id}"), name));
            }
            b"char" => {
                bound(f.len().saturating_add(1), l.max_glyphs, "KBITS glyphs")?;
                budget.add(1536)?;
                if c.int()? != 1 {
                    return Err(invalid("char version"));
                }
                let cp = c.int()?;
                let advance = c.int()?;
                let x = c.int()?;
                let ascent = c.int()?;
                let rows = usize::try_from(c.int()?).map_err(invalid)?;
                bound(rows, l.max_pixels_per_glyph, "KBITS rows")?;
                let start = c.position();
                let mut width = 0usize;
                for _ in 0..rows {
                    let cols = usize::try_from(c.int()?).map_err(invalid)?;
                    bound(cols, l.max_pixels_per_glyph, "KBITS columns")?;
                    width = width.max(cols);
                    c.take(cols)?;
                }
                let pixels = width
                    .checked_mul(rows)
                    .ok_or_else(|| invalid("raster overflow"))?;
                bound(pixels, l.max_pixels_per_glyph, "KBITS pixels")?;
                budget.add(pixels.saturating_mul(2))?;
                let mut data = vec![0; pixels];
                let mut rowcur = wire::Cursor::new(&b[start..c.position()]);
                let mut ragged = false;
                for row in 0..rows {
                    let n = usize::try_from(rowcur.int()?).map_err(invalid)?;
                    ragged |= n != width;
                    data[row * width..row * width + n].copy_from_slice(rowcur.take(n)?);
                }
                if ragged {
                    loss(
                        &mut r,
                        "kbits",
                        "ragged_rows",
                        0,
                        Some(f.len()),
                        "Ragged rows padded with zero",
                    );
                }
                let bitmap = Bitmap::<Gray8>::from_raw(
                    u32::try_from(width).map_err(invalid)?,
                    u32::try_from(rows).map_err(invalid)?,
                    &data,
                    bl(l),
                )?;
                let mut g = Glyph::new(bitmap);
                g.metrics.bounds.x = i64::from(x);
                g.metrics.bounds.y = i64::from(ascent) - rows as i64;
                g.metrics.advance = Some(Vector::new(i64::from(advance), 0));
                if cp >= 0 {
                    g.codepoints.push(Codepoint::Unicode(cp as u32));
                } else {
                    g.properties
                        .push(Property::new("kbits.negative_code", i64::from(cp)));
                }
                f.add_glyph(g)?;
            }
            _ => return Err(invalid("unknown record tag")),
        }
    }
    f.validate(l)?;
    Ok((f, r))
}
fn native(f: &Font, key: &str, default: i64) -> i64 {
    match f.properties.get(key) {
        Some(PropertyValue::Integer(n)) => *n,
        _ => default,
    }
}
/// Emit standard Kbits, reporting every model difference after bounded reread.
pub fn write(f: &Font, o: &KbitsWriteOptions) -> Result<(Vec<u8>, Report)> {
    write_charged(f, o, ModelCharge::Estimate)
}
fn write_charged(
    f: &Font,
    o: &KbitsWriteOptions,
    charge: ModelCharge,
) -> Result<(Vec<u8>, Report)> {
    write_inner(f, o, charge).map_err(|e| {
        fidelity::failed_write(e, "kbits", std::iter::once((0, f)), 0, &o.limits, charge)
    })
}
fn write_inner(f: &Font, o: &KbitsWriteOptions, charge: ModelCharge) -> Result<(Vec<u8>, Report)> {
    f.validate(&o.limits)?;
    // Held for the whole write; zero when the caller reserved the model.
    let source_bytes = charge.of(fidelity::model_bytes(f)?);
    let mut budget = Budget::new(source_bytes, &o.limits)?;
    budget.temporary(4096)?;
    let mut name_limits = o.limits.clone();
    name_limits.max_total_bytes = budget.remaining();
    let mut b = Vec::new();
    b.extend_from_slice(b"KBnPbits");
    wire::put(&mut b, 1)?;
    let a = f
        .metrics
        .ascent
        .unwrap_or_else(|| i64::from(f.metrics.pixel_size.unwrap_or(0)));
    let d = f.metrics.descent.unwrap_or(0);
    let gap = f.metrics.line_gap.unwrap_or(0);
    for n in [
        a,
        d,
        native(f, "kbits.line_ascent", a),
        native(f, "kbits.line_descent", d),
        gap,
        native(f, "kbits.x_height", a),
    ] {
        wire::put(&mut b, n)?;
    }
    let names = [
        (0, &f.names.copyright),
        (1, &f.names.family),
        (2, &f.names.style),
        (4, &f.names.full),
        (5, &f.names.version),
        (6, &f.names.postscript),
        (8, &f.names.foundry),
        (13, &f.names.license),
        (14, &f.names.license_url),
    ];
    let mut written = [false; 9];
    for p in f.properties.iter() {
        if let Some(id) = p
            .name
            .strip_prefix("kbits.name.")
            .and_then(|s| s.parse::<i32>().ok())
            && let PropertyValue::Text(text) = &p.value
        {
            let text = names
                .iter()
                .find(|(n, _)| *n == id)
                .and_then(|(_, v)| v.as_deref())
                .unwrap_or(text);
            wire::name(&mut b, id, text, &name_limits)?;
            if let Some(index) = names.iter().position(|(n, _)| *n == id) {
                written[index] = true;
            }
        }
    }
    for (index, (id, value)) in names.into_iter().enumerate() {
        if !written[index]
            && let Some(s) = value
        {
            wire::name(&mut b, id, s, &name_limits)?;
        }
    }
    budget.add(b.capacity())?;
    for (_, g) in f.glyphs() {
        let cp = match g.codepoints.first() {
            Some(Codepoint::Unicode(n)) => i64::from(*n),
            Some(Codepoint::Legacy(_)) => {
                return Err(invalid("legacy glyph needs an explicit Unicode mapping"));
            }
            None => match g.properties.get("kbits.negative_code") {
                Some(PropertyValue::Integer(n)) if *n < 0 => *n,
                _ => {
                    return Err(invalid(
                        "unencoded glyph cannot be assigned a Kbits identity",
                    ));
                }
            },
        };
        let w = g.bitmap.width() as usize;
        let h = g.bitmap.height() as usize;
        let size = w
            .checked_add(4)
            .and_then(|n| n.checked_mul(h))
            .and_then(|n| n.checked_add(28))
            .ok_or_else(|| invalid("output overflow"))?;
        budget.add(size.saturating_add(4))?;
        b.try_reserve_exact(size.saturating_add(4))
            .map_err(invalid)?;
        bound(
            b.len().saturating_add(size),
            o.limits.max_input_bytes,
            "KBITS output",
        )?;
        b.extend_from_slice(b"char");
        for n in [
            1,
            cp,
            g.metrics.advance.unwrap_or_default().x,
            g.metrics.bounds.x,
            g.metrics.bounds.y + i64::from(g.bitmap.height()),
            h as i64,
        ] {
            wire::put(&mut b, n)?;
        }
        for y in 0..h {
            wire::put(&mut b, w as i64)?;
            match &g.bitmap {
                GlyphBitmap::Gray8(v) => b.extend_from_slice(&v.as_bytes()[y * w..(y + 1) * w]),
                GlyphBitmap::Mono(v) => {
                    let stride = w.div_ceil(8);
                    let row = &v.as_bytes()[y * stride..(y + 1) * stride];
                    for x in 0..w {
                        b.push(if row[x / 8] & (0x80 >> (x % 8)) != 0 {
                            255
                        } else {
                            0
                        });
                    }
                }
            }
        }
    }
    b.try_reserve_exact(4).map_err(invalid)?;
    b.extend_from_slice(b"fin.");
    let held = source_bytes.saturating_add(b.capacity());
    let mut read_limits = o.limits.clone();
    read_limits.max_total_bytes = Budget::new(held, &o.limits)?.remaining();
    let (out, mut r) = read(&b, &read_limits)?;
    let report_budget =
        Budget::new(held.saturating_add(fidelity::model_bytes(&out)?), &o.limits)?.remaining();
    audit(f, &out, "kbits", 0, &mut r, report_budget)?;
    Ok((b, r))
}
