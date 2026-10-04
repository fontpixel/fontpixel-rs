//! Bounded CVT directory, strip and Unicode map decoding.
use super::*;
pub(super) fn is_cvt(b: &[u8]) -> bool {
    b.get(30..53)
        .is_some_and(|s| s == b"PRG formatted GEOS file" || s == b"SEQ formatted GEOS file")
}
#[derive(Clone, Copy)]
struct Strike<'a> {
    bytes: &'a [u8],
    ascent: u32,
    stride: usize,
    height: u32,
    xo: usize,
    count: usize,
    bo: usize,
    ko: Option<usize>,
    uo: Option<usize>,
    bitmap_len: usize,
}
impl<'a> Strike<'a> {
    fn new(b: &'a [u8]) -> Result<Self> {
        slice(b, 0, 8)?;
        let xo = usize::from(word(b, 4)?);
        let bo = usize::from(word(b, 6)?);
        let extended = xo >= 14 && word(b, 8)? & 0x8000 != 0;
        let flags = if extended { word(b, 8)? } else { 0 };
        let ko = if flags & 0x2000 != 0 {
            Some(usize::from(word(b, 10)?))
        } else {
            None
        };
        let uo = if flags & 0x1000 != 0 {
            Some(usize::from(word(b, 12)?))
        } else {
            None
        };
        let header = if extended { 14 } else { 8 };
        let points = [Some(xo), Some(bo), ko, uo];
        if points.iter().flatten().any(|&p| p < header || p > b.len()) {
            return Err(invalid("GEOS section offset outside strike"));
        }
        let end = |at: usize| {
            points
                .iter()
                .flatten()
                .copied()
                .filter(|&p| p > at)
                .min()
                .unwrap_or(b.len())
        };
        let xl = end(xo) - xo;
        if xl < 2 || xl % 2 != 0 {
            return Err(invalid("invalid GEOS boundary table size"));
        }
        let count = xl / 2 - 1;
        if count > 256 {
            return Err(invalid("GEOS native strip character count exceeds 256"));
        }
        if let Some(k) = ko {
            slice(b, k, count * 2)?;
            if end(k) - k < count * 2 {
                return Err(invalid("GEOS offset-width table overlaps next section"));
            }
        }
        let bitmap_len = if uo == Some(bo) { 0 } else { end(bo) - bo };
        Ok(Self {
            bytes: b,
            ascent: u32::from(b[0]) + 1,
            stride: usize::from(word(b, 1)?),
            height: u32::from(b[3]),
            xo,
            count,
            bo,
            ko,
            uo,
            bitmap_len,
        })
    }
    fn glyph(
        &self,
        index: usize,
        cp: Codepoint,
        l: &ResourceLimits,
        budget: &mut Budget,
    ) -> Result<Glyph> {
        if index >= self.count {
            return Err(invalid("GEOS glyph index exceeds strip boundary table"));
        }
        let x = usize::from(word(self.bytes, self.xo + 2 * index)?);
        let end = usize::from(word(self.bytes, self.xo + 2 * index + 2)?);
        if end < x || end > self.stride * 8 {
            return Err(invalid("GEOS glyph slice outside strip"));
        }
        let n = self.stride * self.height as usize;
        if self.bitmap_len < n {
            return Err(invalid("truncated GEOS bitmap strip"));
        }
        let mut g = Glyph::new(extract(
            slice(self.bytes, self.bo, n)?,
            self.stride,
            x,
            (end - x) as u32,
            self.height,
            l,
            budget,
        )?);
        g.metrics.bounds.y = i64::from(self.ascent) - i64::from(self.height);
        let advance = if let Some(k) = self.ko {
            g.metrics.bounds.x = i64::from(self.bytes[k + 2 * index] as i8);
            i64::from(self.bytes[k + 2 * index + 1])
        } else {
            (end - x) as i64
        };
        g.metrics.advance = Some(Vector::new(advance, 0));
        g.codepoints.push(cp);
        Ok(g)
    }
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Leaf {
    record: usize,
    offset: usize,
    len: usize,
}
fn leaf(b: &[u8], at: usize) -> Result<Option<Leaf>> {
    let e = slice(b, at, 4)?;
    if e == [0; 4] {
        return Ok(None);
    }
    if e[0] >= 126 || word(e, 2)? == 0 {
        return Err(invalid("invalid GEOS Unicode block reference"));
    }
    Ok(Some(Leaf {
        record: usize::from(e[0]),
        offset: usize::from(e[1]) * 254,
        len: usize::from(word(e, 2)?),
    }))
}
fn unicode_map(b: &[u8], at: usize, budget: &mut Budget) -> Result<BTreeMap<u32, Leaf>> {
    slice(b, at, 172)?;
    let mut out = BTreeMap::new();
    let mut add = |block: u32, p: usize| -> Result<()> {
        if !(2..=0x10ffff / 64).contains(&block) || (0xd800 / 64..=0xdfff / 64).contains(&block) {
            if leaf(b, p)?.is_some() {
                return Err(invalid("GEOS map references non-Unicode codepoints"));
            }
            return Ok(());
        }
        if let Some(e) = leaf(b, p)? {
            budget.add(96)?;
            out.insert(block, e);
        }
        Ok(())
    };
    for i in 2..32 {
        add(i as u32, at + 4 * i)?;
    }
    for i in 0..16 {
        let p = usize::from(word(b, at + 128 + 2 * i)?);
        if p != 0 {
            slice(b, p, 256)?;
            for j in 0..64 {
                let block = (i * 64 + j) as u32;
                if block >= 32 {
                    add(block, p + j * 4)?;
                }
            }
        }
    }
    for i in 0..5 {
        let a = usize::from(word(b, at + 160 + 2 * i)?);
        if a != 0 {
            slice(b, a, 128)?;
            for j in 0..64 {
                let p = usize::from(word(b, a + 2 * j)?);
                if p != 0 {
                    slice(b, p, 256)?;
                    for k in 0..64 {
                        let block = (i * 4096 + j * 64 + k) as u32;
                        if block >= 1024 {
                            add(block, p + k * 4)?;
                        }
                    }
                }
            }
        }
    }
    // The sixth publisher slot covers values above Unicode's maximum; reject live data.
    if word(b, at + 170)? != 0 {
        return Err(invalid(
            "GEOS Unicode map contains a non-Unicode sixth plane group",
        ));
    }
    Ok(out)
}
pub(super) fn parse(
    b: &[u8],
    o: &GeosReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "GEOS file bytes")?;
    let cvt = is_cvt(b);
    let mut records: [Option<&[u8]>; 127] = [None; 127];
    let mut family = None;
    let mut font_id = 0;
    if cvt {
        slice(b, 0, 762)?;
        if b[21] != 1 || b[22] != 8 || b[254 + 67] != 8 || b[254 + 68] != 1 {
            return Err(invalid("CVT is not a GEOS VLIR font"));
        }
        let name = &b[3..19];
        let n = name
            .iter()
            .rposition(|&v| v != 0xa0 && v != 0)
            .map_or(0, |i| i + 1);
        family = Some(latin(&name[..n]));
        font_id = word(b, 254 + 126)?;
        let mut at = 762usize;
        for (i, record) in records.iter_mut().enumerate() {
            let sectors = usize::from(b[508 + 2 * i]);
            let last = usize::from(b[509 + 2 * i]);
            if sectors == 0 {
                if last == 0 {
                    break;
                }
                if last != 255 {
                    return Err(invalid("invalid empty CVT record marker"));
                }
                continue;
            }
            if last < 2 {
                return Err(invalid("invalid CVT final-sector byte count"));
            }
            let n = (sectors - 1) * 254 + last - 1;
            *record = Some(slice(b, at, n)?);
            at = at
                .checked_add(n.div_ceil(254) * 254)
                .ok_or_else(|| invalid("CVT record offset overflow"))?;
        }
    } else {
        records[0] = Some(b);
    }
    let mut aux = BTreeSet::new();
    if let Some(index) = records[126] {
        if index.len() % 4 != 0 {
            return Err(invalid(
                "GEOS Unicode index length is not a multiple of four",
            ));
        }
        for p in (0..index.len()).step_by(4) {
            if let Some(e) = leaf(index, p)? {
                let record = records[e.record]
                    .ok_or_else(|| invalid("GEOS Unicode index references an empty record"))?;
                slice(record, e.offset, e.len)?;
                aux.insert(e.record);
            } else {
                break;
            }
        }
    }
    let mega = if let Some(r) = records[54] {
        let st = Strike::new(r)?;
        st.bitmap_len == 0 && (48..54).all(|i| records[i].is_some())
    } else {
        false
    };
    let mut set = FontSet {
        source_format: Some("geos".into()),
        ..Default::default()
    };
    budget.add(b.len().saturating_mul(2).saturating_add(4096))?;
    set.properties.push(Property::new(
        "geos.native",
        PropertyValue::Bytes(b.to_vec()),
    ));
    set.properties.push(Property::new(
        "geos.container",
        if cvt { "cvt" } else { "bare" },
    ));
    set.properties
        .push(Property::new("geos.include_del", i64::from(o.include_del)));
    let mut total = 0usize;
    for (slot, record) in records.iter().enumerate() {
        let Some(data) = *record else {
            continue;
        };
        if slot == 126 || aux.contains(&slot) || mega && (49..=54).contains(&slot) {
            continue;
        }
        check(set.fonts.len() + 1, l.max_resources, "GEOS strikes")?;
        budget.items::<Font>(1)?;
        let st = Strike::new(if mega && slot == 48 {
            records[54].unwrap()
        } else {
            data
        })?;
        if st.height == 0 && !(mega && slot == 48) {
            return Err(invalid("zero-height GEOS bitmap strike"));
        }
        let unicode = st.uo.is_some();
        let variant = match (mega && slot == 48, unicode, st.ko.is_some()) {
            (true, true, _) => "mega-utf8",
            (true, false, _) => "mega",
            (_, true, _) => "utf8",
            (_, _, true) => "extended",
            _ => "standard",
        };
        let mut f = Font::default();
        f.source_format = Some("geos".into());
        f.names.family = family.clone();
        f.charset.name = Some(if unicode { "unicode" } else { "geos-native" }.into());
        f.metrics.ascent = Some(i64::from(st.ascent));
        f.metrics.descent = Some(i64::from(st.height) - i64::from(st.ascent));
        f.metrics.line_height = Some(i64::from(st.height));
        f.metrics.pixel_size = Some(if cvt { slot as u32 } else { st.height });
        f.properties.push(Property::new("geos.record", slot as i64));
        f.properties
            .push(Property::new("geos.font_id", i64::from(font_id)));
        f.properties.push(Property::new("geos.variant", variant));
        let count = if o.include_del { 96 } else { 95 };
        if st.count < count {
            return Err(invalid("GEOS base strike has too few character boundaries"));
        }
        for i in 0..count {
            total += 1;
            check(total, l.max_glyphs, "GEOS cumulative glyphs")?;
            budget.items::<Glyph>(1)?;
            budget.add(128)?;
            let selected = if mega && slot == 48 {
                Strike::new(records[48 + i / 16].unwrap())?
            } else {
                st
            };
            let cp = if unicode {
                Codepoint::Unicode((32 + i) as u32)
            } else {
                Codepoint::Legacy((32 + i) as u32)
            };
            f.add_glyph(selected.glyph(i, cp, l, budget)?)?;
        }
        if let Some(p) = st.uo {
            for (block, e) in unicode_map(st.bytes, p, budget)? {
                let record = records[e.record]
                    .ok_or_else(|| invalid("GEOS Unicode map references an empty record"))?;
                let ext = Strike::new(slice(record, e.offset, e.len)?)?;
                if ext.count < 64 {
                    return Err(invalid("GEOS Unicode block has fewer than 64 glyphs"));
                }
                for i in 0..64 {
                    let x = word(ext.bytes, ext.xo + 2 * i)?;
                    let end = word(ext.bytes, ext.xo + 2 * i + 2)?;
                    let advance = ext
                        .ko
                        .map_or(end.wrapping_sub(x), |k| u16::from(ext.bytes[k + 2 * i + 1]));
                    if x == end && advance == 0 {
                        continue;
                    }
                    total += 1;
                    check(total, l.max_glyphs, "GEOS cumulative glyphs")?;
                    budget.items::<Glyph>(1)?;
                    budget.add(128)?;
                    f.add_glyph(ext.glyph(
                        i,
                        Codepoint::Unicode(block * 64 + i as u32),
                        l,
                        budget,
                    )?)?;
                }
            }
        }
        set.fonts.push(f);
    }
    if set.fonts.is_empty() {
        return Err(invalid("GEOS resource has no font strikes"));
    }
    Ok((set, Report::default()))
}
