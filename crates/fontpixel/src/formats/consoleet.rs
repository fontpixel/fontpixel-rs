//! Native Consoleet/vfontas directories with bounded per-glyph text resources.
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{
    Input, OutputAsset, OutputBundle, ReadContext, ResourcePath, ResourceResolver, Written,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
/// Meaning of numeric hexadecimal filenames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CodepointMode {
    /// Preserve encoding-unspecified native positions.
    #[default]
    Legacy,
    /// Require Unicode scalar values.
    Unicode,
}
/// Native directory interpretation.
#[derive(Debug, Clone, Default)]
pub struct ConsoleetReadOptions {
    /// Baseline below each bitmap top.
    pub baseline: Option<i64>,
    /// Explicit encoding metadata.
    pub charset: Option<String>,
    /// Meaning of file names.
    pub codepoint_mode: CodepointMode,
}
/// Native directory output choices.
#[derive(Debug, Clone, Default)]
pub struct ConsoleetWriteOptions {
    /// Optional common cell; omitted preserves each bitmap rectangle.
    pub cell: Option<Bounds>,
    /// Explicit Gray8 threshold.
    pub gray_threshold: Option<u8>,
    /// Permit Unicode numbers as legacy positions.
    pub unicode_as_legacy: bool,
    /// Numeric filename interpretation.
    pub codepoint_mode: CodepointMode,
    /// Aggregate directory, model and output limits.
    pub limits: ResourceLimits,
}
/// Consoleet directory codec.
#[derive(Debug, Clone, Default)]
pub struct ConsoleetCodec {
    /// Reader options.
    pub read_options: ConsoleetReadOptions,
    /// Writer options.
    pub write_options: ConsoleetWriteOptions,
}
impl FormatDetector for ConsoleetCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.is_directory() {
            Confidence::Possible
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for ConsoleetCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for ConsoleetCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn read_internal(
    c: &mut ReadContext<'_>,
    o: &ConsoleetReadOptions,
) -> Result<(FontSet, Report, usize)> {
    if !c.is_directory() {
        return Err(invalid("Consoleet needs a native directory input"));
    }
    let l = c.limits().clone();
    let mut b = Budget::new(c.retained_input_bytes(), &l)?;
    b.add(o.charset.as_ref().map_or(0, String::len))?;
    let options = NativeReadOptions {
        baseline: o.baseline,
        charset: o.charset.clone(),
    };
    let mut f = base("consoleet", &options, &mut b)?;
    let before = c.retained_input_bytes();
    let paths = c.list_resources_limited(
        None,
        l.max_glyphs.min(l.max_resources.saturating_sub(1)),
        b.max - b.used,
    )?;
    b.add(c.retained_input_bytes() - before)?;
    if paths.is_empty() {
        return Err(invalid("Consoleet directory contains no glyph files"));
    }
    let mut seen = BTreeSet::new();
    for path in paths {
        let name = path.as_str();
        let stem = name
            .strip_suffix(".txt")
            .ok_or_else(|| invalid("Consoleet glyph filename must end in .txt"))?;
        if stem.is_empty() || stem.len() > 8 || !stem.bytes().all(|v| v.is_ascii_hexdigit()) {
            return Err(invalid("Consoleet glyph filename is not hexadecimal"));
        }
        let cp = u32::from_str_radix(stem, 16).map_err(|_| invalid("Consoleet code overflow"))?;
        b.add(64)?;
        if !seen.insert(cp) {
            return Err(invalid("duplicate Consoleet numeric filename"));
        }
        let code = match o.codepoint_mode {
            CodepointMode::Legacy => Codepoint::Legacy(cp),
            CodepointMode::Unicode => {
                if char::from_u32(cp).is_none() {
                    return Err(invalid("Consoleet filename is not a Unicode scalar"));
                }
                Codepoint::Unicode(cp)
            }
        };
        let before = c.retained_input_bytes();
        let bytes = c.resource_limited(&path, b.max - b.used)?;
        b.add(c.retained_input_bytes() - before)?;
        check(bytes.len(), l.max_table_bytes, "Consoleet glyph text bytes")?;
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid("Consoleet is ASCII text"))?;
        let mut lines = text.lines();
        if lines.next() != Some("PCLT") {
            return Err(invalid("invalid Consoleet magic"));
        }
        let mut dims = lines
            .next()
            .ok_or_else(|| invalid("missing Consoleet dimensions"))?
            .split_ascii_whitespace();
        let mut number = || {
            dims.next()
                .ok_or_else(|| invalid("missing Consoleet dimension"))?
                .parse::<u32>()
                .map_err(|_| invalid("invalid Consoleet dimension"))
        };
        let w = number()?;
        let h = number()?;
        if dims.next().is_some() {
            return Err(invalid("extra Consoleet dimensions"));
        }
        let minimum_text = (w as usize)
            .checked_mul(2)
            .and_then(|v| v.checked_mul(h as usize))
            .and_then(|v| v.checked_add((h as usize).saturating_sub(1)))
            .ok_or_else(|| invalid("Consoleet raster text size overflow"))?;
        if minimum_text > bytes.len() {
            return Err(invalid("Consoleet dimensions exceed available raster text"));
        }
        let n = size(w, h, &l)?;
        let mut raw = allocated(n, &mut b)?;
        let stride = (w as usize).div_ceil(8);
        for y in 0..h as usize {
            let row = lines
                .next()
                .ok_or_else(|| invalid("truncated Consoleet row"))?
                .as_bytes();
            if row.len() != (w as usize).saturating_mul(2) {
                return Err(invalid("Consoleet row length mismatch"));
            }
            for (x, pair) in row.as_chunks::<2>().0.iter().enumerate() {
                match pair {
                    b".." => {}
                    b"##" => raw[y * stride + x / 8] |= 128 >> (x % 8),
                    _ => return Err(invalid("Consoleet pixels must be .. or ##")),
                }
            }
        }
        if lines.next().is_some() {
            return Err(invalid("extra Consoleet raster rows"));
        }
        glyph(&mut f, code, w, h, &raw, &options, &l, &mut b)?;
    }
    let mut set = complete(f, "consoleet")?;
    let font = &mut set.fonts[0];
    let variable = {
        let mut dimensions = font
            .glyphs()
            .map(|(_, g)| (g.bitmap.width(), g.bitmap.height()));
        dimensions
            .next()
            .is_some_and(|first| dimensions.any(|v| v != first))
    };
    if variable {
        font.metrics.spacing = Some(Spacing::Proportional);
    }
    Ok((set, Report::default(), b.used))
}
/// Read immediate native glyph files, with explicit numeric code interpretation.
pub fn read(c: &mut ReadContext<'_>, o: &ConsoleetReadOptions) -> Result<(FontSet, Report)> {
    let (s, r, _) = read_internal(c, o)?;
    Ok((s, r))
}
struct Files<'a>(BTreeMap<&'a str, &'a [u8]>);
impl ResourceResolver for Files<'_> {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let v = self
            .0
            .get(p.as_str())
            .ok_or_else(|| crate::Error::MissingResource(p.to_string()))?;
        check(v.len(), max, "Consoleet readback bytes")?;
        Ok(v.to_vec())
    }
    fn list(
        &self,
        d: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        if d.is_some() {
            return Err(invalid("Consoleet readback has no nested directories"));
        }
        check(self.0.len(), max, "Consoleet readback entries")?;
        let mut used = 0usize;
        self.0
            .keys()
            .map(|s| {
                used = used.saturating_add((std::mem::size_of::<ResourcePath>() + s.len()) * 2);
                check(used, bytes, "Consoleet readback listing bytes")?;
                ResourcePath::new(*s)
            })
            .collect()
    }
}
/// Write one PCLT asset for every selected code and reopen the resulting directory.
pub fn write(s: &FontSet, o: &WriteOptions, p: &ConsoleetWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let native = NativeWriteOptions {
            cell: p.cell,
            gray_threshold: p.gray_threshold,
            unicode_as_legacy: p.unicode_as_legacy,
            limits: p.limits.clone(),
        };
        let mut budget = source(s, &native)?;
        budget.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let source = convert_gray(s, native.gray_threshold, &p.limits, &mut budget)?;
        let f = source.single()?;
        let mut map = BTreeMap::new();
        for (_, g) in f.glyphs() {
            for code in &g.codepoints {
                let cp = match (code, p.codepoint_mode) {
                    (Codepoint::Legacy(v), CodepointMode::Legacy)
                    | (Codepoint::Unicode(v), CodepointMode::Unicode) => *v,
                    (Codepoint::Unicode(v), CodepointMode::Legacy) if native.unicode_as_legacy => {
                        *v
                    }
                    _ => continue,
                };
                budget.add(128)?;
                if map.insert(cp, g).is_some() {
                    return Err(invalid("conflicting Consoleet filenames"));
                }
            }
        }
        if map.is_empty() {
            return Err(invalid("Consoleet requires storable code labels"));
        }
        check(
            map.len().saturating_add(1),
            p.limits.max_resources,
            "Consoleet resource operations",
        )?;
        check(map.len(), p.limits.max_glyphs, "Consoleet glyph count")?;
        budget.items::<OutputAsset>(map.len())?;
        let mut assets = Vec::new();
        for (cp, g) in map {
            let bounds = native.cell.unwrap_or(g.metrics.bounds);
            let pixels = raster(Some(g), bounds, &p.limits, &mut budget)?;
            let estimate = 32usize.saturating_add(
                (bounds.width as usize)
                    .saturating_mul(2)
                    .saturating_add(1)
                    .saturating_mul(bounds.height as usize),
            );
            check(
                estimate,
                p.limits.max_resource_bytes,
                "Consoleet output glyph bytes",
            )?;
            check(
                estimate,
                p.limits.max_table_bytes,
                "Consoleet output glyph text",
            )?;
            budget.add(estimate.saturating_mul(2) + o.primary_name.as_str().len() + 128)?;
            let mut text = String::new();
            writeln!(text, "PCLT\n{} {}", bounds.width, bounds.height).expect("String write");
            for y in 0..bounds.height {
                for x in 0..bounds.width {
                    text.push_str(if pixels.get_pixel(x, y).unwrap_or(0) == 0 {
                        ".."
                    } else {
                        "##"
                    });
                }
                text.push('\n');
            }
            let path = ResourcePath::new(format!("{}/{cp:04X}.txt", o.primary_name))?;
            assets.push(OutputAsset {
                path,
                bytes: text.into_bytes(),
            });
        }
        budget.add(assets.len().saturating_mul(128))?;
        let prefix = o.primary_name.as_str().len() + 1;
        let files = Files(
            assets
                .iter()
                .map(|v| (&v.path.as_str()[prefix..], v.bytes.as_slice()))
                .collect(),
        );
        let input = Input::directory("consoleet", &files);
        let mut remaining = p.limits.clone();
        remaining.max_total_bytes = budget.max - budget.used;
        let mut context = ReadContext::new(&input, &remaining)?;
        let (back, r, used) = read_internal(
            &mut context,
            &ConsoleetReadOptions {
                codepoint_mode: p.codepoint_mode,
                ..Default::default()
            },
        )?;
        budget.add(used)?;
        report.extend(r);
        audit(s, &back, "consoleet", &mut report, &mut budget)?;
        OutputBundle::directory(o.primary_name.clone(), assets)
    })();
    finish(result, report, "consoleet", o.strict)
}
