// W3C WOFF2 §5: stream transforms, without contour flattening or hint execution.
// Triplet class equations also cross-checked against fontTools (MIT).
use super::*;
type Tables = BTreeMap<[u8; 4], Vec<u8>>;
struct Source<'a> {
    glyf: &'a [u8],
    offsets: Vec<usize>,
    index_format: u16,
}
fn source<'a>(t: &'a Tables, l: &Limits) -> Result<Source<'a>> {
    let get = |tag: &[u8; 4]| {
        t.get(tag)
            .map(Vec::as_slice)
            .ok_or_else(|| invalid(format!("missing table {:?}", tag)))
    };
    let n = u16_at(get(b"maxp")?, 4)? as usize;
    bounded(n, l.sfnt.resources.max_glyphs, "WOFF2 glyphs")?;
    if n == 0 {
        return Err(invalid("zero glyph count"));
    }
    let format = u16_at(get(b"head")?, 50)?;
    let unit = match format {
        0 => 2,
        1 => 4,
        _ => return Err(invalid("bad loca format")),
    };
    let loca = get(b"loca")?;
    let glyf = get(b"glyf")?;
    if loca.len() != (n + 1) * unit {
        return Err(invalid("incorrect loca length"));
    }
    let mut offsets = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let offset = if unit == 2 {
            u16_at(loca, i * 2)? as usize * 2
        } else {
            u32_at(loca, i * 4)? as usize
        };
        if offset > glyf.len() || offsets.last().is_some_and(|v| *v > offset) {
            return Err(invalid("invalid loca offsets"));
        }
        offsets.push(offset);
    }
    Ok(Source {
        glyf,
        offsets,
        index_format: format,
    })
}
// Google/FreeType reconstruct at four-byte glyph boundaries and consult the
// advertised sizes. Normalize the transform input, upgrading short loca only
// when the normalized offsets require it, before recording original lengths.
pub(super) fn normalize(t: &mut Tables, l: &Limits) -> Result<()> {
    let s = source(t, l)?;
    let mut glyf = Vec::new();
    let mut offsets = Vec::with_capacity(s.offsets.len());
    for p in s.offsets.windows(2) {
        offsets.push(glyf.len());
        let bytes = &s.glyf[p[0]..p[1]];
        bounded(
            glyf.len().saturating_add(align(bytes.len())?),
            l.sfnt.resources.max_table_bytes,
            "WOFF2 normalized glyf bytes",
        )?;
        glyf.extend(bytes);
        pad(&mut glyf);
    }
    offsets.push(glyf.len());
    let format = if s.index_format == 0 && glyf.len() / 2 <= u16::MAX as usize {
        0u16
    } else {
        1u16
    };
    let mut loca = Vec::with_capacity(offsets.len() * if format == 0 { 2 } else { 4 });
    for offset in offsets {
        if format == 0 {
            p16(&mut loca, (offset / 2) as u16);
        } else {
            p32(&mut loca, as32(offset)?);
        }
    }
    let head = t.get_mut(b"head").ok_or_else(|| invalid("missing head"))?;
    head[50..52].copy_from_slice(&format.to_be_bytes());
    t.insert(*b"glyf", glyf);
    t.insert(*b"loca", loca);
    Ok(())
}
pub(super) fn xmins(t: &Tables, l: &Limits) -> Result<Vec<i16>> {
    let s = source(t, l)?;
    s.offsets
        .windows(2)
        .map(|p| {
            if p[0] == p[1] {
                Ok(0)
            } else {
                let b = slice(s.glyf, p[0], p[1] - p[0])?;
                slice(b, 0, 10)?;
                Ok(if u16_at(b, 0)? == 0 {
                    0
                } else {
                    u16_at(b, 2)? as i16
                })
            }
        })
        .collect()
}
#[derive(Default)]
struct Budget {
    points: usize,
    components: usize,
    instructions: usize,
}
impl Budget {
    fn points(&mut self, n: usize, l: &Limits) -> Result<()> {
        bounded(n, l.max_points_per_glyph.min(65536), "WOFF2 glyph points")?;
        self.points = self.points.saturating_add(n);
        bounded(self.points, l.max_total_points, "WOFF2 total points")
    }
    fn instructions(&mut self, n: usize, l: &Limits) -> Result<()> {
        self.instructions = self.instructions.saturating_add(n);
        bounded(
            self.instructions,
            l.max_total_instructions,
            "WOFF2 instructions",
        )
    }
}
fn components<'a>(
    r: &mut Cursor<'a>,
    n: usize,
    budget: &mut Budget,
    l: &Limits,
) -> Result<(&'a [u8], bool)> {
    let start = r.pos;
    let mut instructions = false;
    loop {
        budget.components = budget.components.saturating_add(1);
        bounded(
            budget.components,
            l.max_total_components,
            "WOFF2 components",
        )?;
        let flags = r.word()?;
        let glyph = r.word()? as usize;
        if glyph >= n
            || flags & !0x1fff != 0
            || flags & 0x1800 == 0x1800
            || (flags & 0xc8).count_ones() > 1
        {
            return Err(invalid("invalid component flags or glyph ID"));
        }
        instructions |= flags & 0x100 != 0;
        r.take(if flags & 1 != 0 { 4 } else { 2 })?;
        r.take(if flags & 8 != 0 {
            2
        } else if flags & 64 != 0 {
            4
        } else if flags & 128 != 0 {
            8
        } else {
            0
        })?;
        if flags & 32 == 0 {
            break;
        }
    }
    Ok((&r.data[start..r.pos], instructions))
}
fn bounds(points: &[(i16, i16, bool)]) -> [i16; 4] {
    let mut b = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
    for &(x, y, _) in points {
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
    b
}
fn bbox(data: &[u8]) -> Result<[i16; 4]> {
    Ok([
        u16_at(data, 0)? as i16,
        u16_at(data, 2)? as i16,
        u16_at(data, 4)? as i16,
        u16_at(data, 6)? as i16,
    ])
}
fn coord(v: i32) -> Result<i16> {
    i16::try_from(v).map_err(|_| invalid("glyph coordinate or delta overflow"))
}
type SimpleParts = (Vec<u16>, Vec<(i16, i16, bool)>, Vec<u8>, bool);
fn read_simple(
    r: &mut Cursor<'_>,
    nc: usize,
    budget: &mut Budget,
    l: &Limits,
) -> Result<SimpleParts> {
    let mut lengths = Vec::with_capacity(nc);
    let mut n = 0;
    for _ in 0..nc {
        let end = r.word()? as usize + 1;
        if end <= n {
            return Err(invalid("non-increasing contour ends"));
        }
        lengths.push(
            u16::try_from(end - n)
                .map_err(|_| invalid("contour has too many points for 255UInt16"))?,
        );
        n = end;
    }
    budget.points(n, l)?;
    let ni = r.word()? as usize;
    budget.instructions(ni, l)?;
    let instructions = r.take(ni)?.to_vec();
    let mut flags = Vec::with_capacity(n);
    while flags.len() < n {
        let f = r.byte()?;
        if f & 128 != 0 {
            return Err(invalid("reserved simple glyph flag"));
        }
        let count = if f & 8 != 0 {
            r.byte()? as usize + 1
        } else {
            1
        };
        if count > n - flags.len() {
            return Err(invalid("flag repeat exceeds points"));
        }
        flags.extend(std::iter::repeat_n(f, count));
    }
    let mut points = flags
        .iter()
        .map(|f| (0i16, 0i16, f & 1 != 0))
        .collect::<Vec<_>>();
    for (axis, short, same) in [(0, 2, 16), (1, 4, 32)] {
        let mut pos = 0i32;
        for (p, f) in points.iter_mut().zip(&flags) {
            let d = if f & short != 0 {
                let b = r.byte()? as i32;
                if f & same != 0 { b } else { -b }
            } else if f & same != 0 {
                0
            } else {
                r.word()? as i16 as i32
            };
            pos += d;
            if axis == 0 {
                p.0 = coord(pos)?;
            } else {
                p.1 = coord(pos)?;
            }
        }
    }
    Ok((
        lengths,
        points,
        instructions,
        flags.first().is_some_and(|f| f & 64 != 0),
    ))
}
fn triplet(flags: &mut Vec<u8>, out: &mut Vec<u8>, dx: i32, dy: i32, on: bool) {
    let (x, y) = (dx.unsigned_abs(), dy.unsigned_abs());
    let (sx, sy) = ((dx >= 0) as u32, (dy >= 0) as u32);
    let sign = sx + 2 * sy;
    let off = if on { 0 } else { 128 };
    let flag = if x == 0 && y < 1280 {
        out.push(y as u8);
        ((y & 0xf00) >> 7) + sy
    } else if y == 0 && x < 1280 {
        out.push(x as u8);
        10 + ((x & 0xf00) >> 7) + sx
    } else if x > 0 && y > 0 && x < 65 && y < 65 {
        out.push((((x - 1) & 15) << 4 | ((y - 1) & 15)) as u8);
        20 + ((x - 1) & 48) + (((y - 1) & 48) >> 2) + sign
    } else if x > 0 && y > 0 && x < 769 && y < 769 {
        out.extend([(x - 1) as u8, (y - 1) as u8]);
        84 + 12 * ((x - 1) >> 8) + 4 * ((y - 1) >> 8) + sign
    } else if x < 4096 && y < 4096 {
        out.extend([(x >> 4) as u8, ((x & 15) << 4 | (y >> 8)) as u8, y as u8]);
        120 + sign
    } else {
        p16(out, x as u16);
        p16(out, y as u16);
        124 + sign
    };
    flags.push(off | flag as u8);
}
fn read_triplet(flag: u8, r: &mut Cursor<'_>) -> Result<(i32, i32, bool)> {
    let f = flag & 127;
    let sign = |v: i32, s: u8| if s & 1 != 0 { v } else { -v };
    let (x, y) = if f < 10 {
        (0, sign((((f & 14) as i32) << 7) + r.byte()? as i32, f))
    } else if f < 20 {
        (
            sign(((((f - 10) & 14) as i32) << 7) + r.byte()? as i32, f),
            0,
        )
    } else if f < 84 {
        let b = r.byte()? as i32;
        let k = (f - 20) as i32;
        (
            sign(1 + (k & 48) + (b >> 4), f),
            sign(1 + ((k & 12) << 2) + (b & 15), f >> 1),
        )
    } else if f < 120 {
        let k = (f - 84) as i32;
        (
            sign(1 + ((k / 12) << 8) + r.byte()? as i32, f),
            sign(1 + (((k % 12) >> 2) << 8) + r.byte()? as i32, f >> 1),
        )
    } else if f < 124 {
        let b = r.take(3)?;
        (
            sign(((b[0] as i32) << 4) + (b[1] as i32 >> 4), f),
            sign((((b[1] & 15) as i32) << 8) + b[2] as i32, f >> 1),
        )
    } else {
        (sign(r.word()? as i32, f), sign(r.word()? as i32, f >> 1))
    };
    Ok((x, y, flag & 128 == 0))
}
pub(super) fn encode(t: &Tables, l: &Limits) -> Result<Vec<u8>> {
    let s = source(t, l)?;
    let n = s.offsets.len() - 1;
    let mut streams: [Vec<u8>; 7] = Default::default();
    let mut bitmap = vec![0u8; n.div_ceil(32) * 4];
    let mut overlap = vec![0u8; n.div_ceil(8)];
    let mut budget = Budget::default();
    for (gid, p) in s.offsets.windows(2).enumerate() {
        let data = &s.glyf[p[0]..p[1]];
        if data.is_empty() {
            p16(&mut streams[0], 0);
            continue;
        }
        let mut r = Cursor::new(data);
        let nc = r.word()? as i16;
        let original_bbox = bbox(r.take(8)?)?;
        if nc < -1 {
            return Err(invalid("invalid contour count"));
        }
        p16(&mut streams[0], nc as u16);
        if nc == 0 {
            if original_bbox != [0; 4] || (data.len() > 10 && r.word()? != 0) {
                return Err(invalid(
                    "cannot transform zero-contour glyph with bounds or instructions; select null transform",
                ));
            }
            continue;
        }
        let mut store_bbox = true;
        if nc > 0 {
            let (lengths, points, instructions, has_overlap) =
                read_simple(&mut r, nc as usize, &mut budget, l)?;
            for len in lengths {
                ushort255(&mut streams[1], len);
            }
            let (mut x, mut y) = (0, 0);
            let (first, second) = streams.split_at_mut(3);
            for &(px, py, on) in &points {
                triplet(
                    &mut first[2],
                    &mut second[0],
                    px as i32 - x,
                    py as i32 - y,
                    on,
                );
                x = px as i32;
                y = py as i32;
            }
            ushort255(&mut streams[3], instructions.len() as u16);
            streams[6].extend(instructions);
            store_bbox = bounds(&points) != original_bbox;
            if has_overlap {
                overlap[gid / 8] |= 0x80 >> (gid % 8);
            }
        } else {
            let (raw, has_instructions) = components(&mut r, n, &mut budget, l)?;
            streams[4].extend(raw);
            if has_instructions {
                let len = r.word()?;
                budget.instructions(len as usize, l)?;
                ushort255(&mut streams[3], len);
                streams[6].extend(r.take(len as usize)?);
            }
        }
        if store_bbox {
            bitmap[gid / 8] |= 0x80 >> (gid % 8);
            for v in original_bbox {
                p16(&mut streams[5], v as u16);
            }
        }
        let total = streams.iter().map(Vec::len).sum::<usize>();
        bounded(
            total,
            l.sfnt.resources.max_table_bytes,
            "WOFF2 transformed glyf bytes",
        )?;
    }
    bitmap.extend(&streams[5]);
    streams[5] = bitmap;
    let has_overlap = overlap.iter().any(|v| *v != 0);
    let size = 36
        + streams.iter().map(Vec::len).sum::<usize>()
        + if has_overlap { overlap.len() } else { 0 };
    bounded(
        size,
        l.sfnt.resources.max_table_bytes,
        "WOFF2 transformed glyf bytes",
    )?;
    let mut out = buffer(size)?;
    for v in [0, has_overlap as u16, n as u16, s.index_format] {
        p16(&mut out, v);
    }
    for stream in &streams {
        p32(&mut out, as32(stream.len())?);
    }
    for stream in streams {
        out.extend(stream);
    }
    if has_overlap {
        out.extend(overlap);
    }
    Ok(out)
}
struct SimpleOutput<'a> {
    nc: i16,
    bounds: [i16; 4],
    ends: &'a [u16],
    points: &'a [(i16, i16, bool)],
    instructions: &'a [u8],
    overlap: bool,
}
fn write_simple(g: SimpleOutput<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    p16(&mut out, g.nc as u16);
    for v in g.bounds {
        p16(&mut out, v as u16);
    }
    for &end in g.ends {
        p16(&mut out, end);
    }
    p16(&mut out, g.instructions.len() as u16);
    out.extend(g.instructions);
    let mut flags = Vec::with_capacity(g.points.len());
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    for (i, &(px, py, on)) in g.points.iter().enumerate() {
        let mut f = on as u8 | if i == 0 && g.overlap { 64 } else { 0 };
        for (v, out, short, same) in [
            (px as i32 - x, &mut xs, 2, 16),
            (py as i32 - y, &mut ys, 4, 32),
        ] {
            if v == 0 {
                f |= same;
            } else if v.unsigned_abs() <= 255 {
                f |= short;
                if v > 0 {
                    f |= same;
                }
                out.push(v.unsigned_abs() as u8);
            } else {
                p16(out, coord(v)? as u16);
            }
        }
        flags.push(f);
        x = px as i32;
        y = py as i32;
    }
    let mut i = 0;
    while i < flags.len() {
        let f = flags[i];
        let mut count = 1;
        while i + count < flags.len() && count < 256 && flags[i + count] == f {
            count += 1;
        }
        if count > 1 {
            out.extend([f | 8, (count - 1) as u8]);
        } else {
            out.push(f);
        }
        i += count;
    }
    out.extend(xs);
    out.extend(ys);
    Ok(out)
}
pub(super) struct Reconstructed {
    pub glyf: Vec<u8>,
    pub loca: Vec<u8>,
    pub index_format: u16,
}
pub(super) fn decode(data: &[u8], num_glyphs: u16, l: &Limits) -> Result<Reconstructed> {
    let version = u16_at(data, 0)?;
    let options = u16_at(data, 2)?;
    let n = u16_at(data, 4)? as usize;
    let format = u16_at(data, 6)?;
    bounded(n, l.sfnt.resources.max_glyphs, "WOFF2 glyphs")?;
    if version != 0 || options & !1 != 0 || n == 0 || n != num_glyphs as usize || format > 1 {
        return Err(invalid("invalid transformed glyf header"));
    }
    let mut parent = Cursor { data, pos: 36 };
    let mut streams = [Cursor::new(&[]); 7];
    for (i, s) in streams.iter_mut().enumerate() {
        *s = Cursor::new(parent.take(u32_at(data, 8 + i * 4)? as usize)?);
    }
    if streams[0].data.len() != n * 2 {
        return Err(invalid("wrong contour stream length"));
    }
    let overlap = if options & 1 != 0 {
        parent.take(n.div_ceil(8))?
    } else {
        &[]
    };
    parent.end()?;
    let bbox_bits = streams[5].take(n.div_ceil(32) * 4)?;
    let mut glyf = Vec::new();
    let mut offsets = Vec::with_capacity(n + 1);
    let mut budget = Budget::default();
    for gid in 0..n {
        offsets.push(glyf.len());
        let nc = streams[0].word()? as i16;
        let have_bbox = bbox_bits[gid / 8] & (0x80 >> (gid % 8)) != 0;
        if nc == 0 {
            if have_bbox {
                return Err(invalid("empty glyph has explicit bbox"));
            }
            continue;
        }
        let glyph = if nc > 0 {
            let mut ends = Vec::with_capacity(nc as usize);
            let mut count = 0usize;
            for _ in 0..nc {
                let np = streams[1].ushort255()? as usize;
                if np == 0 {
                    return Err(invalid("empty contour"));
                }
                count += np;
                if count > 65536 {
                    return Err(invalid("too many simple points"));
                }
                ends.push((count - 1) as u16);
            }
            budget.points(count, l)?;
            let flags = streams[2].take(count)?;
            let mut points = Vec::with_capacity(count);
            let (mut x, mut y) = (0i32, 0i32);
            for &f in flags {
                let (dx, dy, on) = read_triplet(f, &mut streams[3])?;
                x += dx;
                y += dy;
                points.push((coord(x)?, coord(y)?, on));
            }
            let ni = streams[3].ushort255()? as usize;
            budget.instructions(ni, l)?;
            let instructions = streams[6].take(ni)?;
            let b = if have_bbox {
                bbox(streams[5].take(8)?)?
            } else {
                bounds(&points)
            };
            write_simple(SimpleOutput {
                nc,
                bounds: b,
                ends: &ends,
                points: &points,
                instructions,
                overlap: !overlap.is_empty() && overlap[gid / 8] & (0x80 >> (gid % 8)) != 0,
            })?
        } else if nc == -1 {
            if !have_bbox {
                return Err(invalid("composite missing bbox"));
            }
            let (raw, has_instructions) = components(&mut streams[4], n, &mut budget, l)?;
            let mut glyph = Vec::new();
            p16(&mut glyph, u16::MAX);
            glyph.extend(streams[5].take(8)?);
            glyph.extend(raw);
            if has_instructions {
                let ni = streams[3].ushort255()?;
                budget.instructions(ni as usize, l)?;
                p16(&mut glyph, ni);
                glyph.extend(streams[6].take(ni as usize)?);
            }
            glyph
        } else {
            return Err(invalid("invalid negative contour count"));
        };
        bounded(
            glyf.len().saturating_add(glyph.len()).saturating_add(1),
            l.sfnt
                .resources
                .max_table_bytes
                .min(l.sfnt.resources.max_total_bytes),
            "WOFF2 reconstructed glyf bytes",
        )?;
        glyf.extend(glyph);
        if format == 0 && glyf.len() % 2 != 0 {
            glyf.push(0);
        }
    }
    offsets.push(glyf.len());
    for s in streams {
        s.end()?;
    }
    let mut loca = buffer((n + 1) * if format == 0 { 2 } else { 4 })?;
    for offset in offsets {
        if format == 0 {
            p16(
                &mut loca,
                u16::try_from(offset / 2)
                    .map_err(|_| invalid("reconstructed short loca overflow"))?,
            );
        } else {
            p32(&mut loca, as32(offset)?);
        }
    }
    Ok(Reconstructed {
        glyf,
        loca,
        index_format: format,
    })
}
fn hmtx_lengths(n: usize, h: usize) -> Result<()> {
    if h == 0 || h > n {
        return Err(invalid("invalid numberOfHMetrics"));
    }
    Ok(())
}
pub(super) fn encode_hmtx(data: &[u8], xs: &[i16], h: usize) -> Result<Option<Vec<u8>>> {
    let n = xs.len();
    hmtx_lengths(n, h)?;
    if data.len() != 4 * h + 2 * (n - h) {
        return Err(invalid("incorrect hmtx length"));
    }
    // fontTools' WOFF2 reader recompiles hmtx into its minimum long-metric
    // count without changing the exposed hhea bytes. Keep the legal null
    // transform for nonminimal input counts so every consumer sees all metrics.
    if h > 1 && u16_at(data, 4 * (h - 1))? == u16_at(data, 4 * (h - 2))? {
        return Ok(None);
    }
    let a = (0..h).all(|i| u16_at(data, 4 * i + 2).is_ok_and(|v| v as i16 == xs[i]));
    let b = (h..n).all(|i| u16_at(data, 4 * h + 2 * (i - h)).is_ok_and(|v| v as i16 == xs[i]));
    if !a && !b {
        return Ok(None);
    }
    let mut out = vec![a as u8 | ((b as u8) << 1)];
    for i in 0..h {
        p16(&mut out, u16_at(data, 4 * i)?);
    }
    if !a {
        for i in 0..h {
            p16(&mut out, u16_at(data, 4 * i + 2)?);
        }
    }
    if !b {
        out.extend(&data[4 * h..]);
    }
    Ok(Some(out))
}
pub(super) fn decode_hmtx(data: &[u8], xs: &[i16], h: usize) -> Result<Vec<u8>> {
    let n = xs.len();
    hmtx_lengths(n, h)?;
    let mut r = Cursor::new(data);
    let flags = r.byte()?;
    if flags == 0 || flags & !3 != 0 {
        return Err(invalid("invalid hmtx transform flags"));
    }
    let advances = r.take(h * 2)?;
    let a = if flags & 1 == 0 {
        Some(r.take(h * 2)?)
    } else {
        None
    };
    let b = if flags & 2 == 0 {
        Some(r.take((n - h) * 2)?)
    } else {
        None
    };
    r.end()?;
    let mut out = buffer(h * 4 + (n - h) * 2)?;
    for (i, &x) in xs.iter().enumerate() {
        if i < h {
            p16(&mut out, u16_at(advances, i * 2)?);
            p16(
                &mut out,
                if let Some(a) = a {
                    u16_at(a, i * 2)?
                } else {
                    x as u16
                },
            );
        } else {
            p16(
                &mut out,
                if let Some(b) = b {
                    u16_at(b, (i - h) * 2)?
                } else {
                    x as u16
                },
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triplet_classes_and_boundaries() {
        for flag in 0..=255u8 {
            let length = match flag & 127 {
                0..=83 => 1,
                84..=119 => 2,
                120..=123 => 3,
                _ => 4,
            };
            for value in [0, 1, 15, 16, 63, 127, 255] {
                let bytes = vec![value; length];
                let mut r = Cursor::new(&bytes);
                let (x, y, on) = read_triplet(flag, &mut r).unwrap();
                r.end().unwrap();
                let mut flags = Vec::new();
                let mut out = Vec::new();
                triplet(&mut flags, &mut out, x, y, on);
                assert_eq!(
                    read_triplet(flags[0], &mut Cursor::new(&out)).unwrap(),
                    (x, y, on)
                );
            }
        }
        for x in [
            -65535, -4096, -4095, -1280, -1279, -769, -768, -65, -64, -1, 0, 1, 64, 65, 768, 769,
            1279, 1280, 4095, 4096, 65535,
        ] {
            for y in [-65535, -4096, -65, -1, 0, 1, 64, 65, 768, 769, 4095, 65535] {
                let mut f = Vec::new();
                let mut out = Vec::new();
                triplet(&mut f, &mut out, x, y, true);
                assert_eq!(
                    read_triplet(f[0], &mut Cursor::new(&out)).unwrap(),
                    (x, y, true)
                );
            }
        }
    }
    #[test]
    fn horizontal_metrics_arrays() {
        let xs = [0, -20, 30, 40];
        for mismatch in [0, 1, 2, 3] {
            let lsbs = [
                if mismatch & 1 != 0 { 7 } else { 0 },
                -20,
                if mismatch & 2 != 0 { 9 } else { 30 },
                40,
            ];
            let mut data = Vec::new();
            p16(&mut data, 500);
            p16(&mut data, lsbs[0] as u16);
            p16(&mut data, 600);
            p16(&mut data, lsbs[1] as u16);
            p16(&mut data, lsbs[2] as u16);
            p16(&mut data, lsbs[3] as u16);
            match encode_hmtx(&data, &xs, 2).unwrap() {
                Some(t) => {
                    assert_ne!(mismatch, 3);
                    assert_eq!(decode_hmtx(&t, &xs, 2).unwrap(), data);
                    for cut in 0..t.len() {
                        assert!(decode_hmtx(&t[..cut], &xs, 2).is_err());
                    }
                }
                None => assert_eq!(mismatch, 3),
            }
        }
        for flags in [0, 4, 255] {
            assert!(decode_hmtx(&[flags; 17], &xs, 2).is_err());
        }
        assert!(decode_hmtx(&[3; 17], &xs, 0).is_err());
        assert!(decode_hmtx(&[3; 17], &xs, 5).is_err());
    }
}
