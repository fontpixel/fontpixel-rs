use super::*;
use crate::Error;
use crate::model::GlyphMetrics;
use crate::working::{Budget as Ledger, Lease, Vector};
/// Native, deterministic uncompressed PF2 output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Pf2WriteOptions {
    /// Explicit threshold required for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Input, table, glyph and aggregate limits, also applied to output readback.
    pub limits: ResourceLimits,
    /// Maximum emitted bytes.
    pub max_output_bytes: usize,
}
impl Default for Pf2WriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
fn output_limit(o: &Pf2WriteOptions) -> usize {
    o.max_output_bytes.min(o.limits.max_input_bytes)
}
fn section(out: &mut Vector<u8>, tag: &[u8], body: &[u8], o: &Pf2WriteOptions) -> Result<()> {
    check(body.len(), o.limits.max_table_bytes, "PF2 output section")?;
    check(
        out.len().saturating_add(8).saturating_add(body.len()),
        output_limit(o),
        "PF2 output bytes",
    )?;
    let length =
        u32::try_from(body.len()).map_err(|_| invalid("PF2 section length exceeds u32"))?;
    out.extend(tag)?;
    out.extend(&length.to_be_bytes())?;
    out.extend(body)
}
/// Admit `bytes` on `ledger` and allocate `make`'s value of that size.
fn admitted<T>(ledger: &Ledger, bytes: usize, make: impl FnOnce() -> T) -> Result<(T, Lease)> {
    let lease = ledger.reserve(bytes)?;
    Ok((make(), lease))
}
/// Encode Unicode mappings and per-glyph signed metrics without external tools.
pub fn write(f: &Font, o: &Pf2WriteOptions) -> Result<(Vec<u8>, Report)> {
    write_charged(f, o, crate::formats::ModelCharge::Estimate)
}
pub(super) fn write_charged(
    f: &Font,
    o: &Pf2WriteOptions,
    charge: crate::formats::ModelCharge,
) -> Result<(Vec<u8>, Report)> {
    let ledger = Ledger::new(o.limits.max_total_bytes, "PF2 aggregate bytes");
    let mut d = Diagnostics::charged("pf2", &o.limits, &ledger)?;
    let bytes = write_into(f, o, charge, &mut d)?;
    Ok((bytes, std::mem::take(&mut d.report)))
}
/// Write on `d`'s exact working ledger (built from `o.limits.max_total_bytes`
/// by the caller). Live at once, each admitted before it is allocated: the
/// model's estimate unless the caller reserved it, every diagnostic (entry
/// array growth and exact strings), the mapping tree, the per-glyph code
/// lists, the output buffer (old and new buffers overlapping while it
/// grows), the glyph offsets and, per glyph only, its monochrome copy and
/// packed rows. The mirror of what a reader would decode keeps only the
/// font-level fields it compares, never glyph copies.
pub(super) fn write_into(
    f: &Font,
    o: &Pf2WriteOptions,
    charge: crate::formats::ModelCharge,
    d: &mut Diagnostics,
) -> Result<Vec<u8>> {
    let ledger = d
        .budget()
        .cloned()
        .ok_or_else(|| invalid("PF2 writer requires a working ledger"))?;
    f.validate(&o.limits)?;
    // The estimate is still validated (property nesting) when reserved.
    let estimate = source_cost(f, &unbounded(&o.limits))?;
    if charge == crate::formats::ModelCharge::Estimate {
        d.reserve(estimate)?;
    }
    let map = mappings(
        f,
        "unicode",
        |c| char::from_u32(c).is_some() || joined_code(c),
        d,
    )?;
    if map.is_empty() {
        return Err(d.fatal_mut("PF2 output has no representable Unicode mappings"));
    }
    check(map.len(), o.limits.max_glyphs, "PF2 mappings")?;
    check(
        map.len().saturating_mul(9),
        o.limits.max_table_bytes,
        "PF2 CHIX",
    )?;
    // Every (glyph, code) pair, ordered by glyph and then code: each glyph's
    // codes in ascending order, as the mapping tree yields them.
    let mut grouped: Vector<(usize, u32)> = Vector::with_capacity(&ledger, map.len())?;
    for (&cp, id) in &map {
        grouped.push((id.index(), cp))?;
    }
    grouped.sort_unstable();
    let glyph_count = grouped
        .iter()
        .enumerate()
        .filter(|&(i, &(g, _))| i == 0 || grouped[i - 1].0 != g)
        .count();
    let codes = |index: usize| {
        let start = grouped.partition_point(|&(g, _)| g < index);
        let end = grouped.partition_point(|&(g, _)| g <= index);
        &grouped[start..end]
    };
    let mut expected = Font::default();
    d.reserve(3 + 7)?;
    expected.source_format = Some("pf2".into());
    expected.charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    let mut headers = HeaderMirror::new(&ledger)?;
    let mut out = Vector::new(&ledger)?;
    out.extend(b"FILE\0\0\0\x04PFF2")?;
    let full = f.names.full.as_deref().or(Some("Unknown"));
    for (tag, value) in [(b"NAME", full), (b"FAMI", f.names.family.as_deref())] {
        if let Some(value) = value {
            if !value.is_ascii() || value.as_bytes().contains(&0) {
                d.loss(
                    "write.name",
                    None,
                    None,
                    "Non-ASCII or NUL-containing PF2 name was omitted",
                )?;
                continue;
            }
            check(
                value.len().saturating_add(1),
                o.limits.max_table_bytes,
                "PF2 name",
            )?;
            let (body, body_lease) = admitted(&ledger, value.len() + 1, || {
                let mut b = Vec::with_capacity(value.len() + 1);
                b.extend_from_slice(value.as_bytes());
                b.push(0);
                b
            })?;
            section(&mut out, tag, &body, o)?;
            drop(body);
            drop(body_lease);
            d.reserve(value.len())?;
            if tag == b"NAME" {
                expected.names.full = Some(value.to_owned());
            } else {
                expected.names.family = Some(value.to_owned());
            }
        }
    }
    if expected.names.full.is_none() {
        section(&mut out, b"NAME", b"Unknown\0", o)?;
        d.reserve(7)?;
        expected.names.full = Some("Unknown".into());
    }
    let fallback_height = f.metrics.pixel_size.unwrap_or_else(|| {
        f.glyphs()
            .map(|(_, g)| g.bitmap.height())
            .max()
            .unwrap_or(0)
    });
    if let Some(point) = f
        .metrics
        .point_size
        .or(Some(Rational::integer(i64::from(fallback_height))))
    {
        let value = point.numerator() / i64::from(point.denominator());
        let value = value.clamp(0, i64::from(u16::MAX)) as u16;
        section(&mut out, b"PTSZ", &value.to_be_bytes(), o)?;
        expected.metrics.point_size = Some(Rational::integer(i64::from(value)));
    }
    let ascent = f.metrics.ascent.or_else(|| {
        Some(
            f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                .max()
                .unwrap_or(0)
                .max(0),
        )
    });
    let descent = f.metrics.descent.or_else(|| {
        Some(
            f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y.saturating_neg())
                .max()
                .unwrap_or(0)
                .max(0),
        )
    });
    for (tag, value) in [(b"ASCE", ascent), (b"DESC", descent)] {
        if let Some(value) = value {
            let n = value.clamp(0, i64::from(u16::MAX)) as u16;
            section(&mut out, tag, &n.to_be_bytes(), o)?;
            if tag == b"ASCE" {
                expected.metrics.ascent = Some(i64::from(n));
            } else {
                expected.metrics.descent = Some(i64::from(n));
            }
        }
    }
    let mut maximum_seen = [false; 2];
    for p in f.headers.iter() {
        if let Some(hex) = p.name.strip_prefix("pf2.section.")
            && hex.len() == 8
            && hex.is_ascii()
        {
            let mut tag = [0u8; 4];
            let parsed = (0..4).all(|i| {
                u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                    .map(|b| tag[i] = b)
                    .is_ok()
            });
            if let (true, PropertyValue::Bytes(body)) = (parsed, &p.value)
                && !matches!(
                    &tag,
                    b"FILE" | b"NAME" | b"FAMI" | b"PTSZ" | b"ASCE" | b"DESC" | b"CHIX" | b"DATA"
                )
                && (!matches!(&tag, b"MAXW" | b"MAXH") || body.len() == 2)
            {
                if let Some(i) = [b"MAXW", b"MAXH"].iter().position(|t| **t == tag) {
                    if maximum_seen[i] {
                        continue;
                    }
                    maximum_seen[i] = true;
                }
                let maximum = if &tag == b"MAXW" {
                    f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0)
                } else if &tag == b"MAXH" {
                    f.glyphs()
                        .map(|(_, g)| g.bitmap.height())
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };
                let adjusted;
                let body: &[u8] = if matches!(&tag, b"MAXW" | b"MAXH")
                    && maximum > u32::from(u16::from_be_bytes([body[0], body[1]]))
                {
                    adjusted = u16::try_from(maximum).unwrap_or(u16::MAX).to_be_bytes();
                    &adjusted
                } else {
                    body
                };
                section(&mut out, &tag, body, o)?;
                headers.push(&mut expected, &tag, body)?;
            }
        }
    }
    for (tag, n) in [
        (
            b"MAXW",
            f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0),
        ),
        (
            b"MAXH",
            f.glyphs()
                .map(|(_, g)| g.bitmap.height())
                .max()
                .unwrap_or(0),
        ),
    ] {
        if !expected.headers.iter().any(|p| is_section(&p.name, tag)) {
            let data = u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes();
            section(&mut out, tag, &data, o)?;
            headers.push(&mut expected, tag, &data)?;
        }
    }
    let index_at = out.len();
    let index_len = map.len() * 9;
    check(index_len, o.limits.max_table_bytes, "PF2 output section")?;
    check(
        out.len().saturating_add(8).saturating_add(index_len),
        output_limit(o),
        "PF2 output bytes",
    )?;
    out.extend(b"CHIX")?;
    out.extend(
        &u32::try_from(index_len)
            .map_err(|_| invalid("PF2 section length exceeds u32"))?
            .to_be_bytes(),
    )?;
    out.resize(out.len() + index_len, 0)?;
    check(
        out.len().saturating_add(8),
        output_limit(o),
        "PF2 output bytes",
    )?;
    out.extend(b"DATA\xff\xff\xff\xff")?;
    let mut offsets: Vector<(usize, u32)> = Vector::with_capacity(&ledger, glyph_count)?;
    // What a reader of this output would retain, for the same-limit readback
    // check below, and the union of the written glyph boxes.
    let mut readback = 0usize;
    let mut union: Option<(i64, i64, i64, i64)> = None;

    for (id, g) in f.glyphs() {
        let cps = codes(id.index());
        if cps.is_empty() {
            continue;
        }
        let bounds = g.metrics.bounds;
        let advance = g
            .metrics
            .advance
            .unwrap_or(Vector2::new(i64::from(bounds.width), 0));
        let fields = (
            u16::try_from(bounds.width),
            u16::try_from(bounds.height),
            i16::try_from(bounds.x),
            i16::try_from(bounds.y),
            i16::try_from(advance.x),
        );
        let (Ok(w), Ok(h), Ok(x), Ok(y), Ok(advance)) = fields else {
            for &(_, cp) in cps {
                d.loss(
                    "write.omitted",
                    Some(id.index()),
                    Some(cp),
                    "PF2 glyph dimensions or metrics exceed 16-bit fields",
                )?;
            }
            continue;
        };
        // The monochrome copy (and a grayscale clone while it is thresholded).
        let mono_bytes = (g.bitmap.width() as usize)
            .div_ceil(8)
            .saturating_mul(g.bitmap.height() as usize);
        let copy = match &g.bitmap {
            GlyphBitmap::Mono(b) => b.as_bytes().len(),
            GlyphBitmap::Gray8(b) => b.as_bytes().len().saturating_add(mono_bytes),
        };
        let (bitmap, copy) = admitted(&ledger, copy, || {
            mono(g, None, o.gray_threshold, id.index(), d)
        })?;
        let bitmap = match bitmap {
            Ok(bitmap) => bitmap,
            Err(error @ Error::ResourceLimit { .. }) => return Err(error),
            Err(error) => {
                for &(_, cp) in cps {
                    d.loss_fmt(
                        "write.omitted",
                        Some(id.index()),
                        Some(cp),
                        format_args!("{error}"),
                    )?;
                }
                continue;
            }
        };
        let packed_bytes = (bitmap.width() as usize * bitmap.height() as usize).div_ceil(8);
        let (packed, packed_lease) = admitted(&ledger, packed_bytes, || pack(&bitmap))?;
        check(
            out.len().saturating_add(10).saturating_add(packed.len()),
            output_limit(o),
            "PF2 output bytes",
        )?;
        offsets.push((
            id.index(),
            u32::try_from(out.len()).map_err(|_| invalid("PF2 offset exceeds u32"))?,
        ))?;
        out.extend(&w.to_be_bytes())?;
        out.extend(&h.to_be_bytes())?;
        out.extend(&x.to_be_bytes())?;
        out.extend(&y.to_be_bytes())?;
        out.extend(&advance.to_be_bytes())?;
        out.extend(&packed)?;
        drop(packed);
        drop(packed_lease);
        let metrics = GlyphMetrics {
            bounds,
            advance: Some(Vector2::new(i64::from(advance), 0)),
            ..Default::default()
        };
        glyph_metadata(g, &metrics, id.index(), d)?;
        if !g
            .codepoints
            .iter()
            .copied()
            .eq(cps.iter().map(|&(_, cp)| Codepoint::Unicode(cp)))
        {
            d.loss(
                "write.aliases",
                Some(id.index()),
                None,
                "Aliases were reordered, converted, or omitted",
            )?;
        }
        readback = readback
            .saturating_add(bitmap.as_bytes().len() * 3)
            .saturating_add(cps.len() * 16);
        drop(bitmap);
        drop(copy);
        if bounds.width != 0 && bounds.height != 0 {
            let r = bounds.x + i64::from(bounds.width);
            let t = bounds.y + i64::from(bounds.height);
            union = Some(match union {
                None => (bounds.x, bounds.y, r, t),
                Some((ux, uy, ur, ut)) => {
                    (ux.min(bounds.x), uy.min(bounds.y), ur.max(r), ut.max(t))
                }
            });
        }
    }
    if offsets.len() != glyph_count {
        return Err(d.fatal_mut("PF2 glyph fields are not representable; output was not emitted"));
    }
    for (i, (&cp, id)) in map.iter().enumerate() {
        let at = index_at + 8 + i * 9;
        let slot = offsets.partition_point(|&(g, _)| g < id.index());
        let offset = offsets[slot].1;
        out[at..at + 4].copy_from_slice(&cp.to_be_bytes());
        out[at + 4] = 0;
        out[at + 5..at + 9].copy_from_slice(&offset.to_be_bytes());
    }
    // The mirror holds no glyphs: its box is the union of the written ones,
    // exactly as `update_bounds` would compute it from them.
    if let Some((x, y, r, t)) = union {
        let w = u32::try_from(i128::from(r) - i128::from(x))
            .map_err(|_| Error::InvalidInput("font union width overflows".into()))?;
        let h = u32::try_from(i128::from(t) - i128::from(y))
            .map_err(|_| Error::InvalidInput("font union height overflows".into()))?;
        expected.metrics.bounds = Some(crate::bitmap::Bounds::new(w, h, x, y));
    }
    normalize_metrics(&mut expected)?;
    metadata(f, &expected, d)?;

    // Same-limit readback acceptance (not a charge): the reader's own
    // conservative accounting of this output must fit the same total.
    let mut read_budget = Budget::new(out.len(), map.len(), &o.limits)?;
    read_budget.add(1024)?;
    read_budget.add(map.keys().filter(|&&cp| joined_code(cp)).count() * 768)?;
    read_budget.add(readback)?;
    for p in expected.headers.iter() {
        if let PropertyValue::Bytes(b) = &p.value {
            read_budget.add(b.len() * 2 + 256)?;
        }
    }
    read_budget.add(
        expected.names.full.as_ref().map_or(0, |s| s.len() * 2 + 2)
            + expected
                .names
                .family
                .as_ref()
                .map_or(0, |s| s.len() * 2 + 2),
    )?;
    let (bytes, _lease) = out.into_parts();
    Ok(bytes)
}
/// Whether `name` is `section_key(tag)`, without allocating the key.
fn is_section(name: &str, tag: &[u8]) -> bool {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    name.strip_prefix("pf2.section.").is_some_and(|hex| {
        hex.len() == tag.len() * 2
            && tag
                .iter()
                .zip(hex.as_bytes().chunks(2))
                .all(|(b, pair)| pair == [HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]])
    })
}
/// The mirror's header properties: the array and every key and body are
/// admitted before they are allocated.
struct HeaderMirror {
    ledger: Ledger,
    array: Lease,
    strings: Lease,
}
impl HeaderMirror {
    fn new(ledger: &Ledger) -> Result<Self> {
        Ok(Self {
            ledger: ledger.clone(),
            array: ledger.reserve(0)?,
            strings: ledger.reserve(0)?,
        })
    }
    fn push(&mut self, expected: &mut Font, tag: &[u8], body: &[u8]) -> Result<()> {
        let headers = &mut expected.headers;
        let strings = self
            .strings
            .bytes()
            .checked_add(section_key_len(tag))
            .and_then(|n| n.checked_add(body.len()))
            .ok_or_else(|| self.ledger.error())?;
        self.strings.grow_to(strings)?;
        let capacity = headers.allocation_capacity();
        // A full array grows (`Vec::push`: twice its capacity, at least
        // four) into a new one admitted while the old one is live.
        let grown = if headers.len() == capacity {
            Some(
                self.ledger
                    .reserve_array::<Property>(capacity.saturating_mul(2).max(4))?,
            )
        } else {
            None
        };
        headers.push(Property::new(
            section_key(tag),
            PropertyValue::Bytes(body.to_vec()),
        ));
        if let Some(mut grown) = grown {
            grown.grow_to(
                headers
                    .allocation_capacity()
                    .saturating_mul(std::mem::size_of::<Property>()),
            )?;
            // The old array has dropped.
            self.array = grown;
        }
        Ok(())
    }
}
use crate::model::{GlyphBitmap, Vector as Vector2};
