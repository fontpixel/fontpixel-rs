//! Grid fitting / scan conversion policies, versions 0 and 1.
use super::{Limits, buffer, invalid, p16, to16};
use crate::Result;
use read_fonts::{FontData, FontRead, tables::gasp::Gasp};
/// Inclusive upper ppem bound and behavior flags (bits 0–3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Range {
    /// Last ppem covered by this range; final range must be 65535.
    pub max_ppem: u16,
    /// Gridfit=1, grayscale=2, symmetric gridfit=4, symmetric smoothing=8.
    pub behavior: u16,
}
/// Explicit gasp table, ordered by inclusive upper bound.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct GaspTable {
    /// Version 0 permits bits 0–1, version 1 permits bits 0–3.
    pub version: u16,
    /// Strictly ascending ranges ending at 65535.
    pub ranges: Vec<Range>,
}
impl GaspTable {
    fn validate(&self) -> Result<()> {
        if self.version > 1
            || self.ranges.is_empty()
            || self.ranges.last().is_none_or(|r| r.max_ppem != 65535)
        {
            return Err(invalid("invalid gasp version or terminal range"));
        }
        let allowed = if self.version == 0 { 3 } else { 15 };
        let mut previous = None;
        for r in &self.ranges {
            if r.behavior & !allowed != 0 || previous.is_some_and(|p| p >= r.max_ppem) {
                return Err(invalid("invalid gasp flags or range order"));
            }
            previous = Some(r.max_ppem);
        }
        Ok(())
    }
    /// Read and validate every range with Fontations.
    pub fn decode(data: &[u8], limits: &Limits) -> Result<Self> {
        limits.table(data.len())?;
        let t = Gasp::read(FontData::new(data)).map_err(invalid)?;
        if t.gasp_ranges().len() != usize::from(t.num_ranges()) {
            return Err(invalid("truncated gasp ranges"));
        }
        let result = Self {
            version: t.version(),
            ranges: t
                .gasp_ranges()
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    Ok(Range {
                        max_ppem: r.range_max_ppem(),
                        behavior: super::u16_at(data, 6 + 4 * i)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        result.validate()?;
        Ok(result)
    }
    /// Serialize the explicit policy without changing scan-conversion flags.
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>> {
        self.validate()?;
        let count = to16(self.ranges.len())?;
        let size = 4 + self.ranges.len() * 4;
        limits.table(size)?;
        let mut out = buffer(size)?;
        p16(&mut out, self.version);
        p16(&mut out, count);
        for r in &self.ranges {
            p16(&mut out, r.max_ppem);
            p16(&mut out, r.behavior);
        }
        Ok(out)
    }
}
