//! Original bounded primitives shared by native image-font families.
#[cfg(any(feature = "sfont", feature = "vlw"))]
pub use super::legacy_common::int;
#[cfg(feature = "pilfont")]
pub use super::legacy_common::source_budget;
#[cfg(feature = "vlw")]
pub use super::legacy_common::source_budget_for;
pub use super::legacy_common::{Budget, audit, bitmap_limits, check, finish, invalid};
use crate::bitmap::{Bitmap, Bounds, Gray8, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Vector};
use crate::{ResourceLimits, Result};
use std::collections::BTreeMap;
pub fn slice(b: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(len)
            .ok_or_else(|| invalid("image font offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated image font"))
}
#[cfg(any(feature = "pilfont", feature = "sfont", feature = "vlw"))]
pub fn r16(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(slice(b, at, 2)?.try_into().unwrap()))
}
#[cfg(any(feature = "sfont", feature = "vlw"))]
pub fn r32(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(slice(b, at, 4)?.try_into().unwrap()))
}
#[cfg(feature = "vlw")]
pub fn i32be(b: &[u8], at: usize) -> Result<i32> {
    Ok(r32(b, at)? as i32)
}
pub fn area(w: u32, h: u32) -> Result<usize> {
    usize::try_from(u64::from(w) * u64::from(h))
        .map_err(|_| invalid("image font pixel count overflow"))
}
pub fn allocation(n: usize, l: &ResourceLimits, b: &mut Budget) -> Result<()> {
    check(n, l.max_glyphs, "image font glyphs")?;
    b.items::<Font>(1)?;
    b.items::<Option<Glyph>>(n)?;
    b.add(n.saturating_mul(512).saturating_add(1024))
}
#[allow(clippy::too_many_arguments)]
pub fn glyph(
    w: u32,
    h: u32,
    x: i64,
    y: i64,
    advance: Vector,
    pixels: &[u8],
    gray: bool,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Glyph> {
    let n = area(w, h)?;
    check(n, l.max_pixels_per_glyph, "image font glyph pixels")?;
    if pixels.len() != n {
        return Err(invalid("image glyph pixel length mismatch"));
    }
    b.add(n.saturating_mul(2).saturating_add(512))?;
    let bitmap = if gray || pixels.iter().any(|&x| x != 0 && x != 255) {
        GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(w, h, pixels, bitmap_limits(l))?)
    } else {
        let stride = (w as usize).div_ceil(8);
        let mut packed = vec![0; stride * h as usize];
        for (y, row) in pixels.chunks(w.max(1) as usize).enumerate() {
            for (x, &p) in row.iter().enumerate() {
                if p != 0 {
                    packed[y * stride + x / 8] |= 0x80 >> (x % 8)
                }
            }
        }
        GlyphBitmap::Mono(Bitmap::<Mono>::from_raw(w, h, &packed, bitmap_limits(l))?)
    };
    let mut g = Glyph::new(bitmap);
    g.metrics.bounds = Bounds::new(w, h, x, y);
    g.metrics.advance = Some(advance);
    g.metrics.left_bearing = Some(x);
    g.metrics.right_bearing = Some(advance.x - x - i64::from(w));
    Ok(g)
}
pub fn set(mut f: Font, id: &str) -> FontSet {
    f.source_format = Some(id.into());
    f.into()
}
pub fn select<'a>(
    f: &'a Font,
    unicode: bool,
    max: u32,
    coerce: bool,
    b: &mut Budget,
) -> Result<BTreeMap<u32, &'a Glyph>> {
    let mut out = BTreeMap::new();
    for (_, g) in f.glyphs() {
        for &cp in &g.codepoints {
            let c = match (cp, unicode) {
                (Codepoint::Unicode(c), true) | (Codepoint::Legacy(c), false) => Some(c),
                (Codepoint::Unicode(c), false) if coerce => Some(c),
                _ => None,
            };
            if let Some(c) = c.filter(|&c| c <= max) {
                b.add(96)?;
                out.insert(c, g);
            }
        }
    }
    if out.is_empty() {
        return Err(invalid("no representable image-font mappings"));
    }
    Ok(out)
}
pub fn pixel(g: &Glyph, x: u32, y: u32) -> u8 {
    match &g.bitmap {
        GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap() * 255,
        GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
    }
}
#[cfg(any(feature = "pilfont", feature = "sfont", feature = "vlw"))]
pub fn output(n: usize, cap: usize, l: &ResourceLimits, b: &mut Budget) -> Result<Vec<u8>> {
    check(
        n,
        cap.min(l.max_input_bytes)
            .min(l.max_table_bytes)
            .min(l.max_resource_bytes),
        "image font output bytes",
    )?;
    b.add(n)?;
    Ok(vec![0; n])
}
#[cfg(any(feature = "pilfont", feature = "sfont", feature = "vlw"))]
macro_rules! codec {
    ($codec:ident,$read:ident,$write:ident) => {
        /// Native format codec and typed options.
        #[derive(Debug, Clone, Default)]
        pub struct $codec {
            /// Native reader options.
            pub read_options: $read,
            /// Native writer options.
            pub write_options: $write,
        }
        impl super::FormatDetector for $codec {
            fn detect(
                &self,
                c: &mut crate::resources::ReadContext<'_>,
            ) -> crate::Result<super::Confidence> {
                Ok(detect(c.bytes()))
            }
        }
        impl super::FormatReader<crate::FontSet> for $codec {
            fn read(
                &self,
                c: &mut crate::resources::ReadContext<'_>,
                _: &super::ReadOptions,
            ) -> crate::Result<(crate::FontSet, crate::Report)> {
                read(c, &self.read_options)
            }
        }
        impl super::FormatWriter<crate::FontSet> for $codec {
            fn write(
                &self,
                s: &crate::FontSet,
                o: &super::WriteOptions,
            ) -> crate::Result<crate::resources::Written> {
                write(s, o, &self.write_options)
            }
        }
    };
}
#[cfg(any(feature = "pilfont", feature = "sfont", feature = "vlw"))]
pub(crate) use codec;

