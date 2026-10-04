//! Original interpretation of native RFont control pixels and duplicate PNG tables.
use super::*;
use crate::model::Property;
const COLORS: [[u8; 4]; 8] = [
    [255, 0, 0, 255],
    [255, 128, 0, 255],
    [255, 255, 0, 255],
    [0, 0, 0, 0],
    [0, 255, 0, 255],
    [0, 255, 255, 255],
    [0, 0, 255, 255],
    [128, 0, 255, 255],
];
fn crc(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &v in data {
        c ^= u32::from(v);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !c
}
fn chunks(bytes: &[u8]) -> Result<Vec<(&[u8], &[u8])>> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(Vec::new());
    }
    let mut at = 8;
    let mut out = Vec::new();
    while at < bytes.len() {
        let n = r32(bytes, at)? as usize;
        let kind = slice(bytes, at + 4, 4)?;
        let payload = slice(bytes, at + 8, n)?;
        let expected = r32(bytes, at + 8 + n)?;
        if crc(slice(bytes, at + 4, n + 4)?) != expected {
            return Err(invalid("RFont PNG chunk CRC mismatch"));
        }
        if kind == b"rfVM" || kind == b"rfHM" || kind == b"tEXt" {
            out.push((kind, payload));
        }
        at = at
            .checked_add(n)
            .and_then(|n| n.checked_add(12))
            .ok_or_else(|| invalid("RFont PNG offset overflow"))?;
        if kind == b"IEND" {
            if at != bytes.len() {
                return Err(invalid("trailing RFont PNG bytes"));
            }
            break;
        }
    }
    Ok(out)
}
pub(super) fn table_evidence(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return false;
    }
    let mut pos = 8usize;
    while let Ok(n) = r32(bytes, pos) {
        let Some(end) = pos.checked_add(n as usize).and_then(|p| p.checked_add(12)) else {
            return false;
        };
        if end > bytes.len() {
            return false;
        }
        if &bytes[pos + 4..pos + 8] == b"rfVM" {
            let payload = &bytes[pos + 8..end - 4];
            let native = matches!((n, r16(payload, 0)), (12, Ok(1)) | (22, Ok(2)));
            if native && r32(bytes, end - 4).is_ok_and(|v| v == crc(&bytes[pos + 4..end - 4])) {
                return true;
            }
        }
        if &bytes[pos + 4..pos + 8] == b"IEND" {
            return false;
        }
        pos = end;
    }
    false
}
pub(super) fn recognized(bytes: &[u8], r: &Raster) -> bool {
    let mut pos = 8;
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        while let Ok(n) = r32(bytes, pos) {
            if slice(bytes, pos + 4, 4).is_ok_and(|v| v == b"rfVM") {
                return true;
            }
            let Some(next) = pos.checked_add(n as usize).and_then(|n| n.checked_add(12)) else {
                break;
            };
            if next <= pos || next >= bytes.len() {
                break;
            }
            pos = next;
        }
    }
    r.width >= 3
        && r.height >= 2
        && at(r, 0, 0) == PINK
        && at(r, 1, 0) == PINK
        && at(r, 1, 1)[3] == 255
        && (1..r.height).any(|y| {
            COLORS
                .iter()
                .enumerate()
                .any(|(i, &c)| i != 3 && at(r, 0, y) == c)
        })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vm {
    max_a: u32,
    line_a: u32,
    em_a: u32,
    x: u32,
    em_d: u32,
    line_d: u32,
    max_d: u32,
    gap: u32,
}
impl Vm {
    fn height(self) -> Result<u32> {
        self.max_a
            .checked_add(self.max_d)
            .and_then(|v| v.checked_add(self.gap))
            .ok_or_else(|| invalid("RFont height overflow"))
    }
    fn counts(self) -> [u32; 8] {
        [
            self.max_a - self.line_a,
            self.line_a - self.em_a,
            self.em_a - self.x,
            self.x,
            self.em_d,
            self.line_d - self.em_d,
            self.max_d - self.line_d,
            self.gap,
        ]
    }
    fn fields(self) -> Result<[u32; 10]> {
        Ok([
            self.max_a,
            self.line_a,
            self.em_a,
            self.x,
            self.em_d,
            self.line_d,
            self.max_d,
            self.gap,
            self.max_a.wrapping_sub(1),
            self.height()?,
        ])
    }
}
fn vm(r: &Raster, start: u32, end: u32) -> Result<Vm> {
    if at(r, 0, start) != PINK {
        return Err(invalid("RFont strip lacks vertical control header"));
    }
    let mut counts = [0u32; 8];
    let mut phase = 0;
    for y in start + 1..end {
        let p = at(r, 0, y);
        while phase < 8 && !(p == COLORS[phase] || phase == 3 && p[3] == 0) {
            phase += 1;
        }
        if phase == 8 {
            return Err(invalid("invalid RFont vertical metric color order"));
        }
        counts[phase] += 1;
    }
    let max_a = counts[..4].iter().sum();
    let line_a = max_a - counts[0];
    let em_a = line_a - counts[1];
    Ok(Vm {
        max_a,
        line_a,
        em_a,
        x: counts[3],
        em_d: counts[4],
        line_d: counts[4] + counts[5],
        max_d: counts[4] + counts[5] + counts[6],
        gap: counts[7],
    })
}
fn code_colors(cp: u32, h: u32, mut f: impl FnMut(u32, [u8; 4])) {
    let mut row = 0;
    let mut repetition = 1;
    while row < h {
        for block in 0..repetition {
            if row == h {
                return;
            }
            let mut rgb = 0u32;
            for nibble in 0..6 {
                let index = (block * 6 + nibble) / repetition;
                let digit = (cp >> (4 * (5 - index))) & 15;
                rgb = (rgb << 4) | digit;
            }
            f(row, [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
            row += 1;
        }
        repetition += 1;
    }
}
fn code(r: &Raster, x: u32, y: u32, h: u32) -> Result<u32> {
    if at(r, x, y) != PINK || h == 0 {
        return Err(invalid("invalid RFont codepoint header"));
    }
    let p = at(r, x, y + 1);
    if p[3] != 255 {
        return Err(invalid("transparent RFont codepoint"));
    }
    let cp = (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
    let mut matches = true;
    code_colors(cp, h, |row, c| {
        matches &= at(r, x, y + 1 + row) == c;
    });
    if !matches {
        return Err(invalid("inconsistent RFont codepoint redundancy"));
    }
    Ok(cp)
}
#[derive(Clone, Copy)]
struct Hm {
    cp: u32,
    left: u32,
    advance: u32,
    right: u32,
    width: u32,
    x: u32,
    y: u32,
}
fn aux_vm(data: &[u8], v: Vm) -> Result<u16> {
    let version = r16(data, 0)?;
    let size = match version {
        1 => 1,
        2 => 2,
        _ => return Err(invalid("unsupported RFont rfVM version")),
    };
    if data.len() != 2 + 10 * size {
        return Err(invalid("invalid RFont rfVM length"));
    }
    for (i, n) in v.fields()?.into_iter().enumerate() {
        let actual = if size == 1 {
            u32::from(data[2 + i])
        } else {
            u32::from(r16(data, 2 + 2 * i)?)
        };
        let expected = if i == 8 && v.max_a == 0 {
            if size == 1 { 255 } else { 65535 }
        } else {
            n
        };
        if actual != expected {
            return Err(invalid("RFont rfVM disagrees with metric pixels"));
        }
    }
    Ok(version)
}
fn aux_hm(data: &[u8], records: &BTreeMap<u32, Hm>) -> Result<u16> {
    let version = r16(data, 0)?;
    let stride = match version {
        1 => 8,
        2 => 16,
        _ => return Err(invalid("unsupported RFont rfHM version")),
    };
    let ranges = r32(data, 2)? as usize;
    if ranges > records.len() {
        return Err(invalid("RFont rfHM range count exceeds glyphs"));
    }
    let mut pos = 6;
    let mut matched = 0;
    let mut last = None;
    for _ in 0..ranges {
        let start = r32(data, pos)?;
        let count = r32(data, pos + 4)?;
        pos += 8;
        if count == 0
            || count as usize > records.len() - matched
            || last.is_some_and(|n| start <= n)
        {
            return Err(invalid("invalid RFont rfHM range"));
        }
        for i in 0..count {
            let cp = start
                .checked_add(i)
                .ok_or_else(|| invalid("RFont rfHM codepoint overflow"))?;
            let r = records
                .get(&cp)
                .ok_or_else(|| invalid("RFont rfHM names absent glyph"))?;
            let d = slice(data, pos, stride)?;
            let mut values = [0u32; 6];
            for (j, val) in values.iter_mut().enumerate() {
                *val = if version == 1 {
                    if j < 4 {
                        u32::from(d[j])
                    } else {
                        u32::from(r16(d, 4 + (j - 4) * 2)?)
                    }
                } else if j < 4 {
                    u32::from(r16(d, j * 2)?)
                } else {
                    r32(d, 8 + (j - 4) * 4)?
                };
            }
            if values != [r.left, r.advance, r.right, r.width, r.x, r.y] {
                return Err(invalid("RFont rfHM disagrees with control pixels"));
            }
            pos += stride;
            matched += 1;
            last = Some(cp);
        }
    }
    if matched != records.len() || pos != data.len() {
        return Err(invalid("incomplete RFont rfHM table"));
    }
    Ok(version)
}
pub(super) fn read(
    bytes: &[u8],
    r: Raster,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    if r.width < 3 || r.height < 2 {
        return Err(invalid("RFont image too small"));
    }
    b.add(bytes.len().saturating_mul(4))?;
    let mut starts = vec![0];
    for y in 1..r.height {
        if at(&r, 0, y) == PINK {
            b.add(16)?;
            starts.push(y);
        }
    }
    starts.push(r.height);
    let first = vm(&r, 0, starts[1])?;
    let h = first.height()?;
    if h == 0 {
        return Err(invalid("RFont has no bitmap rows"));
    }
    let mut f = Font::default();
    f.metrics.ascent = Some(i64::from(first.line_a));
    f.metrics.descent = Some(i64::from(first.line_d));
    f.metrics.line_gap = Some(i64::from(first.gap));
    f.metrics.line_height = Some(i64::from(first.line_a + first.line_d + first.gap));
    f.metrics.pixel_size = Some(first.em_a + first.em_d);
    for (name, value) in [
        ("max_ascent", first.max_a),
        ("max_descent", first.max_d),
        ("em_ascent", first.em_a),
        ("em_descent", first.em_d),
        ("x_height", first.x),
    ] {
        f.properties
            .push(Property::new(format!("rfont.{name}"), i64::from(value)));
    }
    b.add(4096)?;
    let mut records = BTreeMap::new();
    let mut colored = false;
    for row in starts.windows(2) {
        if vm(&r, row[0], row[1])? != first || row[1] - row[0] != h + 1 {
            return Err(invalid("RFont strips have inconsistent vertical metrics"));
        }
        let y = row[0];
        let mut x = 1;
        loop {
            if x >= r.width {
                return Err(invalid("RFont strip lacks terminator"));
            }
            let cp = code(&r, x, y, h)?;
            if cp == 0xffffff {
                for yy in y..row[1] {
                    for xx in x + 1..r.width {
                        if at(&r, xx, yy)[3] != 0 {
                            return Err(invalid("visible pixels after RFont strip terminator"));
                        }
                    }
                }
                break;
            }
            if char::from_u32(cp).is_none() || records.contains_key(&cp) {
                return Err(invalid("invalid or duplicate RFont Unicode scalar"));
            }
            check(records.len() + 1, l.max_glyphs, "RFont glyph count")?;
            b.add(1024)?;
            let left_start = x + 1;
            let mut end = left_start;
            while end < r.width && at(&r, end, y) != PINK {
                end += 1;
            }
            if end == r.width {
                return Err(invalid("RFont glyph lacks closing control column"));
            }
            let mut a = left_start;
            while a < end && at(&r, a, y) == COLORS[0] {
                a += 1;
            }
            let left = a - left_start;
            let mut z = a;
            while z < end && at(&r, z, y)[3] == 0 {
                z += 1;
            }
            let advance = z - a;
            let mut e = z;
            while e < end && at(&r, e, y) == COLORS[6] {
                e += 1;
            }
            if e != end {
                return Err(invalid("invalid RFont horizontal control colors"));
            }
            let right = end - z;
            let w = end - left_start;
            let n = area(w, h)?;
            check(n, l.max_pixels_per_glyph, "RFont glyph pixels")?;
            b.add(n)?;
            let mut pixels = Vec::with_capacity(n);
            for yy in y + 1..row[1] {
                for xx in left_start..end {
                    let p = at(&r, xx, yy);
                    colored |= p[3] != 0 && (p[0] != p[1] || p[1] != p[2]);
                    pixels.push(p[3]);
                }
            }
            let mut g = glyph(
                w,
                h,
                -i64::from(left),
                i64::from(first.max_a) - i64::from(h),
                Vector::new(i64::from(advance), 0),
                &pixels,
                false,
                l,
                b,
            )?;
            g.codepoints.push(Codepoint::Unicode(cp));
            f.add_glyph(g)?;
            records.insert(
                cp,
                Hm {
                    cp,
                    left,
                    advance,
                    right,
                    width: w,
                    x: left_start,
                    y: y + 1,
                },
            );
            x = end;
        }
    }
    if records.is_empty() {
        return Err(invalid("RFont has no glyphs"));
    }
    let mut seen_vm = false;
    let mut seen_hm = false;
    for (kind, data) in chunks(bytes)? {
        match kind {
            b"rfVM" => {
                if seen_vm {
                    return Err(invalid("duplicate RFont rfVM"));
                }
                seen_vm = true;
                f.properties.push(Property::new(
                    "rfont.vm_version",
                    i64::from(aux_vm(data, first)?),
                ));
            }
            b"rfHM" => {
                if seen_hm {
                    return Err(invalid("duplicate RFont rfHM"));
                }
                seen_hm = true;
                f.properties.push(Property::new(
                    "rfont.hm_version",
                    i64::from(aux_hm(data, &records)?),
                ));
            }
            b"tEXt" => apply_text(data, &mut f, "rfont", b)?,
            _ => unreachable!(),
        }
    }
    let mut report = Report::default();
    if colored {
        b.diagnostic(
            &mut report,
            "sfont",
            "read.color",
            crate::report::Impact::Loss,
            Some(0),
            None,
            "RFont color was projected to alpha coverage",
        )?;
    }
    if r.color.bits_per_pixel() > r.color.channel_count() as u16 * 8 {
        b.diagnostic(
            &mut report,
            "sfont",
            "read.depth",
            crate::report::Impact::Loss,
            Some(0),
            None,
            "Image samples were reduced to 8-bit coverage",
        )?;
    }
    Ok((set(f, "sfont"), report))
}
fn prop(f: &Font, key: &str, default: i64) -> Result<u32> {
    u32::try_from(int(&f.properties, key, default)?).map_err(|_| invalid(format!("invalid {key}")))
}
fn vm_from(f: &Font, selection: &BTreeMap<u32, &Glyph>) -> Result<Vm> {
    let mut max_a = 0i64;
    let mut max_d = 0i64;
    for g in selection.values() {
        max_a = max_a.max(
            g.metrics
                .bounds
                .y
                .checked_add(i64::from(g.bitmap.height()))
                .ok_or_else(|| invalid("RFont ascent overflow"))?,
        );
        max_d = max_d.max(
            g.metrics
                .bounds
                .y
                .checked_neg()
                .ok_or_else(|| invalid("RFont descent overflow"))?,
        );
    }
    let gap = u32::try_from(f.metrics.line_gap.unwrap_or(0))
        .map_err(|_| invalid("RFont negative line gap"))?;
    max_d = (max_d - i64::from(gap)).max(0);
    let line_a = u32::try_from(f.metrics.ascent.unwrap_or(max_a))
        .map_err(|_| invalid("RFont negative ascent"))?;
    let line_d = u32::try_from(f.metrics.descent.unwrap_or(max_d))
        .map_err(|_| invalid("RFont negative descent"))?;
    let max_a = prop(f, "rfont.max_ascent", max_a.max(i64::from(line_a)))?
        .max(u32::try_from(max_a).map_err(|_| invalid("RFont ascent overflow"))?);
    let max_d = prop(f, "rfont.max_descent", max_d.max(i64::from(line_d)))?
        .max(u32::try_from(max_d).map_err(|_| invalid("RFont descent overflow"))?);
    let em_a = prop(f, "rfont.em_ascent", i64::from(line_a))?;
    let em_d = prop(f, "rfont.em_descent", i64::from(line_d))?;
    let x = prop(f, "rfont.x_height", i64::from(em_a))?;
    if max_a < line_a || line_a < em_a || em_a < x || max_d < line_d || line_d < em_d {
        return Err(invalid("RFont metric color bands must be nested"));
    }
    let vm = Vm {
        max_a,
        line_a,
        em_a,
        x,
        em_d,
        line_d,
        max_d,
        gap,
    };
    let h = vm.height()?;
    if h == 0 || h > 65535 {
        return Err(invalid("RFont bitmap height must fit native rfVM u16"));
    }
    Ok(vm)
}
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let checksum = crc(&out[start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}
fn table_vm(v: Vm, version: u16) -> Result<Vec<u8>> {
    let mut out = version.to_be_bytes().to_vec();
    for (i, n) in v.fields()?.into_iter().enumerate() {
        let n = if i == 8 && v.max_a == 0 {
            if version == 1 { 255 } else { 65535 }
        } else {
            n
        };
        if version == 1 {
            out.push(u8::try_from(n).map_err(|_| invalid("rfVM v1 field overflow"))?);
        } else {
            out.extend_from_slice(
                &u16::try_from(n)
                    .map_err(|_| invalid("rfVM v2 field overflow"))?
                    .to_be_bytes(),
            );
        }
    }
    Ok(out)
}
fn table_hm(records: &[Hm], version: u16) -> Result<Vec<u8>> {
    let ranges = records
        .iter()
        .enumerate()
        .filter(|(i, r)| *i == 0 || records[i - 1].cp + 1 != r.cp)
        .count();
    let mut out = version.to_be_bytes().to_vec();
    out.extend_from_slice(&(ranges as u32).to_be_bytes());
    let mut at = 0;
    while at < records.len() {
        let mut end = at + 1;
        while end < records.len() && records[end].cp == records[end - 1].cp + 1 {
            end += 1;
        }
        out.extend_from_slice(&records[at].cp.to_be_bytes());
        out.extend_from_slice(&((end - at) as u32).to_be_bytes());
        for r in &records[at..end] {
            for n in [r.left, r.advance, r.right, r.width] {
                if version == 1 {
                    out.push(u8::try_from(n).map_err(|_| invalid("rfHM v1 field overflow"))?);
                } else {
                    out.extend_from_slice(
                        &u16::try_from(n)
                            .map_err(|_| invalid("rfHM v2 field overflow"))?
                            .to_be_bytes(),
                    );
                }
            }
            for n in [r.x, r.y] {
                if version == 1 {
                    out.extend_from_slice(
                        &u16::try_from(n)
                            .map_err(|_| invalid("rfHM v1 coordinate overflow"))?
                            .to_be_bytes(),
                    );
                } else {
                    out.extend_from_slice(&n.to_be_bytes());
                }
            }
        }
        at = end;
    }
    Ok(out)
}
pub(super) fn write(f: &Font, o: &SfontWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let selection = select(f, true, 0x10ffff, false, b)?;
    let vm = vm_from(f, &selection)?;
    let h = vm.height()?;
    let bh = h + 1;
    b.items::<Hm>(selection.len())?;
    b.add(selection.len().saturating_mul(128))?;
    let mut records = Vec::with_capacity(selection.len());
    let mut last_page = None;
    let mut x = 1u32;
    let mut y = 0u32;
    let mut width = 0u32;
    for (&cp, &g) in &selection {
        let page = cp >> 8;
        if last_page != Some(page) {
            if last_page.is_some() {
                width = width.max(x + 1);
                y = y
                    .checked_add(bh)
                    .ok_or_else(|| invalid("RFont image height overflow"))?;
                x = 1;
            }
            last_page = Some(page);
        }
        let left = u32::try_from(
            g.metrics
                .bounds
                .x
                .checked_neg()
                .ok_or_else(|| invalid("RFont bearing overflow"))?
                .max(0),
        )
        .map_err(|_| invalid("RFont bearing overflow"))?;
        let advance = u32::try_from(
            g.metrics
                .advance
                .map_or(i64::from(g.bitmap.width()), |v| v.x),
        )
        .map_err(|_| invalid("RFont negative advance"))?;
        let right = u32::try_from(
            g.metrics
                .bounds
                .x
                .checked_add(i64::from(g.bitmap.width()))
                .and_then(|v| v.checked_sub(i64::from(advance)))
                .ok_or_else(|| invalid("RFont bearing overflow"))?
                .max(0),
        )
        .map_err(|_| invalid("RFont bearing overflow"))?;
        let w = left
            .checked_add(advance)
            .and_then(|v| v.checked_add(right))
            .ok_or_else(|| invalid("RFont glyph width overflow"))?;
        if w > 65535 {
            return Err(invalid("RFont glyph width exceeds rfHM u16"));
        }
        records.push(Hm {
            cp,
            left,
            advance,
            right,
            width: w,
            x: x + 1,
            y: y + 1,
        });
        x = x
            .checked_add(w)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("RFont strip width overflow"))?;
    }
    width = width.max(
        x.checked_add(1)
            .ok_or_else(|| invalid("RFont strip width overflow"))?,
    );
    let height = y
        .checked_add(bh)
        .ok_or_else(|| invalid("RFont image height overflow"))?;
    let n = area(width, height)?;
    check(
        n.saturating_mul(4),
        o.limits.max_decompressed_bytes,
        "RFont decoded image bytes",
    )?;
    b.add(n.saturating_mul(4))?;
    let mut pixels = vec![0; n * 4];
    let mut yy = 0;
    while yy < height {
        put(&mut pixels, width, 0, yy, PINK);
        let mut row = yy + 1;
        for (count, color) in vm.counts().into_iter().zip(COLORS) {
            for _ in 0..count {
                put(&mut pixels, width, 0, row, color);
                row += 1;
            }
        }
        yy += bh;
    }
    for (i, r) in records.iter().enumerate() {
        let g = selection[&r.cp];
        let control = r.x - 1;
        let row = r.y - 1;
        put(&mut pixels, width, control, row, PINK);
        code_colors(r.cp, h, |dy, c| {
            put(&mut pixels, width, control, r.y + dy, c)
        });
        for dx in 0..r.left {
            put(&mut pixels, width, r.x + dx, row, COLORS[0]);
        }
        for dx in 0..r.right {
            put(
                &mut pixels,
                width,
                r.x + r.left + r.advance + dx,
                row,
                COLORS[6],
            );
        }
        let gx = i64::from(r.x) + i64::from(r.left) + g.metrics.bounds.x;
        let gy = i64::from(r.y) + i64::from(vm.max_a)
            - g.metrics.bounds.y
            - i64::from(g.bitmap.height());
        for py in 0..if g.bitmap.width() == 0 {
            0
        } else {
            g.bitmap.height()
        } {
            for px in 0..g.bitmap.width() {
                let xx = u32::try_from(gx + i64::from(px))
                    .map_err(|_| invalid("RFont image placement overflow"))?;
                let yy = u32::try_from(gy + i64::from(py))
                    .map_err(|_| invalid("RFont image placement overflow"))?;
                if xx >= r.x + r.width || yy >= r.y + h {
                    return Err(invalid("RFont glyph outside native cell"));
                }
                put(
                    &mut pixels,
                    width,
                    xx,
                    yy,
                    [255, 255, 255, pixel(g, px, py)],
                );
            }
        }
        if i + 1 == records.len() || records[i + 1].y != r.y {
            let xx = r.x + r.width;
            put(&mut pixels, width, xx, row, PINK);
            code_colors(0xffffff, h, |dy, c| {
                put(&mut pixels, width, xx, r.y + dy, c)
            });
        }
    }
    let base = png(
        width,
        height,
        &pixels,
        true,
        o.max_output_bytes,
        &o.limits,
        b,
    )?;
    let default_vm = if h < 256 { 1 } else { 2 };
    let vm_version = u16::try_from(prop(f, "rfont.vm_version", default_vm)?)
        .map_err(|_| invalid("rfVM version overflow"))?;
    if !matches!(vm_version, 1 | 2) {
        return Err(invalid("unsupported rfVM version"));
    }
    let default_hm = if records.iter().all(|r| r.width < 256) && width < 65536 && height < 65536 {
        1
    } else {
        2
    };
    let hm_version = u16::try_from(prop(f, "rfont.hm_version", default_hm)?)
        .map_err(|_| invalid("rfHM version overflow"))?;
    if !matches!(hm_version, 1 | 2) {
        return Err(invalid("unsupported rfHM version"));
    }
    b.add(records.len().saturating_mul(32).saturating_add(4096))?;
    let vmt = table_vm(vm, vm_version)?;
    let hmt = table_hm(&records, hm_version)?;
    let text = collect_text(f, "rfont", b)?;
    let extra = vmt.len() + hmt.len() + 24 + text.iter().map(|v| v.len() + 12).sum::<usize>();
    let len = base
        .len()
        .checked_add(extra)
        .ok_or_else(|| invalid("RFont PNG length overflow"))?;
    let mut out = output(len, o.max_output_bytes, &o.limits, b)?;
    out.clear();
    let end = base
        .len()
        .checked_sub(12)
        .ok_or_else(|| invalid("RFont PNG lacks end chunk"))?;
    out.extend_from_slice(&base[..end]);
    for data in text {
        chunk(&mut out, b"tEXt", &data);
    }
    chunk(&mut out, b"rfVM", &vmt);
    chunk(&mut out, b"rfHM", &hmt);
    out.extend_from_slice(&base[end..]);
    Ok(out)
}

fn valid_text(data: &[u8]) -> bool {
    data.iter()
        .position(|&c| c == 0)
        .is_some_and(|i| i > 0 && i <= 79 && !data[i + 1..].contains(&0))
}
fn apply_text(data: &[u8], f: &mut Font, prefix: &str, b: &mut Budget) -> Result<()> {
    if !valid_text(data) {
        return Err(invalid("invalid native PNG text"));
    }
    let sep = data.iter().position(|&c| c == 0).unwrap();
    let (key, value) = (&data[..sep], &data[sep + 1..]);
    b.add(data.len().saturating_mul(4).saturating_add(256))?;
    let field = match key {
        b"FontFamily" => Some(&mut f.names.family),
        b"FontStyle" => Some(&mut f.names.style),
        b"Copyright" => Some(&mut f.names.copyright),
        _ => None,
    };
    if let Some(field) = field {
        if field.is_some() {
            return Err(invalid("duplicate native PNG font name"));
        }
        *field = Some(value.iter().map(|&c| char::from(c)).collect());
    } else {
        f.properties.push(Property::new(
            format!("{prefix}.png_text"),
            crate::model::PropertyValue::Bytes(data.to_vec()),
        ));
    }
    Ok(())
}
pub(super) fn read_text(bytes: &[u8], f: &mut Font, b: &mut Budget) -> Result<()> {
    b.add(bytes.len().saturating_mul(4))?;
    for (kind, data) in chunks(bytes)? {
        if kind == b"tEXt" {
            apply_text(data, f, "sfont", b)?;
        }
    }
    Ok(())
}
fn collect_text(f: &Font, prefix: &str, b: &mut Budget) -> Result<Vec<Vec<u8>>> {
    let mut text = Vec::new();
    for (key, value) in [
        ("FontFamily", &f.names.family),
        ("FontStyle", &f.names.style),
        ("Copyright", &f.names.copyright),
    ] {
        if let Some(value) = value
            && value.chars().all(|c| u32::from(c) <= 255 && c != '\0')
        {
            b.add(value.len().saturating_mul(4).saturating_add(128))?;
            let mut data = key.as_bytes().to_vec();
            data.push(0);
            data.extend(value.chars().map(|c| c as u8));
            text.push(data);
        }
    }
    for p in f
        .properties
        .iter()
        .filter(|p| p.name == format!("{prefix}.png_text"))
    {
        if let crate::model::PropertyValue::Bytes(data) = &p.value {
            if !valid_text(data) {
                return Err(invalid("invalid native RFont PNG text property"));
            }
            b.add(data.len().saturating_mul(2))?;
            text.push(data.clone());
        }
    }
    Ok(text)
}
pub(super) fn add_text(
    base: Vec<u8>,
    f: &Font,
    o: &SfontWriteOptions,
    b: &mut Budget,
) -> Result<Vec<u8>> {
    let text = collect_text(f, "sfont", b)?;
    if text.is_empty() {
        return Ok(base);
    }
    let extra = text
        .iter()
        .try_fold(0usize, |n, t| {
            n.checked_add(t.len()).and_then(|n| n.checked_add(12))
        })
        .ok_or_else(|| invalid("SFont text length overflow"))?;
    let len = base
        .len()
        .checked_add(extra)
        .ok_or_else(|| invalid("SFont PNG length overflow"))?;
    let mut out = output(len, o.max_output_bytes, &o.limits, b)?;
    out.clear();
    let end = base.len() - 12;
    out.extend_from_slice(&base[..end]);
    for data in text {
        chunk(&mut out, b"tEXt", &data);
    }
    out.extend_from_slice(&base[end..]);
    Ok(out)
}
