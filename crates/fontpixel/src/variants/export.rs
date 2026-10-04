use super::*;
use crate::{
    Bitmap, Report,
    outline::{self, Shape},
    report::{Impact, Level},
    sfnt::{
        Flavor, SfntBuilder, bitmap as sbit, glyf,
        metrics::{self, Axis, Header, LongMetric},
        name::{NameRecord, NameTable},
        os2::Os2Table,
        post::PostTable,
    },
};
/// Outline encoding inside the sfnt container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutlineFlavor {
    /// Quadratic outlines with shared-pixel composites for dots.
    #[default]
    TrueType,
    /// Cubic CFF1 outlines (requires `cff`).
    Cff,
}
/// Native default-scale dot composite representation; pixels and glyph IDs agree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DotReuse {
    /// One circle component per pixel, favoring repetitive webfont compression.
    Literal,
    /// Reuse existing one-row patterns, balancing sfnt size and compression ratio.
    Rows,
    /// Reuse bounded rows, columns and tiny 2D patterns for fastest native export.
    #[default]
    Patterns,
}
/// Native variant output choices, independent from the outer webfont container.
#[derive(Clone, Debug)]
pub struct Options {
    /// Outline shape.
    pub shape: Shape,
    /// Pixel diameter/side; merged squares require 1.
    pub dot_size: f64,
    /// Dot representation at 100 units/pixel and diameter 0.9; default Patterns.
    pub dot_reuse: DotReuse,
    /// Font design units per source pixel.
    pub units_per_pixel: u16,
    /// Override units per em; default pixel size times scale.
    pub units_per_em: Option<u16>,
    /// TrueType or CFF.
    pub flavor: OutlineFlavor,
    /// Include a matching monochrome EBDT/EBLC strike.
    pub embed_bitmaps: bool,
    /// Explicit Gray8 conversion; absence rejects gray outlines.
    pub gray_threshold: Option<u8>,
    /// Override full family name.
    pub family: Option<String>,
    /// Style name; default Square/SquareSeparate/Dot.
    pub style: Option<String>,
    /// Legacy subfamily (name ID 2); absent preserves `Regular`, independent of ID 17.
    /// Ignored when an exact `names` table is supplied.
    pub legacy_style: Option<String>,
    /// Exact full display name; absent retains family/style generation.
    /// Ignored when an exact `names` table is supplied.
    pub full_name: Option<String>,
    /// Exact PostScript name; absent retains existing generation.
    /// Ignored when an exact `names` table is supplied.
    pub postscript_name: Option<String>,
    /// Deterministic version text without the `Version ` prefix.
    pub version: String,
    /// Four-byte vendor identifier.
    pub vendor: [u8; 4],
    /// Optional copyright override.
    pub copyright: Option<String>,
    /// Optional license override.
    pub license: Option<String>,
    /// Optional license URL override.
    pub license_url: Option<String>,
    /// Replace generated names with an exact caller-supplied name table.
    pub names: Option<NameTable>,
    /// Layout, source and output limits.
    pub limits: LayoutLimits,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            shape: Shape::Square,
            dot_size: 1.,
            dot_reuse: DotReuse::default(),
            units_per_pixel: 100,
            units_per_em: None,
            flavor: OutlineFlavor::TrueType,
            embed_bitmaps: false,
            gray_threshold: None,
            family: None,
            style: None,
            legacy_style: None,
            full_name: None,
            postscript_name: None,
            version: "1.0".into(),
            vendor: *b"NONE",
            copyright: None,
            license: None,
            license_url: None,
            names: None,
            limits: LayoutLimits::default(),
        }
    }
}
/// Complete standalone sfnt output and conversion diagnostics.
#[derive(Clone, Debug)]
pub struct Encoded {
    /// TTF or OTF bytes with native GSUB.
    pub bytes: Vec<u8>,
    /// Source information that could not be represented.
    pub report: Report,
}
fn validate_names(o: &Options) -> Result<()> {
    if o.names.is_some() {
        return Ok(());
    }
    for (value, resource, missing) in [
        (&o.legacy_style, "legacy style bytes", "legacy_style requires nonempty identity text"),
        (&o.full_name, "full name bytes", "full_name requires nonempty identity text"),
        (&o.postscript_name, "PostScript name bytes", "postscript_name requires nonempty identity text"),
    ] {
        if let Some(value) = value {
            sfnt::bounded(value.len(), o.limits.sfnt.resources.max_table_bytes, resource)?;
            if value.trim().is_empty() {
                return Err(invalid(missing));
            }
        }
    }
    Ok(())
}
fn i16n(v: i64) -> Result<i16> {
    i16::try_from(v).map_err(|_| invalid("metric exceeds signed 16-bit range"))
}
fn u16n(v: i64) -> Result<u16> {
    u16::try_from(v).map_err(|_| invalid("advance exceeds unsigned 16-bit range"))
}
fn note(r: &mut Report, code: &str, message: &str) {
    r.push(crate::ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: code.into(),
        location: Default::default(),
        message: message.into(),
    });
}
fn mono(g: &Glyph, threshold: Option<u8>, r: &mut Report) -> Result<Bitmap> {
    match &g.bitmap {
        GlyphBitmap::Mono(b) => Ok(b.clone()),
        GlyphBitmap::Gray8(b) => {
            let t =
                threshold.ok_or_else(|| invalid("gray outline requires an explicit threshold"))?;
            if !r
                .entries
                .iter()
                .any(|e| e.code == "variants.gray_threshold")
            {
                note(
                    r,
                    "variants.gray_threshold",
                    "Gray coverage was explicitly thresholded for monochrome outlines",
                );
            }
            b.to_mono(t)
        }
    }
}
fn notdef() -> Result<Glyph> {
    let mut b: Bitmap = Bitmap::new(5, 10)?;
    for y in 0..10 {
        for x in 0..5 {
            if x == 0 || x == 4 || y == 0 || y == 9 {
                b.set_pixel(x, y, 1)?;
            }
        }
    }
    let mut g = Glyph::new(b);
    g.metrics.bounds.x = 1;
    g.metrics.advance = Some(crate::model::Vector::new(7, 0));
    Ok(g)
}
/// Vectorize planned variants using the native, independently checked table codecs.
pub fn encode(variants: &Variants, o: &Options) -> Result<Encoded> {
    validate_names(o)?;
    if !(o.dot_size > 0. && o.dot_size <= 1.5) || (o.shape == Shape::Square && o.dot_size != 1.) {
        return Err(invalid("invalid dot size for the chosen outline shape"));
    }
    let plan = variants.plan(&o.limits)?;
    encode_planned(&variants.base, o, &plan, None)
}
pub(super) fn encode_prepared(base: &Font, o: &Options, plan: &Plan) -> Result<Encoded> {
    encode_planned(base, o, plan, None)
}
fn encode_planned(
    base: &Font,
    o: &Options,
    plan: &Plan,
    source_notdef: Option<Glyph>,
) -> Result<Encoded> {
    if !(o.dot_size > 0. && o.dot_size <= 1.5) || (o.shape == Shape::Square && o.dot_size != 1.) {
        return Err(invalid("invalid dot size for the chosen outline shape"));
    }
    let l = &o.limits.sfnt;
    validate_names(o)?;
    let scale = i64::from(o.units_per_pixel);
    if scale == 0 {
        return Err(invalid("units_per_pixel must be positive"));
    }
    let upm = o.units_per_em.unwrap_or_else(|| {
        u16::try_from(
            base.metrics
                .pixel_size
                .unwrap_or(14)
                .saturating_mul(u32::from(o.units_per_pixel)),
        )
        .unwrap_or(0)
    });
    if !(16..=16384).contains(&upm) {
        return Err(invalid("units_per_em outside 16..=16384"));
    }
    let ascent = i16n(
        base.metrics
            .ascent
            .unwrap_or(i64::from(base.metrics.pixel_size.unwrap_or(14)))
            .checked_mul(scale)
            .ok_or_else(|| invalid("ascent overflow"))?,
    )?;
    let descent = i16n(
        base.metrics
            .descent
            .unwrap_or(0)
            .checked_mul(-scale)
            .ok_or_else(|| invalid("descent overflow"))?,
    )?;
    let family = o
        .family
        .as_deref()
        .or(base.names.family.as_deref())
        .unwrap_or("Fontpixel");
    let style = o.style.as_deref().unwrap_or(match o.shape {
        Shape::Square => "Square",
        Shape::SquareSeparate => "SquareSeparate",
        Shape::Dot => "Dot",
    });
    let fullname = o.full_name.as_ref().filter(|_| o.names.is_none()).cloned().unwrap_or_else(|| format!("{family} {style}"));
    let psname = o.postscript_name.as_ref().filter(|_| o.names.is_none()).cloned().unwrap_or_else(|| format!(
        "{}-{style}",
        family
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
    ));
    let mut report = Report::default();
    let retain_names = source_notdef.is_some();
    let notdef = source_notdef.map_or_else(notdef, Ok)?;
    let sources = std::iter::once((".notdef", &notdef))
        .chain(plan.glyphs.iter().map(|(n, g)| (n.as_str(), g)))
        .collect::<Vec<_>>();
    let mut tt = glyf::GlyfTable { glyphs: Vec::new() };
    #[cfg(feature = "cff")]
    let mut cff_glyphs = Vec::new();
    let shared = o.flavor == OutlineFlavor::TrueType && o.shape == Shape::Dot;
    // Existing small ink-pattern glyphs shorten default Tong dot composites without
    // introducing any new glyph IDs. Other scales/diameters retain direct dots.
    let dot_rows = (shared
        && o.units_per_pixel == 100
        && o.dot_size == 0.9
        && o.dot_reuse != DotReuse::Literal)
        .then(|| {
            super::dot_rows::Catalogue::new(&sources, scale, o.dot_reuse == DotReuse::Patterns)
        });
    let count = sources.len() + usize::from(shared);
    l.glyphs(count)?;
    let dot_id = sources.len() as u16;
    let vertical = base.metrics.glyph_defaults.vertical_advance.is_some()
        || sources.iter().any(|(_, g)| {
            g.metrics.vertical_advance.is_some() || g.metrics.vertical_origin.is_some()
        });
    let mut vm = Vec::new();
    let mut vertical_extents = Vec::new();
    let mut hm = Vec::new();
    let mut extents = Vec::new();
    let gl = glyf::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    let mut total_points = 0usize;
    let mut total_components = 0usize;
    let mut total_dot_pixels = 0usize;
    #[cfg(feature = "cff")]
    let mut total_commands = 0usize;
    let mut bitmaps = BTreeMap::new();
    for (i, (name, g)) in sources.iter().enumerate() {
        let bitmap = mono(g, o.gray_threshold, &mut report)?;
        let x = i32::try_from(g.metrics.bounds.x).map_err(|_| invalid("glyph x exceeds int32"))?;
        let y = i32::try_from(g.metrics.bounds.y).map_err(|_| invalid("glyph y exceeds int32"))?;
        let adv = g
            .metrics
            .advance
            .unwrap_or(crate::model::Vector::new(i64::from(bitmap.width()), 0));
        if adv.y != 0 {
            return Err(invalid(
                "sfnt cannot represent nonzero horizontal advance y",
            ));
        }
        let advance = u16n(
            adv.x
                .checked_mul(scale)
                .ok_or_else(|| invalid("advance overflow"))?,
        )?;
        let generated_notdef = i == 0 && !retain_names;
        let shape = if generated_notdef {
            Shape::Square
        } else {
            o.shape
        };
        let opts = outline::Options {
            shape,
            units_per_pixel: i32::from(o.units_per_pixel),
            dot_size: if generated_notdef { 1. } else { o.dot_size },
            origin: (x, y),
        };
        let ink = bitmap.bbox();
        let inset = if shape == Shape::Square {
            0.
        } else {
            (1. - opts.dot_size) * f64::from(o.units_per_pixel) / 2.
        };
        let bearing = if generated_notdef {
            i16n(scale)?
        } else if ink.width == 0 {
            0
        } else {
            i16n(((g.metrics.bounds.x + ink.x) as f64 * scale as f64 + inset).round() as i64)?
        };
        let width = if ink.width == 0 {
            None
        } else {
            Some(u16n(
                (f64::from(ink.width) * scale as f64 - 2. * inset).round() as i64,
            )?)
        };
        let mut bitmap_vertical = None;
        if vertical {
            let va = g
                .metrics
                .vertical_advance
                .unwrap_or(crate::model::Vector::new(
                    0,
                    -i64::from(base.metrics.pixel_size.unwrap_or(14)),
                ));
            if va.x != 0 {
                return Err(invalid("vertical advance x cannot be represented by vmtx"));
            }
            let vadvance = u16n(
                va.y.checked_mul(-scale)
                    .ok_or_else(|| invalid("vertical advance overflow"))?,
            )?;
            let origin = if generated_notdef {
                None
            } else {
                g.metrics.vertical_origin
            };
            if origin.is_some_and(|p| p.x.checked_mul(2) != Some(adv.x)) {
                return Err(invalid(
                    "non-centered vertical origin x cannot be represented by vmtx",
                ));
            }
            let origin_y = if let Some(p) = origin {
                p.y.checked_mul(scale)
                    .ok_or_else(|| invalid("vertical origin overflow"))?
            } else {
                i64::from(ascent)
            };
            let top = if ink.height == 0 && retain_names {
                0
            } else {
                (g.metrics.bounds.y + ink.y + i64::from(ink.height))
                    .checked_mul(scale)
                    .ok_or_else(|| invalid("vertical top overflow"))?
            };
            let bearing = if ink.height == 0 && !retain_names {
                0
            } else {
                i16n(
                    origin_y
                        .checked_sub(top)
                        .ok_or_else(|| invalid("vertical bearing overflow"))?
                        .checked_add(if ink.height == 0 {
                            0
                        } else {
                            inset.round() as i64
                        })
                        .ok_or_else(|| invalid("vertical bearing overflow"))?,
                )?
            };
            vm.push(LongMetric {
                advance: vadvance,
                bearing,
            });
            vertical_extents.push(if ink.height == 0 {
                None
            } else {
                Some(u16n(
                    (f64::from(ink.height) * scale as f64 - 2. * inset).round() as i64,
                )?)
            });
            if o.embed_bitmaps {
                let pixel_origin_y = origin.map_or(i64::from(ascent) / scale, |p| p.y);
                let pixel_origin_x = origin.map_or(adv.x / 2, |p| p.x);
                let narrow = |v| {
                    i8::try_from(v).map_err(|_| invalid("embedded vertical bearing exceeds int8"))
                };
                bitmap_vertical = Some(sbit::Bearing {
                    x: narrow(
                        g.metrics
                            .bounds
                            .x
                            .checked_sub(pixel_origin_x)
                            .ok_or_else(|| invalid("embedded vertical bearing overflow"))?,
                    )?,
                    y: narrow(
                        g.metrics
                            .bounds
                            .y
                            .checked_add(i64::from(bitmap.height()))
                            .and_then(|top| top.checked_sub(pixel_origin_y))
                            .ok_or_else(|| invalid("embedded vertical bearing overflow"))?,
                    )?,
                    advance: u8::try_from(
                        va.y.checked_neg()
                            .ok_or_else(|| invalid("embedded vertical advance overflow"))?,
                    )
                    .map_err(|_| invalid("embedded vertical advance exceeds uint8"))?,
                });
            }
        }
        hm.push(LongMetric { advance, bearing });
        extents.push(width);
        match o.flavor {
            OutlineFlavor::TrueType => {
                if shared && !generated_notdef {
                    // Sharing must not bypass the former literal-dot work budget.
                    total_dot_pixels = total_dot_pixels.saturating_add(
                        bitmap
                            .as_bytes()
                            .iter()
                            .map(|byte| byte.count_ones() as usize)
                            .sum::<usize>(),
                    );
                    sfnt::bounded(
                        total_dot_pixels,
                        gl.max_total_components,
                        "variant outline components",
                    )?;
                }
                let glyph = if shared && !generated_notdef {
                    if let Some(rows) = &dot_rows {
                        rows.glyph(i, &bitmap, &opts, dot_id, &gl)?
                    } else {
                        glyf::Glyph::pixels(&bitmap, &opts, dot_id, &gl)?
                    }
                } else {
                    glyf::Glyph::simple(outline::contours(&bitmap, &opts, &gl.geometry)?)
                };
                match &glyph {
                    glyf::Glyph::Simple(g) => {
                        total_points = total_points
                            .saturating_add(g.contours.iter().map(Vec::len).sum::<usize>())
                    }
                    glyf::Glyph::Composite(g) => {
                        total_components = total_components.saturating_add(g.components.len())
                    }
                    _ => (),
                }
                sfnt::bounded(total_points, gl.max_total_points, "variant outline points")?;
                sfnt::bounded(
                    total_components,
                    gl.max_total_components,
                    "variant outline components",
                )?;
                tt.glyphs.push(glyph);
            }
            OutlineFlavor::Cff => {
                #[cfg(feature = "cff")]
                {
                    let cl = sfnt::cff::Limits {
                        sfnt: l.clone(),
                        ..Default::default()
                    };
                    let glyph = sfnt::cff::Glyph::from_bitmap(
                        *name,
                        f64::from(advance),
                        &bitmap,
                        &opts,
                        &cl,
                    )?;
                    let commands = match &glyph.outline {
                        sfnt::cff::Outline::Path(p) => p.len(),
                        sfnt::cff::Outline::Dots(p) => p.len().saturating_mul(6),
                    };
                    total_commands = total_commands.saturating_add(commands);
                    sfnt::bounded(
                        total_commands,
                        cl.max_total_commands,
                        "variant CFF commands",
                    )?;
                    cff_glyphs.push(glyph);
                }
                #[cfg(not(feature = "cff"))]
                {
                    let _ = name;
                    return Err(Error::FeatureDisabled {
                        format: "cff".into(),
                    });
                }
            }
        }
        if o.embed_bitmaps {
            let narrow = |v: i64| {
                i8::try_from(v).map_err(|_| invalid("embedded bitmap bearing exceeds int8"))
            };
            bitmaps.insert(
                i as u16,
                sbit::BitmapGlyph {
                    metrics: sbit::Metrics {
                        width: u8::try_from(bitmap.width())
                            .map_err(|_| invalid("embedded bitmap width exceeds uint8"))?,
                        height: u8::try_from(bitmap.height())
                            .map_err(|_| invalid("embedded bitmap height exceeds uint8"))?,
                        horizontal: Some(sbit::Bearing {
                            x: narrow(g.metrics.bounds.x)?,
                            y: narrow(g.metrics.bounds.y + i64::from(bitmap.height()))?,
                            advance: u8::try_from(adv.x)
                                .map_err(|_| invalid("embedded bitmap advance exceeds uint8"))?,
                        }),
                        vertical: bitmap_vertical,
                    },
                    bitmap: GlyphBitmap::Mono(bitmap),
                },
            );
        }
    }
    if shared {
        tt.glyphs
            .push(glyf::Glyph::simple(vec![outline::pixel_contour(
                &outline::Options {
                    shape: Shape::Dot,
                    units_per_pixel: i32::from(o.units_per_pixel),
                    dot_size: o.dot_size,
                    origin: (0, 0),
                },
            )?]));
        hm.push(LongMetric {
            advance: o.units_per_pixel,
            bearing: i16n(((1. - o.dot_size) * scale as f64 / 2.).round() as i64)?,
        });
        extents.push(Some(u16n((o.dot_size * scale as f64).round() as i64)?));
    }
    if vertical && shared {
        vm.push(LongMetric {
            advance: upm,
            bearing: i16n(i64::from(ascent) - (o.dot_size * scale as f64).round() as i64)?,
        });
        vertical_extents.push(Some(u16n((o.dot_size * scale as f64).round() as i64)?));
    }
    let mut b = SfntBuilder::new(if o.flavor == OutlineFlavor::TrueType {
        Flavor::TrueType
    } else {
        Flavor::Cff
    });
    let (bounds, loca_format) = match o.flavor {
        OutlineFlavor::TrueType => {
            let e = tt.encode(glyf::LocaFormat::Auto, &gl)?;
            b.insert(*b"glyf", e.glyf)?;
            b.insert(*b"loca", e.loca)?;
            b.insert(*b"maxp", e.statistics.unhinted_maxp(count as u16)?)?;
            (e.bounds, e.index_to_loc_format)
        }
        OutlineFlavor::Cff => {
            #[cfg(feature = "cff")]
            {
                let f = sfnt::cff::Font {
                    postscript_name: psname.clone(),
                    full_name: if fullname.is_ascii()
                        || (o.names.is_none() && o.full_name.is_some())
                    {
                        fullname.clone()
                    } else {
                        psname.clone()
                    },
                    units_per_em: upm,
                    glyphs: cff_glyphs,
                    dots: sfnt::cff::DotOptions {
                        cell_size: scale as f64,
                        diameter: o.dot_size * scale as f64,
                    },
                };
                let e = f.encode(&sfnt::cff::Limits {
                    sfnt: l.clone(),
                    ..Default::default()
                })?;
                b.insert(*b"CFF ", e.data)?;
                b.insert(*b"maxp", e.maxp)?;
                (e.bounds, 0)
            }
            #[cfg(not(feature = "cff"))]
            return Err(Error::FeatureDisabled {
                format: "cff".into(),
            });
        }
    };
    let mut head = vec![0; 54];
    head[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    head[4..8].copy_from_slice(&0x10000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    head[16..18].copy_from_slice(&3u16.to_be_bytes());
    head[18..20].copy_from_slice(&upm.to_be_bytes());
    for (i, n) in bounds.iter().enumerate() {
        head[36 + 2 * i..38 + 2 * i].copy_from_slice(&n.to_be_bytes());
    }
    head[46..48].copy_from_slice(&3u16.to_be_bytes());
    head[48..50].copy_from_slice(&2i16.to_be_bytes());
    head[50..52].copy_from_slice(&loca_format.to_be_bytes());
    b.insert(*b"head", head)?;
    let mut h = Header::new(Axis::Horizontal);
    h.ascender = ascent;
    h.descender = descent;
    h.line_gap = i16n(
        base.metrics
            .line_gap
            .unwrap_or(0)
            .checked_mul(scale)
            .ok_or_else(|| invalid("line gap overflow"))?,
    )?;
    h.recompute(&hm, &extents)?;
    let he = metrics::encode(Axis::Horizontal, &h, &hm, true, l)?;
    b.insert(*b"hhea", he.header)?;
    b.insert(*b"hmtx", he.metrics)?;
    if vertical {
        let mut vh = Header::new(Axis::Vertical);
        vh.ascender = (upm / 2) as i16;
        vh.descender = -((upm - upm / 2) as i16);
        vh.recompute(&vm, &vertical_extents)?;
        let e = metrics::encode(Axis::Vertical, &vh, &vm, true, l)?;
        b.insert(*b"vhea", e.header)?;
        b.insert(*b"vmtx", e.metrics)?;
    }
    let cps = plan.cmap.keys().copied().collect();
    let mut os = Os2Table {
        version: 3,
        ..Default::default()
    };
    os.base.vendor_id = o.vendor;
    os.base.typo_metrics = [ascent, descent, h.line_gap];
    os.base.windows_metrics = [ascent.unsigned_abs(), descent.unsigned_abs()];
    let nonzero = hm.iter().filter(|m| m.advance != 0).collect::<Vec<_>>();
    os.base.average_width = if nonzero.is_empty() {
        0
    } else {
        i16n(
            (nonzero.iter().map(|m| u64::from(m.advance)).sum::<u64>() as f64
                / nonzero.len() as f64)
                .round() as i64,
        )?
    };
    os.extension.as_mut().unwrap().max_context = u16::from(!plan.gsub.features.is_empty());
    os.set_unicode_coverage(&cps)?;
    let mut pages = vec![1252, 1250, 1251, 1253, 1254, 1255, 1256, 1257, 1258, 874];
    if cfg!(feature = "cjk-charsets") {
        pages.extend([932, 936, 949, 950, 1361]);
    }
    os.code_page_ranges = Some(sfnt::os2::code_page_ranges(&cps, &pages)?);
    b.insert(*b"OS/2", os.encode(l)?)?;
    b.insert(
        *b"cmap",
        sfnt::cmap::encode(&plan.cmap, count as u16, l)?.data,
    )?;
    let mut post = PostTable::default();
    if retain_names {
        post.header.underline_position = i16n(
            base.metrics
                .underline_position
                .unwrap_or(0)
                .checked_mul(scale)
                .ok_or_else(|| invalid("underline position overflow"))?,
        )?;
        post.header.underline_thickness = i16n(
            base.metrics
                .underline_thickness
                .unwrap_or(0)
                .checked_mul(scale)
                .ok_or_else(|| invalid("underline thickness overflow"))?,
        )?;
        let mut names = sources
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect::<Vec<_>>();
        if shared {
            names.push("fontpixel.dot".into());
        }
        post.names = Some(names);
    }
    post.header.is_fixed_pitch = u32::from(matches!(
        base.metrics.spacing,
        Some(crate::model::Spacing::Monospaced | crate::model::Spacing::CharacterCell)
    ));
    b.insert(*b"post", post.encode(count as u16, l)?)?;
    let names = if let Some(n) = &o.names {
        n.clone()
    } else {
        let mut values = vec![
            (1, family.to_owned()),
            (2, o.legacy_style.as_deref().unwrap_or("Regular").into()),
            (3, format!("{family} {style} {}", o.version)),
            (4, fullname),
            (5, format!("Version {}", o.version)),
            (6, psname),
            (16, family.to_owned()),
            (17, style.to_owned()),
        ];
        for (id, text) in [
            (0, o.copyright.as_ref().or(base.names.copyright.as_ref())),
            (13, o.license.as_ref().or(base.names.license.as_ref())),
            (
                14,
                o.license_url.as_ref().or(base.names.license_url.as_ref()),
            ),
        ] {
            if let Some(s) = text {
                values.push((id, s.clone()));
            }
        }
        let mut records = Vec::new();
        for (id, value) in values {
            if value.is_ascii() {
                records.push(NameRecord {
                    platform_id: 1,
                    encoding_id: 0,
                    language_id: 0,
                    name_id: id,
                    value: value.as_bytes().to_vec(),
                });
            }
            records.push(NameRecord::unicode(id, 0x409, &value));
        }
        NameTable {
            records,
            language_tags: Vec::new(),
        }
    };
    b.insert(*b"name", names.encode(l)?)?;
    if !plan.gsub.features.is_empty() {
        b.insert(*b"GSUB", plan.gsub.encode(count as u16, &o.limits)?)?;
    }
    b.insert(
        *b"gasp",
        sfnt::gasp::GaspTable {
            version: 1,
            ranges: vec![sfnt::gasp::Range {
                max_ppem: 65535,
                behavior: 0,
            }],
        }
        .encode(l)?,
    )?;
    if o.embed_bitmaps {
        let ppem = u8::try_from(base.metrics.pixel_size.unwrap_or(14))
            .map_err(|_| invalid("bitmap ppem exceeds uint8"))?;
        let mut line = [0; 12];
        line[0] = i8::try_from(i64::from(ascent) / scale)
            .map_err(|_| invalid("bitmap ascent exceeds int8"))? as u8;
        line[1] = i8::try_from(i64::from(descent) / scale)
            .map_err(|_| invalid("bitmap descent exceeds int8"))? as u8;
        let strike = sbit::Strike {
            ppem_x: ppem,
            ppem_y: ppem,
            bit_depth: 1,
            flags: if vertical { 3 } else { 1 },
            horizontal: sbit::LineMetrics(line),
            vertical: sbit::LineMetrics::default(),
            glyphs: bitmaps,
        };
        let e = sbit::encode(
            &[strike],
            sbit::Kind::OpenType,
            count as u16,
            &sbit::Limits {
                sfnt: l.clone(),
                ..Default::default()
            },
        )?;
        b.insert(*b"EBDT", e.data)?;
        b.insert(*b"EBLC", e.location)?;
    }
    Ok(Encoded {
        bytes: b.build(l)?,
        report,
    })
}

/// Vectorize one complete Font while retaining aliases and unencoded glyphs.
/// Unlike regional planning this preserves physical glyph identity and order.
pub fn encode_single(font: &Font, o: &Options) -> Result<Encoded> {
    validate_names(o)?;
    font.validate(&o.limits.sfnt.resources)?;
    let mut native_bytes = 0usize;
    for (_, g) in font.glyphs() {
        native_bytes = native_bytes
            .checked_add(
                1024usize
                    .saturating_add(g.bitmap.as_bytes().len().saturating_mul(2))
                    .saturating_add(g.codepoints.len().saturating_mul(64))
                    .saturating_add(g.name.as_ref().map_or(0, String::len)),
            )
            .ok_or_else(|| invalid("native source size overflow"))?;
        sfnt::bounded(
            native_bytes,
            o.limits.sfnt.resources.max_total_bytes,
            "native outline source bytes",
        )?;
    }
    let mut mapping = BTreeMap::new();
    let mut glyphs = Vec::new();
    let notdef_id = font
        .glyphs()
        .find(|(_, g)| g.name.as_deref() == Some(".notdef"))
        .map(|(id, _)| id);
    o.limits
        .sfnt
        .glyphs(font.len().saturating_add(usize::from(notdef_id.is_none())))?;
    let source_notdef = notdef_id
        .and_then(|id| font.glyph(id))
        .map(|g| planned_glyph(font, g))
        .unwrap_or_else(|| {
            let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
            g.metrics.advance = Some(crate::model::Vector::new(0, 0));
            g
        });
    let mut names = std::collections::BTreeSet::from([".notdef".to_owned()]);
    if o.flavor == OutlineFlavor::TrueType && o.shape == Shape::Dot {
        names.insert("fontpixel.dot".into());
    }
    let mut report = Report::default();
    for (id, g) in font.glyphs() {
        let gid = if Some(id) == notdef_id {
            0
        } else {
            o.limits.sfnt.glyphs(glyphs.len().saturating_add(2))?;
            let mut name = g
                .name
                .clone()
                .filter(|n| {
                    !n.is_empty()
                        && n.len() <= 63
                        && n.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                })
                .unwrap_or_else(|| format!("glyph{}", id.index()));
            if names.contains(&name) {
                name = format!("glyph{}", id.index());
                while names.contains(&name) {
                    name.push('_');
                }
            }
            if g.name.as_deref() != Some(&name) && g.name.is_some() {
                note(
                    &mut report,
                    "opentype.glyph_name",
                    "Unrepresentable/duplicate glyph names were replaced with unique PostScript names",
                );
            }
            names.insert(name.clone());
            glyphs.push((name, planned_glyph(font, g)));
            glyphs.len() as u16
        };
        for cp in &g.codepoints {
            let Codepoint::Unicode(cp) = *cp else {
                return Err(invalid(
                    "native OpenType output requires Unicode codepoints",
                ));
            };
            if char::from_u32(cp).is_none() {
                return Err(invalid("non-scalar Unicode codepoint"));
            }
            if mapping.insert(cp, gid).is_some()
                && !report
                    .entries
                    .iter()
                    .any(|e| e.code == "opentype.duplicate_codepoint")
            {
                note(
                    &mut report,
                    "opentype.duplicate_codepoint",
                    "Duplicate cmap entries resolve to the last source glyph",
                );
            }
            sfnt::bounded(
                mapping.len(),
                o.limits.sfnt.max_cmap_mappings,
                "native Unicode mappings",
            )?;
        }
    }
    let plan = Plan {
        glyphs,
        cmap: mapping,
        gsub: Gsub::default(),
    };
    let mut encoded = encode_planned(font, o, &plan, Some(source_notdef))?;
    encoded.report.extend(report);
    Ok(encoded)
}