#[cfg(any(feature = "pilfont", feature = "sfont", feature = "image"))]
mod raster_io {
    use super::*;
    use image_codec::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader};
    use std::io::{Cursor, Write};
    fn gif_single_frame(b: &[u8]) -> Result<()> {
        if b.len() < 13 {
            return Err(invalid("truncated GIF"));
        }
        let mut p = 13usize;
        if b[10] & 128 != 0 {
            p += 3 * (1usize << ((b[10] & 7) + 1));
        }
        let mut frames = 0;
        loop {
            let marker = *b.get(p).ok_or_else(|| invalid("truncated GIF blocks"))?;
            p += 1;
            match marker {
                0x3b => {
                    return if frames == 1 {
                        Ok(())
                    } else {
                        Err(invalid("GIF has no image"))
                    };
                }
                0x21 => {
                    p = p.checked_add(1).ok_or_else(|| invalid("GIF overflow"))?;
                }
                0x2c => {
                    frames += 1;
                    if frames > 1 {
                        return Err(invalid("animated GIF is not a single font atlas"));
                    }
                    let d = b
                        .get(p..p + 9)
                        .ok_or_else(|| invalid("truncated GIF descriptor"))?;
                    p += 9;
                    if d[8] & 128 != 0 {
                        p += 3 * (1usize << ((d[8] & 7) + 1));
                    }
                    p += 1;
                }
                _ => return Err(invalid("invalid GIF block")),
            }
            loop {
                let n = usize::from(*b.get(p).ok_or_else(|| invalid("truncated GIF subblock"))?);
                p += 1;
                if n == 0 {
                    break;
                }
                p = p.checked_add(n).ok_or_else(|| invalid("GIF overflow"))?;
                if p > b.len() {
                    return Err(invalid("truncated GIF data"));
                }
            }
        }
    }

    /// Decoded RGBA pixels with their original native channel interpretation.
    pub struct Raster {
        pub width: u32,
        pub height: u32,
        pub color: image_codec::ColorType,
        pub pixels: Vec<u8>,
    }
    pub fn decode(b: &[u8], name: &str, l: &ResourceLimits, budget: &mut Budget) -> Result<Raster> {
        check(
            b.len(),
            l.max_resource_bytes.min(l.max_table_bytes),
            "image font resource bytes",
        )?;
        let format = image_codec::guess_format(b)
            .or_else(|_| ImageFormat::from_path(name))
            .map_err(|_| invalid("unknown font image container"))?;
        if !matches!(
            format,
            ImageFormat::Png
                | ImageFormat::Bmp
                | ImageFormat::Gif
                | ImageFormat::Pnm
                | ImageFormat::Tga
        ) {
            return Err(invalid("unsupported font image container"));
        }
        if format == ImageFormat::Gif {
            gif_single_frame(b)?;
        }
        let mut hint = 0;
        if format == ImageFormat::Png {
            let header = slice(b, 16, 8)?;
            hint = area(
                u32::from_be_bytes(header[..4].try_into().unwrap()),
                u32::from_be_bytes(header[4..].try_into().unwrap()),
            )?;
            let mut at = 8;
            while at < b.len() {
                let n = u32::from_be_bytes(slice(b, at, 4)?.try_into().unwrap()) as usize;
                let tag = slice(b, at + 4, 4)?;
                slice(b, at + 8, n.saturating_add(4))?;
                if tag == b"acTL" {
                    return Err(invalid("animated PNG is not a single font atlas"));
                }
                at = at
                    .checked_add(n)
                    .and_then(|n| n.checked_add(12))
                    .ok_or_else(|| invalid("PNG chunk overflow"))?;
            }
        }
        // Decoder limits also cover compressed metadata before the first frame.
        let header_cap = hint
            .saturating_mul(16)
            .saturating_add(b.len().saturating_mul(4))
            .saturating_add(65536)
            .min(l.max_decompressed_bytes)
            .min(budget.max.saturating_sub(budget.used));
        budget.add(header_cap)?;
        let mut reader = ImageReader::with_format(Cursor::new(b), format);
        let mut limits = image_codec::Limits::default();
        limits.max_alloc = Some(header_cap as u64);
        reader.limits(limits);
        let mut decoder = reader
            .into_decoder()
            .map_err(|e| invalid(format!("font image header: {e}")))?;
        let (w, h) = decoder.dimensions();
        if w == 0 || h == 0 {
            return Err(invalid("zero font image dimension"));
        }
        let n = area(w, h)?;
        check(
            n.saturating_mul(4),
            l.max_decompressed_bytes,
            "decoded font image bytes",
        )?;
        let native = decoder.color_type();
        let scratch = n
            .saturating_mul(16)
            .saturating_add(65536)
            .saturating_add(b.len().saturating_mul(2));
        budget.add(scratch)?;
        let mut limits = image_codec::Limits::default();
        limits.max_alloc = Some(scratch as u64);
        decoder
            .set_limits(limits)
            .map_err(|e| invalid(format!("font image limits: {e}")))?;
        let pixels = DynamicImage::from_decoder(decoder)
            .map_err(|e| invalid(format!("font image pixels: {e}")))?
            .into_rgba8()
            .into_raw();
        Ok(Raster {
            width: w,
            height: h,
            color: native,
            pixels,
        })
    }
    struct Sink {
        bytes: Vec<u8>,
        cap: usize,
        limited: bool,
    }
    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let next = self.bytes.len().checked_add(b.len());
            if next.is_none_or(|n| n > self.cap) {
                self.limited = true;
                return Err(std::io::Error::other("font PNG output limit"));
            }
            self.bytes
                .try_reserve_exact(b.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    /// Bounded deterministic PNG; external PNG libraries are ordinary codecs.
    pub fn png(
        w: u32,
        h: u32,
        pixels: &[u8],
        rgba: bool,
        cap: usize,
        l: &ResourceLimits,
        budget: &mut Budget,
    ) -> Result<Vec<u8>> {
        let n = area(w, h)?;
        let channels = if rgba { 4 } else { 1 };
        if w == 0 || h == 0 || pixels.len() != n.saturating_mul(channels) {
            return Err(invalid("invalid font PNG geometry"));
        }
        check(pixels.len(), l.max_decompressed_bytes, "font PNG pixels")?;
        let cap = cap
            .min(l.max_input_bytes)
            .min(l.max_resource_bytes)
            .min(l.max_table_bytes);
        let bound = pixels
            .len()
            .saturating_mul(2)
            .saturating_add(h as usize)
            .saturating_add(1024);
        budget.add(
            pixels
                .len()
                .saturating_mul(8)
                .saturating_add(65536)
                .saturating_add(bound.min(cap).saturating_mul(2)),
        )?;
        let mut sink = Sink {
            bytes: Vec::new(),
            cap,
            limited: false,
        };
        let encoder = image_codec::codecs::png::PngEncoder::new_with_quality(
            &mut sink,
            image_codec::codecs::png::CompressionType::Fast,
            image_codec::codecs::png::FilterType::Adaptive,
        );
        let result = encoder.write_image(
            pixels,
            w,
            h,
            if rgba {
                image_codec::ExtendedColorType::Rgba8
            } else {
                image_codec::ExtendedColorType::L8
            },
        );
        if sink.limited {
            return Err(crate::Error::ResourceLimit {
                resource: "font PNG output bytes",
                limit: cap,
            });
        }
        result.map_err(|e| invalid(format!("font PNG encoding: {e}")))?;
        Ok(sink.bytes)
    }
}
#[cfg(any(feature = "pilfont", feature = "sfont", feature = "image"))]
pub use raster_io::{Raster, decode, png};
