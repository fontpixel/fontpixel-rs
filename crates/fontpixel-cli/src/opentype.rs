//! End-to-end regional OpenType generation and byte-preserving input subsetting.
use super::{ReportStyle, emit_render_report, input_budget::InputBudget, persist, read_primary};
use clap::Args;
use fontpixel::{
    Error, Font, ReadOptions, Report, ResourceLimits, Result, Variants,
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
    /// Native dot reuse: literal favors webfont ratio; patterns favors export speed.
    #[arg(long,default_value="patterns",value_parser=["literal","rows","patterns"])]
    dot_reuse: String,
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
    /// Legacy subfamily (name ID 2); defaults to Regular independently of --style.
    #[arg(long)]
    legacy_style: Option<String>,
    /// Exact full display name override (name ID 4).
    #[arg(long)]
    full_name: Option<String>,
    /// Exact PostScript name override (name ID 6); no silent sanitization.
    #[arg(long)]
    postscript_name: Option<String>,
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
    resources: ResourceArguments,
    #[command(flatten)]
    output: Output,
}
/// Explicit variants budgets; absent values keep the library's defaults.
/// This CLI surface is reconstructed, not a byte-certified historical recovery.
#[derive(Args, Default)]
struct ResourceArguments {
    /// Maximum primary input bytes (default 64 MiB).
    #[arg(long)]
    max_input_bytes: Option<usize>,
    /// Maximum bytes in one companion resource.
    #[arg(long)]
    max_resource_bytes: Option<usize>,
    /// Aggregate input/model and codec allocation budget (default 256 MiB).
    #[arg(long)]
    max_total_bytes: Option<usize>,
    /// Maximum companion resource resolutions.
    #[arg(long)]
    max_resources: Option<usize>,
    /// Maximum decompressed bytes.
    #[arg(long)]
    max_decompressed_bytes: Option<usize>,
    /// Maximum aggregate source glyph slots and writer glyph count.
    #[arg(long)]
    max_glyphs: Option<usize>,
    /// Maximum pixels in a source glyph.
    #[arg(long)]
    max_pixels_per_glyph: Option<usize>,
    /// Maximum table or text-record bytes.
    #[arg(long)]
    max_table_bytes: Option<usize>,
}
impl ResourceArguments {
    fn limits(&self) -> ResourceLimits {
        let mut limits = ResourceLimits::default();
        macro_rules! apply {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = self.$field { limits.$field = value; })*
            };
        }
        apply!(
            max_input_bytes, max_resource_bytes, max_total_bytes, max_resources,
            max_decompressed_bytes, max_glyphs, max_pixels_per_glyph, max_table_bytes,
        );
        limits
    }
}
fn sfnt_limits(resources: &ResourceLimits) -> sfnt::Limits {
    sfnt::Limits { resources: resources.clone(), ..Default::default() }
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
    #[arg(long, default_value_t = 2)]
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
        resources: &ResourceLimits,
        out: &mut dyn Write,
    ) -> Result<()> {
        let kind = self.kind()?;
        let sfnt_limits = sfnt_limits(resources);
        let woff_limits = woff::Limits {
            sfnt: sfnt_limits.clone(),
            ..Default::default()
        };
        let bytes = match kind.as_str() {
            "woff" => {
                let e = woff::woff1::encode(
                    &sfnt,
                    &woff::woff1::Options {
                        auxiliary: aux,
                        ..Default::default()
                    },
                    &woff_limits,
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
                    &woff_limits,
                )?;
                report.extend(e.report);
                e.bytes
            }
            _ => {
                let f = sfnt::Sfnt::parse(&sfnt, &sfnt_limits)?;
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
        persist(&self.output, bytes)?;
        emit_render_report(&report, self.report, out)
    }
}
fn assignment(s: &str) -> Result<(&str, &str)> {
    s.split_once('=')
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .ok_or_else(|| Error::InvalidInput("expected TAG=VALUE".into()))
}
fn load(path: PathBuf, report: &mut Report, budget: &mut InputBudget) -> Result<Font> {
    let o = ReadOptions {
        limits: budget.remaining_limits()?,
        ..Default::default()
    };
    let file = super::assets::FileInput::open(&path, &o.limits, super::batch::sources())?;
    let loaded = fontpixel::registry().read(&file.input(), &o)?;
    drop(file);
    budget.admit(&loaded)?;
    report.extend(loaded.report);
    loaded.fonts.into_single()
}
fn add_language_routes(
    variants: &mut Variants,
    tags: &str,
    font: Font,
    budget: &mut InputBudget,
) -> Result<()> {
    let mut tags = tags.split(',').peekable();
    while let Some(language) = tags.next() {
        if tags.peek().is_some() {
            budget.reserve_font_clone(&font)?;
            variants.add_language(language, font.clone())?;
        } else {
            // The decoder already admitted this owned font. Only additional
            // routes need a new allocation; the final route consumes it.
            variants.add_language(language, font)?;
            break;
        }
    }
    Ok(())
}
pub(super) fn variants(a: VariantsArgs, out: &mut dyn Write) -> Result<()> {
    let mut report = Report::default();
    let limits = a.resources.limits();
    let mut input_budget = InputBudget::new(limits.clone());
    let mut v = Variants::new(load(a.base, &mut report, &mut input_budget)?);
    for s in &a.languages {
        let (tags, path) = assignment(s)?;
        let font = load(PathBuf::from(path), &mut report, &mut input_budget)?;
        add_language_routes(&mut v, tags, font, &mut input_budget)?;
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
            font: load(PathBuf::from(path), &mut report, &mut input_budget)?,
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
        dot_reuse: dot_reuse(Some(&a.dot_reuse)),
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
        legacy_style: a.legacy_style,
        full_name: a.full_name,
        postscript_name: a.postscript_name,
        version: a.version,
        vendor,
        copyright: a.copyright,
        license: a.license,
        license_url: a.license_url,
        limits: sfnt::gsub::LayoutLimits {
            sfnt: sfnt_limits(&limits),
            ..Default::default()
        },
        ..Default::default()
    };
    let e = variants::encode(&v, &o)?;
    report.extend(e.report);
    a.output.save(e.bytes, report, Default::default(), &limits, out)
}
pub(super) fn dot_reuse(value: Option<&str>) -> variants::DotReuse {
    match value {
        Some("literal") => variants::DotReuse::Literal,
        Some("rows") => variants::DotReuse::Rows,
        _ => variants::DotReuse::Patterns,
    }
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
    a.output.save(e.bytes, report, decoded.auxiliary, &limits, out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use fontpixel::{Bitmap, FontSet, Glyph, ResourceLimits, formats::Loaded};

    #[derive(Parser)]
    struct ParseVariants {
        #[command(flatten)]
        args: VariantsArgs,
    }

    #[test]
    fn direct_name_flags_preserve_exact_cli_values_and_defaults() {
        let defaults = ParseVariants::try_parse_from([
            "fontpixel", "--base", "base.bdf", "-o", "out.ttf",
        ]).unwrap().args;
        assert!(defaults.legacy_style.is_none());
        assert!(defaults.full_name.is_none());
        assert!(defaults.postscript_name.is_none());
        let chosen = ParseVariants::try_parse_from([
            "fontpixel", "--base", "base.bdf", "-o", "out.ttf",
            "--style", "Heavy Extended",
            "--legacy-style", "Medium",
            "--full-name", " Catalogue Full Name ",
            "--postscript-name", "Catalogue-HeavyExtended",
        ]).unwrap().args;
        assert_eq!(chosen.style.as_deref(), Some("Heavy Extended"));
        assert_eq!(chosen.legacy_style.as_deref(), Some("Medium"));
        assert_eq!(chosen.full_name.as_deref(), Some(" Catalogue Full Name "));
        assert_eq!(chosen.postscript_name.as_deref(), Some("Catalogue-HeavyExtended"));
    }

    fn loaded_font() -> Loaded<FontSet> {
        let glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
        Loaded {
            fonts: FontSet::from(Font::from_glyphs([glyph]).unwrap()),
            report: Report::default(),
            format: "test",
        }
    }

    #[test]
    fn language_routes_reserve_extra_clones_and_move_the_final_font() {
        for (tags, routes) in [("ja", 1), ("zh-Hant,zh-HK", 2)] {
            let input = loaded_font();
            let mut budget = InputBudget::new(ResourceLimits {
                max_glyphs: routes,
                ..Default::default()
            });
            budget.admit(&input).unwrap();
            let font = input.fonts.into_single().unwrap();
            let original_pointer = font.glyphs().next().unwrap().1.bitmap.as_bytes().as_ptr();
            let mut variants = Variants::new(Font::default());
            add_language_routes(&mut variants, tags, font, &mut budget).unwrap();
            assert_eq!(variants.languages.len(), routes);
            assert_eq!(budget.remaining_limits().unwrap().max_glyphs, 0);
            let last = &variants.languages.last().unwrap().font;
            assert_eq!(last.glyphs().next().unwrap().1.bitmap.as_bytes().as_ptr(), original_pointer);
            if routes == 2 {
                let first = &variants.languages[0].font;
                assert_ne!(first.glyphs().next().unwrap().1.bitmap.as_bytes().as_ptr(), original_pointer);
            }
        }
    }

    #[test]
    fn denied_extra_clone_does_not_consume_budget_or_append_a_route() {
        let input = loaded_font();
        let mut budget = InputBudget::new(ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        });
        budget.admit(&input).unwrap();
        let before = budget.remaining_limits().unwrap();
        let mut variants = Variants::new(Font::default());
        let result = add_language_routes(
            &mut variants, "zh-Hant,zh-HK", input.fonts.into_single().unwrap(), &mut budget,
        );
        assert!(matches!(result, Err(Error::ResourceLimit { .. })));
        assert!(variants.languages.is_empty());
        assert_eq!(budget.remaining_limits().unwrap(), before);
    }

    #[cfg(feature = "bdf")]
    #[test]
    fn repeated_loads_share_source_scope_and_remaining_model_allowance() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/data/bdf/3x5-block.bdf");
        let bytes = std::fs::read(&path).unwrap();
        let probe = fontpixel::read(&bytes, &ReadOptions::default()).unwrap();
        let glyph_slots = probe.fonts.fonts[0].slot_count();
        assert!(glyph_slots > 0);
        super::super::batch::with_source_scope(|| {
            let sources = super::super::batch::sources();
            let mut budget = InputBudget::new(ResourceLimits {
                max_glyphs: glyph_slots,
                ..Default::default()
            });
            let mut report = Report::default();
            let first = load(path.clone(), &mut report, &mut budget).unwrap();
            assert_eq!(first.slot_count(), glyph_slots);
            assert!(sources.is_single_primary());
            let before = budget.remaining_limits().unwrap();
            let previous_report = report.clone();
            assert_eq!(before.max_glyphs, 0);
            assert!(load(path.clone(), &mut report, &mut budget).is_err());
            assert_eq!(budget.remaining_limits().unwrap(), before);
            assert_eq!(report, previous_report);
            assert!(!sources.is_single_primary());
        });
    }


    #[test]
    fn explicit_resource_flags_match_resource_limits_without_raising_defaults() {
        let defaults = ParseVariants::try_parse_from([
            "fontpixel", "--base", "base.bdf", "-o", "out.ttf",
        ]).unwrap().args.resources.limits();
        assert_eq!(defaults, ResourceLimits::default());
        let selected = ParseVariants::try_parse_from([
            "fontpixel", "--base", "base.bdf", "-o", "out.ttf",
            "--max-input-bytes", "1", "--max-resource-bytes", "2",
            "--max-total-bytes", "3", "--max-resources", "4",
            "--max-decompressed-bytes", "5", "--max-glyphs", "6",
            "--max-pixels-per-glyph", "7", "--max-table-bytes", "8",
        ]).unwrap().args.resources.limits();
        assert_eq!(selected, ResourceLimits {
            max_input_bytes: 1,
            max_resource_bytes: 2,
            max_total_bytes: 3,
            max_resources: 4,
            max_decompressed_bytes: 5,
            max_glyphs: 6,
            max_pixels_per_glyph: 7,
            max_table_bytes: 8,
        });
        assert_eq!(sfnt_limits(&selected).resources, selected);
        let zero = ParseVariants::try_parse_from([
            "fontpixel", "--base", "base.bdf", "-o", "out.ttf", "--max-total-bytes", "0",
        ]).unwrap().args.resources.limits();
        assert_eq!(zero.max_total_bytes, 0);
        assert_eq!(zero.max_input_bytes, defaults.max_input_bytes);
    }

    #[test]
    fn repeated_language_clones_obey_exact_total_byte_admission() {
        let input = loaded_font();
        let mut probe = InputBudget::new(ResourceLimits::default());
        probe.admit(&input).unwrap();
        probe.reserve_font_clone(&input.fonts.fonts[0]).unwrap();
        let exact = ResourceLimits::default().max_total_bytes
            - probe.remaining_limits().unwrap().max_total_bytes;
        for (maximum, succeeds) in [(exact - 1, false), (exact, true)] {
            let mut budget = InputBudget::new(ResourceLimits {
                max_total_bytes: maximum,
                ..Default::default()
            });
            budget.admit(&input).unwrap();
            let before = budget.remaining_limits().unwrap();
            let mut variants = Variants::new(Font::default());
            let result = add_language_routes(
                &mut variants, "zh-Hant,zh-HK", input.fonts.fonts[0].clone(), &mut budget,
            );
            assert_eq!(result.is_ok(), succeeds);
            if succeeds {
                assert_eq!(variants.languages.len(), 2);
                assert_eq!(budget.remaining_limits().unwrap().max_total_bytes, 0);
            } else {
                assert!(matches!(result, Err(Error::ResourceLimit { .. })));
                assert!(variants.languages.is_empty());
                assert_eq!(budget.remaining_limits().unwrap(), before);
            }
        }
    }

    #[cfg(feature = "bdf")]
    #[test]
    fn max_total_bytes_rejects_variant_input_before_output_or_reporting() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/data/bdf/3x5-block.bdf");
        let args = ParseVariants::try_parse_from([
            "fontpixel", "--base", path.to_str().unwrap(), "-o", "must-not-exist.ttf",
            "--max-total-bytes", "1",
        ]).unwrap().args;
        let mut output = Vec::new();
        let result = super::super::batch::with_source_scope(|| variants(args, &mut output));
        assert!(matches!(result, Err(Error::ResourceLimit { .. })));
        assert!(output.is_empty());
    }

    #[test]
    fn selected_output_limits_reach_sfnt_and_both_woff_encoders() {
        let mut font = Font::default();
        let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
        glyph.codepoints.push(fontpixel::model::Codepoint::Unicode(65));
        font.add_glyph(glyph).unwrap();
        let encoded = variants::encode(&Variants::new(font), &Options::default()).unwrap();
        let limits = ResourceLimits {
            max_input_bytes: encoded.bytes.len() - 1,
            ..Default::default()
        };
        for filename in ["must-not-exist.ttf", "must-not-exist.woff", "must-not-exist.woff2"] {
            let output = ParseVariants::try_parse_from([
                "fontpixel", "--base", "base.bdf", "-o", filename,
            ]).unwrap().args.output;
            let mut report = Vec::new();
            let result = output.save(
                encoded.bytes.clone(), Report::default(), Default::default(), &limits, &mut report,
            );
            assert!(matches!(result, Err(Error::ResourceLimit { .. })));
            assert!(report.is_empty());
        }
    }
}
