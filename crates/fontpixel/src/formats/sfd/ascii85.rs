use super::*;
pub(super) fn decode(lines: &[&str], expected: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected.saturating_add(3));
    let mut value = 0u64;
    let mut digits = 0;
    for c in lines
        .iter()
        .flat_map(|s| s.bytes())
        .filter(|c| !c.is_ascii_whitespace())
    {
        if c == b'z' {
            if digits != 0 {
                return Err(invalid("ASCII85 z inside group"));
            }
            out.extend_from_slice(&[0; 4]);
        } else if (b'!'..=b'u').contains(&c) {
            value = value * 85 + u64::from(c - b'!');
            digits += 1;
            if digits == 5 {
                let n = u32::try_from(value).map_err(|_| invalid("ASCII85 group overflow"))?;
                out.extend_from_slice(&n.to_be_bytes());
                value = 0;
                digits = 0;
            }
        } else {
            return Err(invalid("invalid ASCII85 byte"));
        }
        if out.len() > expected.saturating_add(3) {
            return Err(invalid("excess bitmap ASCII85 data"));
        }
    }
    if digits == 1 {
        return Err(invalid("short ASCII85 group"));
    }
    if digits > 1 {
        let keep = digits - 1;
        for _ in digits..5 {
            value = value * 85 + 84;
        }
        let n = u32::try_from(value).map_err(|_| invalid("ASCII85 tail overflow"))?;
        out.extend_from_slice(&n.to_be_bytes()[..keep]);
    }
    if out.len() < expected || out.len() > expected.saturating_add(3) {
        return Err(invalid("ASCII85 raster length mismatch"));
    }
    if out[expected..].iter().any(|&v| v != 0) {
        return Err(invalid("nonzero ASCII85 padding"));
    }
    out.truncate(expected);
    Ok(out)
}
pub(super) fn encode(data: &[u8], out: &mut String) {
    let mut column = 0;
    for chunk in data.chunks(4) {
        let mut raw = [0; 4];
        raw[..chunk.len()].copy_from_slice(chunk);
        let mut n = u32::from_be_bytes(raw);
        if n == 0 {
            out.push('z');
            column += 1;
        } else {
            let mut digits = [0; 5];
            for d in digits.iter_mut().rev() {
                *d = (n % 85) as u8 + b'!';
                n /= 85;
            }
            for c in digits {
                out.push(char::from(c));
            }
            column += 5;
        }
        if column >= 70 {
            out.push('\n');
            column = 0;
        }
    }
    if column > 0 || data.is_empty() {
        out.push('\n');
    }
}
