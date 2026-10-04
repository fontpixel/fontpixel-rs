//! Explicitly configured headerless cell fonts.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{BitOrder, RowOrder};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
/// Binary cell layout; no field is inferred from arbitrary input bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLayout {
    /// Cell width in pixels.
    pub width: u32,
    /// Cell height in pixels.
    pub height: u32,
    /// Cell count; absent consumes the entire payload exactly.
    pub count: Option<usize>,
    /// First legacy code assigned in file order.
    pub first_codepoint: u32,
    /// Significance of bits within each byte.
    pub bit_order: BitOrder,
    /// Ordering of glyph rows.
    pub row_order: RowOrder,
    /// Extra bytes after every row.
    pub row_padding: usize,
    /// Prefix bytes before the first glyph.
    pub offset: usize,
    /// Default little-endian PRG load address when no matching `raw.prefix` exists.
    /// Requires a two-byte offset; import preserves the actual prefix instead.
    pub default_load_address: Option<u16>,
    /// Explicit legacy charset identity.
    pub charset: Option<String>,
}
impl RawLayout {
    /// Build an unpadded MSB-first layout.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            count: None,
            first_codepoint: 0,
            bit_order: BitOrder::MsbFirst,
            row_order: RowOrder::TopDown,
            row_padding: 0,
            offset: 0,
            default_load_address: None,
            charset: None,
        }
    }
}
/// Common layouts selected explicitly by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawPreset {
    /// IBM VGA 8 by 8, 256 positions.
    Vga8,
    /// IBM VGA 8 by 14, 256 positions.
    Vga14,
    /// IBM VGA 8 by 16, 256 positions.
    Vga16,
    /// C64 screen-code order, 256 positions.
    C64,
    /// C64 .64c with a PRG load address and an exact, inferred glyph count.
    C64Prg,
    /// ZX Spectrum printable character ROM, codes 32 through 127.
    Spectrum,
    /// Generic .ch8 glyphs, with unknown legacy charset.
    Ch8,
    /// MSX 8 by 8, 256 positions.
    Msx,
}
/// Explicit raw import configuration.
#[derive(Debug, Clone, Default)]
pub struct RawReadOptions {
    /// Required layout or explicitly selected preset.
    pub layout: Option<RawLayout>,
}
/// Raw export settings.
#[derive(Debug, Clone)]
pub struct RawWriteOptions {
    /// Explicit output layout; absent uses uniform source cells.
    pub layout: Option<RawLayout>,
    /// Explicit grayscale threshold.
    pub gray_threshold: Option<u8>,
    /// Output byte bound.
    pub max_output_bytes: usize,
    /// Model and read-back limits.
    pub limits: ResourceLimits,
}
impl Default for RawWriteOptions {
    fn default() -> Self {
        Self {
            layout: None,
            gray_threshold: None,
            max_output_bytes: 64 * 1024 * 1024,
            limits: ResourceLimits::default(),
        }
    }
}
/// Raw registry codec; its default reader refuses unspecified layouts.
#[derive(Debug, Clone, Default)]
pub struct RawCodec {
    /// Import configuration.
    pub read_options: RawReadOptions,
    /// Export configuration.
    pub write_options: RawWriteOptions,
}
impl FormatDetector for RawCodec {
    fn detect(&self, _c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(Confidence::NoMatch)
    }
}
impl FormatReader<FontSet> for RawCodec {
    fn read(&self, c: &mut ReadContext<'_>, o: &ReadOptions) -> Result<(FontSet, Report)> {
        {
            let (set, mut report) = parse(
                c.bytes(),
                &self.read_options,
                &o.limits,
                c.retained_input_bytes(),
                c.name().map_or(0, str::len),
            )?;
            for entry in &mut report.entries {
                entry.location.asset = c.name().map(str::to_owned);
            }
            Ok((set, report))
        }
    }
}
impl FormatWriter<FontSet> for RawCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: crate::resources::OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}

