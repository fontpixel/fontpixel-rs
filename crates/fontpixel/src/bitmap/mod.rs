//! Bounded, row-major bitmaps: packed MSB-left [`Mono`] and byte-per-pixel [`Gray8`].
//!
//! Pixels and [`Bitmap::paste`] use a top-left origin (y down). Cropping,
//! shifting, shadows and bounding boxes use a bottom-left origin (y up), as in
//! bdfparser SPEC-v3 §2.5. Operations returning a bitmap leave their input alone.
//! In-place operations never change dimensions. Padding bits are always zero.
//!
//! ```
//! use fontpixel::bitmap::{Bitmap, Mono};
//! let source: Bitmap<Mono> = ".#.\n###".parse()?;
//! let enlarged = source.enlarge(2, 3)?;
//! assert_eq!((enlarged.width(), enlarged.height()), (6, 6));
//! let gray = source.to_gray8()?;
//! assert_eq!(source, gray.to_mono(128)?); // threshold is always explicit
//! # Ok::<(), fontpixel::Error>(())
//! ```

use crate::{Error, Result};
use std::{fmt, marker::PhantomData, str::FromStr};

mod bytes;
mod effects;
mod geometry;
pub use bytes::{BitOrder, ByteLayout, ByteOrder, RowOrder};
pub use geometry::{Alignment, Direction};

mod sealed {
    pub trait Sealed {}
}

/// Sealed storage format. External formats convert explicitly to Mono or Gray8.
pub trait PixelFormat: sealed::Sealed + Clone + Copy + fmt::Debug + Eq {
    /// Whether eight samples share a byte.
    const PACKED: bool;
    /// Maximum representable pixel value (1 or 255).
    const MAX: u8;
}

/// One bit per pixel, each row byte-aligned, leftmost pixel in the MSB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mono;
impl sealed::Sealed for Mono {}
impl PixelFormat for Mono {
    const PACKED: bool = true;
    const MAX: u8 = 1;
}

/// Eight-bit coverage/intensity, with 0 empty and 255 full ink.
/// Explicit palettes may also store discrete labels such as SPEC's 0, 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gray8;
impl sealed::Sealed for Gray8 {}
impl PixelFormat for Gray8 {
    const PACKED: bool = false;
    const MAX: u8 = 255;
}

