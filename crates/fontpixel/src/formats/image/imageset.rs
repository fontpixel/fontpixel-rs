//! Native one-file-per-glyph directory, with labels in numeric filenames.
use super::{Encoding, ImageWriteOptions, Sink};
use crate::formats::image_bundle_common::*;
use crate::model::{Codepoint, Font, FontSet, Vector};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Report, ResourceLimits, Result, WriteOptions};
use image_codec::{DynamicImage, GrayImage};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
/// Native image organization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageLayout {
    /// One atlas with a separate mapping document.
    #[default]
    Atlas,
    /// A directory whose numeric filenames are the glyph labels.
    Imageset,
}
/// Domain supplied by the caller; filenames themselves carry only integers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageSetDomain {
    /// Original encoding numbers.
    #[default]
    Legacy,
    /// Unicode scalar values.
    Unicode,
}
/// Explicit coverage projection for the native glyph images.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageSetBackground {
    /// Black background, white foreground; grayscale is coverage.
    #[default]
    Black,
    /// White background, black foreground; grayscale is inverted coverage.
    White,
    /// Transparent background; the alpha channel is coverage.
    Alpha,
}
/// Native directory naming and interpretation, with no private metadata file.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ImageSetOptions {
    /// Literal filename prefix, default empty; must contain no path separator.
    pub prefix: String,
    /// Integer radix from 2 through 36, default hexadecimal.
    pub radix: u32,
    /// Explicit interpretation of the filename number.
    pub domain: ImageSetDomain,
    /// Explicit grayscale or transparency interpretation.
    pub background: ImageSetBackground,
    /// Optional baseline measured from each image's top; absent means bottom.
    pub baseline: Option<i64>,
}
impl Default for ImageSetOptions {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            radix: 16,
            domain: ImageSetDomain::Legacy,
            background: ImageSetBackground::Black,
            baseline: None,
        }
    }
}
fn validate(o: &ImageSetOptions) -> Result<()> {
    if !(2..=36).contains(&o.radix) || o.prefix.contains(['/', '\\', '\0']) {
        return Err(invalid("invalid imageset filename profile"));
    }
    Ok(())
}
fn label(path: &ResourcePath, o: &ImageSetOptions) -> Result<Codepoint> {
    let name = path.as_str().rsplit('/').next().unwrap();
    let (stem, _) = name
        .rsplit_once('.')
        .ok_or_else(|| invalid("imageset file has no extension"))?;
    Encoding::from_name(name)?;
    let digits = stem
        .strip_prefix(&o.prefix)
        .ok_or_else(|| invalid("imageset filename prefix mismatch"))?;
    if digits.is_empty() || !digits.bytes().all(|v| char::from(v).is_digit(o.radix)) {
        return Err(invalid("imageset filename has no valid numeric label"));
    }
    let value =
        u32::from_str_radix(digits, o.radix).map_err(|_| invalid("imageset label overflow"))?;
    Ok(match o.domain {
        ImageSetDomain::Legacy => Codepoint::Legacy(value),
        ImageSetDomain::Unicode if char::from_u32(value).is_some() => Codepoint::Unicode(value),
        ImageSetDomain::Unicode => return Err(invalid("imageset label is not a Unicode scalar")),
    })
}
fn append(
    f: &mut Font,
    cp: Codepoint,
    raster: Raster,
    o: &ImageSetOptions,
    l: &ResourceLimits,
    b: &mut Budget,
    report: &mut Report,
) -> Result<()> {
    let n = area(raster.width, raster.height)?;
    check(n, l.max_pixels_per_glyph, "imageset glyph pixels")?;
    b.add(n)?;
    let mut pixels = Vec::with_capacity(n);
    let mut colored = false;
    for p in raster.pixels.as_chunks::<4>().0 {
        let value = match o.background {
            ImageSetBackground::Alpha => {
                colored |= p[3] != 0 && (p[0] != p[1] || p[1] != p[2]);
                p[3]
            }
            ImageSetBackground::Black | ImageSetBackground::White => {
                colored |= p[3] != 0 && (p[0] != p[1] || p[1] != p[2]);
                let gray =
                    ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114 + 500)
                        / 1000) as u8;
                let coverage = if o.background == ImageSetBackground::White {
                    255 - gray
                } else {
                    gray
                };
                ((u16::from(coverage) * u16::from(p[3]) + 127) / 255) as u8
            }
        };
        pixels.push(value);
    }
    if raster.color.bits_per_pixel() > raster.color.channel_count() as u16 * 8 {
        b.diagnostic(
            report,
            "image",
            "imageset.depth",
            crate::report::Impact::Loss,
            Some(0),
            Some(f.len()),
            "Image sample depth was reduced to 8-bit coverage",
        )?;
    }
    if colored {
        b.diagnostic(
            report,
            "image",
            "imageset.color",
            crate::report::Impact::Loss,
            Some(0),
            Some(f.len()),
            "Colored glyph pixels were projected to grayscale coverage",
        )?;
    }
    let y = o
        .baseline
        .unwrap_or(i64::from(raster.height))
        .checked_sub(i64::from(raster.height))
        .ok_or_else(|| invalid("imageset baseline overflow"))?;
    let mut g = glyph(
        raster.width,
        raster.height,
        0,
        y,
        Vector::new(i64::from(raster.width), 0),
        &pixels,
        false,
        l,
        b,
    )?;
    g.codepoints.push(cp);
    f.add_glyph(g)?;
    Ok(())
}
fn read_budget(
    c: &mut ReadContext<'_>,
    o: &ImageSetOptions,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    validate(o)?;
    b.add(o.prefix.len().saturating_mul(8))?;
    let l = c.limits().clone();
    let before = c.retained_input_bytes();
    let paths = c.list_resources_limited(
        None,
        l.max_glyphs.min(l.max_resources.saturating_sub(1)),
        b.max - b.used,
    )?;
    b.add(c.retained_input_bytes() - before)?;
    if paths.is_empty() {
        return Err(invalid("empty imageset directory"));
    }
    allocation(paths.len(), &l, b)?;
    b.items::<(Codepoint, ResourcePath)>(paths.len())?;
    let mut ordered = BTreeMap::new();
    for p in paths {
        let cp = label(&p, o)?;
        if ordered.insert(cp, p).is_some() {
            return Err(invalid("duplicate imageset numeric label"));
        }
    }
    let mut f = Font::default();
    let mut report = Report::default();
    for (cp, path) in ordered {
        let bytes = c.resource_limited(&path, b.max - b.used)?;
        b.add(bytes.len())?;
        let mut raster_limits = l.clone();
        raster_limits.max_decompressed_bytes = raster_limits
            .max_decompressed_bytes
            .min(l.max_pixels_per_glyph.saturating_mul(4));
        let raster = decode(&bytes, path.as_str(), &raster_limits, b)?;
        append(&mut f, cp, raster, o, &l, b, &mut report)?;
    }
    Ok((set(f, "image"), report))
}
pub(super) fn read(c: &mut ReadContext<'_>, o: &ImageSetOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    read_budget(c, o, &mut b)
}
fn number(mut n: u32, radix: u32) -> String {
    let mut digits = [0u8; 32];
    let mut at = digits.len();
    loop {
        at -= 1;
        let d = (n % radix) as u8;
        digits[at] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        n /= radix;
        if n == 0 {
            break;
        }
    }
    String::from_utf8(digits[at..].to_vec()).unwrap()
}
fn encode(
    g: &crate::model::Glyph,
    enc: Encoding,
    v: &ImageWriteOptions,
    b: &mut Budget,
) -> Result<Vec<u8>> {
    let l = &v.limits;
    let (w, h) = (g.bitmap.width(), g.bitmap.height());
    if w == 0 || h == 0 {
        return Err(invalid(
            "native imageset images require nonzero width and height",
        ));
    }
    let n = area(w, h)?;
    check(
        n.saturating_mul(4),
        l.max_decompressed_bytes,
        "imageset decoded pixels",
    )?;
    b.add(n.saturating_mul(16).saturating_add(65536))?;
    let mut p = Vec::with_capacity(n);
    for y in 0..h {
        for x in 0..w {
            let mut z = pixel(g, x, y);
            if matches!(enc, Encoding::Gif | Encoding::Pbm)
                && matches!(g.bitmap, crate::model::GlyphBitmap::Gray8(_))
            {
                let threshold = v
                    .gray_threshold
                    .ok_or_else(|| invalid("Gray8 imageset GIF/PBM requires gray_threshold"))?;
                z = if z >= threshold { 255 } else { 0 };
            }
            p.push(if v.imageset.background == ImageSetBackground::White {
                255 - z
            } else {
                z
            });
        }
    }
    if enc == Encoding::Png {
        if v.imageset.background == ImageSetBackground::Alpha {
            let mut rgba = Vec::with_capacity(n * 4);
            for alpha in p {
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            return png(w, h, &rgba, true, l.max_resource_bytes, l, b);
        }
        return png(w, h, &p, false, l.max_resource_bytes, l, b);
    }
    let cap = l
        .max_input_bytes
        .min(l.max_resource_bytes)
        .min(l.max_table_bytes)
        .min((b.max - b.used) / 2);
    let mut sink = Sink {
        cursor: Cursor::new(Vec::new()),
        max: cap,
    };
    if v.imageset.background == ImageSetBackground::Alpha {
        if !matches!(enc, Encoding::Png | Encoding::Tga) {
            return Err(invalid("alpha imageset output requires PNG or TGA"));
        }
        let mut rgba = Vec::with_capacity(n * 4);
        for alpha in p {
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
        image_codec::codecs::tga::TgaEncoder::new(&mut sink)
            .disable_rle()
            .encode(&rgba, w, h, image_codec::ExtendedColorType::Rgba8)
            .map_err(|e| invalid(e.to_string()))?;
    } else if enc == Encoding::Pbm {
        write!(sink, "P4\n{w} {h}\n")?;
        let mut row = vec![0; (w as usize).div_ceil(8)];
        for y in 0..h {
            row.fill(0);
            for x in 0..w {
                if p[(y * w + x) as usize] == 0 {
                    row[x as usize / 8] |= 0x80 >> (x % 8);
                }
            }
            sink.write_all(&row)?;
        }
    } else if enc == Encoding::Pgm {
        write!(sink, "P5\n{w} {h}\n255\n")?;
        sink.write_all(&p)?;
    } else if enc == Encoding::Tga {
        image_codec::codecs::tga::TgaEncoder::new(&mut sink)
            .disable_rle()
            .encode(&p, w, h, image_codec::ExtendedColorType::L8)
            .map_err(|e| invalid(e.to_string()))?;
    } else {
        let image = DynamicImage::ImageLuma8(GrayImage::from_raw(w, h, p).unwrap());
        let image = if matches!(enc, Encoding::Bmp | Encoding::Gif) {
            DynamicImage::ImageRgba8(image.to_rgba8())
        } else {
            image
        };
        image
            .write_to(&mut sink, enc.format())
            .map_err(|e| invalid(e.to_string()))?;
    }
    let bytes = sink.cursor.into_inner();
    b.add(bytes.len().saturating_mul(2))?;
    Ok(bytes)
}
pub(super) fn write(s: &FontSet, o: &WriteOptions, v: &ImageWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        validate(&v.imageset)?;
        let mut b = source_budget(s, &v.limits)?;
        b.add(
            v.imageset
                .prefix
                .len()
                .saturating_add(o.primary_name.as_str().len())
                .saturating_mul(8),
        )?;
        let f = s.single()?;
        let selected = select(
            f,
            v.imageset.domain == ImageSetDomain::Unicode,
            u32::MAX,
            false,
            &mut b,
        )?;
        check(
            selected.len().saturating_add(1),
            v.limits.max_resources,
            "imageset resource operations",
        )?;
        allocation(selected.len(), &v.limits, &mut b)?;
        b.items::<OutputAsset>(selected.len())?;
        let enc = v.encoding.unwrap_or(Encoding::Png);
        let ext = match enc {
            Encoding::Png => "png",
            Encoding::Bmp => "bmp",
            Encoding::Gif => "gif",
            Encoding::Tga => "tga",
            Encoding::Pbm => "pbm",
            Encoding::Pgm => "pgm",
        };
        let mut assets = Vec::new();
        let mut target = Font::default();
        for (cp, g) in selected {
            let filename = format!(
                "{}{}.{}",
                v.imageset.prefix,
                number(cp, v.imageset.radix),
                ext
            );
            b.add(
                filename
                    .len()
                    .saturating_add(o.primary_name.as_str().len())
                    .saturating_mul(8)
                    .saturating_add(256),
            )?;
            let path = ResourcePath::new(format!("{}/{filename}", o.primary_name))?;
            let bytes = encode(g, enc, v, &mut b)?;
            let raster = decode(&bytes, &filename, &v.limits, &mut b)?;
            let label = label(&path, &v.imageset)?;
            append(
                &mut target,
                label,
                raster,
                &v.imageset,
                &v.limits,
                &mut b,
                &mut report,
            )?;
            assets.push(OutputAsset { path, bytes });
        }
        audit(s, &set(target, "image"), "image", &mut report, &mut b)?;
        OutputBundle::directory(o.primary_name.clone(), assets)
    })();
    finish(result, report, "image", o.strict)
}
