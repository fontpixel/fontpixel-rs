//! Structural detector probes. No bitmap decoding, retained metadata or heap tables.
use super::{raster, support::*};
use crate::Result;

/// Identify an actual bitmap resource in a bounded resource-fork map.
pub(super) fn fork(b: &[u8]) -> Result<bool> {
    let da = u32be(b, 0)? as usize;
    let ma = u32be(b, 4)? as usize;
    let dl = u32be(b, 8)? as usize;
    let ml = u32be(b, 12)? as usize;
    let data = slice(b, da, dl)?;
    let map = slice(b, ma, ml)?;
    if da < 16 || ma < 16 || dl < 4 || ml < 28 || (da < ma + ml && ma < da + dl) {
        return Ok(false);
    }
    if map[..16].iter().any(|&v| v != 0) && map[..16] != b[..16] {
        return Ok(false);
    }
    let ta = usize::from(u16be(map, 24)?);
    let na = usize::from(u16be(map, 26)?);
    if ta < 28 || na > map.len() {
        return Ok(false);
    }
    let nt = usize::from(u16be(map, ta)?.wrapping_add(1));
    let type_end = ta + 2 + nt * 8;
    slice(map, ta, 2 + nt * 8)?;
    let mut references = 0usize;
    for t in 0..nt {
        let at = ta + 2 + t * 8;
        let kind = slice(map, at, 4)?;
        let n = usize::from(u16be(map, at + 4)?.wrapping_add(1));
        references += n;
        // Valid reference lists are disjoint. This also bounds work for forged
        // maps whose many type entries point repeatedly at the same list.
        if references > map.len() / 12 {
            return Ok(false);
        }
        let ra = ta + usize::from(u16be(map, at + 6)?);
        slice(map, ra, n * 12)?;
        if n > 0 && (ra < 28 || (ra < type_end && ta < ra + n * 12)) {
            return Ok(false);
        }
        if kind != b"NFNT" && kind != b"FONT" {
            continue;
        }
        for i in 0..n {
            let at = ra + i * 12;
            let id = u16be(map, at)?;
            if kind == b"FONT" && id & 127 == 0 {
                continue;
            }
            let name = u16be(map, at + 2)?;
            if name != 0xffff {
                let start = na + usize::from(name);
                let len = usize::from(slice(map, start, 1)?[0]);
                slice(map, start + 1, len)?;
            }
            let offset = (u32be(map, at + 4)? & 0xffffff) as usize;
            let len = u32be(data, offset)? as usize;
            let payload = slice(data, offset + 4, len)?;
            if raster::tables_plausible(payload)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// A MacBinary header alone is not font evidence; follow its bounded forks.
pub(super) fn mac(b: &[u8]) -> Result<bool> {
    slice(b, 0, 128)?;
    if b[122] >= 129 && (b[123] > 130 || u16be(b, 124)? != crc16(&b[..124])) {
        return Ok(false);
    }
    let secondary = if b[122] >= 129 {
        usize::from(u16be(b, 120)?)
    } else {
        0
    };
    slice(b, 128, secondary)?;
    let data_len = u32be(b, 83)? as usize;
    let fork_len = u32be(b, 87)? as usize;
    let data_at = 128 + align(secondary, 128)?;
    let fork_at = data_at
        .checked_add(align(data_len, 128)?)
        .ok_or_else(|| invalid("MacBinary probe offset overflow"))?;
    let data = slice(b, data_at, data_len)?;
    let resource = slice(b, fork_at, fork_len)?;
    let comment_len = usize::from(u16be(b, 99)?);
    if comment_len > 0 {
        let at = fork_at
            .checked_add(align(fork_len, 128)?)
            .ok_or_else(|| invalid("MacBinary probe comment overflow"))?;
        slice(b, at, comment_len)?;
    }
    fork(if resource.is_empty() { data } else { resource })
}

/// AppleSingle/Double can contain unrelated files, so require bitmap resources.
pub(super) fn apple(b: &[u8]) -> Result<bool> {
    if ![0x10000, 0x20000].contains(&u32be(b, 4)?) {
        return Ok(false);
    }
    let double = b.get(3) == Some(&7);
    let n = usize::from(u16be(b, 24)?);
    let end = 26 + n * 12;
    slice(b, 0, end)?;
    let mut data = None;
    let mut resource = None;
    for i in 0..n {
        let at = 26 + i * 12;
        let id = u32be(b, at)?;
        let offset = u32be(b, at + 4)? as usize;
        let len = u32be(b, at + 8)? as usize;
        if len > 0 && offset < end {
            return Ok(false);
        }
        let payload = slice(b, offset, len)?;
        if id == 2 {
            resource = Some(payload);
        }
        if id == 1 {
            if double {
                return Ok(false);
            }
            data = Some(payload);
        }
    }
    match resource.or(data) {
        Some(b) => fork(b),
        None => Ok(false),
    }
}
