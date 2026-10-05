//! End-to-end regional OpenType generation and byte-preserving input subsetting.
use super::{ReportStyle, emit_render_report, input_budget::InputBudget, persist, read_primary};
use clap::Args;
use fontpixel::{
    Error, Font, ReadOptions, Report, ReportEntry, ResourceLimits, Result, Variants,
    outline::Shape,
    sfnt,
    variants::{self, Options, OutlineFlavor, ScriptVariant},
    woff,
};
use std::{collections::BTreeSet, io::Write, mem::size_of, path::PathBuf};
#[derive(Args)]
#[command(
    after_help = "Example: fontpixel variants --base base.bdf --lang ja=jp.bdf -o regional.ttf\nExample: fontpixel variants --base base.bdf --shape dot -o dots.woff2 --max-total-bytes 536870912"
)]
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
    /// Glyph 0: keep the generated box (preserve) or write an empty outline
    /// with exact design-unit metrics (empty; requires --notdef-advance).
    #[arg(long, default_value = "preserve", value_parser = ["preserve", "empty"])]
    notdef: String,
    /// Empty .notdef advance width in design units.
    #[arg(long)]
    notdef_advance: Option<u16>,
    /// Empty .notdef left side bearing in design units (default 0).
    #[arg(long, allow_hyphen_values = true)]
    notdef_lsb: Option<i16>,
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
            max_input_bytes,
            max_resource_bytes,
            max_total_bytes,
            max_resources,
            max_decompressed_bytes,
            max_glyphs,
            max_pixels_per_glyph,
            max_table_bytes,
        );
        super::batch::standalone_working_limits(limits)
    }
}
fn sfnt_limits(resources: &ResourceLimits) -> sfnt::Limits {
    sfnt::Limits {
        resources: resources.clone(),
        ..Default::default()
    }
}
// New reconstruction correction: reserve caller-owned overlap between codecs.
// These checks cover retained CLI storage, not every codec transient or process RSS.
// Immutable variants argument/Options strings are caller-owned configuration and
// excluded; copies created by codecs remain subject to their own budgets. Subset
// selection strings/arrays are explicitly reserved until they are dropped.
fn retained_error(limit: usize) -> Error {
    Error::ResourceLimit {
        resource: "OpenType CLI retained bytes",
        limit,
    }
}
fn charge(used: &mut usize, bytes: usize, limit: usize) -> Result<()> {
    *used = used
        .checked_add(bytes)
        .filter(|n| *n <= limit)
        .ok_or_else(|| retained_error(limit))?;
    Ok(())
}
fn array_bytes<T>(count: usize, limit: usize) -> Result<usize> {
    count
        .checked_mul(size_of::<T>())
        .filter(|n| *n <= limit)
        .ok_or_else(|| retained_error(limit))
}
fn report_bytes(report: &Report, limit: usize) -> Result<usize> {
    let mut used = size_of::<Report>();
    charge(
        &mut used,
        array_bytes::<ReportEntry>(report.entries.capacity(), limit)?,
        limit,
    )?;
    for entry in &report.entries {
        charge(&mut used, entry.code.capacity(), limit)?;
        charge(&mut used, entry.message.capacity(), limit)?;
        charge(
            &mut used,
            entry.location.asset.as_ref().map_or(0, String::capacity),
            limit,
        )?;
    }
    // Check the empty report header too.
    charge(&mut used, 0, limit)?;
    Ok(used)
}
fn auxiliary_bytes(aux: &woff::Auxiliary, limit: usize) -> Result<usize> {
    let mut used = size_of::<woff::Auxiliary>();
    for bytes in [&aux.metadata, &aux.private_data].into_iter().flatten() {
        charge(&mut used, bytes.capacity(), limit)?;
    }
    charge(&mut used, 0, limit)?;
    Ok(used)
}
/// The library's own count, so the charge `Variants::encode_owned` admits and
/// releases face by face is exactly the one reserved here.
fn variants_bytes(value: &Variants, limit: usize) -> Result<usize> {
    value.retained_usage(limit)
}
fn codec_limits(limits: &ResourceLimits, retained: usize) -> Result<ResourceLimits> {
    let mut available = limits.clone();
    available.max_total_bytes = available
        .max_total_bytes
        .checked_sub(retained)
        .ok_or_else(|| retained_error(limits.max_total_bytes))?;
    // Already-admitted glyphs and borrowed codec input are not new CLI input.
    // Preserve their validation ceilings; reduce the codec allocation/output cap.
    Ok(available)
}
fn variants_limits(
    value: &Variants,
    report: &Report,
    limits: &ResourceLimits,
) -> Result<ResourceLimits> {
    let mut retained = variants_bytes(value, limits.max_total_bytes)?;
    charge(
        &mut retained,
        report_bytes(report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    codec_limits(limits, retained)
}
fn output_retained(capacity: usize, report: &Report, limit: usize) -> Result<usize> {
    let mut retained = capacity;
    charge(&mut retained, report_bytes(report, limit)?, limit)?;
    Ok(retained)
}
fn append_report(report: &mut Report, incoming: Report, owned: usize, limit: usize) -> Result<()> {
    if incoming.entries.is_empty() {
        // An empty result may retain a large array. Release it before checking
        // the storage that survives this merge; no diagnostics need moving.
        drop(incoming);
        output_retained(owned, report, limit)?;
        return Ok(());
    }
    let mut retained = owned;
    charge(&mut retained, report_bytes(report, limit)?, limit)?;
    charge(&mut retained, report_bytes(&incoming, limit)?, limit)?;
    let count = report
        .entries
        .len()
        .checked_add(incoming.entries.len())
        .ok_or_else(|| retained_error(limit))?;
    if count > report.entries.capacity() {
        // During growth the old receiving and incoming arrays can both coexist
        // with the new receiving array. Entry strings are moved, not cloned.
        charge(
            &mut retained,
            array_bytes::<ReportEntry>(count, limit)?,
            limit,
        )?;
    }
    report
        .entries
        .try_reserve_exact(incoming.entries.len())
        .map_err(|_| retained_error(limit))?;
    let mut actual = owned;
    charge(&mut actual, report_bytes(report, limit)?, limit)?;
    charge(&mut actual, report_bytes(&incoming, limit)?, limit)?;
    report.extend(incoming);
    Ok(())
}
fn auxiliary_removed(report: &mut Report, owned: usize, limit: usize) -> Result<()> {
    const CODE: &str = "subset.web_auxiliary_removed";
    const MESSAGE: &str = "sfnt cannot retain WOFF metadata/private blocks";
    let count = report
        .entries
        .len()
        .checked_add(1)
        .ok_or_else(|| retained_error(limit))?;
    let mut retained = owned;
    charge(&mut retained, report_bytes(report, limit)?, limit)?;
    charge(&mut retained, CODE.len(), limit)?;
    charge(&mut retained, MESSAGE.len(), limit)?;
    if count > report.entries.capacity() {
        charge(
            &mut retained,
            array_bytes::<ReportEntry>(count, limit)?,
            limit,
        )?;
    }
    let copy = |value: &str| -> Result<String> {
        let mut result = String::new();
        result
            .try_reserve_exact(value.len())
            .map_err(|_| retained_error(limit))?;
        result.push_str(value);
        Ok(result)
    };
    let code = copy(CODE)?;
    let message = copy(MESSAGE)?;
    report
        .entries
        .try_reserve_exact(1)
        .map_err(|_| retained_error(limit))?;
    let mut actual = owned;
    charge(&mut actual, report_bytes(report, limit)?, limit)?;
    charge(&mut actual, code.capacity(), limit)?;
    charge(&mut actual, message.capacity(), limit)?;
    report.push(ReportEntry {
        level: fontpixel::report::Level::Warning,
        impact: fontpixel::report::Impact::Loss,
        code,
        location: Default::default(),
        message,
    });
    Ok(())
}
#[derive(Args)]
#[command(
    after_help = "Example: fontpixel subset regional.ttf --text Hello -o hello.woff2\nExample: fontpixel subset regional.ttf --unicodes U+0020-007E -o ascii.ttf"
)]
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
        let limit = resources.max_total_bytes;
        let mut retained = output_retained(sfnt.capacity(), &report, limit)?;
        charge(&mut retained, auxiliary_bytes(&aux, limit)?, limit)?;
        let bytes = match kind.as_str() {
            "woff" | "woff2" => {
                // The codec borrows sfnt and auxiliary buffers while producing
                // a distinct output. Earlier reports remain owned by the CLI.
                let available = codec_limits(resources, retained)?;
                let woff_limits = woff::Limits {
                    sfnt: sfnt_limits(&available),
                    ..Default::default()
                };
                let e = if kind == "woff" {
                    woff::woff1::encode(
                        &sfnt,
                        &woff::woff1::Options {
                            auxiliary: aux,
                            ..Default::default()
                        },
                        &woff_limits,
                    )?
                } else {
                    woff::woff2::encode(
                        &sfnt,
                        &woff::woff2::Options {
                            quality: self.woff2_quality,
                            compression_threads: self.woff2_threads,
                            auxiliary: aux,
                            ..Default::default()
                        },
                        &woff_limits,
                    )?
                };
                drop(sfnt);
                append_report(&mut report, e.report, e.bytes.capacity(), limit)?;
                e.bytes
            }
            _ => {
                // Directory validation is a codec transient beside the retained
                // sfnt, report and auxiliary data; it only gets the remainder.
                let available = codec_limits(resources, retained)?;
                let f = sfnt::Sfnt::parse(&sfnt, &sfnt_limits(&available))?;
                if (kind == "otf") != (f.flavor() == sfnt::Flavor::Cff)
                    || (kind == "otb"
                        && (f.table(*b"glyf").is_some() || f.table(*b"CFF ").is_some()))
                {
                    return Err(Error::InvalidInput("output extension does not match source outline flavor; use .woff/.woff2 or its sfnt flavor".into()));
                }
                drop(f);
                let loses_auxiliary = aux.metadata.is_some() || aux.private_data.is_some();
                drop(aux);
                if loses_auxiliary {
                    auxiliary_removed(&mut report, sfnt.capacity(), limit)?;
                }
                // Ownership moves directly to persistence; no second sfnt copy.
                sfnt
            }
        };
        output_retained(bytes.capacity(), &report, limit)?;
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
    let notdef = match (a.notdef.as_str(), a.notdef_advance, a.notdef_lsb) {
        ("empty", Some(advance_width), lsb) => variants::NotdefPolicy::Empty {
            advance_width,
            left_side_bearing: lsb.unwrap_or(0),
        },
        ("empty", None, _) => {
            return Err(Error::InvalidInput(
                "--notdef empty requires --notdef-advance".into(),
            ));
        }
        (_, None, None) => variants::NotdefPolicy::Preserve,
        _ => {
            return Err(Error::InvalidInput(
                "--notdef-advance/--notdef-lsb require --notdef empty".into(),
            ));
        }
    };
    // Sources plus report must fit first (same check and error as before).
    // Export then runs on everything except the report: the library admits the
    // sources on its own ledger and releases each face after planning it.
    variants_limits(&v, &report, &limits)?;
    let encode_limits = codec_limits(&limits, report_bytes(&report, limits.max_total_bytes)?)?;
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
        notdef,
        limits: sfnt::gsub::LayoutLimits {
            sfnt: sfnt_limits(&encode_limits),
            ..Default::default()
        },
        ..Default::default()
    };
    let e = v.encode_owned(&o)?;
    drop(o);
    append_report(
        &mut report,
        e.report,
        e.bytes.capacity(),
        limits.max_total_bytes,
    )?;
    a.output
        .save(e.bytes, report, Default::default(), &limits, out)
}
pub(super) fn dot_reuse(value: Option<&str>) -> variants::DotReuse {
    match value {
        Some("literal") => variants::DotReuse::Literal,
        Some("rows") => variants::DotReuse::Rows,
        _ => variants::DotReuse::Patterns,
    }
}
fn decode_subset_input(data: Vec<u8>, limits: &ResourceLimits) -> Result<woff::Decoded> {
    if data.starts_with(b"wOFF") || data.starts_with(b"wOF2") {
        let available = codec_limits(limits, data.capacity())?;
        let web_limits = woff::Limits {
            sfnt: sfnt_limits(&available),
            ..Default::default()
        };
        let decoded = if data.starts_with(b"wOFF") {
            woff::woff1::decode(&data, &web_limits)?
        } else {
            woff::woff2::decode(&data, &web_limits)?
        };
        drop(data);
        Ok(decoded)
    } else {
        Ok(woff::Decoded {
            sfnt: data,
            auxiliary: Default::default(),
            report: Default::default(),
        })
    }
}
fn subset_input_bytes(decoded: &woff::Decoded, limit: usize) -> Result<usize> {
    let mut retained = output_retained(decoded.sfnt.capacity(), &decoded.report, limit)?;
    charge(
        &mut retained,
        auxiliary_bytes(&decoded.auxiliary, limit)?,
        limit,
    )?;
    Ok(retained)
}
// Match the shared model's conservative B-tree fallback rather than relying on
// private capacity/occupancy details: charge n+1 possible nodes, each with room
// for 16 keys/edges plus 128 bytes. Keys here are fixed-size scalars/table tags.
fn selection_bytes<T>(count: usize, limit: usize) -> Result<usize> {
    let node = size_of::<T>()
        .checked_add(size_of::<usize>())
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(128))
        .ok_or_else(|| retained_error(limit))?;
    count
        .checked_add(1)
        .and_then(|n| n.checked_mul(node))
        .and_then(|n| n.checked_add(size_of::<BTreeSet<T>>()))
        .filter(|n| *n <= limit)
        .ok_or_else(|| retained_error(limit))
}
fn selection_insert<T: Ord>(
    set: &mut BTreeSet<T>,
    value: T,
    other: usize,
    limit: usize,
) -> Result<()> {
    if !set.contains(&value) {
        let count = set
            .len()
            .checked_add(1)
            .ok_or_else(|| retained_error(limit))?;
        let mut retained = other;
        charge(&mut retained, selection_bytes::<T>(count, limit)?, limit)?;
        set.insert(value);
    }
    Ok(())
}
fn subset_text_selection(text: &str, other: usize, limit: usize) -> Result<BTreeSet<u32>> {
    let mut retained = other;
    charge(&mut retained, selection_bytes::<u32>(0, limit)?, limit)?;
    let mut set = BTreeSet::new();
    for cp in text.chars().map(u32::from) {
        selection_insert(&mut set, cp, other, limit)?;
    }
    Ok(set)
}
fn subset_selection_argument_bytes(args: &SubsetArgs, limit: usize) -> Result<usize> {
    let mut retained = array_bytes::<String>(args.drop_tables.capacity(), limit)?;
    for text in args
        .text
        .iter()
        .chain(args.unicodes.iter())
        .chain(args.drop_tables.iter())
    {
        charge(&mut retained, text.capacity(), limit)?;
    }
    Ok(retained)
}
pub(super) fn subset(a: SubsetArgs, out: &mut dyn Write) -> Result<()> {
    let limits = super::batch::standalone_working_limits(fontpixel::ResourceLimits::default());
    let argument_bytes = subset_selection_argument_bytes(&a, limits.max_total_bytes)?;
    let input_limits = codec_limits(&limits, argument_bytes)?;
    let data = read_primary(&a.input, &input_limits)?;
    let decoded = decode_subset_input(data, &input_limits)?;
    let decoded_storage = subset_input_bytes(&decoded, limits.max_total_bytes)?;
    let mut retained = decoded_storage;
    charge(&mut retained, argument_bytes, limits.max_total_bytes)?;
    let codepoints = if let Some(text) = a.text {
        subset_text_selection(&text, retained, limits.max_total_bytes)?
    } else if let Some(path) = a.text_file {
        let read_limits = codec_limits(&limits, retained)?;
        let text = String::from_utf8(read_primary(&path, &read_limits)?)
            .map_err(|_| Error::InvalidInput("text file must be UTF-8".into()))?;
        let mut with_text = retained;
        charge(&mut with_text, text.capacity(), limits.max_total_bytes)?;
        subset_text_selection(&text, with_text, limits.max_total_bytes)?
    } else {
        unicodes(
            a.unicodes.as_deref().unwrap_or(""),
            retained,
            limits.max_total_bytes,
        )?
    };
    let mut with_codepoints = retained;
    charge(
        &mut with_codepoints,
        selection_bytes::<u32>(codepoints.len(), limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let mut drops = BTreeSet::<[u8; 4]>::new();
    for s in &a.drop_tables {
        let tag = s.as_bytes().try_into().map_err(|_| {
            Error::InvalidInput("table tag must have exactly four ASCII bytes".into())
        })?;
        selection_insert(&mut drops, tag, with_codepoints, limits.max_total_bytes)?;
    }
    // Text/range/drop-table arguments remain conservatively reserved throughout
    // selection construction, including a text-file's separate live contents.
    // Release their exact owners before restoring that allowance for the codec.
    drop(a.unicodes);
    drop(a.drop_tables);
    let mut with_selection = decoded_storage;
    charge(
        &mut with_selection,
        selection_bytes::<u32>(codepoints.len(), limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    charge(
        &mut with_selection,
        selection_bytes::<[u8; 4]>(drops.len(), limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let available = codec_limits(&limits, with_selection)?;
    let options = sfnt::subset::Options {
        codepoints,
        drop_tables: drops,
        limits: sfnt::gsub::LayoutLimits {
            sfnt: sfnt_limits(&available),
            ..Default::default()
        },
    };
    let sfnt::subset::Subset {
        bytes,
        glyph_mapping,
        report: incoming,
    } = sfnt::subset::subset(&decoded.sfnt, &options)?;
    drop(options);
    drop(glyph_mapping);
    drop(decoded.sfnt);
    let mut report = decoded.report;
    let mut retained = bytes.capacity();
    charge(
        &mut retained,
        auxiliary_bytes(&decoded.auxiliary, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    append_report(&mut report, incoming, retained, limits.max_total_bytes)?;
    a.output
        .save(bytes, report, decoded.auxiliary, &limits, out)
}
fn unicodes(s: &str, other: usize, limit: usize) -> Result<BTreeSet<u32>> {
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
    let mut retained = other;
    charge(&mut retained, selection_bytes::<u32>(0, limit)?, limit)?;
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
            selection_insert(&mut set, cp, other, limit)?;
        }
    }
    Ok(set)
}

impl VariantsArgs {
    pub(super) fn source_identity_total_bytes(&self) -> usize {
        self.resources.limits().max_total_bytes
    }
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
        let defaults =
            ParseVariants::try_parse_from(["fontpixel", "--base", "base.bdf", "-o", "out.ttf"])
                .unwrap()
                .args;
        assert!(defaults.legacy_style.is_none());
        assert!(defaults.full_name.is_none());
        assert!(defaults.postscript_name.is_none());
        let chosen = ParseVariants::try_parse_from([
            "fontpixel",
            "--base",
            "base.bdf",
            "-o",
            "out.ttf",
            "--style",
            "Heavy Extended",
            "--legacy-style",
            "Medium",
            "--full-name",
            " Catalogue Full Name ",
            "--postscript-name",
            "Catalogue-HeavyExtended",
        ])
        .unwrap()
        .args;
        assert_eq!(chosen.style.as_deref(), Some("Heavy Extended"));
        assert_eq!(chosen.legacy_style.as_deref(), Some("Medium"));
        assert_eq!(chosen.full_name.as_deref(), Some(" Catalogue Full Name "));
        assert_eq!(
            chosen.postscript_name.as_deref(),
            Some("Catalogue-HeavyExtended")
        );
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
            assert_eq!(
                last.glyphs().next().unwrap().1.bitmap.as_bytes().as_ptr(),
                original_pointer
            );
            if routes == 2 {
                let first = &variants.languages[0].font;
                assert_ne!(
                    first.glyphs().next().unwrap().1.bitmap.as_bytes().as_ptr(),
                    original_pointer
                );
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
            &mut variants,
            "zh-Hant,zh-HK",
            input.fonts.into_single().unwrap(),
            &mut budget,
        );
        assert!(matches!(result, Err(Error::ResourceLimit { .. })));
        assert!(variants.languages.is_empty());
        assert_eq!(budget.remaining_limits().unwrap(), before);
    }

    #[cfg(feature = "bdf")]
    #[test]
    fn repeated_loads_share_source_scope_and_remaining_model_allowance() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/bdf/3x5-block.bdf");
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
        let defaults =
            ParseVariants::try_parse_from(["fontpixel", "--base", "base.bdf", "-o", "out.ttf"])
                .unwrap()
                .args
                .resources
                .limits();
        assert_eq!(defaults, ResourceLimits::default());
        let selected = ParseVariants::try_parse_from([
            "fontpixel",
            "--base",
            "base.bdf",
            "-o",
            "out.ttf",
            "--max-input-bytes",
            "1",
            "--max-resource-bytes",
            "2",
            "--max-total-bytes",
            "3",
            "--max-resources",
            "4",
            "--max-decompressed-bytes",
            "5",
            "--max-glyphs",
            "6",
            "--max-pixels-per-glyph",
            "7",
            "--max-table-bytes",
            "8",
        ])
        .unwrap()
        .args
        .resources
        .limits();
        assert_eq!(
            selected,
            ResourceLimits {
                max_input_bytes: 1,
                max_resource_bytes: 2,
                max_total_bytes: 3,
                max_resources: 4,
                max_decompressed_bytes: 5,
                max_glyphs: 6,
                max_pixels_per_glyph: 7,
                max_table_bytes: 8,
            }
        );
        assert_eq!(sfnt_limits(&selected).resources, selected);
        let zero = ParseVariants::try_parse_from([
            "fontpixel",
            "--base",
            "base.bdf",
            "-o",
            "out.ttf",
            "--max-total-bytes",
            "0",
        ])
        .unwrap()
        .args
        .resources
        .limits();
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
                &mut variants,
                "zh-Hant,zh-HK",
                input.fonts.fonts[0].clone(),
                &mut budget,
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
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/bdf/3x5-block.bdf");
        let args = ParseVariants::try_parse_from([
            "fontpixel",
            "--base",
            path.to_str().unwrap(),
            "-o",
            "must-not-exist.ttf",
            "--max-total-bytes",
            "1",
        ])
        .unwrap()
        .args;
        let mut output = Vec::new();
        let result = super::super::batch::with_source_scope(|| variants(args, &mut output));
        assert!(matches!(result, Err(Error::ResourceLimit { .. })));
        assert!(output.is_empty());
    }

    #[test]
    fn selected_output_limits_reach_sfnt_and_both_woff_encoders() {
        let mut font = Font::default();
        let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
        glyph
            .codepoints
            .push(fontpixel::model::Codepoint::Unicode(65));
        font.add_glyph(glyph).unwrap();
        let encoded = variants::encode(&Variants::new(font), &Options::default()).unwrap();
        let limits = ResourceLimits {
            max_input_bytes: encoded.bytes.len() - 1,
            ..Default::default()
        };
        for filename in [
            "must-not-exist.ttf",
            "must-not-exist.woff",
            "must-not-exist.woff2",
        ] {
            let output =
                ParseVariants::try_parse_from(["fontpixel", "--base", "base.bdf", "-o", filename])
                    .unwrap()
                    .args
                    .output;
            let mut report = Vec::new();
            let result = output.save(
                encoded.bytes.clone(),
                Report::default(),
                Default::default(),
                &limits,
                &mut report,
            );
            assert!(matches!(result, Err(Error::ResourceLimit { .. })));
            assert!(report.is_empty());
        }
    }

    fn loss_report(code: &str) -> Report {
        let mut report = Report {
            entries: Vec::with_capacity(1),
        };
        report.push(ReportEntry {
            level: fontpixel::report::Level::Warning,
            impact: fontpixel::report::Impact::Loss,
            code: code.into(),
            location: Default::default(),
            message: "retained diagnostic".into(),
        });
        report
    }

    fn encoded_fixture() -> Vec<u8> {
        let mut font = Font::default();
        let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
        glyph
            .codepoints
            .push(fontpixel::model::Codepoint::Unicode(65));
        font.add_glyph(glyph).unwrap();
        variants::encode(&Variants::new(font), &Options::default())
            .unwrap()
            .bytes
    }

    #[test]
    fn subset_selection_admits_unique_nodes_before_insertion() {
        let retained = 73;
        let exact = retained + selection_bytes::<u32>(1, usize::MAX).unwrap();
        assert_eq!(
            subset_text_selection("AAAA", retained, exact).unwrap(),
            BTreeSet::from([65])
        );
        assert!(matches!(
            subset_text_selection("A", retained, exact - 1),
            Err(Error::ResourceLimit { .. })
        ));
        let mut selected = BTreeSet::from([65_u32]);
        let before = selected.clone();
        assert!(matches!(
            selection_insert(&mut selected, 66, retained, exact),
            Err(Error::ResourceLimit { .. })
        ));
        assert_eq!(selected, before);
        selection_insert(&mut selected, 65, retained, exact).unwrap();
        assert_eq!(selected, before);
        assert!(selection_bytes::<u32>(usize::MAX, usize::MAX).is_err());
    }

    #[test]
    fn subset_selection_arguments_include_text_ranges_and_spare_array_storage() {
        #[derive(Parser)]
        struct ParseSubset {
            #[command(flatten)]
            args: SubsetArgs,
        }
        for flag in ["--text", "--unicodes"] {
            let value = if flag == "--text" { "A" } else { "U+0041" };
            let mut args = ParseSubset::try_parse_from([
                "fontpixel",
                "source.ttf",
                flag,
                value,
                "--drop-table",
                "DSIG",
                "-o",
                "out.ttf",
            ])
            .unwrap()
            .args;
            args.drop_tables.try_reserve_exact(8).unwrap();
            args.drop_tables[0].try_reserve_exact(1024).unwrap();
            let text = if flag == "--text" {
                &mut args.text
            } else {
                &mut args.unicodes
            };
            text.as_mut().unwrap().try_reserve_exact(2048).unwrap();
            let expected = args.drop_tables.capacity() * size_of::<String>()
                + args.drop_tables[0].capacity()
                + args.text.as_ref().map_or(0, String::capacity)
                + args.unicodes.as_ref().map_or(0, String::capacity);
            assert_eq!(
                subset_selection_argument_bytes(&args, expected).unwrap(),
                expected
            );
            assert!(matches!(
                subset_selection_argument_bytes(&args, expected - 1),
                Err(Error::ResourceLimit { .. })
            ));
        }
    }

    #[test]
    fn subset_ranges_deduplicate_within_budget_and_keep_unicode_boundaries() {
        let retained = 113;
        let exact = retained + selection_bytes::<u32>(2, usize::MAX).unwrap();
        assert_eq!(
            unicodes("U+0041-0042,U+0041", retained, exact).unwrap(),
            BTreeSet::from([65, 66])
        );
        assert!(matches!(
            unicodes("U+0041-0042", retained, exact - 1),
            Err(Error::ResourceLimit { .. })
        ));
        for value in ["U+D800", "U+D7FF-E000", "U+110000", "U+0042-0041", ""] {
            assert!(matches!(
                unicodes(value, retained, usize::MAX),
                Err(Error::InvalidInput(_))
            ));
        }
        assert_eq!(
            unicodes("U+D7FF,U+E000,U+10FFFF", retained, usize::MAX).unwrap(),
            BTreeSet::from([0xd7ff, 0xe000, 0x10ffff])
        );
    }

    #[test]
    fn subset_web_decode_reserves_the_live_wire_allocation() {
        let sfnt = encoded_fixture();
        let woff = woff::woff1::encode(&sfnt, &Default::default(), &Default::default()).unwrap();
        let woff2 = woff::woff2::encode(&sfnt, &Default::default(), &Default::default()).unwrap();
        for mut wire in [woff.bytes, woff2.bytes] {
            wire.try_reserve_exact(1024).unwrap();
            let limits = ResourceLimits {
                max_total_bytes: wire.capacity(),
                ..Default::default()
            };
            assert!(matches!(
                decode_subset_input(wire, &limits),
                Err(Error::ResourceLimit { .. })
            ));
        }
    }

    #[test]
    fn subset_retention_includes_decoded_sfnt_auxiliary_and_diagnostics() {
        let mut sfnt = encoded_fixture();
        sfnt.try_reserve_exact(1024).unwrap();
        let mut metadata = Vec::with_capacity(2048);
        metadata.extend_from_slice(b"<metadata/>");
        let decoded = woff::Decoded {
            sfnt,
            auxiliary: woff::Auxiliary {
                metadata: Some(metadata),
                ..Default::default()
            },
            report: loss_report("subset.incoming"),
        };
        let exact = decoded.sfnt.capacity()
            + report_bytes(&decoded.report, usize::MAX).unwrap()
            + auxiliary_bytes(&decoded.auxiliary, usize::MAX).unwrap();
        assert_eq!(subset_input_bytes(&decoded, exact).unwrap(), exact);
        assert!(matches!(
            subset_input_bytes(&decoded, exact - 1),
            Err(Error::ResourceLimit { .. })
        ));
    }

    fn strict_output(filename: &str) -> Output {
        ParseVariants::try_parse_from([
            "fontpixel",
            "--base",
            "base.bdf",
            "-o",
            filename,
            "--strict",
        ])
        .unwrap()
        .args
        .output
    }

    #[test]
    fn encoder_reservation_counts_metadata_and_spare_route_storage_once() {
        let mut variants = Variants::new(Font::default());
        let before = variants_bytes(&variants, usize::MAX).unwrap();
        let mut copyright = String::with_capacity(4096);
        copyright.push('x');
        let added = copyright.capacity();
        variants.base.names.copyright = Some(copyright);
        assert_eq!(
            variants_bytes(&variants, usize::MAX).unwrap(),
            before + added
        );
        variants.languages.try_reserve_exact(3).unwrap();
        let route_capacity = variants.languages.capacity();
        assert_eq!(
            variants_bytes(&variants, usize::MAX).unwrap(),
            before + added + route_capacity * size_of::<variants::LanguageVariant>()
        );
        variants.add_language("ja", Font::default()).unwrap();
        let route_before = variants_bytes(&variants, usize::MAX).unwrap();
        let tags = &mut variants.languages[0].scripts;
        let old_capacity = tags.capacity();
        tags.try_reserve_exact(30).unwrap();
        let tag_growth = (tags.capacity() - old_capacity) * size_of::<[u8; 4]>();
        assert_eq!(
            variants_bytes(&variants, usize::MAX).unwrap(),
            route_before + tag_growth
        );
        let report = loss_report("source.notice");
        let exact = variants_bytes(&variants, usize::MAX).unwrap()
            + report_bytes(&report, usize::MAX).unwrap();
        let limits = ResourceLimits {
            max_total_bytes: exact + 17,
            ..Default::default()
        };
        let available = variants_limits(&variants, &report, &limits).unwrap();
        let mut expected = limits.clone();
        expected.max_total_bytes = 17;
        assert_eq!(available, expected);
        assert_eq!(
            variants_limits(
                &variants,
                &report,
                &ResourceLimits {
                    max_total_bytes: exact,
                    ..limits.clone()
                }
            )
            .unwrap()
            .max_total_bytes,
            0
        );
        assert!(matches!(
            variants_limits(
                &variants,
                &report,
                &ResourceLimits {
                    max_total_bytes: exact - 1,
                    ..limits
                }
            ),
            Err(Error::ResourceLimit { .. })
        ));
    }

    #[test]
    fn report_growth_has_an_exact_boundary_and_preserves_order() {
        const OWNED: usize = 73;
        for succeeds in [false, true] {
            let mut earlier = loss_report("read.first");
            let incoming = loss_report("encode.second");
            let before = earlier.clone();
            let capacity = earlier.entries.capacity();
            let count = earlier.entries.len() + incoming.entries.len();
            let exact = OWNED
                + report_bytes(&earlier, usize::MAX).unwrap()
                + report_bytes(&incoming, usize::MAX).unwrap()
                + count * size_of::<ReportEntry>();
            let result = append_report(
                &mut earlier,
                incoming,
                OWNED,
                if succeeds { exact } else { exact - 1 },
            );
            if succeeds {
                result.unwrap();
                assert_eq!(
                    earlier
                        .entries
                        .iter()
                        .map(|e| e.code.as_str())
                        .collect::<Vec<_>>(),
                    ["read.first", "encode.second"]
                );
            } else {
                assert!(matches!(result, Err(Error::ResourceLimit { .. })));
                assert_eq!(earlier, before);
                assert_eq!(earlier.entries.capacity(), capacity);
            }
        }
    }

    #[test]
    fn empty_incoming_report_releases_spare_capacity_before_retained_check() {
        let mut earlier = loss_report("read.first");
        let before = earlier.clone();
        let earlier_capacity = earlier.entries.capacity();
        let incoming = Report {
            entries: Vec::with_capacity(1024),
        };
        let owned = 73;
        let exact = owned + report_bytes(&earlier, usize::MAX).unwrap();
        assert!(report_bytes(&incoming, usize::MAX).unwrap() > exact);
        append_report(&mut earlier, incoming, owned, exact).unwrap();
        assert_eq!(earlier, before);
        assert_eq!(earlier.entries.capacity(), earlier_capacity);
    }

    #[test]
    fn report_reservation_counts_string_capacity_and_location_assets() {
        let mut report = loss_report("source.notice");
        let before = report_bytes(&report, usize::MAX).unwrap();
        let old = report.entries[0].message.capacity();
        report.entries[0].message.try_reserve_exact(1024).unwrap();
        let growth = report.entries[0].message.capacity() - old;
        let mut asset = String::with_capacity(2048);
        asset.push_str("source.bdf");
        let asset_capacity = asset.capacity();
        report.entries[0].location.asset = Some(asset);
        let exact = before + growth + asset_capacity;
        assert_eq!(report_bytes(&report, exact).unwrap(), exact);
        assert!(matches!(
            report_bytes(&report, exact - 1),
            Err(Error::ResourceLimit { .. })
        ));
    }

    #[test]
    fn webfont_allowance_reserves_original_sfnt_report_and_auxiliary() {
        for filename in ["must-not-exist.woff", "must-not-exist.woff2"] {
            let bytes = encoded_fixture();
            let report = loss_report("source.notice");
            let mut metadata = Vec::with_capacity(512);
            metadata.extend_from_slice(b"<metadata/>");
            let aux = woff::Auxiliary {
                metadata: Some(metadata),
                ..Default::default()
            };
            let retained = bytes.capacity()
                + report_bytes(&report, usize::MAX).unwrap()
                + auxiliary_bytes(&aux, usize::MAX).unwrap();
            let limits = ResourceLimits {
                max_total_bytes: retained,
                ..Default::default()
            };
            let available = codec_limits(&limits, retained).unwrap();
            assert_eq!(available.max_total_bytes, 0);
            assert_eq!(available.max_input_bytes, limits.max_input_bytes);
            assert_eq!(available.max_glyphs, limits.max_glyphs);
            let mut emitted = Vec::new();
            let result = strict_output(filename).save(bytes, report, aux, &limits, &mut emitted);
            // No output/working allowance remains, even though the source itself
            // fits. Resource failure precedes an already known strict loss.
            assert!(matches!(result, Err(Error::ResourceLimit { .. })));
            assert!(emitted.is_empty());
        }
    }

    #[test]
    fn direct_sfnt_is_counted_once_and_codec_errors_precede_strict_loss() {
        let bytes = encoded_fixture();
        let report = loss_report("source.notice");
        let aux = woff::Auxiliary::default();
        let exact = bytes.capacity()
            + report_bytes(&report, usize::MAX).unwrap()
            + auxiliary_bytes(&aux, usize::MAX).unwrap();
        // The retained sfnt, report and auxiliary data fit exactly, but the
        // directory validation transient has no allowance left.
        let limits = ResourceLimits {
            max_total_bytes: exact,
            ..Default::default()
        };
        let mut emitted = Vec::new();
        assert!(matches!(
            strict_output("must-not-exist.ttf").save(
                bytes.clone(),
                report.clone(),
                aux.clone(),
                &limits,
                &mut emitted
            ),
            Err(Error::ResourceLimit { .. })
        ));
        assert!(emitted.is_empty());
        // Directory validation alone needs `working` bytes. The save boundary is
        // exactly retained + working, so the sfnt is charged once, not twice.
        let parses = |total| {
            let limits = ResourceLimits {
                max_total_bytes: total,
                ..Default::default()
            };
            sfnt::Sfnt::parse(&bytes, &sfnt_limits(&limits)).is_ok()
        };
        let working = (0..1_000_000usize)
            .find(|total| parses(*total))
            .expect("tiny sfnt directory parses");
        let saves = |margin: usize| {
            let limits = ResourceLimits {
                max_total_bytes: exact + margin,
                ..Default::default()
            };
            strict_output("must-not-exist.ttf").save(
                bytes.clone(),
                report.clone(),
                aux.clone(),
                &limits,
                &mut Vec::new(),
            )
        };
        assert!(matches!(
            saves(working - 1),
            Err(Error::ResourceLimit { .. })
        ));
        let limits = ResourceLimits {
            max_total_bytes: exact + working,
            ..Default::default()
        };
        assert!(matches!(
            strict_output("must-not-exist.ttf").save(bytes, report, aux, &limits, &mut emitted),
            Err(Error::StrictLoss(_))
        ));
        assert!(emitted.is_empty());
        for filename in [
            "must-not-exist.ttf",
            "must-not-exist.woff",
            "must-not-exist.woff2",
        ] {
            let result = strict_output(filename).save(
                b"bad".to_vec(),
                loss_report("source.notice"),
                Default::default(),
                &ResourceLimits::default(),
                &mut emitted,
            );
            assert!(matches!(result, Err(Error::InvalidInput(_))));
            assert!(emitted.is_empty());
        }
    }

    #[test]
    fn opentype_help_examples_are_parsable() {
        use clap::CommandFactory;
        #[derive(Parser)]
        struct ParseSubset {
            #[command(flatten)]
            args: SubsetArgs,
        }
        for args in [
            vec![
                "fontpixel",
                "--base",
                "base.bdf",
                "--lang",
                "ja=jp.bdf",
                "-o",
                "regional.ttf",
            ],
            vec![
                "fontpixel",
                "--base",
                "base.bdf",
                "--shape",
                "dot",
                "-o",
                "dots.woff2",
                "--max-total-bytes",
                "536870912",
            ],
        ] {
            ParseVariants::try_parse_from(args).unwrap();
        }
        for args in [
            [
                "fontpixel",
                "regional.ttf",
                "--text",
                "Hello",
                "-o",
                "hello.woff2",
            ],
            [
                "fontpixel",
                "regional.ttf",
                "--unicodes",
                "U+0020-007E",
                "-o",
                "ascii.ttf",
            ],
        ] {
            let parsed = ParseSubset::try_parse_from(args).unwrap();
            assert_eq!(parsed.args.input, PathBuf::from("regional.ttf"));
        }
        assert!(
            ParseVariants::command()
                .render_long_help()
                .to_string()
                .contains("Example:")
        );
        assert!(
            ParseSubset::command()
                .render_long_help()
                .to_string()
                .contains("Example:")
        );
    }
}
