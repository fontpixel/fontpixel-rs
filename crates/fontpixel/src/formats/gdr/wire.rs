use crate::formats::legacy_common::*;
use crate::{ResourceLimits, Result};
pub const MAGIC: [u8; 16] = [
    0x37, 0, 0, 0x10, 0x39, 0, 0, 0x10, 0, 0, 0, 0, 0x53, 0x38, 0x39, 0x47,
];
pub const E: ByteOrder = ByteOrder::Little;
pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub at: usize,
}
impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], at: usize) -> Self {
        Self { data, at }
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let v = slice(self.data, self.at, n)?;
        self.at += n;
        Ok(v)
    }
    pub fn byte(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        let v = u16_at(self.data, self.at, E)?;
        self.at += 2;
        Ok(v)
    }
    pub fn u32(&mut self) -> Result<u32> {
        let v = u32_at(self.data, self.at, E)?;
        self.at += 4;
        Ok(v)
    }
    pub fn count(&mut self, limit: usize, kind: &'static str) -> Result<usize> {
        let v = self.u32()?;
        if v > i32::MAX as u32 {
            return Err(invalid("GDR negative count"));
        }
        check(v as usize, limit, kind)?;
        Ok(v as usize)
    }
    pub fn string(&mut self, l: &ResourceLimits, b: &mut Budget) -> Result<String> {
        let first = self.byte()?;
        let tag = if first & 1 == 0 {
            u32::from(first) >> 1
        } else if first & 2 == 0 {
            (u32::from(first) | u32::from(self.byte()?) << 8) >> 2
        } else if first & 4 == 0 {
            (u32::from(first)
                | u32::from(self.byte()?) << 8
                | u32::from(self.byte()?) << 16
                | u32::from(self.byte()?) << 24)
                >> 3
        } else {
            return Err(invalid("invalid GDR descriptor length tag"));
        };
        let n = (tag >> 1) as usize;
        check(n, l.max_table_bytes, "GDR name length")?;
        b.add(n.saturating_mul(6).saturating_add(32))?;
        if tag & 1 != 0 {
            return Ok(latin(self.bytes(n)?));
        }
        let mut out = Vec::with_capacity(n);
        let mut dynamic = [0x80, 0xc0, 0x400, 0x600, 0x900, 0x3040, 0x30a0, 0xff00];
        let stat = [0, 0x80, 0x100, 0x300, 0x2000, 0x2080, 0x2100, 0x3000];
        let mut unicode = false;
        let mut window = 0;
        while out.len() < n {
            let v = self.byte()?;
            let cp = if unicode {
                match v {
                    0xe0..=0xe7 => {
                        window = usize::from(v - 0xe0);
                        unicode = false;
                        continue;
                    }
                    0xe8..=0xef => {
                        window = usize::from(v - 0xe8);
                        dynamic[window] = window_offset(self.byte()?)?;
                        unicode = false;
                        continue;
                    }
                    0xf0 => u32::from(self.byte()?) << 8 | u32::from(self.byte()?),
                    0xf1 => {
                        let a = self.byte()?;
                        window = usize::from(a >> 5);
                        dynamic[window] =
                            0x10000 + ((u32::from(a & 31) << 8 | u32::from(self.byte()?)) * 128);
                        unicode = false;
                        continue;
                    }
                    0xf2 => return Err(invalid("reserved SCSU Unicode tag")),
                    _ => u32::from(v) << 8 | u32::from(self.byte()?),
                }
            } else {
                match v {
                    0 | 9 | 10 | 13 | 0x20..=0x7f => u32::from(v),
                    0x80..=0xff => dynamic[window] + u32::from(v - 0x80),
                    1..=8 => {
                        let q = self.byte()?;
                        if q < 128 {
                            stat[usize::from(v - 1)] + u32::from(q)
                        } else {
                            dynamic[usize::from(v - 1)] + u32::from(q - 128)
                        }
                    }
                    0x0b => {
                        let a = self.byte()?;
                        window = usize::from(a >> 5);
                        dynamic[window] =
                            0x10000 + ((u32::from(a & 31) << 8 | u32::from(self.byte()?)) * 128);
                        continue;
                    }
                    0x0e => u32::from(self.byte()?) << 8 | u32::from(self.byte()?),
                    0x0f => {
                        unicode = true;
                        continue;
                    }
                    0x10..=0x17 => {
                        window = usize::from(v - 0x10);
                        continue;
                    }
                    0x18..=0x1f => {
                        window = usize::from(v - 0x18);
                        dynamic[window] = window_offset(self.byte()?)?;
                        continue;
                    }
                    _ => return Err(invalid("reserved SCSU single-byte tag")),
                }
            };
            if cp > 0xffff {
                let x = cp - 0x10000;
                if cp > 0x10ffff || out.len() + 2 > n {
                    return Err(invalid("SCSU scalar exceeds descriptor"));
                }
                out.push(0xd800 + (x >> 10) as u16);
                out.push(0xdc00 + (x & 1023) as u16);
            } else {
                out.push(cp as u16);
            }
        }
        String::from_utf16(&out).map_err(|_| invalid("invalid UTF-16 in GDR descriptor"))
    }
}
fn window_offset(v: u8) -> Result<u32> {
    Ok(match v {
        1..=0x67 => u32::from(v) * 128,
        0x68..=0xa7 => u32::from(v) * 128 + 0xac00,
        0xf9 => 0xc0,
        0xfa => 0x250,
        0xfb => 0x370,
        0xfc => 0x530,
        0xfd => 0x3040,
        0xfe => 0x30a0,
        0xff => 0xff60,
        _ => return Err(invalid("reserved SCSU window definition")),
    })
}
pub fn string_bytes(s: &str) -> Result<Vec<u8>> {
    let n = s.encode_utf16().count();
    let tag = u32::try_from(n)
        .ok()
        .and_then(|n| n.checked_mul(2))
        .filter(|n| *n < 0x20000000)
        .ok_or_else(|| invalid("GDR string exceeds descriptor length"))?;
    let mut b = Vec::with_capacity(n * 3 + 5);
    if tag < 0x80 {
        b.push((tag << 1) as u8);
    } else if tag < 0x4000 {
        b.extend_from_slice(&((tag << 2 | 1) as u16).to_le_bytes());
    } else {
        b.extend_from_slice(&(tag << 3 | 3).to_le_bytes());
    }
    if n != 0 {
        b.push(0x0f);
        for v in s.encode_utf16() {
            if (0xe0..=0xf2).contains(&(v >> 8)) {
                b.push(0xf0);
            }
            b.extend_from_slice(&v.to_be_bytes());
        }
    }
    Ok(b)
}
pub struct Bits<'a> {
    pub bytes: &'a [u8],
    pub at: usize,
}
impl Bits<'_> {
    pub fn get(&mut self, n: usize) -> Result<u32> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| invalid("GDR bit offset overflow"))?;
        if end > self.bytes.len().saturating_mul(8) {
            return Err(invalid("truncated GDR bitmap bits"));
        }
        let mut v = 0;
        for i in 0..n {
            v |= u32::from((self.bytes[self.at / 8] >> (self.at % 8)) & 1) << i;
            self.at += 1;
        }
        Ok(v)
    }
}
pub struct BitWriter {
    pub bytes: Vec<u8>,
    pub at: usize,
}
impl BitWriter {
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            at: 0,
        }
    }
    pub fn put(&mut self, v: u32, n: usize) {
        for i in 0..n {
            if self.at.is_multiple_of(8) {
                self.bytes.push(0);
            }
            self.bytes[self.at / 8] |= ((v >> i & 1) as u8) << (self.at % 8);
            self.at += 1;
        }
    }
}
