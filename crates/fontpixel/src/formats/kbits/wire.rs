use super::*;
pub(super) struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    pub(super) fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    pub(super) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .p
            .checked_add(n)
            .ok_or_else(|| invalid("length overflow"))?;
        let out = self
            .b
            .get(self.p..end)
            .ok_or_else(|| invalid("truncated record"))?;
        self.p = end;
        Ok(out)
    }
    pub(super) fn int(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().map_err(invalid)?,
        ))
    }
    pub(super) fn short(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(invalid)?,
        ))
    }
    pub(super) fn position(&self) -> usize {
        self.p
    }
    pub(super) fn rest(&self) -> &'a [u8] {
        &self.b[self.p..]
    }
}
pub(super) fn decode(b: &[u8]) -> Result<String> {
    let mut units = Vec::new();
    let mut p = 0;
    while p < b.len() {
        let a = b[p];
        p += 1;
        let n = if (1..=127).contains(&a) {
            u16::from(a)
        } else if a & 0xe0 == 0xc0 {
            let c = *b.get(p).ok_or_else(|| invalid("short modified UTF-8"))?;
            p += 1;
            if c & 0xc0 != 0x80 {
                return Err(invalid("modified UTF-8 continuation"));
            }
            let v = (u16::from(a & 31) << 6) | u16::from(c & 63);
            if v != 0 && v < 128 {
                return Err(invalid("overlong modified UTF-8"));
            }
            v
        } else if a & 0xf0 == 0xe0 {
            let tail = b
                .get(p..p + 2)
                .ok_or_else(|| invalid("short modified UTF-8"))?;
            p += 2;
            if tail.iter().any(|v| v & 0xc0 != 0x80) {
                return Err(invalid("modified UTF-8 continuation"));
            }
            let v = (u16::from(a & 15) << 12)
                | (u16::from(tail[0] & 63) << 6)
                | u16::from(tail[1] & 63);
            if v < 2048 {
                return Err(invalid("overlong modified UTF-8"));
            }
            v
        } else {
            return Err(invalid("invalid modified UTF-8 lead"));
        };
        units.push(n);
    }
    String::from_utf16(&units).map_err(invalid)
}
pub(super) fn put(b: &mut Vec<u8>, n: i64) -> Result<()> {
    b.extend_from_slice(&i32::try_from(n).map_err(invalid)?.to_be_bytes());
    Ok(())
}
pub(super) fn name(b: &mut Vec<u8>, id: i32, s: &str, l: &ResourceLimits) -> Result<()> {
    let n = s
        .encode_utf16()
        .map(|v| {
            if v == 0 || v >= 0x80 {
                if v < 0x800 { 2 } else { 3 }
            } else {
                1
            }
        })
        .sum::<usize>();
    bound(n, u16::MAX as usize, "KBITS modified UTF-8")?;
    bound(n, l.max_table_bytes, "KBITS name bytes")?;
    bound(
        b.len().saturating_add(n).saturating_add(14),
        l.max_input_bytes.min(l.max_total_bytes),
        "KBITS output",
    )?;
    b.try_reserve_exact(n.saturating_add(14)).map_err(invalid)?;
    b.extend_from_slice(b"name");
    put(b, 1)?;
    put(b, i64::from(id))?;
    b.extend_from_slice(&(n as u16).to_be_bytes());
    for v in s.encode_utf16() {
        if (1..128).contains(&v) {
            b.push(v as u8);
        } else if v < 0x800 {
            b.push(0xc0 | (v >> 6) as u8);
            b.push(0x80 | (v & 63) as u8);
        } else {
            b.push(0xe0 | (v >> 12) as u8);
            b.push(0x80 | ((v >> 6) & 63) as u8);
            b.push(0x80 | (v & 63) as u8);
        }
    }
    Ok(())
}
