use super::{GdrReadOptions, GdrVersion, GdrWriteOptions, reader, wire::*};
use crate::formats::legacy_common::*;
use crate::formats::{ModelCharge, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Glyph, PropertyValue, Spacing};
use crate::resources::{OutputBundle, Written};
use crate::{Report, Result};
use std::collections::{BTreeMap, BTreeSet};
struct Section {
    first: u16,
    last: u16,
    offsets: Vec<u16>,
    bits: Vec<u8>,
    inline_metrics: Vec<[u8; 5]>,
}
struct Strike {
    header: Vec<u8>,
    metrics: Vec<[u8; 5]>,
    sections: Vec<Section>,
}
/// Write a native direct-file-store GDR, including bitmap compression and typeface mappings.
pub fn write(s: &FontSet, w: &WriteOptions, o: &GdrWriteOptions) -> Result<Written> {
    write_charged(s, w, o, ModelCharge::Estimate)
}
pub(crate) fn write_charged(
    s: &FontSet,
    w: &WriteOptions,
    o: &GdrWriteOptions,
    charge: ModelCharge,
) -> Result<Written> {
    let mut r = Report::default();
    let result = encode(s, w, o, charge, &mut r);
    finish(result, r, "gdr", w.strict)
}
fn byte(n: i64) -> Result<u8> {
    Ok(i8::try_from(n).map_err(|_| invalid("GDR metric exceeds signed byte"))? as u8)
}
fn append32(b: &mut Vec<u8>, v: usize) -> Result<()> {
    let n = u32::try_from(v).map_err(|_| invalid("GDR count exceeds 32 bits"))?;
    b.extend_from_slice(&n.to_le_bytes());
    Ok(())
}
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &GdrWriteOptions,
    charge: ModelCharge,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget_for(s, &o.limits, charge)?;
    let fonts = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    if fonts.fonts.is_empty() {
        return Err(invalid("GDR needs at least one strike"));
    }
    check(
        fonts.fonts.len(),
        o.limits.max_resources,
        "GDR strike count",
    )?;
    let mut strikes = Vec::new();
    let mut uids = BTreeSet::new();
    let mut uid_vec = Vec::new();
    for (fi, f) in fonts.fonts.iter().enumerate() {
        let uid = u32::try_from(int(&f.properties, "gdr.uid", 0x10000000 + fi as i64)?)
            .map_err(|_| invalid("invalid GDR font UID"))?;
        if !uids.insert(uid) {
            return Err(invalid("duplicate GDR strike UID"));
        }
        uid_vec.push(uid);
        strikes.push(strike(f, fi, uid, o, r, &mut budget)?);
    }
    let mut copyrights = Vec::new();
    for p in fonts
        .properties
        .iter()
        .filter(|p| p.name == "gdr.copyright")
    {
        if let PropertyValue::Text(s) = &p.value {
            budget.add(s.len().saturating_mul(4) + 16)?;
            copyrights.push(string_bytes(s)?);
        }
    }
    if copyrights.is_empty()
        && let Some(s) = &fonts.fonts[0].names.copyright
    {
        budget.add(s.len().saturating_mul(4) + 16)?;
        copyrights.push(string_bytes(s)?);
    }
    let mut faces = Vec::new();
    for p in fonts.properties.iter().filter(|p| p.name == "gdr.typeface") {
        let PropertyValue::List(parts) = &p.value else {
            return Err(invalid("malformed gdr.typeface property"));
        };
        let [
            PropertyValue::Text(name),
            PropertyValue::Integer(flags),
            PropertyValue::List(refs),
        ] = parts.as_slice()
        else {
            return Err(invalid("malformed gdr.typeface fields"));
        };
        budget.add(name.len().saturating_mul(4) + refs.len().saturating_mul(12) + 32)?;
        let mut b = string_bytes(name)?;
        b.push(u8::try_from(*flags).map_err(|_| invalid("GDR typeface flags exceed byte"))?);
        append32(&mut b, refs.len())?;
        for rf in refs {
            let PropertyValue::List(parts) = rf else {
                return Err(invalid("malformed GDR typeface reference"));
            };
            let [
                PropertyValue::Integer(uid),
                PropertyValue::Integer(x),
                PropertyValue::Integer(y),
            ] = parts.as_slice()
            else {
                return Err(invalid("malformed GDR typeface reference fields"));
            };
            b.extend_from_slice(
                &u32::try_from(*uid)
                    .map_err(|_| invalid("GDR invalid typeface UID"))?
                    .to_le_bytes(),
            );
            if *x <= 0 || *y <= 0 {
                return Err(invalid("GDR nonpositive typeface scale"));
            }
            b.push(byte(*x)?);
            b.push(byte(*y)?);
        }
        faces.push(b);
    }
    if faces.is_empty() {
        let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (fi, f) in fonts.fonts.iter().enumerate() {
            groups
                .entry(f.names.family.as_deref().unwrap_or("fontpixel"))
                .or_default()
                .push(fi);
        }
        for (name, indices) in groups {
            budget.add(name.len().saturating_mul(4) + indices.len().saturating_mul(12) + 32)?;
            let mut b = string_bytes(name)?;
            b.push(0);
            append32(&mut b, indices.len())?;
            for i in indices {
                b.extend_from_slice(&uid_vec[i].to_le_bytes());
                b.extend_from_slice(&[1, 1]);
            }
            faces.push(b);
        }
    }
    check(faces.len(), o.limits.max_resources, "GDR typefaces")?;
    let inline = o.version == GdrVersion::V40;
    let data = 40 + copyrights.iter().map(Vec::len).sum::<usize>();
    let descriptors = 4
        + strikes
            .iter()
            .map(|s| s.header.len() + s.sections.len() * 12)
            .sum::<usize>()
        + 4
        + faces.iter().map(Vec::len).sum::<usize>();
    let mut n = data.saturating_add(descriptors);
    for st in &strikes {
        if !inline {
            n = n.saturating_add(4 + st.metrics.len() * 5);
        }
        for sec in &st.sections {
            n = n.saturating_add(
                8 + sec.offsets.len() * if inline { 8 } else { 2 } + sec.bits.len(),
            );
        }
    }
    check(
        n,
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(u32::MAX as usize),
        "GDR output",
    )?;
    budget.add(n)?;
    let mut b = vec![0; n];
    b[..16].copy_from_slice(&MAGIC);
    put32(&mut b, 16, 20, E);
    put32(
        &mut b,
        20,
        match o.version {
            GdrVersion::V39 => 39,
            GdrVersion::V40 => 40,
            GdrVersion::V41 => 41,
            GdrVersion::V42 => 42,
        },
        E,
    );
    let uid = o.collection_uid.unwrap_or(
        u32::try_from(int(&fonts.properties, "gdr.collection_uid", 0)?)
            .map_err(|_| invalid("GDR collection UID exceeds 32 bits"))?,
    );
    put32(&mut b, 24, uid, E);
    put32(
        &mut b,
        28,
        u32::try_from(int(&fonts.properties, "gdr.pixel_aspect", 1000)?)
            .map_err(|_| invalid("GDR invalid aspect ratio"))?,
        E,
    );
    put32(&mut b, 32, data as u32, E);
    put32(&mut b, 36, copyrights.len() as u32, E);
    let mut pos = 40;
    for s in &copyrights {
        b[pos..pos + s.len()].copy_from_slice(s);
        pos += s.len();
    }
    put32(&mut b, pos, strikes.len() as u32, E);
    pos += 4;
    let mut payload = data + descriptors;
    for st in &strikes {
        b[pos..pos + st.header.len()].copy_from_slice(&st.header);
        if !inline {
            let metric_field = pos + st.header.len() - 12;
            put32(&mut b, metric_field, payload as u32, E);
            put32(&mut b, payload, st.metrics.len() as u32, E);
            payload += 4;
            for m in &st.metrics {
                b[payload..payload + 5].copy_from_slice(m);
                payload += 5;
            }
        }
        pos += st.header.len();
        for sec in &st.sections {
            put16(&mut b, pos, sec.first, E);
            put16(&mut b, pos + 2, sec.last, E);
            put32(&mut b, pos + 4, payload as u32, E);
            put32(&mut b, payload, sec.offsets.len() as u32, E);
            payload += 4;
            for (i, &v) in sec.offsets.iter().enumerate() {
                put16(&mut b, payload, v, E);
                payload += 2;
                if inline {
                    b[payload..payload + 5].copy_from_slice(&sec.inline_metrics[i]);
                    payload += 6;
                }
            }
            put32(&mut b, pos + 8, payload as u32, E);
            put32(&mut b, payload, sec.bits.len() as u32, E);
            payload += 4;
            b[payload..payload + sec.bits.len()].copy_from_slice(&sec.bits);
            payload += sec.bits.len();
            pos += 12;
        }
    }
    put32(&mut b, pos, faces.len() as u32, E);
    pos += 4;
    for f in &faces {
        b[pos..pos + f.len()].copy_from_slice(f);
        pos += f.len();
    }
    let (decoded, rr) = reader::parse(&b, &GdrReadOptions { unicode: o.unicode }, &o.limits)?;
    r.extend(rr);
    budget.add(source_budget(&decoded, &o.limits)?.used)?;
    audit(s, &decoded, "gdr", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), b))
}
fn strike(
    f: &Font,
    fi: usize,
    uid: u32,
    o: &GdrWriteOptions,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<Strike> {
    let mut map = BTreeMap::new();
    for (id, g) in f.glyphs() {
        if g.codepoints.is_empty() {
            budget.loss(
                r,
                "gdr",
                "write.unencoded",
                Some(fi),
                Some(id.index()),
                "GDR requires an encoded glyph",
            )?;
        }
        for &cp in &g.codepoints {
            let allowed = if o.unicode {
                matches!(cp,Codepoint::Unicode(v) if char::from_u32(v).is_some())
            } else {
                matches!(cp, Codepoint::Legacy(_))
            };
            if !allowed || cp.value() > 65535 {
                budget.loss(
                    r,
                    "gdr",
                    "write.mapping",
                    Some(fi),
                    Some(id.index()),
                    format!("Mapping {cp:?} cannot be stored in a GDR code section"),
                )?;
                continue;
            }
            budget.add(64)?;
            if map.insert(cp.value() as u16, g).is_some() {
                budget.loss(
                    r,
                    "gdr",
                    "write.duplicate",
                    Some(fi),
                    Some(id.index()),
                    format!("Last glyph wins for {cp:?}"),
                )?;
            }
        }
    }
    if map.is_empty() {
        return Err(invalid("GDR strike has no representable glyphs"));
    }
    let mut metric_map = BTreeMap::new();
    let mut metrics = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut top = 0;
    let mut bottom = 0;
    let mut max = 0;
    for (&cp, &g) in &map {
        let bounds = g.metrics.bounds;
        let advance = g.metrics.advance.map_or(i64::from(bounds.width), |v| v.x);
        let m = [
            byte(bounds.y + i64::from(bounds.height))?,
            byte(i64::from(bounds.height))?,
            byte(bounds.x)?,
            byte(advance)?,
            byte(advance - bounds.x - i64::from(bounds.width))?,
        ];
        let index = if let Some(&i) = metric_map.get(&m) {
            i
        } else {
            let i = metrics.len();
            check(i + 1, 32768, "GDR distinct metrics")?;
            budget.add(64)?;
            metric_map.insert(m, i);
            metrics.push(m);
            i
        };
        top = top.max(bounds.y + i64::from(bounds.height));
        bottom = bottom.min(bounds.y);
        max = max.max(advance);
        let max_bytes = ((bounds.width as usize + 5) * bounds.height as usize).div_ceil(8) + 2;
        check(max_bytes, o.limits.max_table_bytes, "GDR encoded glyph")?;
        budget.add(max_bytes.saturating_mul(2))?;
        let inline = o.version == GdrVersion::V40;
        let limit = if inline { 0xffff } else { 0x7fff };
        let bytes = compress(g, if inline { None } else { Some(index) })?;
        if sections.last().is_none_or(|s| {
            u32::from(s.last) + 1 != u32::from(cp) || s.bits.len() + bytes.len() >= limit
        }) {
            check(
                sections.len() + 1,
                o.limits.max_resources,
                "GDR code sections",
            )?;
            budget.add(128)?;
            sections.push(Section {
                first: cp,
                last: cp,
                offsets: Vec::new(),
                bits: Vec::new(),
                inline_metrics: Vec::new(),
            });
        }
        let sec = sections
            .last_mut()
            .ok_or_else(|| invalid("GDR missing section"))?;
        if bytes.len() >= limit {
            return Err(invalid("GDR glyph exceeds section address space"));
        }
        budget.add(8)?;
        sec.offsets.push(sec.bits.len() as u16);
        if inline {
            sec.inline_metrics.push(m);
        }
        sec.bits.extend_from_slice(&bytes);
        sec.last = cp;
    }
    let ascent = f.metrics.ascent.unwrap_or(top);
    let height = if let Some(size) = f.metrics.pixel_size {
        i64::from(size)
    } else {
        ascent
            .checked_add(f.metrics.descent.unwrap_or(-bottom))
            .ok_or_else(|| invalid("GDR cell height overflow"))?
    };
    let style = f.names.style.as_deref().unwrap_or("").to_ascii_lowercase();
    let mut header = uid.to_le_bytes().to_vec();
    header.extend_from_slice(&[
        u8::from(style.contains("italic") || style.contains("oblique")),
        u8::from(style.contains("bold")),
        u8::from(!matches!(
            f.metrics.spacing,
            Some(Spacing::Monospaced | Spacing::CharacterCell)
        )),
        byte(height)?,
        byte(ascent)?,
        byte(int(&f.properties, "gdr.max_width", max)?)?,
        byte(int(&f.properties, "gdr.max_normal_width", max)?)?,
    ]);
    if o.version == GdrVersion::V42 {
        for (k, v) in [
            ("gdr.capital_ascent", ascent),
            ("gdr.max_ascent", top),
            ("gdr.standard_descent", height - ascent),
            ("gdr.max_descent", -bottom),
            ("gdr.line_gap", f.metrics.line_gap.unwrap_or(0)),
        ] {
            header.push(byte(int(&f.properties, k, v)?)?);
        }
    }
    append32(&mut header, 0)?;
    if o.version != GdrVersion::V40 {
        append32(&mut header, 0)?;
        append32(&mut header, metrics.len())?;
    }
    append32(&mut header, sections.len())?;
    Ok(Strike {
        header,
        metrics,
        sections,
    })
}
fn compress(g: &Glyph, index: Option<usize>) -> Result<Vec<u8>> {
    let bitmap = mono(g)?;
    let mut bits = BitWriter::new();
    if let Some(index) = index {
        if index < 128 {
            bits.put((index as u32) << 1, 8);
        } else {
            bits.put((index as u32) << 1 | 1, 16);
        }
    }
    let stride = bitmap.stride();
    let mut y = 0usize;
    let height = bitmap.height() as usize;
    while y < height {
        let row = &bitmap.as_bytes()[y * stride..(y + 1) * stride];
        let mut repeats = 1;
        while repeats < 15
            && y + repeats < height
            && bitmap.as_bytes()[(y + repeats) * stride..(y + repeats + 1) * stride] == *row
        {
            repeats += 1;
        }
        bits.put(0, 1);
        bits.put(repeats as u32, 4);
        for x in 0..bitmap.width() as usize {
            bits.put(u32::from((row[x / 8] >> (7 - x % 8)) & 1), 1);
        }
        y += repeats;
    }
    if bits.bytes.is_empty() {
        bits.bytes.push(0);
    }
    Ok(bits.bytes)
}
