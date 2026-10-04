use super::support::*;
use crate::Result;
use std::collections::BTreeMap;
const MARK: &[u8] = b"FiLeStArTfIlEsTaRt";
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789()";
fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|&c| c == b'\r' || c == b'\n')
        .filter(|l| !l.is_empty())
}
pub(super) fn detect(data: &[u8]) -> bool {
    lines(data).any(|l| l == MARK)
}
fn crc(mut crc: u16, data: &[u8]) -> u16 {
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
fn u24(s: &[u8]) -> usize {
    usize::from(s[0]) | usize::from(s[1]) << 8 | usize::from(s[2]) << 16
}
fn put24(s: &mut [u8], n: usize) {
    s[0] = n as u8;
    s[1] = (n >> 8) as u8;
    s[2] = (n >> 16) as u8;
}
fn decode_six(input: &[u8], table: &[u8; 256], out: &mut [u8]) -> Result<()> {
    if !input.len().is_multiple_of(4) || input.len() / 4 * 3 != out.len() {
        return Err(invalid("BinSCII encoded line length"));
    }
    for (input, out) in input
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<3>().0.iter_mut())
    {
        let mut x = 0u32;
        for (i, &byte) in input.iter().enumerate() {
            let digit = table[usize::from(byte)];
            if digit == 255 {
                return Err(invalid("BinSCII character outside dictionary"));
            }
            x |= u32::from(digit) << (i * 6);
        }
        out.copy_from_slice(&x.to_be_bytes()[1..]);
    }
    Ok(())
}
pub(super) fn decode(data: &[u8], b: &mut Budget<'_>) -> Result<Vec<u8>> {
    let mut input = lines(data);
    let mut output = Vec::new();
    let mut identity = None;
    let mut intervals = BTreeMap::new();
    let mut count = 0usize;
    while let Some(line) = input.next() {
        if line != MARK {
            continue;
        }
        count += 1;
        check(count, b.limits.max_resources, "BinSCII segments")?;
        b.add(128)?;
        let alphabet = input
            .next()
            .ok_or_else(|| invalid("BinSCII missing dictionary"))?;
        if alphabet.len() != 64 {
            return Err(invalid("BinSCII dictionary must contain 64 symbols"));
        }
        let mut table = [255u8; 256];
        for (i, &c) in alphabet.iter().enumerate() {
            if !c.is_ascii_graphic() || table[usize::from(c)] != 255 {
                return Err(invalid("BinSCII duplicate or invalid dictionary symbol"));
            }
            table[usize::from(c)] = i as u8;
        }
        let line = input
            .next()
            .ok_or_else(|| invalid("BinSCII missing metadata"))?;
        if line.len() != 52 || !(0x41..=0x4f).contains(&line[0]) {
            return Err(invalid("BinSCII invalid filename/header record"));
        }
        let name_len = usize::from(line[0] - 0x40);
        let name = &line[1..1 + name_len];
        if !name.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'.')
            || !line[1 + name_len..16].iter().all(|&c| c == b' ')
        {
            return Err(invalid("BinSCII invalid filename"));
        }
        let mut header = [0; 27];
        decode_six(&line[16..], &table, &mut header)?;
        if crc(0, &header[..24]) != u16::from_le_bytes([header[24], header[25]]) {
            return Err(invalid("BinSCII header CRC mismatch"));
        }
        if header[7] != 0xc8 || header[8] != 0 || header[9] != 0 || !(1..=3).contains(&header[10]) {
            return Err(invalid("BinSCII requires one ProDOS C8/0000 data fork"));
        }
        let size = u24(&header[..3]);
        let start = u24(&header[3..6]);
        let len = u24(&header[21..24]);
        let end = start
            .checked_add(len)
            .ok_or_else(|| invalid("BinSCII segment overflow"))?;
        if size == 0 || len == 0 || end > size {
            return Err(invalid("BinSCII invalid segment bounds"));
        }
        let mut id = [0u8; 34];
        id[..16].copy_from_slice(&line[..16]);
        id[16..19].copy_from_slice(&header[..3]);
        id[19..].copy_from_slice(&header[6..21]);
        if let Some(old) = identity {
            if id != old {
                return Err(invalid("BinSCII conflicting files or attributes"));
            }
        } else {
            check(
                size,
                b.limits
                    .max_decompressed_bytes
                    .min(b.limits.max_input_bytes),
                "BinSCII expanded data bytes",
            )?;
            b.table(size)?;
            output = vec![0; size];
            identity = Some(id);
        }
        if intervals
            .range(..=start)
            .next_back()
            .is_some_and(|(_, &e)| e > start)
            || intervals
                .range(start..)
                .next()
                .is_some_and(|(&s, _)| s < end)
        {
            return Err(invalid("BinSCII overlapping or duplicated segment"));
        }
        let mut checksum = 0;
        let mut position = start;
        let mut remaining = len;
        for _ in 0..len.div_ceil(48) {
            let line = input
                .next()
                .ok_or_else(|| invalid("BinSCII truncated data"))?;
            let mut decoded = [0; 48];
            decode_six(line, &table, &mut decoded)?;
            checksum = crc(checksum, &decoded);
            let used = remaining.min(48);
            output[position..position + used].copy_from_slice(&decoded[..used]);
            if decoded[used..].iter().any(|&c| c != 0) {
                return Err(invalid("BinSCII nonzero final padding"));
            }
            position += used;
            remaining -= used;
        }
        let line = input
            .next()
            .ok_or_else(|| invalid("BinSCII missing data CRC"))?;
        let mut decoded = [0; 3];
        decode_six(line, &table, &mut decoded)?;
        if checksum != u16::from_le_bytes([decoded[0], decoded[1]]) || decoded[2] != 0 {
            return Err(invalid("BinSCII data CRC mismatch"));
        }
        intervals.insert(start, end);
    }
    if identity.is_none() {
        return Err(invalid("BinSCII no segments"));
    }
    let mut end = 0;
    for (start, next) in intervals {
        if start != end {
            return Err(invalid("BinSCII missing segment"));
        }
        end = next;
    }
    if end != output.len() {
        return Err(invalid("BinSCII incomplete file"));
    }
    Ok(output)
}
fn encode_six(data: &[u8], out: &mut Vec<u8>) {
    for group in data.as_chunks::<3>().0 {
        let mut n = u32::from_be_bytes([0, group[0], group[1], group[2]]);
        for _ in 0..4 {
            out.push(ALPHABET[(n & 63) as usize]);
            n >>= 6;
        }
    }
}
pub(super) fn encode(data: &[u8], b: &mut Budget<'_>, max: usize) -> Result<Vec<u8>> {
    if data.is_empty() || data.len() > 0xff_ffff {
        return Err(invalid("BinSCII file length is outside 24-bit range"));
    }
    let segments = data.len().div_ceil(12288);
    check(segments, b.limits.max_resources, "BinSCII output segments")?;
    let size = segments
        .checked_mul(MARK.len() + 1 + 65 + 53 + 5)
        .and_then(|v| v.checked_add(data.len().div_ceil(48) * 65))
        .ok_or_else(|| invalid("BinSCII output size overflow"))?;
    check(
        size,
        max.min(b.limits.max_input_bytes),
        "BinSCII output bytes",
    )?;
    b.table(size)?;
    let mut out = Vec::with_capacity(size);
    for (i, segment) in data.chunks(12288).enumerate() {
        out.extend_from_slice(MARK);
        out.push(b'\n');
        out.extend_from_slice(ALPHABET);
        out.push(b'\n');
        out.extend_from_slice(b"DFONT           ");
        let mut header = [0u8; 27];
        put24(&mut header[..3], data.len());
        put24(&mut header[3..6], i * 12288);
        header[6] = 0xe3;
        header[7] = 0xc8;
        header[10] = if data.len() <= 512 {
            1
        } else if data.len() <= 131072 {
            2
        } else {
            3
        };
        header[11..13].copy_from_slice(&(data.len().div_ceil(512) as u16).to_le_bytes());
        put24(&mut header[21..24], segment.len());
        let checksum = crc(0, &header[..24]);
        header[24..26].copy_from_slice(&checksum.to_le_bytes());
        encode_six(&header, &mut out);
        out.push(b'\n');
        let mut checksum = 0;
        for chunk in segment.chunks(48) {
            let mut line = [0; 48];
            line[..chunk.len()].copy_from_slice(chunk);
            checksum = crc(checksum, &line);
            encode_six(&line, &mut out);
            out.push(b'\n');
        }
        encode_six(&[checksum as u8, (checksum >> 8) as u8, 0], &mut out);
        out.push(b'\n');
    }
    debug_assert_eq!(out.len(), size);
    Ok(out)
}