/// Allocation and dimension limits, propagated through bitmap operations.
/// A zero-area bitmap still has to respect both dimension limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitmapLimits {
    /// Maximum width or height, independently.
    pub max_dimension: u32,
    /// Maximum width times height.
    pub max_pixels: usize,
    /// Maximum bytes in one result (including exported padding/palettes).
    pub max_bytes: usize,
}
impl Default for BitmapLimits {
    fn default() -> Self {
        Self {
            max_dimension: 1_048_576,
            max_pixels: 16 * 1024 * 1024,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}
impl BitmapLimits {
    pub(crate) fn check(self, width: u32, height: u32, bytes: usize) -> Result<()> {
        if width.max(height) > self.max_dimension {
            return Err(limit("bitmap dimension", self.max_dimension as usize));
        }
        let pixels = u64::from(width) * u64::from(height);
        if pixels > self.max_pixels as u64 {
            return Err(limit("bitmap pixels", self.max_pixels));
        }
        if bytes > self.max_bytes {
            return Err(limit("bitmap bytes", self.max_bytes));
        }
        Ok(())
    }
}

pub(crate) fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
pub(crate) fn limit(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}
pub(crate) fn zeros(len: usize, maximum: usize) -> Result<Vec<u8>> {
    if len > maximum {
        return Err(limit("bitmap bytes", maximum));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| limit("bitmap allocation", maximum))?;
    bytes.resize(len, 0);
    Ok(bytes)
}

/// Bottom-left, y-up rectangle. Width and height are unsigned; offsets may be negative.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Bounds {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Left edge relative to the origin.
    pub x: i64,
    /// Bottom edge relative to the origin.
    pub y: i64,
}
impl Bounds {
    /// Construct a rectangle; coordinate arithmetic is checked by operations using it.
    pub const fn new(width: u32, height: u32, x: i64, y: i64) -> Self {
        Self {
            width,
            height,
            x,
            y,
        }
    }
}

/// Pixel-compositing rule within the intersection of source and destination.
/// Outside that intersection, destination pixels remain unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeMode {
    /// Nonzero source wins (SPEC `b or a`; this is not alpha blending).
    Over,
    /// Source replaces destination, including zero source pixels.
    Replace,
    /// Bitwise OR.
    Or,
    /// Bitwise AND.
    And,
    /// Bitwise XOR.
    Xor,
    /// Destination AND NOT source.
    Clear,
    /// Saturating addition (OR for Mono).
    Add,
    /// Saturating subtraction (Clear for Mono).
    Subtract,
    /// Larger pixel value.
    Max,
    /// Smaller pixel value.
    Min,
    /// Coverage multiplication, rounded to nearest: a*b/MAX.
    Multiply,
    /// Coverage union, rounded to nearest: a+b-a*b/MAX.
    AlphaOver,
}
impl CompositeMode {
    fn byte(self, a: u8, b: u8, packed: bool) -> u8 {
        if packed {
            return match self {
                Self::Over | Self::Or | Self::Add | Self::Max | Self::AlphaOver => a | b,
                Self::Replace => b,
                Self::And | Self::Min | Self::Multiply => a & b,
                Self::Xor => a ^ b,
                Self::Clear | Self::Subtract => a & !b,
            };
        }
        match self {
            Self::Over => {
                if b == 0 {
                    a
                } else {
                    b
                }
            }
            Self::Replace => b,
            Self::Or => a | b,
            Self::And => a & b,
            Self::Xor => a ^ b,
            Self::Clear => a & !b,
            Self::Add => a.saturating_add(b),
            Self::Subtract => a.saturating_sub(b),
            Self::Max => a.max(b),
            Self::Min => a.min(b),
            Self::Multiply => ((u16::from(a) * u16::from(b) + 127) / 255) as u8,
            Self::AlphaOver => {
                (u16::from(a) + u16::from(b) - (u16::from(a) * u16::from(b) + 127) / 255) as u8
            }
        }
    }
}

/// A validated rectangular raster with private storage and canonical padding.
/// Equality compares dimensions and pixels, not resource budgets.
#[derive(Debug, Clone)]
pub struct Bitmap<F: PixelFormat = Mono> {
    width: u32,
    height: u32,
    data: Vec<u8>,
    limits: BitmapLimits,
    format: PhantomData<F>,
}
impl<F: PixelFormat> PartialEq for Bitmap<F> {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.data == other.data
    }
}
impl<F: PixelFormat> Eq for Bitmap<F> {}
impl<F: PixelFormat> Default for Bitmap<F> {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            data: Vec::new(),
            limits: BitmapLimits::default(),
            format: PhantomData,
        }
    }
}

