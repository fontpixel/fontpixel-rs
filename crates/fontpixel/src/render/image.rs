use crate::bitmap::{Bitmap, BitmapLimits};
use crate::{Error, Result};
use std::fmt::Write as _;
use std::str::FromStr;

/// Unpremultiplied sRGB color and opacity. Channels range from 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
    /// Alpha (0 transparent, 255 opaque).
    pub alpha: u8,
}
impl Rgba {
    /// Opaque black.
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    /// Opaque white.
    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    /// Transparent black.
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    /// Construct an explicit RGBA color.
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
    fn rgb(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}
impl FromStr for Rgba {
    type Err = Error;
    /// Parse `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, or `transparent`.
    fn from_str(value: &str) -> Result<Self> {
        if value == "transparent" {
            return Ok(Self::TRANSPARENT);
        }
        let Some(s) = value.strip_prefix('#') else {
            return Err(super::invalid(
                "color must be #RGB, #RGBA, #RRGGBB, #RRGGBBAA, or transparent",
            ));
        };
        if ![3, 4, 6, 8].contains(&s.len()) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(super::invalid("invalid hexadecimal color"));
        }
        let mut values = [255; 4];
        if s.len() <= 4 {
            for (i, b) in s.bytes().enumerate() {
                values[i] = (b as char).to_digit(16).unwrap_or(0) as u8 * 17;
            }
        } else {
            for (i, pair) in s.as_bytes().as_chunks::<2>().0.iter().enumerate() {
                values[i] = ((pair[0] as char).to_digit(16).unwrap_or(0) * 16
                    + (pair[1] as char).to_digit(16).unwrap_or(0))
                    as u8;
            }
        }
        Ok(Self::new(values[0], values[1], values[2], values[3]))
    }
}

/// Image export options shared by PNG and SVG.
#[derive(Debug, Clone, Copy)]
pub struct ImageOptions {
    /// Positive integer nearest-neighbor scale (SVG viewBox remains unscaled).
    pub scale: u32,
    /// Ink color.
    pub foreground: Rgba,
    /// Paper color; transparent is supported explicitly.
    pub background: Rgba,
    /// Scaled dimensions/pixels and encoded-output byte budget.
    pub limits: BitmapLimits,
}
impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            scale: 1,
            foreground: Rgba::BLACK,
            background: Rgba::WHITE,
            limits: BitmapLimits::default(),
        }
    }
}
fn dimensions(bitmap: &Bitmap, options: &ImageOptions) -> Result<(u32, u32)> {
    if options.scale == 0 {
        return Err(super::invalid("image scale must be positive"));
    }
    let w = bitmap
        .width()
        .checked_mul(options.scale)
        .ok_or_else(|| super::invalid("scaled image width overflows"))?;
    let h = bitmap
        .height()
        .checked_mul(options.scale)
        .ok_or_else(|| super::invalid("scaled image height overflows"))?;
    let bytes = usize::try_from(u64::from(w).div_ceil(8) * u64::from(h))
        .map_err(|_| super::budget("image bytes", options.limits.max_bytes))?;
    options.limits.check(w, h, bytes)?;
    Ok((w, h))
}
fn append(s: &mut String, part: &str, maximum: usize) -> Result<()> {
    if part.len() > maximum.saturating_sub(s.len()) {
        return Err(super::budget("SVG output bytes", maximum));
    }
    s.try_reserve(part.len())
        .map_err(|_| super::budget("SVG allocation", maximum))?;
    s.push_str(part);
    Ok(())
}

/// Export a deterministic, standalone SVG image with one closed rectangle per
/// horizontal ink run. Colors are typed/escaped; no external assets, scripts or
/// fonts are referenced. This is a raster image representation, not the later
/// SPEC contour/SVG-font serializer. Empty bitmaps produce a valid empty SVG.
pub fn to_svg(bitmap: &Bitmap, options: &ImageOptions) -> Result<String> {
    let (width, height) = dimensions(bitmap, options)?;
    let mut out = String::new();
    let maximum = options.limits.max_bytes;
    append(
        &mut out,
        &format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\">\n",
            bitmap.width(),
            bitmap.height()
        ),
        maximum,
    )?;
    let mut buffer = String::with_capacity(80);
    // Disjoint paper/ink paths preserve palette alpha exactly. Drawing a paper
    // rectangle beneath translucent ink would disagree with PNG's RGBA samples.
    for (value, color) in [(0, options.background), (1, options.foreground)] {
        if color.alpha == 0 {
            continue;
        }
        append(
            &mut out,
            &format!(
                "<path fill=\"{}\" fill-opacity=\"{}\" d=\"",
                color.rgb(),
                f64::from(color.alpha) / 255.
            ),
            maximum,
        )?;
        for y in 0..bitmap.height() {
            let mut x = 0;
            while x < bitmap.width() {
                if bitmap.get_pixel(x, y)? != value {
                    x += 1;
                    continue;
                }
                let start = x;
                x += 1;
                while x < bitmap.width() && bitmap.get_pixel(x, y)? == value {
                    x += 1;
                }
                buffer.clear();
                write!(&mut buffer, "M{start} {y}h{}v1h-{}z", x - start, x - start)
                    .map_err(|_| super::invalid("cannot format SVG path"))?;
                append(&mut out, &buffer, maximum)?;
            }
        }
        append(&mut out, "\"/>\n", maximum)?;
    }
    append(&mut out, "</svg>\n", maximum)?;
    Ok(out)
}

