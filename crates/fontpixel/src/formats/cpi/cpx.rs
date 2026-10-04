//! Independent bounded COM/NRV2B reader. Algorithm reference: RetDec (MIT),
//! Copyright (c) 2017 Avast Software. No executable stub is run or copied.
use super::reader::{dword, slice, word};
use super::*;
pub(super) fn signature(b: &[u8]) -> bool {
    b[..b.len().min(128)]
        .windows(4)
        .enumerate()
        .any(|(at, magic)| {
            magic == b"UPX!"
                && b.get(at..at.saturating_add(22))
                    .is_some_and(|h| h[4] >= 10 && h[5] == 1 && h[6] == 4)
        })
}

struct Bits<'a> {
    b: &'a [u8],
    at: usize,
    word: u16,
    left: u8,
}
impl Bits<'_> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .b
            .get(self.at)
            .ok_or_else(|| invalid("truncated CPX compressed stream"))?;
        self.at += 1;
        Ok(b)
    }
    fn bit(&mut self) -> Result<u32> {
        if self.left == 0 {
            self.word = word(self.b, self.at)?;
            self.at += 2;
            self.left = 16;
        }
        let b = u32::from(self.word >> 15);
        self.word <<= 1;
        self.left -= 1;
        Ok(b)
    }
}
fn number(bits: &mut Bits<'_>, max: u32) -> Result<u32> {
    let mut n = 1u32;
    loop {
        n = n
            .checked_mul(2)
            .and_then(|n| n.checked_add(bits.bit().ok()?))
            .ok_or_else(|| invalid("CPX integer overflow or truncated code"))?;
        if n > max {
            return Err(invalid("CPX code exceeds representable range"));
        }
        if bits.bit()? != 0 {
            return Ok(n);
        }
    }
}
fn decode(b: &[u8], len: usize) -> Result<Vec<u8>> {
    let mut bits = Bits {
        b,
        at: 0,
        word: 0,
        left: 0,
    };
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|_| invalid("CPX allocation failed"))?;
    let mut previous = 1usize;
    loop {
        while bits.bit()? != 0 {
            if out.len() >= len {
                return Err(invalid("CPX decoded bytes exceed declared size"));
            }
            out.push(bits.byte()?);
        }
        let value = number(&mut bits, 0x0100_0002)?;
        let distance = if value == 2 {
            previous
        } else {
            let value = (u64::from(value) - 3) * 256 + u64::from(bits.byte()?);
            if value == 0xffff_ffff {
                break;
            }
            let distance =
                usize::try_from(value + 1).map_err(|_| invalid("CPX distance overflow"))?;
            previous = distance;
            distance
        };
        let mut count = (bits.bit()? * 2 + bits.bit()?) as usize;
        if count == 0 {
            count = number(&mut bits, u16::MAX as u32)? as usize + 2;
        }
        count += usize::from(distance > 0xd00) + 1;
        if distance == 0 || distance > out.len() || count > len - out.len() {
            return Err(invalid("CPX match points outside decoded data"));
        }
        for _ in 0..count {
            let value = out[out.len() - distance];
            out.push(value);
        }
    }
    if out.len() != len || bits.at != b.len() {
        return Err(invalid("CPX stream length disagrees with pack header"));
    }
    Ok(out)
}
pub(super) fn adler(b: &[u8]) -> u32 {
    let (mut a, mut c) = (1u32, 0u32);
    for chunk in b.chunks(5552) {
        for &v in chunk {
            a += u32::from(v);
            c += a;
        }
        a %= 65521;
        c %= 65521;
    }
    (c << 16) | a
}
pub(super) fn unpack(b: &[u8], l: &ResourceLimits) -> Result<Vec<u8>> {
    let at = b[..b.len().min(128)]
        .windows(4)
        .position(|w| w == b"UPX!")
        .ok_or_else(|| invalid("missing CPX UPX header"))?;
    let h = slice(b, at, 22)?;
    if h[4] < 10 || h[5] != 1 || h[6] != 4 {
        return Err(invalid(
            "CPX requires the DOS COM NRV2B-LE16 packing method",
        ));
    }
    if h[4..21].iter().map(|&x| u32::from(x)).sum::<u32>() % 251 != u32::from(h[21]) {
        return Err(invalid("CPX pack header checksum mismatch"));
    }
    let len = usize::from(word(h, 16)?);
    let packed = usize::from(word(h, 18)?);
    check(len, l.max_decompressed_bytes, "CPX decompressed bytes")?;
    check(len, l.max_input_bytes, "CPX decoded CPI bytes")?;
    check(len, l.max_table_bytes, "CPX decoded table")?;
    check(
        len.saturating_add(b.len()),
        l.max_total_bytes,
        "CPX retained bytes",
    )?;
    let stream = slice(b, at + 22, packed)?;
    if adler(stream) != dword(h, 12)? {
        return Err(invalid("CPX compressed Adler-32 mismatch"));
    }
    let mut data = decode(stream, len)?;
    if adler(&data) != dword(h, 8)? {
        return Err(invalid("CPX uncompressed Adler-32 mismatch"));
    }
    match h[20] {
        0 => {}
        6 => {
            let mut pos = 0;
            while pos + 2 < data.len() {
                let byte = data[pos];
                pos += 1;
                if byte == 0xe8 || byte == 0xe9 {
                    let value = u16::from_be_bytes([data[pos], data[pos + 1]])
                        .wrapping_sub((pos as u16).wrapping_add(0x100));
                    data[pos..pos + 2].copy_from_slice(&value.to_le_bytes());
                    pos += 2;
                }
            }
        }
        _ => return Err(invalid("unsupported CPX executable filter")),
    }
    if !(data.starts_with(b"\xffFONT   ")
        || data.starts_with(b"\xffFONT.NT")
        || data.starts_with(b"\x7fDRFONT "))
    {
        return Err(invalid("UPX COM payload is not a CPI font"));
    }
    Ok(data)
}
