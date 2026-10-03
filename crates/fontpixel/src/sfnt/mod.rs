//! Bounded sfnt container and common OpenType tables (feature `sfnt`).
//!
//! This layer works in font design units and glyph IDs; it does not rasterize or
//! register a bitmap codec. See `docs/formats/sfnt.md` for scope and loss policy.
use crate::{Error, ResourceLimits, Result};

pub mod cmap;
mod container;
mod data;
pub mod gasp;
pub mod glyf;
pub mod metrics;
pub mod name;
pub mod os2;
pub mod post;
pub use container::{Flavor, Sfnt, SfntBuilder, TableRecord, checksum};
/// Fontations' typed, borrowed readers, used by this layer and outline clients.
pub use read_fonts;

/// Additional allocation/work budgets for sfnt operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Shared input, output, table and glyph budgets.
    pub resources: ResourceLimits,
    /// Maximum tables in a directory.
    pub max_tables: usize,
    /// Maximum nominal character mappings expanded while reading or writing.
    pub max_cmap_mappings: usize,
    /// Maximum name records and language-tag records, combined.
    pub max_name_records: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            resources: ResourceLimits::default(),
            max_tables: 4095,
            max_cmap_mappings: 0x110000,
            max_name_records: 8192,
        }
    }
}
impl Limits {
    pub(super) fn table(&self, size: usize) -> Result<()> {
        bounded(size, self.resources.max_table_bytes, "sfnt table bytes")
    }
    pub(super) fn glyphs(&self, count: usize) -> Result<()> {
        bounded(count, self.resources.max_glyphs, "sfnt glyphs")?;
        if count == 0 || count > u16::MAX as usize {
            return Err(invalid("glyph count must be in 1..=65535"));
        }
        Ok(())
    }
}
pub(super) fn invalid(message: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("sfnt: {message}"))
}
pub(super) fn bounded(size: usize, limit: usize, resource: &'static str) -> Result<()> {
    if size > limit {
        Err(Error::ResourceLimit { resource, limit })
    } else {
        Ok(())
    }
}
pub(super) fn bytes(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or_else(|| invalid("truncated table or overflowing offset"))
}
pub(super) fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    let b = bytes(data, offset, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
pub(super) fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    let b = bytes(data, offset, 4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
pub(super) fn p16(out: &mut Vec<u8>, n: u16) {
    out.extend(n.to_be_bytes());
}
pub(super) fn pi16(out: &mut Vec<u8>, n: i16) {
    out.extend(n.to_be_bytes());
}
pub(super) fn p32(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_be_bytes());
}
pub(super) fn to16(n: usize) -> Result<u16> {
    u16::try_from(n).map_err(|_| invalid("16-bit size overflow"))
}
pub(super) fn to32(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| invalid("32-bit size overflow"))
}
pub(super) fn buffer(size: usize) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(size)
        .map_err(|_| invalid("allocation failed"))?;
    Ok(v)
}