/// Encode a lossless, two-entry indexed 1-bit PNG. Alpha is preserved in tRNS.
/// The positive-size PNG requirement is an explicit error for an empty bitmap;
/// no artificial 1×1 image is silently substituted. Requires feature `png`.
#[cfg(feature = "png")]
pub fn to_png(bitmap: &Bitmap, options: &ImageOptions) -> Result<Vec<u8>> {
    use std::io::{self, Write};
    struct Output {
        bytes: Vec<u8>,
        maximum: usize,
        exceeded: bool,
    }
    impl Output {
        fn chunk(&mut self, kind: &[u8; 4], bytes: &[u8]) -> io::Result<()> {
            if bytes.len() > i32::MAX as usize
                || bytes.len().saturating_add(12) > self.maximum.saturating_sub(self.bytes.len())
            {
                self.exceeded = true;
                return Err(io::Error::other("PNG output byte limit"));
            }
            let additional = bytes.len() + 12;
            if self.bytes.len() + additional > self.bytes.capacity() {
                let capacity = self
                    .bytes
                    .capacity()
                    .saturating_mul(2)
                    .max(self.bytes.len() + additional)
                    .min(self.maximum);
                self.bytes
                    .try_reserve_exact(capacity - self.bytes.len())
                    .map_err(io::Error::other)?;
            }
            self.bytes
                .extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            self.bytes.extend_from_slice(kind);
            self.bytes.extend_from_slice(bytes);
            let mut crc = crc32fast::Hasher::new();
            crc.update(kind);
            crc.update(bytes);
            self.bytes.extend_from_slice(&crc.finalize().to_be_bytes());
            Ok(())
        }
    }
    struct Chunks<'a>(&'a mut Output);
    impl Write for Chunks<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !bytes.is_empty() {
                self.0.chunk(b"IDAT", bytes)?;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (width, height) = dimensions(bitmap, options)?;
    if width == 0 || height == 0 {
        return Err(super::invalid(
            "PNG cannot represent a zero-width or zero-height bitmap",
        ));
    }
    if width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(super::invalid("PNG dimensions exceed 2^31-1"));
    }
    if options.limits.max_bytes < 8 {
        return Err(super::budget("PNG output bytes", options.limits.max_bytes));
    }
    let mut output = Output {
        bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        maximum: options.limits.max_bytes,
        exceeded: false,
    };
    let result = (|| -> io::Result<()> {
        let mut header = [0u8; 13];
        header[..4].copy_from_slice(&width.to_be_bytes());
        header[4..8].copy_from_slice(&height.to_be_bytes());
        header[8] = 1;
        header[9] = 3;
        output.chunk(b"IHDR", &header)?;
        output.chunk(b"sRGB", &[0])?;
        let a = options.background;
        let b = options.foreground;
        output.chunk(b"PLTE", &[a.red, a.green, a.blue, b.red, b.green, b.blue])?;
        if a.alpha != 255 || b.alpha != 255 {
            output.chunk(b"tRNS", &[a.alpha, b.alpha])?;
        }
        let mut encoder =
            flate2::write::ZlibEncoder::new(Chunks(&mut output), flate2::Compression::default());
        let mut scaled = Vec::new();
        if options.scale > 1 {
            scaled
                .try_reserve_exact(width.div_ceil(8) as usize)
                .map_err(io::Error::other)?;
            scaled.resize(width.div_ceil(8) as usize, 0);
        }
        for row in bitmap.as_bytes().chunks_exact(bitmap.stride()) {
            let data = if options.scale == 1 {
                row
            } else {
                scaled.fill(0);
                for x in 0..width {
                    let sx = x / options.scale;
                    if row[sx as usize / 8] & (0x80 >> (sx % 8)) != 0 {
                        scaled[x as usize / 8] |= 0x80 >> (x % 8);
                    }
                }
                &scaled
            };
            for _ in 0..options.scale {
                encoder.write_all(&[0])?;
                encoder.write_all(data)?;
            }
        }
        encoder.finish()?;
        output.chunk(b"IEND", &[])
    })();
    if output.exceeded {
        return Err(super::budget("PNG output bytes", options.limits.max_bytes));
    }
    result?;
    Ok(output.bytes)
}