use super::console::w41 as support;
use super::console::{
    Budget, Output, bitmap_limits, cell_size, check, glyph_metrics, invalid, metrics, raster,
};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{Codepoint, Font, Glyph, Property, PropertyValue};
use crate::report::Impact;
use support::{BoundedReport, compare, fail, failed_projection, model_cost, note};
impl RawPreset {
    /// Construct the complete profile; selecting a profile is explicit configuration.
    pub fn layout(self) -> RawLayout {
        let mut l = RawLayout::new(
            8,
            match self {
                Self::Vga14 => 14,
                Self::Vga16 => 16,
                _ => 8,
            },
        );
        l.count = Some(256);
        match self {
            Self::Vga8 | Self::Vga14 | Self::Vga16 => {}
            Self::C64 | Self::C64Prg => {
                l.charset = Some("c64".into());
                if self == Self::C64Prg {
                    l.offset = 2;
                    l.default_load_address = Some(0x3800);
                    l.count = None;
                }
            }
            Self::Spectrum => {
                l.first_codepoint = 32;
                l.count = Some(96);
                l.charset = Some("zx-spectrum".into());
            }
            Self::Ch8 => l.count = None,
            Self::Msx => {}
        }
        l
    }
}
impl std::str::FromStr for RawPreset {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "vga8" | "vga-8x8" => Ok(Self::Vga8),
            "vga14" | "vga-8x14" => Ok(Self::Vga14),
            "vga16" | "vga-8x16" => Ok(Self::Vga16),
            "c64" => Ok(Self::C64),
            "64c" | "c64-prg" => Ok(Self::C64Prg),
            "spectrum" | "zx-spectrum" => Ok(Self::Spectrum),
            "ch8" => Ok(Self::Ch8),
            "msx" => Ok(Self::Msx),
            _ => Err(invalid("unknown raw preset")),
        }
    }
}
fn size(layout: &RawLayout, l: &ResourceLimits) -> Result<(usize, usize)> {
    if layout.default_load_address.is_some() && layout.offset != 2 {
        return Err(invalid("raw PRG load address requires a two-byte offset"));
    }
    cell_size(layout.width, layout.height, l)?;
    let row = (layout.width as usize)
        .div_ceil(8)
        .checked_add(layout.row_padding)
        .ok_or_else(|| invalid("raw row size overflow"))?;
    let cell = row
        .checked_mul(layout.height as usize)
        .ok_or_else(|| invalid("raw cell size overflow"))?;
    check(cell, l.max_table_bytes, "raw cell bytes")?;
    check(layout.offset, l.max_table_bytes, "raw prefix bytes")?;
    Ok((row, cell))
}
/// Read a configured raw font. Partial cells and trailing bytes are errors.
pub fn read(b: &[u8], o: &RawReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    parse(b, o, l, b.len(), 0)
}
fn parse(
    b: &[u8],
    o: &RawReadOptions,
    l: &ResourceLimits,
    retained: usize,
    asset_bytes: usize,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_input_bytes, "raw input bytes")?;
    let layout = o
        .layout
        .as_ref()
        .ok_or_else(|| invalid("raw input requires an explicit layout or named preset"))?;
    let (row, cell) = size(layout, l)?;
    let data = b
        .get(layout.offset..)
        .ok_or_else(|| invalid("raw prefix exceeds input"))?;
    if data.len() % cell != 0 {
        return Err(invalid("raw input ends in a partial cell"));
    }
    let count = layout.count.unwrap_or(data.len() / cell);
    if count == 0 || count != data.len() / cell {
        return Err(invalid("raw count must consume the payload exactly"));
    }
    check(count, l.max_glyphs, "raw glyph count")?;
    check(data.len(), l.max_table_bytes, "raw bitmap table")?;
    let last = count
        .checked_sub(1)
        .and_then(|n| u32::try_from(n).ok())
        .and_then(|n| layout.first_codepoint.checked_add(n))
        .ok_or_else(|| invalid("raw positional code overflows u32"))?;
    let _ = last;
    let packed = (layout.width as usize).div_ceil(8);
    let n = packed * layout.height as usize;
    let mut budget = Budget::new(retained, l)?;
    budget.add(
        count
            .checked_mul(
                1152usize
                    .checked_add(n)
                    .ok_or_else(|| invalid("raw allocation overflow"))?,
            )
            .ok_or_else(|| invalid("raw allocation overflow"))?,
    )?;
    budget.add(
        layout
            .offset
            .saturating_add(layout.charset.as_ref().map_or(0, String::len))
            .saturating_add(
                std::mem::size_of::<Font>()
                    .saturating_mul(2)
                    .saturating_add(512),
            ),
    )?;
    budget.add(n)?;
    let mut font = Font::default();
    font.source_format = Some("raw".into());
    font.metrics = metrics(layout.width, layout.height, 0);
    font.charset.name = Some(layout.charset.clone().unwrap_or_else(|| "unknown".into()));
    if layout.offset > 0 {
        font.headers.push(Property::new(
            "raw.prefix",
            PropertyValue::Bytes(b[..layout.offset].to_vec()),
        ));
    }
    let mut report = Report::default();
    for (index, raw) in data.chunks_exact(cell).enumerate() {
        let mut bytes = vec![0; n];
        let mut padding = false;
        for (y, source) in raw.chunks_exact(row).enumerate() {
            let dy = if layout.row_order == RowOrder::TopDown {
                y
            } else {
                layout.height as usize - 1 - y
            };
            for (x, &value) in source[..packed].iter().enumerate() {
                bytes[dy * packed + x] = if layout.bit_order == BitOrder::MsbFirst {
                    value
                } else {
                    value.reverse_bits()
                };
            }
            padding |= source[packed..].iter().any(|&x| x != 0);
            if layout.width % 8 != 0 {
                padding |=
                    bytes[dy * packed + packed - 1] & ((1u8 << (8 - layout.width % 8)) - 1) != 0;
            }
        }
        let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
            layout.width,
            layout.height,
            &bytes,
            bitmap_limits(l),
        )?);
        g.metrics = glyph_metrics(Bounds::new(layout.width, layout.height, 0, 0));
        g.codepoints
            .push(Codepoint::Legacy(layout.first_codepoint + index as u32));
        font.add_glyph(g)?;
        if padding {
            budget.add(1024usize.saturating_add(asset_bytes))?;
            note(
                &mut report,
                "raw",
                "read.padding",
                Some(0),
                Some(index),
                None,
                Impact::Repair,
                "Unused raw row padding normalized",
            );
        }
    }
    let mut set = FontSet::from(font);
    set.source_format = Some("raw".into());
    Ok((set, report))
}
/// Write one raw strike in file order, reporting positional remapping explicitly.
pub fn write(s: &FontSet, o: &RawWriteOptions) -> Result<(Vec<u8>, Report)> {
    s.validate(&o.limits)?;
    let retained = model_cost(s, &o.limits)?;
    let font = s.single()?;
    let mut report = BoundedReport::new(retained, &o.limits)?;
    let result = (|| {
        report.reserve(font.charset.name.as_ref().map_or(0, String::len))?;
        let inferred;
        let layout = if let Some(l) = &o.layout {
            l
        } else {
            let first = font
                .glyphs()
                .next()
                .ok_or_else(|| invalid("raw output requires glyphs"))?
                .1;
            inferred = RawLayout {
                count: Some(font.len()),
                first_codepoint: first
                    .codepoints
                    .first()
                    .filter(|cp| matches!(cp, Codepoint::Legacy(_)))
                    .map_or(0, |cp| cp.value()),
                charset: font.charset.name.clone(),
                ..RawLayout::new(first.bitmap.width(), first.bitmap.height())
            };
            &inferred
        };
        let (row, cell) = size(layout, &o.limits)?;
        if layout.count.is_some_and(|n| n != font.len()) {
            return Err(invalid(
                "raw configured count differs from font glyph count",
            ));
        }
        let n = font
            .len()
            .checked_mul(cell)
            .and_then(|n| n.checked_add(layout.offset))
            .ok_or_else(|| invalid("raw output size overflow"))?;
        check(
            n,
            o.max_output_bytes
                .min(o.limits.max_input_bytes)
                .min(o.limits.max_total_bytes),
            "raw output bytes",
        )?;
        let source_bitmap = font
            .glyphs()
            .map(|(_, g)| g.bitmap.as_bytes().len())
            .max()
            .unwrap_or(0);
        report.reserve(
            n.saturating_mul(2)
                .saturating_add(layout.offset)
                .saturating_add(cell.saturating_mul(3))
                .saturating_add(source_bitmap.saturating_mul(2))
                .saturating_add(1024),
        )?;
        let mut out = Output::new(n);
        let prefix = font.headers.get("raw.prefix");
        if let Some(PropertyValue::Bytes(b)) =
            prefix.filter(|p| matches!(p,PropertyValue::Bytes(b) if b.len()==layout.offset))
        {
            out.add(b)?;
        } else if let Some(address) = layout.default_load_address {
            out.add(&address.to_le_bytes())?;
        } else {
            out.add(&vec![0; layout.offset])?;
        }
        let packed = (layout.width as usize).div_ceil(8);
        let mut wire = vec![0; cell];
        for (_, g) in font.glyphs() {
            let bm = raster(
                g,
                Bounds::new(layout.width, layout.height, 0, 0),
                o.gray_threshold,
                &o.limits,
            )?;
            wire.fill(0);
            for (y, source) in bm.as_bytes().chunks_exact(packed).enumerate() {
                let dy = if layout.row_order == RowOrder::TopDown {
                    y
                } else {
                    layout.height as usize - 1 - y
                };
                for (x, &b) in source.iter().enumerate() {
                    wire[dy * row + x] = if layout.bit_order == BitOrder::MsbFirst {
                        b
                    } else {
                        b.reverse_bits()
                    };
                }
            }
            out.add(&wire)?;
        }
        report.reserve(layout.charset.as_ref().map_or(0, String::len))?;
        let read_options = RawReadOptions {
            layout: Some(layout.clone()),
        };
        let before_readback = report.used();
        report.reserve(
            font.len()
                .saturating_mul(2048)
                .saturating_add(n.saturating_mul(2))
                .saturating_add(layout.charset.as_ref().map_or(0, String::len))
                .saturating_add(
                    std::mem::size_of::<Font>()
                        .saturating_mul(2)
                        .saturating_add(2048),
                ),
        )?;
        let (decoded, read_report) =
            parse(&out.bytes, &read_options, &o.limits, before_readback, 0)?;
        if read_report.has_loss() {
            return Err(invalid("raw emitted lossy input"));
        }
        compare(s, &decoded, "raw", &mut report);
        Ok(out.bytes)
    })();
    match result {
        Ok(bytes) => Ok((bytes, report.finish()?)),
        Err(e) => {
            if matches!(e, crate::Error::ResourceLimit { .. }) {
                return Err(e);
            }
            failed_projection(s, "raw", &mut report);
            fail(report, "raw", e)
        }
    }
}
