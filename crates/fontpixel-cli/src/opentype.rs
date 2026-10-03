//! End-to-end regional OpenType generation and byte-preserving input subsetting.
use super::{ReportStyle, emit_render_report, persist, read_primary};
use clap::Args;
use fontpixel::{
    Error, Font, Input, ReadOptions, Report, Result, Variants,
    outline::Shape,
    sfnt,
    variants::{self, Options, OutlineFlavor, ScriptVariant},
    woff,
};
use std::{collections::BTreeSet, io::Write, path::PathBuf};
#[derive(Args)]
pub(super) struct VariantsArgs {
    /// Default Unicode bitmap font.
    #[arg(long)]
    base: PathBuf,
    /// Language tag(s) and font, e.g. zh-Hant,zh-HK=TC.bdf; repeatable.
    #[arg(long = "lang")]
    languages: Vec<String>,
    /// Override scripts for a language: ja=DFLT,hani,kana; repeatable.
    #[arg(long = "lang-scripts")]
    language_scripts: Vec<String>,
    /// Script list and font, e.g. latn,grek,cyrl=Latin.bdf; repeatable.
    #[arg(long = "script")]
    scripts: Vec<String>,
    /// Font design units per source pixel.
    #[arg(long = "ppu", default_value_t = 100)]
    ppu: u16,
    /// Override units per em (otherwise pixel size times ppu).
    #[arg(long)]
    upm: Option<u16>,
    /// Outline shape.
    #[arg(long,default_value="square",value_parser=["square","square-separate","dot"])]
    shape: String,
    /// Pixel diameter/side ratio; default 1 for square, 0.9 for dot.
    #[arg(long)]
    dotsize: Option<f64>,
    /// Embed a matching EBDT/EBLC strike.
    #[arg(long)]
    embed_bitmaps: bool,
    /// Explicit gray-to-mono threshold.
    #[arg(long)]
    gray_threshold: Option<u8>,
    /// Family name override.
    #[arg(long)]
    family: Option<String>,
    /// Style name override.
    #[arg(long)]
    style: Option<String>,
    /// Version without the Version prefix; no timestamps are generated.
    #[arg(long = "font-version", default_value = "1.0")]
    version: String,
    /// Exactly four printable ASCII bytes.
    #[arg(long, default_value = "NONE")]
    vendor: String,
    /// Copyright text override.
    #[arg(long)]
    copyright: Option<String>,
    /// License text override.
    #[arg(long)]
    license: Option<String>,
    /// License URL override.
    #[arg(long)]
    license_url: Option<String>,
    /// Outline source for WOFF/WOFF2; auto uses CFF square and TrueType dots.
    #[arg(long,default_value="auto",value_parser=["auto","cff","ttf"])]
    woff2_source: String,
    #[command(flatten)]
    output: Output,
}
#[derive(Args)]
pub(super) struct SubsetArgs {
    /// sfnt, WOFF or WOFF2 input font.
    input: PathBuf,
    /// Keep all characters in this Unicode text.
    #[arg(long,group="selection",required_unless_present_any=["unicodes","text_file"])]
    text: Option<String>,
    /// Inclusive hexadecimal ranges, e.g. U+0020-007E,U+4E00.
    #[arg(long, group = "selection")]
    unicodes: Option<String>,
    /// Read UTF-8 text from a file.
    #[arg(long, group = "selection")]
    text_file: Option<PathBuf>,
    /// Explicitly discard a table whose glyph references cannot be remapped.
    #[arg(long = "drop-table")]
    drop_tables: Vec<String>,
    #[command(flatten)]
    output: Output,
}
#[derive(Args)]
struct Output {
    /// TTF, OTF, OTB, WOFF or WOFF2 output (TTF/OTF retain subset outline flavor).
    #[arg(short, long)]
    output: PathBuf,
    /// Explicit output container; defaults to extension.
    #[arg(long,value_parser=["ttf","otf","otb","woff","woff2"])]
    to: Option<String>,
    /// Brotli quality, 0 through 11.
    #[arg(long,default_value_t=11,value_parser=clap::value_parser!(u32).range(0..=11))]
    woff2_quality: u32,
    /// Legal intra-font parallel Brotli workers, 1 through 16.
    #[arg(long, default_value_t = 1)]
    woff2_threads: usize,
    /// Reject any conversion loss before replacing output.
    #[arg(long)]
    strict: bool,
    /// Diagnostics format.
    #[arg(long, value_enum, default_value = "text")]
    report: ReportStyle,
}
impl Output {
    fn kind(&self) -> Result<String> {
        self.to
            .clone()
            .or_else(|| {
                self.output
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
            })
            .filter(|s| ["ttf", "otf", "otb", "woff", "woff2"].contains(&s.as_str()))
            .ok_or_else(|| {
                Error::InvalidInput("output requires .ttf/.otf/.otb/.woff/.woff2 or --to".into())
            })
    }
    fn save(
        &self,
        sfnt: Vec<u8>,
        mut report: Report,
        aux: woff::Auxiliary,
        out: &mut dyn Write,
    ) -> Result<()> {
        let kind = self.kind()?;
        let bytes = match kind.as_str() {
            "woff" => {
                let e = woff::woff1::encode(
                    &sfnt,
                    &woff::woff1::Options {
                        auxiliary: aux,
                        ..Default::default()
                    },
                    &Default::default(),
                )?;
                report.extend(e.report);
                e.bytes
            }
            "woff2" => {
                let e = woff::woff2::encode(
                    &sfnt,
                    &woff::woff2::Options {
                        quality: self.woff2_quality,
                        compression_threads: self.woff2_threads,
                        auxiliary: aux,
                        ..Default::default()
                    },
                    &Default::default(),
                )?;
                report.extend(e.report);
                e.bytes
            }
            _ => {
                let f = sfnt::Sfnt::parse(&sfnt, &Default::default())?;
                if (kind == "otf") != (f.flavor() == sfnt::Flavor::Cff)
                    || (kind == "otb"
                        && (f.table(*b"glyf").is_some() || f.table(*b"CFF ").is_some()))
                {
                    return Err(Error::InvalidInput("output extension does not match source outline flavor; use .woff/.woff2 or its sfnt flavor".into()));
                }
                if aux.metadata.is_some() || aux.private_data.is_some() {
                    report.push(fontpixel::ReportEntry {
                        level: fontpixel::report::Level::Warning,
                        impact: fontpixel::report::Impact::Loss,
                        code: "subset.web_auxiliary_removed".into(),
                        location: Default::default(),
                        message: "sfnt cannot retain WOFF metadata/private blocks".into(),
                    });
                }
                sfnt
            }
        };
        if self.strict && report.has_loss() {
            return Err(Error::StrictLoss(report));
        }
        persist(&self.output, &bytes)?;
        emit_render_report(&report, self.report, out)
    }
}
fn assignment(s: &str) -> Result<(&str, &str)> {
    s.split_once('=')
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .ok_or_else(|| Error::InvalidInput("expected TAG=VALUE".into()))
}
fn load(path: PathBuf, report: &mut Report) -> Result<Font> {
    let o = ReadOptions::default();
    let data = read_primary(&path, &o.limits)?;
    let f =
        fontpixel::registry().read(&Input::new(&data).with_name(&path.to_string_lossy()), &o)?;
    report.extend(f.report);
    f.fonts.into_single()
}
pub(super) fn variants(a: VariantsArgs, out: &mut dyn Write) -> Result<()> {
    let mut report = Report::default();
    let mut v = Variants::new(load(a.base, &mut report)?);
    for s in &a.languages {
        let (tags, path) = assignment(s)?;
        let f = load(PathBuf::from(path), &mut report)?;
        for lang in tags.split(',') {
            v.add_language(lang, f.clone())?;
        }
    }
    for s in &a.language_scripts {
        let (lang, tags) = assignment(s)?;
        let lang = variants::language_tag(lang)?;
        let scripts = tags
            .split(',')
            .map(variants::script_tag)
            .collect::<Result<Vec<_>>>()?;
        let mut found = false;
        for l in &mut v.languages {
            if l.language == lang {
                l.scripts = scripts.clone();
                found = true;
            }
        }
        if !found {
            return Err(Error::InvalidInput(
                "--lang-scripts has no corresponding --lang".into(),
            ));
        }
    }
    for s in &a.scripts {
        let (tags, path) = assignment(s)?;
        v.scripts.push(ScriptVariant {
            scripts: tags
                .split(',')
                .map(variants::script_tag)
                .collect::<Result<_>>()?,
            font: load(PathBuf::from(path), &mut report)?,
        });
    }
    let shape = match a.shape.as_str() {
        "dot" => Shape::Dot,
        "square-separate" => Shape::SquareSeparate,
        _ => Shape::Square,
    };
    let flavor = match a.output.kind()?.as_str() {
        "ttf" => OutlineFlavor::TrueType,
        "otf" => OutlineFlavor::Cff,
        "otb" => {
            return Err(Error::InvalidInput(
                "variants needs outline output; use --embed-bitmaps for strikes".into(),
            ));
        }
        _ => match a.woff2_source.as_str() {
            "ttf" => OutlineFlavor::TrueType,
            "cff" => OutlineFlavor::Cff,
            _ => {
                if shape == Shape::Dot {
                    OutlineFlavor::TrueType
                } else {
                    OutlineFlavor::Cff
                }
            }
        },
    };
    let vendor = a
        .vendor
        .as_bytes()
        .try_into()
        .map_err(|_| Error::InvalidInput("vendor must have four bytes".into()))?;
    if !a.vendor.bytes().all(|v| (32..=126).contains(&v)) {
        return Err(Error::InvalidInput("vendor must be printable ASCII".into()));
    }
    let o = Options {
        shape,
        dot_size: a
            .dotsize
            .unwrap_or(if shape == Shape::Dot { 0.9 } else { 1. }),
        units_per_pixel: a.ppu,
        units_per_em: a.upm,
        flavor,
        embed_bitmaps: a.embed_bitmaps,
        gray_threshold: a.gray_threshold,
        family: a.family,
        style: a.style,
        version: a.version,
        vendor,
        copyright: a.copyright,
        license: a.license,
        license_url: a.license_url,
        ..Default::default()
    };
    let e = variants::encode(&v, &o)?;
    report.extend(e.report);
    a.output.save(e.bytes, report, Default::default(), out)
}
pub(super) fn subset(a: SubsetArgs, out: &mut dyn Write) -> Result<()> {
    let limits = fontpixel::ResourceLimits::default();
    let data = read_primary(&a.input, &limits)?;
    let decoded = if data.starts_with(b"wOFF") {
        woff::woff1::decode(&data, &Default::default())?
    } else if data.starts_with(b"wOF2") {
        woff::woff2::decode(&data, &Default::default())?
    } else {
        woff::Decoded {
            sfnt: data,
            auxiliary: Default::default(),
            report: Default::default(),
        }
    };
    let codepoints = if let Some(text) = a.text {
        text.chars().map(u32::from).collect()
    } else if let Some(path) = a.text_file {
        String::from_utf8(read_primary(&path, &limits)?)
            .map_err(|_| Error::InvalidInput("text file must be UTF-8".into()))?
            .chars()
            .map(u32::from)
            .collect()
    } else {
        unicodes(a.unicodes.as_deref().unwrap_or(""))?
    };
    let drops = a
        .drop_tables
        .iter()
        .map(|s| {
            let t = s.as_bytes().try_into().map_err(|_| {
                Error::InvalidInput("table tag must have exactly four ASCII bytes".into())
            })?;
            Ok(t)
        })
        .collect::<Result<_>>()?;
    let e = sfnt::subset::subset(
        &decoded.sfnt,
        &sfnt::subset::Options {
            codepoints,
            drop_tables: drops,
            ..Default::default()
        },
    )?;
    let mut report = decoded.report;
    report.extend(e.report);
    a.output.save(e.bytes, report, decoded.auxiliary, out)
}
fn unicodes(s: &str) -> Result<BTreeSet<u32>> {
    if s.is_empty() {
        return Err(Error::InvalidInput("empty Unicode range selection".into()));
    }
    let parse = |s: &str| {
        u32::from_str_radix(
            s.trim().trim_start_matches("U+").trim_start_matches("u+"),
            16,
        )
        .map_err(|_| Error::InvalidInput("invalid hexadecimal Unicode value".into()))
    };
    let mut set = BTreeSet::new();
    for part in s.split(',') {
        let (a, b) = if let Some((a, b)) = part.split_once('-') {
            (parse(a)?, parse(b)?)
        } else {
            let a = parse(part)?;
            (a, a)
        };
        if a > b || b > 0x10ffff {
            return Err(Error::InvalidInput("invalid Unicode range bounds".into()));
        }
        for cp in a..=b {
            if char::from_u32(cp).is_none() {
                return Err(Error::InvalidInput(
                    "Unicode ranges cannot contain surrogates".into(),
                ));
            }
            set.insert(cp);
        }
    }
    Ok(set)
}

impl VariantsArgs {
    pub(super) fn report_style(&self) -> ReportStyle {
        self.output.report
    }
}
impl SubsetArgs {
    pub(super) fn report_style(&self) -> ReportStyle {
        self.output.report
    }
}
