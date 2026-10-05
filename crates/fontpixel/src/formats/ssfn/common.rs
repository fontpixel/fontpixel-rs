//! Checked scalar, allocation and full-model audit support for SSFN/PFR.
use super::WriteOptions;
pub use super::legacy_common::{
    Budget, audit, bitmap_limits, check, convert_gray, finish, invalid, source_budget_for,
};
use crate::model::{FontSet, Properties, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
pub fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| invalid("font offset overflow"))
}
pub fn mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("font size overflow"))
}
pub fn bytes(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(at..add(at, n)?)
        .ok_or_else(|| invalid("truncated font span"))
}
pub fn number(b: &[u8], at: usize, n: usize, little: bool) -> Result<u32> {
    if n > 4 {
        return Err(invalid("invalid scalar width"));
    }
    let s = bytes(b, at, n)?;
    Ok(if little {
        s.iter().rev().fold(0, |v, &x| v * 256 + u32::from(x))
    } else {
        s.iter().fold(0, |v, &x| v * 256 + u32::from(x))
    })
}
pub fn put(b: &mut [u8], at: usize, n: usize, value: u32, little: bool) -> Result<()> {
    if n > 4 || (n < 4 && value >= 1u32 << (n * 8)) {
        return Err(invalid("font scalar exceeds field width"));
    }
    let end = add(at, n)?;
    let out = b
        .get_mut(at..end)
        .ok_or_else(|| invalid("internal font output span"))?;
    for (i, v) in out.iter_mut().enumerate() {
        *v = (value >> ((if little { i } else { n - 1 - i }) * 8)) as u8
    }
    Ok(())
}
pub fn alloc(n: usize, budget: &mut Budget) -> Result<Vec<u8>> {
    budget.add(n)?;
    let mut b = Vec::new();
    b.try_reserve_exact(n).map_err(|_| Error::ResourceLimit {
        resource: "font allocation",
        limit: budget.max,
    })?;
    b.resize(n, 0);
    Ok(b)
}
pub fn area(w: u32, h: u32, l: &ResourceLimits) -> Result<usize> {
    let n = mul(w as usize, h as usize)?;
    check(n, l.max_pixels_per_glyph, "font glyph pixels")?;
    Ok(n)
}
pub fn scalar(p: &Properties, key: &str, default: i64) -> Result<i64> {
    match p.get(key) {
        None => Ok(default),
        Some(PropertyValue::Integer(v)) => Ok(*v),
        Some(PropertyValue::Unsigned(v)) => {
            i64::try_from(*v).map_err(|_| invalid("font property integer overflow"))
        }
        _ => Err(invalid("font property is not an integer")),
    }
}
#[cfg(feature = "pfr")]
pub fn blob<'a>(p: &'a Properties, key: &str) -> Result<Option<&'a [u8]>> {
    match p.get(key) {
        None => Ok(None),
        Some(PropertyValue::Bytes(v)) => Ok(Some(v)),
        _ => Err(invalid("font property is not bytes")),
    }
}
pub fn property(
    p: &mut Properties,
    key: &str,
    value: PropertyValue,
    budget: &mut Budget,
) -> Result<()> {
    budget.add(256 + key.len())?;
    p.push(crate::model::Property::new(key, value));
    Ok(())
}
pub fn append(out: &mut Vec<u8>, v: &[u8], budget: &mut Budget) -> Result<()> {
    budget.add(mul(v.len(), 2)?)?;
    out.try_reserve(v.len()).map_err(|_| Error::ResourceLimit {
        resource: "font output allocation",
        limit: budget.max,
    })?;
    out.extend_from_slice(v);
    Ok(())
}
#[cfg(feature = "pfr")]
pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub at: usize,
    pub little: bool,
}
#[cfg(feature = "pfr")]
impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], little: bool) -> Self {
        Self {
            data,
            at: 0,
            little,
        }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = bytes(self.data, self.at, n)?;
        self.at = add(self.at, n)?;
        Ok(s)
    }
    pub fn u(&mut self, n: usize) -> Result<u32> {
        let v = number(self.data, self.at, n, self.little)?;
        self.at = add(self.at, n)?;
        Ok(v)
    }
    pub fn i(&mut self, n: usize) -> Result<i32> {
        let v = self.u(n)?;
        Ok(((v << (32 - n * 8)) as i32) >> (32 - n * 8))
    }
    pub fn end(&self) -> Result<()> {
        if self.at != self.data.len() {
            Err(invalid("unexpected bytes after native record"))
        } else {
            Ok(())
        }
    }
}
pub fn input(
    c: &ReadContext<'_>,
    decode: impl FnOnce(&[u8], &ResourceLimits, &mut Budget, &mut Report) -> Result<FontSet>,
) -> Result<(FontSet, Report)> {
    let l = c.limits();
    check(c.bytes().len(), l.max_table_bytes, "font input table")?;
    let mut budget = Budget::new(c.retained_input_bytes(), l)?;
    let mut report = Report::default();
    let s = decode(c.bytes(), l, &mut budget, &mut report)?;
    s.validate(l)?;
    let names = mul(report.entries.len(), c.name().map_or(0, str::len))?;
    budget.add(names)?;
    for e in &mut report.entries {
        e.location.asset = c.name().map(str::to_owned)
    }
    Ok((s, report))
}
#[allow(clippy::too_many_arguments)] // Keep caller-selected limits and the two bounded wire passes explicit.
pub fn output(
    s: &FontSet,
    o: &WriteOptions,
    l: &ResourceLimits,
    max: usize,
    id: &str,
    charge: super::ModelCharge,
    encode: impl FnOnce(&mut Budget, &mut Report) -> Result<Vec<u8>>,
    decode: impl FnOnce(&[u8], &ResourceLimits, &mut Budget, &mut Report) -> Result<FontSet>,
) -> Result<Written> {
    s.validate(l)?;
    let mut budget = source_budget_for(s, l, charge)?;
    let mut report = Report::default();
    let result = (|| {
        let b = encode(&mut budget, &mut report)?;
        check(
            b.len(),
            max.min(l.max_input_bytes).min(l.max_table_bytes),
            "font output bytes",
        )?;
        let mut reread = Report::default();
        let back = decode(&b, l, &mut budget, &mut reread)?;
        if reread.has_loss() {
            return Err(invalid("writer emitted a lossy native representation"));
        }
        audit(s, &back, id, &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), b))
    })();
    let result = match result {
        Err(e @ Error::ResourceLimit { .. }) => return Err(e),
        other => other,
    };
    if result.is_err() {
        audit(s, &FontSet::default(), id, &mut report, &mut budget)?;
    }
    finish(result, report, id, o.strict)
}