impl<F: PixelFormat> Bitmap<F> {
    /// Create an empty raster under default resource limits. Zero sizes are valid.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Self::with_limits(width, height, BitmapLimits::default())
    }

    /// Create an empty raster with caller-specified resource limits.
    pub fn with_limits(width: u32, height: u32, limits: BitmapLimits) -> Result<Self> {
        let stride = if F::PACKED {
            u64::from(width).div_ceil(8)
        } else {
            u64::from(width)
        };
        let len = usize::try_from(stride * u64::from(height))
            .map_err(|_| limit("bitmap bytes", limits.max_bytes))?;
        limits.check(width, height, len)?;
        Ok(Self {
            width,
            height,
            data: zeros(len, limits.max_bytes)?,
            limits,
            format: PhantomData,
        })
    }

    /// Read canonical row storage (Mono: packed MSB; Gray8: samples).
    /// Length must be exact. Unused right-hand padding bits are normalized to zero.
    pub fn from_raw(width: u32, height: u32, data: &[u8], limits: BitmapLimits) -> Result<Self> {
        let mut out = Self::with_limits(width, height, limits)?;
        if data.len() != out.data.len() {
            return Err(invalid("bitmap storage length does not match dimensions"));
        }
        out.data.copy_from_slice(data);
        out.clear_padding();
        Ok(out)
    }

    /// Width, including empty columns.
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Height, including empty rows.
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Bytes per stored row (zero when width is zero).
    pub fn stride(&self) -> usize {
        if F::PACKED {
            self.width.div_ceil(8) as usize
        } else {
            self.width as usize
        }
    }
    /// Canonical, immutable row storage. No dimension header is included.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    /// Limits that subsequent allocating operations inherit.
    pub const fn limits(&self) -> BitmapLimits {
        self.limits
    }
    /// Change resource budgets only after validating the existing raster.
    pub fn set_limits(&mut self, limits: BitmapLimits) -> Result<()> {
        limits.check(self.width, self.height, self.data.len())?;
        self.limits = limits;
        Ok(())
    }
    /// Access one top-left-origin sample. Out-of-bounds coordinates are errors.
    pub fn get_pixel(&self, x: u32, y: u32) -> Result<u8> {
        if x >= self.width || y >= self.height {
            return Err(invalid("pixel coordinates out of bounds"));
        }
        Ok(self.pixel(x, y))
    }
    /// Change one top-left-origin sample. Mono accepts only 0 and 1.
    pub fn set_pixel(&mut self, x: u32, y: u32, value: u8) -> Result<()> {
        if x >= self.width || y >= self.height {
            return Err(invalid("pixel coordinates out of bounds"));
        }
        Self::check_value(value)?;
        self.put(x, y, value);
        Ok(())
    }
    fn check_value(value: u8) -> Result<()> {
        if value > F::MAX {
            return Err(invalid(
                "pixel value is not representable in this bitmap format",
            ));
        }
        Ok(())
    }
    fn pixel(&self, x: u32, y: u32) -> u8 {
        let row = y as usize * self.stride();
        if F::PACKED {
            (self.data[row + x as usize / 8] >> (7 - x % 8)) & 1
        } else {
            self.data[row + x as usize]
        }
    }
    fn put(&mut self, x: u32, y: u32, value: u8) {
        let row = y as usize * self.stride();
        if F::PACKED {
            let mask = 0x80 >> (x % 8);
            let b = &mut self.data[row + x as usize / 8];
            *b = (*b & !mask) | if value == 0 { 0 } else { mask };
        } else {
            self.data[row + x as usize] = value;
        }
    }
    fn row(&self, y: u32) -> &[u8] {
        let stride = self.stride();
        &self.data[y as usize * stride..(y as usize + 1) * stride]
    }
    fn row_mut(&mut self, y: u32) -> &mut [u8] {
        let stride = self.stride();
        &mut self.data[y as usize * stride..(y as usize + 1) * stride]
    }
    fn clear_padding(&mut self) {
        if F::PACKED && !self.width.is_multiple_of(8) {
            let mask = 0xff << (8 - self.width % 8);
            let stride = self.stride();
            for row in self.data.chunks_exact_mut(stride) {
                row[stride - 1] &= mask;
            }
        }
    }

    /// Composite using top-left coordinates; fully clipped sources are harmless.
    /// Packed data is shifted/combined a byte at a time, including unaligned edges.
    pub fn composite(&mut self, source: &Self, x: i64, y: i64, mode: CompositeMode) {
        let left = i128::from(x).max(0);
        let top = i128::from(y).max(0);
        let right = (i128::from(x) + i128::from(source.width)).min(i128::from(self.width));
        let bottom = (i128::from(y) + i128::from(source.height)).min(i128::from(self.height));
        if left >= right || top >= bottom {
            return;
        }
        let sx = (left - i128::from(x)) as u32;
        for ty in top as u32..bottom as u32 {
            let sy = (i128::from(ty) - i128::from(y)) as u32;
            blit::<F>(
                self.row_mut(ty),
                source.row(sy),
                sx,
                left as u32,
                (right - left) as u32,
                mode,
            );
        }
    }
    /// Composite a borrowed subrectangle without allocating a cropped bitmap.
    /// Unlike `crop`, `region.x/y` are source top-left coordinates. Both the
    /// region and destination are clipped; pixels outside the source are absent.
    pub fn composite_region(
        &mut self,
        source: &Self,
        region: Bounds,
        x: i64,
        y: i64,
        mode: CompositeMode,
    ) {
        let left = 0i128.max(-i128::from(region.x)).max(-i128::from(x));
        let top = 0i128.max(-i128::from(region.y)).max(-i128::from(y));
        let right = i128::from(region.width)
            .min(i128::from(source.width) - i128::from(region.x))
            .min(i128::from(self.width) - i128::from(x));
        let bottom = i128::from(region.height)
            .min(i128::from(source.height) - i128::from(region.y))
            .min(i128::from(self.height) - i128::from(y));
        if left >= right || top >= bottom {
            return;
        }
        for row in top..bottom {
            let sy = (i128::from(region.y) + row) as u32;
            let dy = (i128::from(y) + row) as u32;
            blit::<F>(
                self.row_mut(dy),
                source.row(sy),
                (i128::from(region.x) + left) as u32,
                (i128::from(x) + left) as u32,
                (right - left) as u32,
                mode,
            );
        }
    }
    /// Paste nonzero source pixels with clipping (SPEC `paste`).
    pub fn paste(&mut self, source: &Self, x: i64, y: i64) {
        self.composite(source, x, y, CompositeMode::Over);
    }
    /// Top-left-aligned nonzero overlay, preserving this bitmap's dimensions.
    /// Size-mismatch warnings belong to the compatibility/reporting layer.
    pub fn overlay(&mut self, source: &Self) {
        self.paste(source, 0, 0);
    }

    /// Parse characters through an explicit palette. CR is ignored; blank outer
    /// lines are skipped. Ragged rows, duplicate palette keys and unknown chars fail.
    pub fn from_text(text: &str, palette: &[(char, u8)], limits: BitmapLimits) -> Result<Self> {
        for (i, &(c, value)) in palette.iter().enumerate() {
            Self::check_value(value)?;
            if c == '\n' || c == '\r' || palette[..i].iter().any(|p| p.0 == c) {
                return Err(invalid("invalid or duplicate bitmap palette character"));
            }
        }
        if text.len() > limits.max_bytes {
            return Err(limit("bitmap text bytes", limits.max_bytes));
        }
        let clean = text.replace('\r', "");
        let mut body = clean.as_str();
        while let Some((first, rest)) = body.split_once('\n') {
            if first.trim().is_empty() {
                body = rest;
            } else {
                break;
            }
        }
        while let Some((rest, last)) = body.rsplit_once('\n') {
            if last.trim().is_empty() {
                body = rest;
            } else {
                break;
            }
        }
        if body.trim().is_empty() {
            return Self::with_limits(0, 0, limits);
        }
        let first = body.split('\n').next().unwrap_or("");
        let width =
            u32::try_from(first.chars().count()).map_err(|_| invalid("bitmap text too wide"))?;
        let height =
            u32::try_from(body.split('\n').count()).map_err(|_| invalid("bitmap text too tall"))?;
        let mut out = Self::with_limits(width, height, limits)?;
        for (y, row) in body.split('\n').enumerate() {
            if row.chars().count() != width as usize {
                return Err(invalid("bitmap rows have different lengths"));
            }
            for (x, c) in row.chars().enumerate() {
                let value = palette
                    .iter()
                    .find(|p| p.0 == c)
                    .ok_or_else(|| invalid("unknown bitmap character"))?
                    .1;
                out.put(x as u32, y as u32, value);
            }
        }
        Ok(out)
    }

    /// Encode samples through an explicit palette, with LF between rows and no
    /// trailing LF. Unknown sample values fail rather than being thresholded.
    pub fn to_text(&self, palette: &[(u8, char)]) -> Result<String> {
        let mut chars = [None; 256];
        for &(value, c) in palette {
            Self::check_value(value)?;
            if c == '\n' || c == '\r' || chars[value as usize].replace(c).is_some() {
                return Err(invalid("invalid or duplicate bitmap output palette entry"));
            }
        }
        let mut len = self.height.saturating_sub(1) as usize;
        for y in 0..self.height {
            for x in 0..self.width {
                let c = chars[self.pixel(x, y) as usize]
                    .ok_or_else(|| invalid("bitmap value missing from output palette"))?;
                len = len
                    .checked_add(c.len_utf8())
                    .ok_or_else(|| limit("bitmap text bytes", self.limits.max_bytes))?;
            }
        }
        if len > self.limits.max_bytes {
            return Err(limit("bitmap text bytes", self.limits.max_bytes));
        }
        let mut out = String::new();
        out.try_reserve_exact(len)
            .map_err(|_| limit("bitmap text allocation", self.limits.max_bytes))?;
        for y in 0..self.height {
            if y > 0 {
                out.push('\n');
            }
            for x in 0..self.width {
                if let Some(c) = chars[self.pixel(x, y) as usize] {
                    out.push(c);
                }
            }
        }
        Ok(out)
    }

    /// Expand samples into fixed-size byte entries (e.g. RGB or RGBA).
    /// All represented pixel values need an entry; output excludes row padding.
    pub fn to_palette_bytes(&self, palette: &[(u8, &[u8])]) -> Result<Vec<u8>> {
        let size = palette
            .first()
            .ok_or_else(|| invalid("empty byte palette"))?
            .1
            .len();
        if size == 0 {
            return Err(invalid("empty byte palette entries"));
        }
        let mut entries = [None; 256];
        for &(value, bytes) in palette {
            Self::check_value(value)?;
            if bytes.len() != size || entries[value as usize].replace(bytes).is_some() {
                return Err(invalid(
                    "byte palette entries must have one size and unique values",
                ));
            }
        }
        let len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(size))
            .ok_or_else(|| limit("bitmap bytes", self.limits.max_bytes))?;
        let mut out = zeros(len, self.limits.max_bytes)?;
        let mut pos = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                let bytes = entries[self.pixel(x, y) as usize]
                    .ok_or_else(|| invalid("bitmap value missing from byte palette"))?;
                out[pos..pos + size].copy_from_slice(bytes);
                pos += size;
            }
        }
        Ok(out)
    }
}

