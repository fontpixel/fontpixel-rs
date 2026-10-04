//! BinHex 4.0 wire primitives derived from Peter N. Lewis's public definition.
use super::support::*;
use crate::model::{FontSet, Property, PropertyValue};
use crate::{ResourceLimits, Result};
const ALPHABET: &[u8; 64] = b"!\"#$%&'()*+,-012345689@ABCDEFGHIJKLMNPQRSTUVXYZ[`abcdefhijklmpqr";
const MARKER: &[u8] = b"(This file must be converted with BinHex";
const PREAMBLE: &[u8] = b"(This file must be converted with BinHex 4.0)\n\n";
fn whitespace(v: u8) -> bool {
    matches!(v, b' ' | b'\t' | b'\r' | b'\n')
}
fn start(data: &[u8]) -> Option<usize> {
    let mut at = 0;
    while at < data.len() {
        if data[at..].starts_with(MARKER) {
            at += data[at..].iter().position(|v| matches!(v, b'\r' | b'\n'))?;
            while at < data.len() && whitespace(data[at]) {
                at += 1;
            }
            return (data.get(at) == Some(&b':')).then_some(at);
        }
        if data[at] == b':' {
            return Some(at);
        }
        at += data[at..].iter().position(|v| matches!(v, b'\r' | b'\n'))? + 1;
    }
    None
}
pub(super) fn signature(data: &[u8]) -> bool {
    start(data).is_some()
}
fn retained(
    s: &mut FontSet,
    key: &str,
    data: &[u8],
    b: &mut Budget,
    l: &ResourceLimits,
) -> Result<()> {
    check(data.len(), l.max_table_bytes, "BinHex retained metadata")?;
    b.add(data.len() + 128 + key.len())?;
    s.properties
        .push(Property::new(key, PropertyValue::Bytes(data.to_vec())));
    Ok(())
}
/// Returns the decoded resource fork; the other native fields remain properties.
pub(super) fn decode(
    data: &[u8],
    s: &mut FontSet,
    b: &mut Budget,
    l: &ResourceLimits,
) -> Result<Vec<u8>> {
    let begin = start(data).ok_or_else(|| invalid("BinHex opening colon missing"))?;
    let mut lookup = [255u8; 256];
    for (i, &v) in ALPHABET.iter().enumerate() {
        lookup[v as usize] = i as u8;
    }
    let mut decoded = Vec::new();
    let mut at = begin + 1;
    let mut bits = 0u32;
    let mut count = 0u32;
    let mut escape = false;
    let mut last = None;
    let mut last_symbol = 255;
    let mut expected = None;
    let mut header_end = None;
    let end = loop {
        let v = *data
            .get(at)
            .ok_or_else(|| invalid("BinHex closing colon missing"))?;
        if v == b':' {
            break at;
        }
        if matches!(v, b'\r' | b'\n') && data[at + 1..].starts_with(b"--- end of part") {
            let mut next = at + 1;
            loop {
                next += data[next..]
                    .iter()
                    .position(|v| matches!(v, b'\r' | b'\n'))
                    .ok_or_else(|| invalid("BinHex next segment missing"))?
                    + 1;
                if data[next..].starts_with(b"---")
                    && data
                        .get(next + 3)
                        .is_some_and(|v| matches!(v, b'\r' | b'\n'))
                {
                    at = next + 4;
                    break;
                }
            }
            continue;
        }
        at += 1;
        if whitespace(v) {
            continue;
        }
        let symbol = lookup[v as usize];
        if symbol == 255 {
            return Err(invalid("BinHex invalid HQX character"));
        }
        last_symbol = symbol;
        bits = (bits << 6) | u32::from(symbol);
        count += 6;
        if count < 8 {
            continue;
        }
        count -= 8;
        let byte = (bits >> count) as u8;
        bits &= (1 << count) - 1;
        let (value, repeats) = if escape {
            escape = false;
            if byte == 0 {
                (0x90, 1)
            } else {
                (
                    last.ok_or_else(|| invalid("BinHex RLE repeat without previous byte"))?,
                    usize::from(byte - 1),
                )
            }
        } else if byte == 0x90 {
            escape = true;
            continue;
        } else {
            (byte, 1)
        };
        let length = decoded
            .len()
            .checked_add(repeats)
            .ok_or_else(|| invalid("BinHex RLE length overflow"))?;
        check(length, l.max_table_bytes, "BinHex decoded stream")?;
        if let Some(n) = expected {
            check(length, n + 1, "BinHex declared decoded size")?;
        }
        b.expanded = b
            .expanded
            .checked_add(repeats)
            .ok_or_else(|| invalid("BinHex expansion budget overflow"))?;
        check(
            b.expanded,
            l.max_decompressed_bytes,
            "BinHex cumulative decompressed bytes",
        )?;
        b.add(repeats * 2)?; // Reserve geometric Vec growth before expanding.
        decoded.resize(length, value);
        if repeats > 0 {
            last = Some(value);
        }
        if header_end.is_none() && !decoded.is_empty() {
            let name = usize::from(decoded[0]);
            if !(1..=63).contains(&name) {
                return Err(invalid("BinHex filename length outside 1..63"));
            }
            let h = name + 20;
            if decoded.len() >= h + 2 {
                if decoded[name + 1] != 0 {
                    return Err(invalid("BinHex filename terminator is not zero"));
                }
                if u16be(&decoded, h)? != crc16(&decoded[..h]) {
                    return Err(invalid("BinHex header CRC mismatch"));
                }
                let dl = u32be(&decoded, name + 12)? as usize;
                let rl = u32be(&decoded, name + 16)? as usize;
                check(dl, l.max_resource_bytes, "BinHex data fork")?;
                check(rl, l.max_resource_bytes, "BinHex resource fork")?;
                let total = (h + 6)
                    .checked_add(dl)
                    .and_then(|n| n.checked_add(rl))
                    .ok_or_else(|| invalid("BinHex fork length overflow"))?;
                check(total, l.max_table_bytes, "BinHex expanded stream")?;
                expected = Some(total);
                header_end = Some(h);
            }
        }
    };
    let h = header_end.ok_or_else(|| invalid("BinHex truncated header"))?;
    let total = expected.expect("header establishes total");
    if escape
        || bits != 0
        || count == 6 && last_symbol != 0
        || decoded.len() < total
        || decoded.len() > total + usize::from(last_symbol == 0)
        || decoded[total..].iter().any(|v| *v != 0)
    {
        return Err(invalid(
            "BinHex truncated RLE, nonzero HQX padding or trailing data",
        ));
    }
    let name = usize::from(decoded[0]);
    let dl = u32be(&decoded, name + 12)? as usize;
    let rl = u32be(&decoded, name + 16)? as usize;
    let df = slice(&decoded, h + 2, dl)?;
    let rf = slice(&decoded, h + 4 + dl, rl)?;
    if u16be(&decoded, h + 2 + dl)? != crc16(df) || u16be(&decoded, total - 2)? != crc16(rf) {
        return Err(invalid("BinHex fork CRC mismatch"));
    }
    b.resource(2, l)?;
    retained(s, "nfnt.binhex-header", &decoded[..name + 12], b, l)?;
    retained(s, "nfnt.binhex-prefix", &data[..begin], b, l)?;
    retained(s, "nfnt.binhex-suffix", &data[end + 1..], b, l)?;
    retained(s, "nfnt.data-fork", df, b, l)?;
    b.add(rf.len())?;
    Ok(rf.to_vec())
}
pub(super) fn encode(
    fork: &[u8],
    s: &FontSet,
    max: usize,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Vec<u8>> {
    let default_header = b"\x04font\0FFILDMOV\0\0";
    let header = bytes(&s.properties, "nfnt.binhex-header").unwrap_or(default_header);
    let name = usize::from(
        *header
            .first()
            .ok_or_else(|| invalid("BinHex empty retained header"))?,
    );
    if !(1..=63).contains(&name) || header.len() != name + 12 || header[name + 1] != 0 {
        return Err(invalid("BinHex invalid retained header"));
    }
    let data = bytes(&s.properties, "nfnt.data-fork").unwrap_or(&[]);
    check(fork.len(), l.max_resource_bytes, "BinHex resource fork")?;
    check(data.len(), l.max_resource_bytes, "BinHex data fork")?;
    let len = header
        .len()
        .checked_add(14)
        .and_then(|n| n.checked_add(data.len()))
        .and_then(|n| n.checked_add(fork.len()))
        .ok_or_else(|| invalid("BinHex output length overflow"))?;
    check(len, l.max_table_bytes, "BinHex decoded stream")?;
    b.add(len)?;
    let mut decoded = Vec::with_capacity(len);
    decoded.extend_from_slice(header);
    decoded.extend_from_slice(&size(data.len())?.to_be_bytes());
    decoded.extend_from_slice(&size(fork.len())?.to_be_bytes());
    decoded.extend_from_slice(&crc16(&decoded).to_be_bytes());
    decoded.extend_from_slice(data);
    decoded.extend_from_slice(&crc16(data).to_be_bytes());
    decoded.extend_from_slice(fork);
    decoded.extend_from_slice(&crc16(fork).to_be_bytes());
    debug_assert_eq!(decoded.len(), len);
    let prefix = bytes(&s.properties, "nfnt.binhex-prefix").unwrap_or(PREAMBLE);
    let suffix = bytes(&s.properties, "nfnt.binhex-suffix").unwrap_or(b"\n");
    // Exact RLE sizing keeps compressible fonts usable under tight output limits.
    let mut compressed_len = 0usize;
    let mut at = 0;
    while at < decoded.len() {
        let value = decoded[at];
        let run = decoded[at..]
            .iter()
            .take(255)
            .take_while(|v| **v == value)
            .count();
        compressed_len += if run >= 3 {
            3 + usize::from(value == 0x90)
        } else {
            run * (1 + usize::from(value == 0x90))
        };
        at += run;
    }
    let symbols = compressed_len
        .checked_mul(8)
        .ok_or_else(|| invalid("BinHex HQX size overflow"))?
        .div_ceil(6);
    let out_len = prefix
        .len()
        .checked_add(suffix.len())
        .and_then(|n| n.checked_add(symbols + symbols / 64 + 2))
        .ok_or_else(|| invalid("BinHex text size overflow"))?;
    check(out_len, max.min(l.max_input_bytes), "BinHex output bytes")?;
    b.add(compressed_len + out_len)?;
    let mut compressed = Vec::with_capacity(compressed_len);
    at = 0;
    while at < decoded.len() {
        let value = decoded[at];
        let run = decoded[at..]
            .iter()
            .take(255)
            .take_while(|v| **v == value)
            .count();
        let literal_count = if run >= 3 { 1 } else { run };
        for _ in 0..literal_count {
            compressed.push(value);
            if value == 0x90 {
                compressed.push(0);
            }
        }
        if run >= 3 {
            compressed.extend_from_slice(&[0x90, run as u8]);
        }
        at += run;
    }
    let mut out = Vec::with_capacity(out_len);
    out.extend_from_slice(prefix);
    out.push(b':');
    let mut bits = 0u32;
    let mut count = 0u32;
    let mut column = 1;
    let mut emitted = 0;
    for v in compressed {
        bits = (bits << 8) | u32::from(v);
        count += 8;
        while count >= 6 {
            count -= 6;
            out.push(ALPHABET[((bits >> count) & 63) as usize]);
            column += 1;
            emitted += 1;
            if column == 64 && emitted < symbols {
                out.push(b'\n');
                column = 0;
            }
        }
        bits &= (1 << count) - 1;
    }
    if count > 0 {
        out.push(ALPHABET[(bits << (6 - count)) as usize]);
    }
    out.push(b':');
    out.extend_from_slice(suffix);
    debug_assert_eq!(out.len(), out_len);
    Ok(out)
}
