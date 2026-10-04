//! Bounded OS/2 NE segments and LX pages, including EXEPACK1 and EXEPACK2.
use super::*;
pub(super) fn kind(b: &[u8]) -> Result<(GpiContainer, usize)> {
    let base = if b.starts_with(b"MZ") {
        u32v(b, 60)? as usize
    } else {
        0
    };
    match slice(b, base, 2)? {
        b"NE" => {
            slice(b, base, 64)?;
            if b[base + 54] != 1 {
                return Err(invalid("NE module is not OS/2"));
            }
            Ok((GpiContainer::Ne, base))
        }
        b"LX" => {
            slice(b, base, 196)?;
            if b[base + 2] != 0 || b[base + 3] != 0 {
                return Err(invalid("unsupported LX byte order"));
            }
            Ok((GpiContainer::Lx, base))
        }
        _ => Err(invalid("not an OS/2 NE/LX module")),
    }
}
type FontResources = Vec<(u16, Vec<u8>)>;
struct Resource {
    kind: u16,
    id: u16,
    data: Vec<u8>,
}
pub(super) fn extract(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(GpiContainer, FontResources)> {
    let (pack, base) = kind(b)?;
    let records = if pack == GpiContainer::Ne {
        ne(b, base, l, budget)?
    } else {
        lx(b, base, l, budget)?
    };
    let mut ids = BTreeSet::new();
    for r in &records {
        if r.kind == 6 {
            let h = usize::from(u16v(&r.data, 0)?);
            let n = usize::from(u16v(&r.data, 2)?);
            let entry = usize::from(u16v(&r.data, 4)?);
            // Native directories report the metric size; headers in existing modules use 6.
            let stride = if entry >= 170 {
                entry
            } else if r.data.len().saturating_sub(h) >= n.saturating_mul(182) {
                182
            } else {
                170
            };
            if h < 6 {
                return Err(invalid("invalid GPI directory header length"));
            }
            slice(&r.data, h, n.saturating_mul(stride))?;
            for i in 0..n {
                ids.insert(u16v(&r.data, h + i * stride)?);
            }
        }
    }
    let mut out = Vec::new();
    for r in records {
        if r.kind != 6 && (r.kind == 7 || ids.contains(&r.id)) {
            check(out.len() + 1, l.max_resources, "GPI font resources")?;
            out.push((r.id, r.data));
        }
    }
    Ok((pack, out))
}
fn ne(b: &[u8], base: usize, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Resource>> {
    let n = usize::from(u16v(b, base + 28)?);
    let nr = usize::from(u16v(b, base + 52)?);
    let shift = u32::from(u16v(b, base + 50)?);
    if nr > n || shift > 31 {
        return Err(invalid("invalid OS/2 NE segment counts or alignment"));
    }
    check(
        nr,
        l.max_resources.saturating_add(1),
        "OS/2 resource entries",
    )?;
    budget.items::<Resource>(nr)?;
    let segments = base + usize::from(u16v(b, base + 34)?);
    let resources = base + usize::from(u16v(b, base + 36)?);
    slice(b, segments, n * 8)?;
    slice(b, resources, nr * 4)?;
    let mut out = Vec::new();
    for i in 0..nr {
        let sp = segments + (n - nr + i) * 8;
        let offset = usize::from(u16v(b, sp)?)
            .checked_shl(shift)
            .ok_or_else(|| invalid("NE segment offset overflow"))?;
        let size = match u16v(b, sp + 2)? {
            0 => 65536,
            v => usize::from(v),
        };
        let data = slice(b, offset, size)?;
        budget.add(size.saturating_mul(2))?;
        out.push(Resource {
            kind: u16v(b, resources + i * 4)?,
            id: u16v(b, resources + i * 4 + 2)?,
            data: data.to_vec(),
        });
    }
    Ok(out)
}
fn lx(b: &[u8], base: usize, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<Resource>> {
    let page_size = u32v(b, base + 40)? as usize;
    let shift = u32v(b, base + 44)?;
    if page_size != 4096 || shift > 31 {
        return Err(invalid(
            "LX GPI supports native 4096-byte pages and bounded alignment",
        ));
    }
    let objects = base + u32v(b, base + 64)? as usize;
    let object_count = u32v(b, base + 68)? as usize;
    let pages = base + u32v(b, base + 72)? as usize;
    let page_count = u32v(b, base + 20)? as usize;
    let resources = base + u32v(b, base + 80)? as usize;
    let nr = u32v(b, base + 84)? as usize;
    let datapage = u32v(b, base + 128)? as usize;
    check(nr, l.max_resources.saturating_add(1), "LX resource entries")?;
    check(object_count, l.max_table_bytes / 24, "LX objects")?;
    slice(b, objects, object_count.saturating_mul(24))?;
    slice(b, pages, page_count.saturating_mul(8))?;
    slice(b, resources, nr.saturating_mul(14))?;
    budget.items::<Resource>(nr)?;
    let mut out = Vec::new();
    let mut decoded: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for i in 0..nr {
        let p = resources + i * 14;
        let object = usize::from(u16v(b, p + 8)?);
        if object == 0 || object > object_count {
            return Err(invalid("LX resource object index outside table"));
        }
        let data = if let Some(data) = decoded.get(&object) {
            data
        } else {
            let op = objects + (object - 1) * 24;
            let size = u32v(b, op)? as usize;
            let start = u32v(b, op + 12)? as usize;
            let count = u32v(b, op + 16)? as usize;
            if start == 0
                || start - 1 + count > page_count
                || count.saturating_mul(page_size) < size
            {
                return Err(invalid("LX object page range invalid"));
            }
            check(size, l.max_table_bytes, "LX expanded object bytes")?;
            budget.add(size.saturating_mul(2).saturating_add(8192))?;
            let mut obj = vec![0; size];
            for j in 0..count {
                let pp = pages + (start - 1 + j) * 8;
                let offset = (u32v(b, pp)? as usize)
                    .checked_shl(shift)
                    .and_then(|v| v.checked_add(datapage))
                    .ok_or_else(|| invalid("LX page offset overflow"))?;
                let len = usize::from(u16v(b, pp + 4)?);
                let flags = u16v(b, pp + 6)?;
                let dst = j * page_size;
                if dst >= size {
                    break;
                }
                let needed = page_size.min(size - dst);
                match flags {
                    0 => {
                        let data = slice(b, offset, len)?;
                        if len > page_size {
                            return Err(invalid("LX page exceeds page size"));
                        }
                        let n = needed.min(len);
                        obj[dst..dst + n].copy_from_slice(&data[..n]);
                    }
                    1 | 5 => {
                        let data = unpack(slice(b, offset, len)?, flags)?;
                        let n = needed.min(data.len());
                        obj[dst..dst + n].copy_from_slice(&data[..n]);
                    }
                    3 => {}
                    _ => return Err(invalid("LX resource uses invalid/range page mapping")),
                }
            }
            decoded.insert(object, obj);
            decoded.get(&object).unwrap()
        };
        let offset = u32v(b, p + 10)? as usize;
        let size = u32v(b, p + 4)? as usize;
        let data = slice(data, offset, size)?;
        budget.add(size.saturating_mul(2))?;
        out.push(Resource {
            kind: u16v(b, p)?,
            id: u16v(b, p + 2)?,
            data: data.to_vec(),
        });
    }
    Ok(out)
}
fn unpack(b: &[u8], kind: u16) -> Result<Vec<u8>> {
    if b.len() > 4096 {
        return Err(invalid("compressed LX page exceeds page size"));
    }
    let mut out = Vec::with_capacity(4096);
    let mut p = 0;
    while p < b.len() {
        if kind == 1 {
            let repeat = usize::from(u16v(b, p)?);
            if repeat == 0 {
                break;
            }
            let len = usize::from(u16v(b, p + 2)?);
            p += 4;
            let run = slice(b, p, len)?;
            p += len;
            check(
                out.len().saturating_add(repeat.saturating_mul(len)),
                4096,
                "LX expanded page",
            )?;
            if len != 0 {
                for _ in 0..repeat {
                    out.extend_from_slice(run);
                }
            }
            continue;
        }
        let control = usize::from(u16v(b, p)?);
        let tag = control & 3;
        let (literal, copy, distance, header) = match tag {
            0 => {
                if control & 255 == 0 {
                    let n = control >> 8;
                    if n == 0 {
                        break;
                    }
                    let v = slice(b, p + 2, 1)?[0];
                    check(out.len() + n, 4096, "LX expanded page")?;
                    out.resize(out.len() + n, v);
                    p += 3;
                    continue;
                }
                (control >> 2 & 63, 0, 0, 1)
            }
            1 => (control >> 2 & 3, (control >> 4 & 7) + 3, control >> 7, 2),
            2 => (0, (control >> 2 & 3) + 3, control >> 4, 2),
            _ => {
                let v = slice(b, p, 3)?;
                let c = control | usize::from(v[2]) << 16;
                (c >> 2 & 15, c >> 6 & 63, c >> 12 & 4095, 3)
            }
        };
        check(
            out.len().saturating_add(literal).saturating_add(copy),
            4096,
            "LX expanded page",
        )?;
        out.extend_from_slice(slice(b, p + header, literal)?);
        p += header + literal;
        if copy > 0 {
            if distance == 0 || distance > out.len() {
                return Err(invalid("LX back-reference precedes output"));
            }
            for _ in 0..copy {
                out.push(out[out.len() - distance]);
            }
        }
    }
    Ok(out)
}
pub(super) fn wrap(
    resources: &[(u16, Vec<u8>)],
    pack: GpiContainer,
    cap: usize,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    if pack == GpiContainer::Bare {
        if resources.len() != 1 {
            return Err(invalid("bare GPI requires one selected strike"));
        }
        budget.add(resources[0].1.len())?;
        return Ok(resources[0].1.clone());
    }
    // Native font libraries carry a type-6 directory before their type-7 faces.
    // The directory mirrors FOCA metrics and gives consumers stable resource IDs.
    let directory_len = 6usize.saturating_add(resources.len().saturating_mul(182));
    check(directory_len, cap, "GPI font directory bytes")?;
    budget.add(directory_len.saturating_mul(2))?;
    let mut directory = vec![0; directory_len];
    p16(&mut directory, 0, 6);
    p16(
        &mut directory,
        2,
        u16::try_from(resources.len()).map_err(|_| invalid("too many GPI directory faces"))?,
    );
    p16(&mut directory, 4, 168);
    for (i, (id, raw)) in resources.iter().enumerate() {
        if *id == 0 {
            return Err(invalid("GPI library face resource IDs must be nonzero"));
        }
        p16(&mut directory, 6 + i * 182, *id);
        directory[8 + i * 182..176 + i * 182].copy_from_slice(slice(raw, 20, 168)?);
    }
    budget.add(resources.iter().map(|(_, r)| r.len()).sum())?;
    let mut with_directory = Vec::with_capacity(resources.len() + 1);
    with_directory.push((0, directory));
    with_directory.extend(resources.iter().cloned());
    let resources = &with_directory;
    if resources.len() > 65535 {
        return Err(invalid("OS/2 native resource identifier count exceeded"));
    }
    if pack == GpiContainer::Ne {
        let base = 128;
        let tables = 64 + resources.len() * 12;
        if tables + 12 > usize::from(u16::MAX) {
            return Err(invalid("NE resource tables exceed 16-bit header offsets"));
        }
        let start = (base + tables + 16).div_ceil(16) * 16;
        let total = resources.iter().try_fold(start, |n, (_, v)| {
            n.checked_add(v.len().div_ceil(16) * 16)
                .ok_or_else(|| invalid("NE output length overflow"))
        })?;
        check(total, cap, "GPI output bytes")?;
        budget.add(total.saturating_mul(2))?;
        let mut out = vec![0; total];
        out[..2].copy_from_slice(b"MZ");
        p16(&mut out, 2, 128);
        p16(&mut out, 4, 1);
        p16(&mut out, 8, 4);
        p16(&mut out, 12, 0xffff);
        p16(&mut out, 16, 0x100);
        p16(&mut out, 24, 0x40);
        p32(&mut out, 60, base)?;
        // A real DOS entry point which exits with an unsupported-program status.
        out[64..69].copy_from_slice(&[0xb8, 1, 0x4c, 0xcd, 0x21]);
        out[base..base + 2].copy_from_slice(b"NE");
        out[base + 2] = 5;
        p16(&mut out, base + 4, (tables + 12) as u16);
        p16(&mut out, base + 6, 1);
        p16(&mut out, base + 12, 0x8000);
        p16(&mut out, base + 28, resources.len() as u16);
        p16(&mut out, base + 34, 64);
        p16(&mut out, base + 36, (64 + resources.len() * 8) as u16);
        p16(&mut out, base + 38, tables as u16);
        p16(&mut out, base + 40, (tables + 12) as u16);
        p16(&mut out, base + 42, (tables + 12) as u16);
        p16(&mut out, base + 50, 4);
        p16(&mut out, base + 52, resources.len() as u16);
        out[base + 54] = 1;
        out[base + tables] = 9;
        out[base + tables + 1..base + tables + 10].copy_from_slice(b"FONTPIXEL");
        let mut at = start;
        for (i, (id, data)) in resources.iter().enumerate() {
            if data.is_empty() || data.len() > 65536 || at / 16 > 65535 {
                return Err(invalid("NE font segment exceeds native address range"));
            }
            let p = base + 64 + i * 8;
            p16(&mut out, p, (at / 16) as u16);
            p16(&mut out, p + 2, data.len() as u16);
            p16(&mut out, p + 4, 0x31);
            p16(&mut out, p + 6, data.len() as u16);
            let p = base + 64 + resources.len() * 8 + i * 4;
            p16(&mut out, p, if i == 0 { 6 } else { 7 });
            p16(&mut out, p + 2, *id);
            out[at..at + data.len()].copy_from_slice(data);
            at += data.len().div_ceil(16) * 16;
        }
        return Ok(out);
    }
    let count = resources.len();
    let pages: usize = resources.iter().map(|(_, r)| r.len().div_ceil(4096)).sum();
    let objects = 196;
    let page_map = objects + count * 24;
    let rt = page_map + pages * 8;
    let resident = rt + count * 14;
    let data = (resident + 16).div_ceil(4096) * 4096;
    let total = data + pages * 4096;
    check(total, cap, "GPI LX output bytes")?;
    budget.add(total.saturating_mul(2))?;
    let mut out = vec![0; total];
    out[..2].copy_from_slice(b"LX");
    p16(&mut out, 8, 2);
    p16(&mut out, 10, 1);
    p32(&mut out, 16, 0x8000)?;
    p32(&mut out, 20, pages)?;
    p32(&mut out, 40, 4096)?;
    p32(&mut out, 44, 0)?;
    p32(&mut out, 56, data - 196)?;
    p32(&mut out, 64, objects)?;
    p32(&mut out, 68, count)?;
    p32(&mut out, 72, page_map)?;
    p32(&mut out, 80, rt)?;
    p32(&mut out, 84, count)?;
    p32(&mut out, 88, resident)?;
    p32(&mut out, 92, resident + 12)?;
    p32(&mut out, 128, data)?;
    out[resident] = 9;
    out[resident + 1..resident + 10].copy_from_slice(b"FONTPIXEL");
    let mut pi = 0;
    for (i, (id, raw)) in resources.iter().enumerate() {
        let n = raw.len().div_ceil(4096);
        let op = objects + i * 24;
        p32(&mut out, op, raw.len())?;
        p32(&mut out, op + 8, 0x2031)?;
        p32(&mut out, op + 12, pi + 1)?;
        p32(&mut out, op + 16, n)?;
        let rp = rt + i * 14;
        p16(&mut out, rp, if i == 0 { 6 } else { 7 });
        p16(&mut out, rp + 2, *id);
        p32(&mut out, rp + 4, raw.len())?;
        p16(&mut out, rp + 8, (i + 1) as u16);
        for chunk in raw.chunks(4096) {
            let pp = page_map + pi * 8;
            p32(&mut out, pp, pi * 4096)?;
            p16(&mut out, pp + 4, chunk.len() as u16);
            out[data + pi * 4096..data + pi * 4096 + chunk.len()].copy_from_slice(chunk);
            pi += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::unpack;
    #[test]
    fn all_native_exepack_tokens_and_overlap() {
        assert_eq!(
            unpack(&[3, 0, 2, 0, b'A', b'B', 0, 0], 1).unwrap(),
            b"ABABAB"
        );
        assert_eq!(
            unpack(&[16, b'A', b'B', b'C', b'D', 5, 2, b'E', 0, 0], 5).unwrap(),
            b"ABCDEBCD"
        );
        assert_eq!(
            unpack(&[8, b'A', b'B', 0x22, 0, 0, 0], 5).unwrap(),
            b"ABABA"
        );
        assert_eq!(
            unpack(&[0x0b, 0x21, 0, b'A', b'B', 0, 0], 5).unwrap(),
            b"ABABAB"
        );
        assert_eq!(unpack(&[0, 4, b'Z', 0, 0], 5).unwrap(), b"ZZZZ");
        assert!(unpack(&[0x22, 0], 5).is_err());
        assert!(unpack(&[255, 255, 1, 0, b'X'], 1).is_err());
    }
}