impl FromStr for Bitmap<Mono> {
    type Err = Error;
    /// Parse `#`/`.` or `1`/`0`; labeled `2` is deliberately not silently reduced.
    fn from_str(text: &str) -> Result<Self> {
        Self::from_text(
            text,
            &[('#', 1), ('.', 0), ('1', 1), ('0', 0)],
            BitmapLimits::default(),
        )
    }
}
impl fmt::Display for Bitmap<Mono> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for y in 0..self.height {
            if y > 0 {
                f.write_str("\n")?;
            }
            for x in 0..self.width {
                f.write_str(if self.pixel(x, y) == 0 { "." } else { "#" })?;
            }
        }
        Ok(())
    }
}
impl Bitmap<Mono> {
    /// Lossless expansion: 0 becomes 0 and 1 becomes 255.
    pub fn to_gray8(&self) -> Result<Bitmap<Gray8>> {
        self.to_gray8_palette(0, 255)
    }
    /// Explicit expansion palette; useful for SPEC's discrete 0/1/2 labels.
    pub fn to_gray8_palette(&self, paper: u8, ink: u8) -> Result<Bitmap<Gray8>> {
        let mut out = Bitmap::<Gray8>::with_limits(self.width, self.height, self.limits)?;
        for y in 0..self.height {
            for x in 0..self.width {
                out.put(x, y, if self.pixel(x, y) == 0 { paper } else { ink });
            }
        }
        Ok(out)
    }
}
impl Bitmap<Gray8> {
    /// Threshold explicitly, setting pixels >= threshold to ink. Threshold 0
    /// includes every pixel; 255 includes only full ink. No implicit conversion.
    pub fn to_mono(&self, threshold: u8) -> Result<Bitmap<Mono>> {
        let mut out = Bitmap::<Mono>::with_limits(self.width, self.height, self.limits)?;
        for y in 0..self.height {
            for (x, &value) in self.row(y).iter().enumerate() {
                if value >= threshold {
                    out.put(x as u32, y, 1);
                }
            }
        }
        Ok(out)
    }
    /// Continuous tone negative (`255-value`), distinct from binary [`Bitmap::invert`].
    pub fn negate(&mut self) {
        for value in &mut self.data {
            *value = 255 - *value;
        }
    }
}

