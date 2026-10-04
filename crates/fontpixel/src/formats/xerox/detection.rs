//! Allocation-free native table probes; pixels and model objects are not decoded.
use super::XeroxVariant;
fn word(b: &[u8], at: usize) -> Option<usize> {
    let v = b.get(at..at.checked_add(2)?)?;
    Some(usize::from(u16::from_be_bytes([v[0], v[1]])))
}
fn dword(b: &[u8], at: usize) -> Option<usize> {
    let v = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([v[0], v[1], v[2], v[3]]) as usize)
}
fn body(b: &[u8], at: usize) -> Option<(usize, usize, usize, usize)> {
    let end = at.checked_add(word(b, at)?.checked_mul(2)?)?;
    let d = b.get(at..end)?;
    let ascent = word(d, 2)?;
    let descent = word(d, 4)?;
    if word(d, 6)? != 0 {
        return None;
    }
    let row = word(d, 8)?.checked_mul(2)?;
    let tail = 10usize.checked_add(row.checked_mul(ascent + descent)?)?;
    let offsets = d.get(tail..)?;
    if offsets.len() < 2 || !offsets.len().is_multiple_of(2) {
        return None;
    }
    let mut previous = 0;
    for i in (0..offsets.len()).step_by(2) {
        let x = word(offsets, i)?;
        if x < previous || x > row * 8 {
            return None;
        }
        previous = x;
    }
    Some((offsets.len() / 2 - 1, ascent, descent, end))
}
fn bitblt(b: &[u8]) -> Option<()> {
    let flags = word(b, 0)?;
    if flags & 0x8000 == 0 || flags & 4095 != 0 {
        return None;
    }
    let first = word(b, 2)?;
    let last = word(b, 4)?;
    let n = last.checked_sub(first)?.checked_add(1)?;
    let kerned = flags & 0x1000 != 0;
    let indexed = flags & 0x4000 != 0;
    let mut at = if kerned { 16 } else { 8 };
    if indexed {
        let ascent = word(b, at)?;
        let descent = word(b, at + 2)?;
        let count = word(b, at + 4)?;
        if !(1..=128).contains(&count) {
            return None;
        }
        at += 6;
        let map = b.get(at..at.checked_add((n + 1) * 2)?)?;
        at += (n + 1) * 2;
        let mut counts = [0usize; 128];
        for c in &mut counts[..count] {
            let (size, a, d, end) = body(b, at)?;
            if a > ascent || d > descent {
                return None;
            }
            *c = size;
            at = end;
        }
        for i in 0..=n {
            let index = word(map, i * 2)?;
            if index & 0x8000 != 0 {
                if i == n {
                    return None;
                }
                continue;
            }
            let group = index >> 8;
            let code = index & 255;
            if group >= count || code >= counts[group] {
                return None;
            }
        }
    } else {
        let (count, _, _, end) = body(b, at)?;
        if count != n + 1 {
            return None;
        }
        at = end;
    }
    if kerned {
        let spacing = b.get(at..at.checked_add((n + 1) * 2)?)?;
        if spacing[n * 2..] == [255, 255] {
            return None;
        }
        at += (n + 1) * 2;
    }
    (at == b.len()).then_some(())
}
fn ac(b: &[u8]) -> Option<()> {
    let mut at = 0;
    let mut first = b.len();
    let mut records = 0;
    while at < first {
        let tag = word(b, at)?;
        let kind = tag >> 12;
        let words = tag & 4095;
        if kind == 0 {
            if words > 1 {
                b.get(at..at.checked_add(words * 2)?)?;
            }
            break;
        }
        if words == 0 {
            return None;
        }
        let end = at.checked_add(words * 2)?;
        let data = b.get(at..end)?;
        if kind == 1 {
            if data.len() != 24 || data[4] > 19 {
                return None;
            }
        } else if kind == 3 {
            if data.len() != 22 || data[5] < data[4] || word(data, 18)? == 0 || word(data, 20)? == 0
            {
                return None;
            }
            let start = dword(data, 10)?.checked_mul(2)?;
            let len = dword(data, 14)?.checked_mul(2)?;
            if start < end {
                return None;
            }
            let segment = b.get(start..start.checked_add(len)?)?;
            first = first.min(start);
            let n = usize::from(data[5] - data[4]) + 1;
            segment.get(..n * 20)?;
            let anchor = n * 16;
            for i in 0..n {
                let height = word(segment, i * 16 + 14)?;
                let pointer = dword(segment, anchor + i * 4)?;
                if height == 65535 {
                    if pointer != u32::MAX as usize {
                        return None;
                    }
                    continue;
                }
                if height > 32767 || pointer == u32::MAX as usize {
                    return None;
                }
                let p = anchor.checked_add(pointer.checked_mul(2)?)?;
                if p < n * 20 {
                    return None;
                }
                let def = word(segment, p)?;
                let width = def & 1023;
                let row = (def >> 10) * 2;
                if width != word(segment, i * 16 + 12)? || row != height.div_ceil(16) * 2 {
                    return None;
                }
                segment.get(p + 2..(p + 2).checked_add(row * width)?)?;
            }
            records += 1;
        }
        at = end;
    }
    (records != 0).then_some(())
}
pub(super) fn plausible(b: &[u8], variant: XeroxVariant) -> bool {
    match variant {
        XeroxVariant::Bitblt => bitblt(b).is_some(),
        XeroxVariant::Prepress => ac(b).is_some(),
        XeroxVariant::Alto => false,
    }
}
