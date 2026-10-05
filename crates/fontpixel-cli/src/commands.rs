//! Native semantic comparison, structural validation and exact charset mapping.
use crate::{RasterArguments, ReportStyle, assets, batch, input_budget::InputBudget};
use clap::Args;
use fontpixel::charset::CharsetCodec;
use fontpixel::model::{Codepoint, GlyphId};
use fontpixel::report::{Impact, Level, Location};
use fontpixel::{
    Error, Font, FontSet, Glyph, ReadOptions, Report, ReportEntry, ResourceLimits, Result,
};
use serde::Serialize;
use std::{collections::BTreeSet, io::Write, mem::size_of, path::PathBuf};
#[cfg(any(
    feature = "opentype",
    feature = "otb",
    feature = "bdat",
    feature = "color"
))]
#[path = "commands/sfnt_validate.rs"]
mod sfnt_validate;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn limit_error(limit: usize) -> Error {
    Error::ResourceLimit {
        resource: "CLI command retained bytes",
        limit,
    }
}
fn charge(used: &mut usize, amount: usize, limit: usize) -> Result<()> {
    *used = used
        .checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or_else(|| limit_error(limit))?;
    Ok(())
}
fn array_bytes<T>(count: usize, limit: usize) -> Result<usize> {
    count
        .checked_mul(size_of::<T>())
        .filter(|n| *n <= limit)
        .ok_or_else(|| limit_error(limit))
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
    Ok(used)
}
fn strict(report: &Report, requested: bool) -> bool {
    requested && report.has_loss()
}
/// Reserve the actual caller-owned model and incoming diagnostics before native
/// encoding. No credit is guessed for a codec's internal source estimate: some
/// codecs may conservatively charge the model again. This reconstruction policy
/// preserves default limits, but default-workload acceptance needs real testing;
/// it is not a complete codec-transient or process-RSS proof.
/// Reported errors and strict losses precede accounting without report clones.
pub(crate) fn caller_writer_limits(
    fonts: &FontSet,
    limits: &ResourceLimits,
    report: Report,
    strict: bool,
) -> Result<(ResourceLimits, Report)> {
    reserve_caller(Some(fonts), limits, report, strict)
}
/// As [`caller_writer_limits`] for a model that is then moved into
/// `Registry::write_owned`: only the incoming diagnostics are reserved here.
/// The returned allowance still contains the model, which the owned writer
/// (or the registry's borrowing fallback) charges exactly.
pub(crate) fn caller_report_limits(
    limits: &ResourceLimits,
    report: Report,
    strict: bool,
) -> Result<(ResourceLimits, Report)> {
    reserve_caller(None, limits, report, strict)
}
fn reserve_caller(
    fonts: Option<&FontSet>,
    limits: &ResourceLimits,
    report: Report,
    strict: bool,
) -> Result<(ResourceLimits, Report)> {
    if report.has_errors() {
        return Err(Error::ReportedErrors(report));
    }
    if strict && report.has_loss() {
        return Err(Error::StrictLoss(report));
    }
    let mut retained = match fonts {
        Some(fonts) => fonts.retained_usage(limits.max_total_bytes)?.bytes,
        None => 0,
    };
    charge(
        &mut retained,
        report_bytes(&report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let mut remaining = limits.clone();
    remaining.max_total_bytes = remaining
        .max_total_bytes
        .checked_sub(retained)
        .ok_or_else(|| limit_error(limits.max_total_bytes))?;
    remaining.max_input_bytes = remaining.max_input_bytes.min(remaining.max_total_bytes);
    remaining.max_resource_bytes = remaining.max_resource_bytes.min(remaining.max_total_bytes);
    remaining.max_decompressed_bytes = remaining
        .max_decompressed_bytes
        .min(remaining.max_total_bytes);
    remaining.max_table_bytes = remaining.max_table_bytes.min(remaining.max_total_bytes);
    Ok((remaining, report))
}
fn load(
    input: &std::path::Path,
    from: Option<String>,
    raster: &RasterArguments,
    budget: &mut InputBudget,
) -> Result<fontpixel::formats::Loaded<FontSet>> {
    let mut registry = fontpixel::registry();
    raster.apply(&mut registry)?;
    let options = ReadOptions {
        format: from,
        limits: budget.remaining_limits()?,
    };
    let file = assets::FileInput::open(input, &options.limits, batch::sources())?;
    let loaded = registry.read(&file.input(), &options)?;
    drop(file);
    loaded.fonts.validate(&options.limits)?;
    budget.admit(&loaded)?;
    Ok(loaded)
}

#[derive(Args)]
#[command(
    after_help = "Example: fontpixel diff before.bdf after.pcf --json\nCompares all strikes, mappings, bitmap pixels and metrics; source names/wire metadata are excluded."
)]
pub(crate) struct DiffArgs {
    a: PathBuf,
    b: PathBuf,
    #[arg(long,value_parser=crate::format_parser())]
    from: Option<String>,
    #[arg(long,value_parser=crate::format_parser())]
    from_a: Option<String>,
    #[arg(long,value_parser=crate::format_parser())]
    from_b: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    strict: bool,
    /// Maximum detailed records; every difference is still counted.
    #[arg(long,default_value_t=1000,value_parser=clap::value_parser!(u32).range(0..=100000))]
    max_differences: u32,
    #[command(flatten)]
    raster: Box<RasterArguments>,
}
impl DiffArgs {
    pub(crate) fn source_identity_total_bytes(&self) -> usize {
        self.raster.input_limits().max_total_bytes
    }
    pub(crate) fn report_style(&self) -> ReportStyle {
        if self.json {
            ReportStyle::Json
        } else {
            ReportStyle::Text
        }
    }
}
#[derive(Serialize)]
struct Difference {
    kind: &'static str,
    font_index: usize,
    codepoint: Option<u32>,
    glyph_index: Option<usize>,
}
struct Differences {
    count: usize,
    details: Vec<Difference>,
    maximum: usize,
}
impl Differences {
    fn add(
        &mut self,
        kind: &'static str,
        font_index: usize,
        codepoint: Option<u32>,
        glyph_index: Option<usize>,
    ) -> Result<()> {
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| invalid("difference count overflow"))?;
        if self.details.len() < self.maximum {
            self.details.push(Difference {
                kind,
                font_index,
                codepoint,
                glyph_index,
            });
        }
        Ok(())
    }
    fn glyph(
        &mut self,
        a: &Glyph,
        b: &Glyph,
        font: usize,
        cp: Option<u32>,
        index: usize,
    ) -> Result<()> {
        if a.metrics != b.metrics {
            self.add("glyph_metrics", font, cp, Some(index))?;
        }
        if a.bitmap != b.bitmap {
            self.add("glyph_bitmap", font, cp, Some(index))?;
        }
        if a.codepoints != b.codepoints {
            self.add("glyph_mappings", font, cp, Some(index))?;
        }
        if a.unicode_sequences != b.unicode_sequences {
            self.add("glyph_sequences", font, cp, Some(index))?;
        }
        Ok(())
    }
}
#[derive(Serialize)]
struct DiffDocument<'a> {
    equal: bool,
    difference_count: usize,
    differences: &'a [Difference],
    truncated: bool,
    report_a: &'a Report,
    report_b: &'a Report,
}
pub(crate) fn diff(args: DiffArgs, out: &mut dyn Write) -> Result<()> {
    let limits = args.raster.input_limits();
    let mut budget = InputBudget::new(limits.clone());
    let a = load(
        &args.a,
        args.from_a.or_else(|| args.from.clone()),
        &args.raster,
        &mut budget,
    )?;
    let b = load(
        &args.b,
        args.from_b.or(args.from),
        &args.raster,
        &mut budget,
    )?;
    if strict(&a.report, args.strict) {
        return Err(Error::StrictLoss(a.report));
    }
    if strict(&b.report, args.strict) {
        return Err(Error::StrictLoss(b.report));
    }
    let mut retained = a.fonts.retained_usage(limits.max_total_bytes)?.bytes;
    charge(
        &mut retained,
        b.fonts.retained_usage(limits.max_total_bytes)?.bytes,
        limits.max_total_bytes,
    )?;
    charge(
        &mut retained,
        report_bytes(&a.report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    charge(
        &mut retained,
        report_bytes(&b.report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    charge(
        &mut retained,
        array_bytes::<Difference>(args.max_differences as usize, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let mut differences = Differences {
        count: 0,
        details: Vec::new(),
        maximum: args.max_differences as usize,
    };
    differences
        .details
        .try_reserve_exact(differences.maximum)
        .map_err(|_| limit_error(limits.max_total_bytes))?;
    if a.fonts.fonts.len() != b.fonts.fonts.len() {
        differences.add("font_count", 0, None, None)?;
    }
    for (index, (a, b)) in a.fonts.fonts.iter().zip(&b.fonts.fonts).enumerate() {
        compare_font(
            a,
            b,
            index,
            &mut differences,
            retained,
            limits.max_total_bytes,
        )?;
    }
    if args.json {
        crate::emit_json(
            out,
            &DiffDocument {
                equal: differences.count == 0,
                difference_count: differences.count,
                differences: &differences.details,
                truncated: differences.count > differences.details.len(),
                report_a: &a.report,
                report_b: &b.report,
            },
        )?;
    } else {
        writeln!(out, "{} difference(s)", differences.count)?;
        for d in &differences.details {
            writeln!(
                out,
                "font {}: {} code={:?} glyph={:?}",
                d.font_index, d.kind, d.codepoint, d.glyph_index
            )?;
        }
    }
    if differences.count == 0 {
        Ok(())
    } else {
        Err(invalid("font bitmap or metric differences found"))
    }
}
fn compare_font(
    a: &Font,
    b: &Font,
    index: usize,
    d: &mut Differences,
    retained: usize,
    limit: usize,
) -> Result<()> {
    let mut am = a.metrics.clone();
    let mut bm = b.metrics.clone();
    am.default_glyph = None;
    bm.default_glyph = None;
    if am != bm {
        d.add("font_metrics", index, None, None)?;
    }
    if a.charset.canonical_name() != b.charset.canonical_name() || a.charset.gl != b.charset.gl {
        d.add("charset", index, None, None)?;
    }
    let code_count = a
        .codepoints()
        .count()
        .checked_add(b.codepoints().count())
        .ok_or_else(|| limit_error(limit))?;
    let mut scratch = retained;
    charge(
        &mut scratch,
        code_count
            .checked_mul(128)
            .ok_or_else(|| limit_error(limit))?,
        limit,
    )?;
    let mut codes = BTreeSet::new();
    codes.extend(a.codepoints().map(|(cp, _)| cp));
    codes.extend(b.codepoints().map(|(cp, _)| cp));
    for cp in codes {
        let left = a.glyph_ids_for_codepoint(cp);
        let right = b.glyph_ids_for_codepoint(cp);
        if left.len() != right.len() {
            d.add("mapping_count", index, Some(cp.value()), None)?;
        }
        for (ordinal, (left, right)) in left.iter().zip(right).enumerate() {
            d.glyph(
                a.glyph(*left)
                    .ok_or_else(|| invalid("invalid glyph index"))?,
                b.glyph(*right)
                    .ok_or_else(|| invalid("invalid glyph index"))?,
                index,
                Some(cp.value()),
                ordinal,
            )?;
        }
    }
    let mut left = a.glyphs().filter(|(_, g)| g.codepoints.is_empty());
    let mut right = b.glyphs().filter(|(_, g)| g.codepoints.is_empty());
    let mut ordinal = 0;
    loop {
        match (left.next(), right.next()) {
            (None, None) => break,
            (Some((_, a)), Some((_, b))) => d.glyph(a, b, index, None, ordinal)?,
            _ => d.add("unencoded_glyph_count", index, None, Some(ordinal))?,
        }
        ordinal += 1;
    }
    match (
        a.metrics.default_glyph.and_then(|id| a.glyph(id)),
        b.metrics.default_glyph.and_then(|id| b.glyph(id)),
    ) {
        (Some(a), Some(b)) => d.glyph(a, b, index, None, usize::MAX)?,
        (None, None) => {}
        _ => d.add("default_glyph", index, None, None)?,
    }
    Ok(())
}

#[derive(Args)]
#[command(
    after_help = "Example: fontpixel validate font.ttf --json\nNative bitmap models and supported sfnt structures are checked; this is not a complete OpenType sanitizer."
)]
pub(crate) struct ValidateArgs {
    input: PathBuf,
    #[arg(long,value_parser=crate::format_parser())]
    from: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    strict: bool,
    #[command(flatten)]
    raster: Box<RasterArguments>,
}
impl ValidateArgs {
    pub(crate) fn source_identity_total_bytes(&self) -> usize {
        self.raster.input_limits().max_total_bytes
    }
    pub(crate) fn report_style(&self) -> ReportStyle {
        if self.json {
            ReportStyle::Json
        } else {
            ReportStyle::Text
        }
    }
}
#[derive(Serialize)]
struct FaceValidation {
    font_index: usize,
    glyph_count: usize,
    checks: Vec<&'static str>,
}
#[derive(Serialize)]
struct Validation<'a> {
    valid: bool,
    format: &'a str,
    font_count: usize,
    faces: &'a [FaceValidation],
    report: &'a Report,
}
pub(crate) fn validate(args: ValidateArgs, out: &mut dyn Write) -> Result<()> {
    #[cfg(any(
        feature = "opentype",
        feature = "otb",
        feature = "bdat",
        feature = "color"
    ))]
    if sfnt_validate::try_validate(&args, out)? {
        return Ok(());
    }
    let limits = args.raster.input_limits();
    let mut budget = InputBudget::new(limits.clone());
    let loaded = load(&args.input, args.from, &args.raster, &mut budget)?;
    if strict(&loaded.report, args.strict) {
        return Err(Error::StrictLoss(loaded.report));
    }
    let mut remaining = loaded.fonts.retained_usage(limits.max_total_bytes)?.bytes;
    charge(
        &mut remaining,
        report_bytes(&loaded.report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    charge(
        &mut remaining,
        loaded
            .fonts
            .fonts
            .len()
            .checked_mul(size_of::<FaceValidation>() + 32)
            .ok_or_else(|| limit_error(limits.max_total_bytes))?,
        limits.max_total_bytes,
    )?;
    let faces: Vec<_> = loaded
        .fonts
        .fonts
        .iter()
        .enumerate()
        .map(|(font_index, font)| FaceValidation {
            font_index,
            glyph_count: font.len(),
            checks: vec!["native_decode", "model"],
        })
        .collect();
    if args.json {
        crate::emit_json(
            out,
            &Validation {
                valid: true,
                format: loaded.format,
                font_count: faces.len(),
                faces: &faces,
                report: &loaded.report,
            },
        )
    } else {
        writeln!(
            out,
            "Valid {}: {} font(s); native decode and model checks",
            loaded.format,
            faces.len()
        )?;
        write!(out, "{}", loaded.report)?;
        Ok(())
    }
}

#[derive(Args)]
#[command(
    after_help = "Example: fontpixel charset input.bdf -o latin.bdf --to cp1252 --strict\nUnknown or compiled-out mappings are rejected. Unmappable glyph codes are reported as loss."
)]
pub(crate) struct CharsetArgs {
    input: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long)]
    to: String,
    #[arg(long)]
    source_charset: Option<String>,
    #[arg(long,value_parser=crate::format_parser())]
    from: Option<String>,
    #[arg(long,alias="output-format",value_parser=crate::format_parser())]
    format: Option<String>,
    #[arg(long,num_args=1,action=clap::ArgAction::Set)]
    gl: Option<bool>,
    #[arg(long)]
    strict: bool,
    #[arg(long, value_enum, default_value = "text")]
    report: ReportStyle,
    /// Maximum native output table bytes; source decoding keeps its input limits.
    #[arg(long)]
    max_table_bytes: Option<usize>,
    #[command(flatten)]
    raster: Box<RasterArguments>,
}
impl CharsetArgs {
    pub(crate) fn source_identity_total_bytes(&self) -> usize {
        self.raster.input_limits().max_total_bytes
    }
    pub(crate) fn report_style(&self) -> ReportStyle {
        self.report
    }
}
fn codec(name: &str, gl: Option<bool>) -> Result<CharsetCodec> {
    let codec = CharsetCodec::new(name, gl).map_err(|e| invalid(e.to_string()))?;
    if !codec.known() {
        return Err(invalid(format!("unknown charset {name}")));
    }
    Ok(codec)
}
fn mapped(
    cp: Codepoint,
    source: &CharsetCodec,
    target: &CharsetCodec,
    reinterpret: bool,
) -> Option<Codepoint> {
    let unicode = if reinterpret || matches!(cp, Codepoint::Legacy(_)) {
        source.cp2unicode(cp.value())?
    } else {
        cp.scalar()? as u32
    };
    let value = target.unicode2cp(unicode)?;
    Some(if target.kind() == fontpixel::charset::Kind::Unicode {
        Codepoint::Unicode(value)
    } else {
        Codepoint::Legacy(value)
    })
}
fn loss(
    report: &mut Report,
    used: &mut usize,
    limit: usize,
    code: &str,
    location: Location,
    message: &str,
) -> Result<()> {
    // Reserve receiving-vector growth plus both owned strings before allocation.
    let reserve = 4 * size_of::<ReportEntry>() + code.len() + message.len();
    charge(used, reserve, limit)?;
    report
        .entries
        .try_reserve(1)
        .map_err(|_| limit_error(limit))?;
    report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: code.into(),
        message: message.into(),
        location,
    });
    Ok(())
}
#[derive(Serialize)]
struct CharsetDocument<'a> {
    output: &'a std::path::Path,
    format: &'a str,
    charset: &'a str,
    font_count: usize,
    report: &'a Report,
}
pub(crate) fn charset(args: CharsetArgs, out: &mut dyn Write) -> Result<()> {
    let target = codec(&args.to, args.gl)?;
    let limits = args.raster.input_limits();
    let mut registry = fontpixel::registry();
    args.raster.apply(&mut registry)?;
    let id = match args.format {
        Some(id) => id,
        None => registry
            .format_from_name(&args.output.to_string_lossy())?
            .id
            .to_owned(),
    };
    registry.ensure_writable(&id)?;
    let mut budget = InputBudget::new(limits.clone());
    let mut loaded = load(&args.input, args.from, &args.raster, &mut budget)?;
    let mut retained = loaded.fonts.retained_usage(limits.max_total_bytes)?.bytes;
    charge(
        &mut retained,
        report_bytes(&loaded.report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    for (font_index, font) in loaded.fonts.fonts.iter_mut().enumerate() {
        let source = codec(
            args.source_charset
                .as_deref()
                .unwrap_or(&font.charset.canonical_name()),
            font.charset.gl,
        )?;
        // One whole-strike reserve dominates edit_glyph's temporary deep clone;
        // it is computed once, not by rescanning the font for each alias.
        let strike = font.retained_usage(limits.max_total_bytes)?.bytes;
        let id_bytes = array_bytes::<GlyphId>(font.len(), limits.max_total_bytes)?;
        let mut scratch = retained;
        charge(&mut scratch, strike, limits.max_total_bytes)?;
        charge(&mut scratch, id_bytes, limits.max_total_bytes)?;
        charge(&mut scratch, 4096, limits.max_total_bytes)?; // bounded charset/XLFD/property edits
        let mut ids = Vec::new();
        ids.try_reserve_exact(font.len())
            .map_err(|_| limit_error(limits.max_total_bytes))?;
        ids.extend(font.glyphs().map(|(id, _)| id));
        if array_bytes::<GlyphId>(ids.capacity(), limits.max_total_bytes)? > id_bytes {
            return Err(limit_error(limits.max_total_bytes));
        }
        for glyph_id in ids {
            let glyph = font
                .glyph(glyph_id)
                .ok_or_else(|| invalid("invalid glyph index"))?;
            let had_codes = !glyph.codepoints.is_empty();
            let mut alias_budget = scratch;
            charge(
                &mut alias_budget,
                glyph
                    .codepoints
                    .len()
                    .checked_mul(128)
                    .ok_or_else(|| limit_error(limits.max_total_bytes))?,
                limits.max_total_bytes,
            )?;
            let mut codes = Vec::new();
            codes
                .try_reserve_exact(glyph.codepoints.len())
                .map_err(|_| limit_error(limits.max_total_bytes))?;
            let mut seen = BTreeSet::new();
            for cp in &glyph.codepoints {
                if let Some(code) = mapped(*cp, &source, &target, args.source_charset.is_some()) {
                    if seen.insert(code) {
                        codes.push(code);
                    }
                } else {
                    loss(
                        &mut loaded.report,
                        &mut alias_budget,
                        limits.max_total_bytes,
                        "charset.unmappable_code",
                        Location {
                            font_index: Some(font_index),
                            glyph_index: Some(glyph_id.index()),
                            codepoint: Some(cp.value()),
                            asset: None,
                        },
                        "glyph code cannot be represented in the destination charset",
                    )?;
                }
            }
            // Keep diagnostic growth charged after the alias scratch is dropped.
            retained = retained
                .checked_add(alias_budget - scratch - glyph.codepoints.len() * 128)
                .ok_or_else(|| limit_error(limits.max_total_bytes))?;
            scratch = retained
                .checked_add(strike)
                .and_then(|n| n.checked_add(id_bytes + 4096))
                .filter(|n| *n <= limits.max_total_bytes)
                .ok_or_else(|| limit_error(limits.max_total_bytes))?;
            if had_codes && codes.is_empty() && glyph.unicode_sequences.is_empty() {
                font.remove_glyph(glyph_id);
            } else {
                font.edit_glyph(glyph_id, |glyph| {
                    glyph.codepoints = codes;
                    Ok(())
                })?;
            }
        }
        if let Some(old) = font.metrics.default_character {
            font.metrics.default_character =
                mapped(old, &source, &target, args.source_charset.is_some());
            if font.metrics.default_character.is_none() {
                loss(
                    &mut loaded.report,
                    &mut retained,
                    limits.max_total_bytes,
                    "charset.unmappable_default_character",
                    Location {
                        font_index: Some(font_index),
                        glyph_index: None,
                        codepoint: Some(old.value()),
                        asset: None,
                    },
                    "default character cannot be represented in the destination charset",
                )?;
            }
        }
        let (registry_name, encoding) = fontpixel::charset::charset_props(target.name());
        font.charset.registry = Some(registry_name.clone());
        font.charset.encoding = Some(encoding.clone());
        font.charset.name = Some(target.name().into());
        font.charset.gl = Some(target.gl());
        font.charset.code_page = None;
        font.charset.windows_charset = None;
        font.charset.code_ranges.clear();
        font.properties
            .set("CHARSET_REGISTRY", registry_name.clone());
        font.properties.set("CHARSET_ENCODING", encoding.clone());
        if let Some(code) = font.metrics.default_character {
            font.properties.set("DEFAULT_CHAR", i64::from(code.value()));
        } else {
            font.properties.remove("DEFAULT_CHAR");
        }
        if font.names.xlfd_fields.len() == 14 {
            font.names.xlfd_fields[12] = registry_name;
            font.names.xlfd_fields[13] = encoding;
            font.names.xlfd = Some(format!("-{}", font.names.xlfd_fields.join("-")));
        }
        // Carry any persistent metadata/index growth to later strikes. The old
        // strike's charge is replaced, rather than charging the same model twice.
        let updated = font.retained_usage(limits.max_total_bytes)?.bytes;
        retained = retained
            .checked_sub(strike)
            .and_then(|n| n.checked_add(updated))
            .filter(|n| *n <= limits.max_total_bytes)
            .ok_or_else(|| limit_error(limits.max_total_bytes))?;
    }
    let mut writer_resources = limits.clone();
    if let Some(maximum) = args.max_table_bytes {
        writer_resources.max_table_bytes = writer_resources.max_table_bytes.min(maximum);
    }
    let (writer_limits, report) =
        caller_writer_limits(&loaded.fonts, &writer_resources, loaded.report, args.strict)?;
    loaded.fonts.validate(&limits)?;
    let filename = args
        .output
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("output requires a UTF-8 filename"))?;
    let mut options = fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(filename)?);
    options.strict = args.strict;
    options.transport_limits = Some(fontpixel::formats::TransportLimits {
        max_output_bytes: writer_limits.max_total_bytes,
        resources: writer_limits,
    });
    let written = registry.write_with_report(&loaded.fonts, &id, &options, report)?;
    let font_count = loaded.fonts.fonts.len();
    drop(loaded.fonts);
    let sources = batch::sources();
    batch::persist_outputs(
        vec![assets::PendingOutput {
            destination: args.output.clone(),
            bundle: written.output,
        }],
        &sources,
    )?;
    match args.report {
        ReportStyle::Json => crate::emit_json(
            out,
            &CharsetDocument {
                output: &args.output,
                format: &id,
                charset: target.name(),
                font_count,
                report: &written.report,
            },
        ),
        ReportStyle::Text => {
            write!(out, "{}", written.report)?;
            writeln!(
                out,
                "Wrote {font_count} font(s) with charset {}",
                target.name()
            )?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    fn metadata_heavy_model() -> FontSet {
        let mut font = Font::default();
        // Supply the normalized SIZE and FONTBOUNDINGBOX data required by a
        // fresh BDF model so the native writer reaches its resource boundary.
        font.metrics.point_size = Some(fontpixel::model::Rational::integer(1));
        font.metrics.resolution = Some(fontpixel::model::Vector::new(75, 75));
        font.metrics.bounds = Some(fontpixel::bitmap::Bounds::new(1, 1, 0, 0));
        let mut copyright = String::with_capacity(128 * 1024);
        copyright.push_str("Original author notice");
        font.names.copyright = Some(copyright);
        let mut property = String::with_capacity(64 * 1024);
        property.push_str("preserved metadata");
        font.properties.set("EXTRA_NOTE", property);
        let mut glyph = Glyph::new("#".parse::<fontpixel::Bitmap>().unwrap());
        glyph.codepoints.push(Codepoint::Legacy(0));
        glyph.metrics.advance = Some(fontpixel::model::Vector::new(1, 0));
        font.add_glyph(glyph).unwrap();
        let mut fonts = FontSet::from(font);
        fonts.fonts.reserve_exact(3);
        fonts
    }

    fn diagnostic(level: Level, impact: Impact) -> Report {
        let mut report = Report {
            entries: Vec::with_capacity(8),
        };
        let mut code = String::with_capacity(256);
        code.push_str("test.retained");
        let mut message = String::with_capacity(1024);
        message.push_str("retained diagnostic text");
        let mut asset = String::with_capacity(512);
        asset.push_str("source.bdf");
        report.push(ReportEntry {
            level,
            impact,
            code,
            message,
            location: Location {
                asset: Some(asset),
                ..Default::default()
            },
        });
        report
    }

    #[test]
    fn caller_writer_reserves_full_model_and_diagnostic_capacities() {
        let report = diagnostic(Level::Warning, Impact::Repair);
        let expected = size_of::<Report>()
            + report.entries.capacity() * size_of::<ReportEntry>()
            + report.entries[0].code.capacity()
            + report.entries[0].message.capacity()
            + report.entries[0]
                .location
                .asset
                .as_ref()
                .unwrap()
                .capacity();
        let fonts = metadata_heavy_model();
        let model = fonts.retained_usage(usize::MAX).unwrap().bytes;
        let limits = ResourceLimits {
            max_total_bytes: model + expected + 4096,
            ..Default::default()
        };
        let (writer, report) = caller_writer_limits(&fonts, &limits, report, false).unwrap();
        assert_eq!(writer.max_total_bytes, 4096);
        assert_eq!(writer.max_input_bytes, limits.max_input_bytes.min(4096));
        assert_eq!(
            writer.max_resource_bytes,
            limits.max_resource_bytes.min(4096)
        );
        assert_eq!(
            writer.max_decompressed_bytes,
            limits.max_decompressed_bytes.min(4096)
        );
        assert_eq!(writer.max_table_bytes, limits.max_table_bytes.min(4096));
        assert_eq!(writer.max_glyphs, limits.max_glyphs);
        assert_eq!(report.entries[0].message, "retained diagnostic text");
    }

    #[test]
    fn charset_writer_rejects_report_reservation_overflow_before_encoding() {
        let report = diagnostic(Level::Info, Impact::None);
        let fonts = metadata_heavy_model();
        let retained = fonts.retained_usage(usize::MAX).unwrap().bytes
            + report_bytes(&report, usize::MAX).unwrap();
        let limits = ResourceLimits {
            max_total_bytes: retained - 1,
            ..Default::default()
        };
        assert!(matches!(
            caller_writer_limits(&fonts, &limits, report, false),
            Err(Error::ResourceLimit { .. })
        ));
        let report = diagnostic(Level::Info, Impact::None);
        let limits = ResourceLimits {
            max_total_bytes: retained,
            ..Default::default()
        };
        assert_eq!(
            caller_writer_limits(&fonts, &limits, report, false)
                .unwrap()
                .0
                .max_total_bytes,
            0
        );
    }

    #[test]
    fn charset_writer_preserves_error_and_strict_precedence_without_cloning_reports() {
        let limits = ResourceLimits {
            max_total_bytes: 0,
            ..Default::default()
        };
        let fonts = metadata_heavy_model();
        let error = caller_writer_limits(
            &fonts,
            &limits,
            diagnostic(Level::Error, Impact::Loss),
            true,
        )
        .unwrap_err();
        assert!(matches!(error, Error::ReportedErrors(_)));
        let error = caller_writer_limits(
            &fonts,
            &limits,
            diagnostic(Level::Warning, Impact::Loss),
            true,
        )
        .unwrap_err();
        assert!(matches!(error, Error::StrictLoss(_)));
        let error = caller_writer_limits(
            &fonts,
            &limits,
            diagnostic(Level::Warning, Impact::Loss),
            false,
        )
        .unwrap_err();
        assert!(matches!(error, Error::ResourceLimit { .. }));
    }

    #[cfg(any(feature = "bdf", feature = "raw"))]
    fn native_caller_budget_boundary(id: &str) {
        let fonts = metadata_heavy_model();
        let before = fonts.retained_usage(usize::MAX).unwrap();
        let retained = before.bytes
            + report_bytes(&diagnostic(Level::Info, Impact::None), usize::MAX).unwrap();
        let registry = fontpixel::registry();
        let attempt = |total| {
            let limits = ResourceLimits {
                max_total_bytes: total,
                ..Default::default()
            };
            let (work, report) = caller_writer_limits(
                &fonts,
                &limits,
                diagnostic(Level::Info, Impact::None),
                false,
            )?;
            let mut options =
                fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(format!("font.{id}"))?);
            options.transport_limits = Some(fontpixel::formats::TransportLimits {
                max_output_bytes: work.max_total_bytes,
                resources: work,
            });
            registry.write_with_report(&fonts, id, &options, report)
        };
        let mut low = retained;
        let mut high = 8 * 1024 * 1024;
        let expected = attempt(high).unwrap().output.into_single().unwrap();
        assert!(attempt(low).is_err());
        // Find this writer's actual admission boundary instead of inventing or
        // refunding a heuristic source charge. Only bounded tiny-font calls run.
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if attempt(middle).is_ok() {
                high = middle;
            } else {
                low = middle;
            }
        }
        assert_eq!(
            attempt(high).unwrap().output.into_single().unwrap(),
            expected
        );
        assert!(attempt(high - 1).is_err());
        let limits = ResourceLimits {
            max_total_bytes: high,
            ..Default::default()
        };
        let (work, _) = caller_writer_limits(
            &fonts,
            &limits,
            diagnostic(Level::Info, Impact::None),
            false,
        )
        .unwrap();
        assert_eq!(work.max_total_bytes, high - retained);
        assert_eq!(fonts.retained_usage(usize::MAX).unwrap(), before);
        assert_eq!(
            fonts.fonts[0].names.copyright.as_deref(),
            Some("Original author notice")
        );
    }

    #[cfg(feature = "bdf")]
    #[test]
    fn caller_reservation_exact_boundary_for_metadata_heavy_bdf_writer() {
        native_caller_budget_boundary("bdf");
    }

    #[cfg(feature = "raw")]
    #[test]
    fn caller_reservation_exact_boundary_for_metadata_heavy_raw_writer() {
        native_caller_budget_boundary("raw");
    }
}