/// Extract at most eight source bits, left-aligned, without reading outside a row.
fn bits(row: &[u8], start: u32) -> u8 {
    let index = start as usize / 8;
    let shift = start % 8;
    let a = row.get(index).copied().unwrap_or(0);
    if shift == 0 {
        a
    } else {
        (a << shift) | (row.get(index + 1).copied().unwrap_or(0) >> (8 - shift))
    }
}
fn blit<F: PixelFormat>(
    dst: &mut [u8],
    src: &[u8],
    sx: u32,
    dx: u32,
    width: u32,
    mode: CompositeMode,
) {
    if !F::PACKED {
        let a = &mut dst[dx as usize..(dx + width) as usize];
        let b = &src[sx as usize..(sx + width) as usize];
        if mode == CompositeMode::Replace {
            a.copy_from_slice(b);
        } else {
            for (a, &b) in a.iter_mut().zip(b) {
                *a = mode.byte(*a, b, false);
            }
        }
        return;
    }
    let mut count = 0;
    if !dx.is_multiple_of(8) && width > 0 {
        let offset = dx % 8;
        let n = (8 - offset).min(width);
        let mask = (0xffu8 >> offset) & (0xffu8 << (8 - offset - n));
        let b = bits(src, sx) >> offset;
        let a = &mut dst[dx as usize / 8];
        *a = (*a & !mask) | (mode.byte(*a, b, true) & mask);
        count += n;
    }
    let whole = (width - count) as usize / 8;
    if whole > 0 {
        let di = (dx + count) as usize / 8;
        let si = (sx + count) as usize / 8;
        let shift = (sx + count) % 8;
        let target = &mut dst[di..di + whole];
        let source = &src[si..si + whole + usize::from(shift != 0)];
        // The operation and bit alignment are selected once per row, allowing
        // LLVM to vectorize the byte loop instead of branching per sample/byte.
        match mode {
            CompositeMode::Replace if shift == 0 => target.copy_from_slice(source),
            CompositeMode::Replace => merge_bytes(target, source, shift, |_, b| b),
            CompositeMode::Over
            | CompositeMode::Or
            | CompositeMode::Add
            | CompositeMode::Max
            | CompositeMode::AlphaOver => merge_bytes(target, source, shift, |a, b| a | b),
            CompositeMode::And | CompositeMode::Min | CompositeMode::Multiply => {
                merge_bytes(target, source, shift, |a, b| a & b)
            }
            CompositeMode::Xor => merge_bytes(target, source, shift, |a, b| a ^ b),
            CompositeMode::Clear | CompositeMode::Subtract => {
                merge_bytes(target, source, shift, |a, b| a & !b)
            }
        }
        count += whole as u32 * 8;
    }
    if count < width {
        let pos = dx + count;
        let offset = pos % 8;
        let n = width - count;
        let mask = (0xffu8 >> offset) & (0xffu8 << (8 - offset - n));
        let b = bits(src, sx + count) >> offset;
        let a = &mut dst[pos as usize / 8];
        *a = (*a & !mask) | (mode.byte(*a, b, true) & mask);
    }
}
fn merge_bytes(dst: &mut [u8], src: &[u8], shift: u32, operation: impl Fn(u8, u8) -> u8) {
    if shift == 0 {
        for (a, &b) in dst.iter_mut().zip(src) {
            *a = operation(*a, b);
        }
    } else {
        for (a, (&hi, &lo)) in dst.iter_mut().zip(src.iter().zip(&src[1..])) {
            *a = operation(*a, (hi << shift) | (lo >> (8 - shift)));
        }
    }
}
