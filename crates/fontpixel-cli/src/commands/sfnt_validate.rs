//! Reconstructed structural validator; pending compilation and renewed acceptance.
//! Every allocating decoder receives limits derived from remaining owned storage.
#[cfg(feature = "opentype")]
use super::report_bytes;
use super::{FaceValidation, ValidateArgs, Validation, charge, invalid, limit_error, strict};
use crate::{assets, batch};
use fontpixel::{Error, Report, ResourceLimits, Result, sfnt};
use std::{io::Write, mem::size_of};

fn bytes(data: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    offset
        .checked_add(length)
        .and_then(|end| data.get(offset..end))
        .ok_or_else(|| invalid("truncated sfnt structure or overflowing offset"))
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes(data, offset, 2)?
            .try_into()
            .map_err(|_| invalid("invalid u16"))?,
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes(data, offset, 4)?
            .try_into()
            .map_err(|_| invalid("invalid u32"))?,
    ))
}
fn sub(limit: usize, used: usize) -> Result<usize> {
    limit.checked_sub(used).ok_or_else(|| limit_error(limit))
}
fn product(count: usize, size: usize, limit: usize) -> Result<usize> {
    count
        .checked_mul(size)
        .filter(|n| *n <= limit)
        .ok_or_else(|| limit_error(limit))
}
fn bounded(n: usize, limit: usize) -> Result<()> {
    if n > limit {
        Err(limit_error(limit))
    } else {
        Ok(())
    }
}
fn limits(resources: &ResourceLimits, allowance: usize) -> sfnt::Limits {
    let mut resources = resources.clone();
    resources.max_total_bytes = allowance;
    resources.max_table_bytes = resources.max_table_bytes.min(allowance);
    sfnt::Limits {
        resources,
        max_tables: 4095.min(allowance / 256),
        max_cmap_mappings: 0x110000.min(allowance / 128),
        max_name_records: 8192.min(allowance / 128),
    }
}
fn sfnt_magic(data: &[u8]) -> bool {
    data.starts_with(b"\0\x01\0\0")
        || data.starts_with(b"OTTO")
        || data.starts_with(b"true")
        || data.starts_with(b"ttcf")
        || data.starts_with(b"wOFF")
        || data.starts_with(b"wOF2")
}
pub(super) fn try_validate(args: &ValidateArgs, out: &mut dyn Write) -> Result<bool> {
    if args
        .from
        .as_deref()
        .is_some_and(|id| !matches!(id, "ttf" | "otf" | "ttc" | "woff" | "woff2"))
    {
        return Ok(false);
    }
    if let Some(id) = args.from.as_deref() {
        let registry = fontpixel::registry();
        let status = registry
            .formats()
            .find(|status| status.format.id == id)
            .ok_or_else(|| Error::UnknownFormat(id.into()))?;
        if !status.format.feature_enabled {
            return Err(Error::FeatureDisabled { format: id.into() });
        }
        if !status.read {
            return Err(Error::NotImplemented {
                format: id.into(),
                operation: fontpixel::Operation::Read,
            });
        }
    }
    let resources = args.raster.input_limits();
    let file = assets::FileInput::open(&args.input, &resources, batch::sources())?;
    let input = file.input();
    let context = fontpixel::resources::ReadContext::new(&input, &resources)?;
    let data = context.bytes();
    if let Some(id) = args.from.as_deref() {
        let matches = match id {
            "ttf" => data.starts_with(b"\0\x01\0\0") || data.starts_with(b"true"),
            "otf" => data.starts_with(b"OTTO"),
            "ttc" => data.starts_with(b"ttcf"),
            "woff" => data.starts_with(b"wOFF"),
            "woff2" => data.starts_with(b"wOF2"),
            _ => false,
        };
        if !matches {
            return Err(invalid(format!(
                "input container does not match explicit --from {id}"
            )));
        }
    }
    if !sfnt_magic(data) {
        return Ok(false);
    }
    let allowance = sub(resources.max_total_bytes, context.retained_input_bytes())?;
    #[cfg(feature = "opentype")]
    let mut report = Report::default();
    #[cfg(not(feature = "opentype"))]
    let report = Report::default();
    let (format, faces) = if data.starts_with(b"wOFF") || data.starts_with(b"wOF2") {
        #[cfg(feature = "opentype")]
        {
            let mut wl = fontpixel::woff::Limits {
                sfnt: limits(&resources, allowance),
                ..Default::default()
            };
            // WOFF transform storage and auxiliary buffers are not free merely
            // because the compressed source is small.
            wl.max_total_points = wl.max_total_points.min(allowance / 64);
            wl.max_points_per_glyph = wl.max_points_per_glyph.min(wl.max_total_points);
            wl.max_total_components = wl.max_total_components.min(allowance / 128);
            wl.max_total_instructions = wl.max_total_instructions.min(allowance / 4);
            wl.max_auxiliary_bytes = wl.max_auxiliary_bytes.min(allowance / 4);
            let decoded = if data.starts_with(b"wOFF") {
                fontpixel::woff::woff1::decode(data, &wl)?
            } else {
                fontpixel::woff::woff2::decode(data, &wl)?
            };
            report = decoded.report;
            let decoded_bytes = decoded.sfnt;
            drop(decoded.auxiliary);
            let remaining = sub(
                sub(allowance, decoded_bytes.capacity())?,
                report_bytes(&report, allowance)?,
            )?;
            (
                if data.starts_with(b"wOFF") {
                    "woff"
                } else {
                    "woff2"
                },
                single(&decoded_bytes, &resources, remaining, 0)?,
            )
        }
        #[cfg(not(feature = "opentype"))]
        {
            return Err(Error::FeatureDisabled {
                format: if data.starts_with(b"wOFF") {
                    "woff"
                } else {
                    "woff2"
                }
                .into(),
            });
        }
    } else if data.starts_with(b"ttcf") {
        #[cfg(feature = "opentype")]
        {
            let l = sfnt::collection::Limits {
                sfnt: limits(&resources, allowance),
                max_faces: 1024.min(allowance / 512),
                max_table_references: 65536.min(allowance / 256),
                max_expanded_bytes: allowance,
            };
            let collection = sfnt::collection::Collection::parse(data, &l)?;
            collection.validate_checksums()?;
            let references = collection.faces.iter().try_fold(0usize, |sum, face| {
                sum.checked_add(face.tables.len())
                    .ok_or_else(|| limit_error(allowance))
            })?;
            let mut owned = product(references, 256, allowance)?;
            charge(
                &mut owned,
                product(collection.faces.len(), 512, allowance)?,
                allowance,
            )?;
            let mut faces = Vec::new();
            faces
                .try_reserve_exact(collection.faces.len())
                .map_err(|_| limit_error(allowance))?;
            for (index, face) in collection.faces.iter().enumerate() {
                faces.push(tables(
                    |tag| face.tables.iter().find(|r| r.tag == tag).map(|r| r.data),
                    &resources,
                    sub(allowance, owned)?,
                    index,
                )?);
            }
            ("ttc", faces)
        }
        #[cfg(not(feature = "opentype"))]
        {
            return Err(Error::FeatureDisabled {
                format: "ttc".into(),
            });
        }
    } else {
        ("sfnt", single(data, &resources, allowance, 0)?)
    };
    if strict(&report, args.strict) {
        return Err(Error::StrictLoss(report));
    }
    if args.json {
        crate::emit_json(
            out,
            &Validation {
                valid: true,
                format,
                font_count: faces.len(),
                faces: &faces,
                report: &report,
            },
        )?;
    } else {
        writeln!(
            out,
            "Valid {format}: {} face(s); bounded supported sfnt structures",
            faces.len()
        )?;
        for face in &faces {
            writeln!(
                out,
                "face {}: {} glyphs; {}",
                face.font_index,
                face.glyph_count,
                face.checks.join(", ")
            )?;
        }
        write!(out, "{report}")?;
    }
    Ok(true)
}
fn single(
    data: &[u8],
    resources: &ResourceLimits,
    allowance: usize,
    index: usize,
) -> Result<Vec<FaceValidation>> {
    let l = limits(resources, allowance);
    let count = usize::from(u16_at(data, 4)?);
    let directory = product(count, 256, allowance)?;
    let font = sfnt::Sfnt::parse(data, &l)?;
    font.validate_checksums()?;
    Ok(vec![tables(
        |tag| font.table(tag),
        resources,
        sub(allowance, directory)?,
        index,
    )?])
}
fn tables<'a>(
    table: impl Fn([u8; 4]) -> Option<&'a [u8]>,
    resources: &ResourceLimits,
    allowance: usize,
    index: usize,
) -> Result<FaceValidation> {
    // Reserve the bounded, small scope report while typed tables are decoded one
    // at a time. None of the large decoded table trees survives the next table.
    let allowance = sub(allowance, 1024)?;
    let l = limits(resources, allowance);
    let head = table(*b"head").ok_or_else(|| invalid("missing head"))?;
    bytes(head, 0, 54)?;
    if u32_at(head, 12)? != 0x5f0f3cf5 {
        return Err(invalid("invalid head magic"));
    }
    let upm = u16_at(head, 18)?;
    if !(16..=16384).contains(&upm) {
        return Err(invalid("invalid units per em"));
    }
    let maxp = table(*b"maxp").ok_or_else(|| invalid("missing maxp"))?;
    let version = u32_at(maxp, 0)?;
    if version == 0x10000 {
        bytes(maxp, 0, 32)?;
    } else if version != 0x5000 {
        return Err(invalid("unsupported maxp version"));
    }
    let glyphs = u16_at(maxp, 4)?;
    if glyphs == 0 || usize::from(glyphs) > resources.max_glyphs {
        return Err(Error::ResourceLimit {
            resource: "sfnt glyph count",
            limit: resources.max_glyphs,
        });
    }
    let mut checks = vec!["directory", "checksums", "head", "maxp"];
    let cmap = table(*b"cmap").ok_or_else(|| invalid("missing cmap"))?;
    drop(sfnt::cmap::decode(cmap, glyphs, &l)?);
    checks.push("cmap");
    for (header, data, axis, label) in [
        (
            *b"hhea",
            *b"hmtx",
            sfnt::metrics::Axis::Horizontal,
            "hhea/hmtx",
        ),
        (
            *b"vhea",
            *b"vmtx",
            sfnt::metrics::Axis::Vertical,
            "vhea/vmtx",
        ),
    ] {
        match (table(header), table(data)) {
            (Some(header), Some(data)) => {
                bounded(
                    product(
                        usize::from(glyphs),
                        size_of::<sfnt::metrics::LongMetric>() * 2,
                        allowance,
                    )?,
                    allowance,
                )?;
                drop(sfnt::metrics::decode(axis, header, data, glyphs, &l)?);
                checks.push(label);
            }
            (None, None) if header == *b"vhea" => {}
            _ => return Err(invalid("missing paired horizontal/vertical metric table")),
        }
    }
    if let Some(data) = table(*b"name") {
        name_budget(data, allowance)?;
        let names = sfnt::name::NameTable::decode(data, &l)?;
        for record in &names.records {
            drop(record.text()?);
        }
        drop(names);
        checks.push("name");
    }
    if let Some(data) = table(*b"OS/2") {
        sfnt::os2::Os2Table::decode(data, &l)?;
        checks.push("OS/2");
    }
    if let Some(data) = table(*b"post") {
        // Pascal strings have <=255 bytes; duplicate indices can each allocate
        // a fresh string. Include Vec growth and string payload for every glyph.
        if u32_at(data, 0)? != 0x30000 {
            bounded(
                product(
                    usize::from(glyphs).max(4),
                    2 * size_of::<String>() + 255,
                    allowance,
                )?,
                allowance,
            )?;
        }
        drop(sfnt::post::PostTable::decode(data, glyphs, &l)?);
        checks.push("post");
    }
    if let Some(data) = table(*b"gasp") {
        bounded(
            product(
                usize::from(u16_at(data, 2)?),
                2 * size_of::<sfnt::gasp::Range>(),
                allowance,
            )?,
            allowance,
        )?;
        drop(sfnt::gasp::GaspTable::decode(data, &l)?);
        checks.push("gasp");
    }
    let mut has_glyphs = false;
    match (table(*b"glyf"), table(*b"loca")) {
        (Some(glyf), Some(loca)) => {
            if version != 0x10000 {
                return Err(invalid("TrueType outlines require maxp 1.0"));
            }
            let format = u16_at(head, 50)? as i16;
            let gl = glyf_limits(glyf, loca, glyphs, format, &l, allowance)?;
            drop(sfnt::glyf::GlyfTable::decode(
                glyf, loca, glyphs, format, &gl,
            )?);
            checks.push("glyf/loca/component_graph");
            has_glyphs = true;
        }
        (None, None) => {}
        _ => return Err(invalid("missing paired glyf/loca table")),
    }
    if table(*b"CFF2").is_some() {
        return Err(invalid("CFF2 validation is not supported"));
    }
    if let Some(data) = table(*b"CFF ") {
        #[cfg(feature = "opentype")]
        {
            validate_cff(data, upm, glyphs, &l, allowance)?;
            checks.push("CFF1/charstrings");
            has_glyphs = true;
        }
        #[cfg(not(feature = "opentype"))]
        {
            let _ = data;
            return Err(Error::FeatureDisabled {
                format: "cff".into(),
            });
        }
    }
    let mut bl = sfnt::bitmap::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    bl.max_strikes = bl.max_strikes.min(allowance / 256);
    bl.max_pixel_work = bl.max_pixel_work.min(allowance);
    bl.max_visits = bl.max_visits.min(allowance / 16);
    for kind in [
        sfnt::bitmap::Kind::OpenType,
        sfnt::bitmap::Kind::Apple,
        sfnt::bitmap::Kind::Color,
    ] {
        let (data_tag, location_tag) = kind.tags();
        match (table(location_tag), table(data_tag)) {
            (Some(location), Some(data)) => {
                drop(sfnt::bitmap::decode(location, data, kind, glyphs, &bl)?);
                checks.push(match kind {
                    sfnt::bitmap::Kind::OpenType => "EBLC/EBDT",
                    sfnt::bitmap::Kind::Apple => "bloc/bdat",
                    sfnt::bitmap::Kind::Color => "CBLC/CBDT",
                });
                has_glyphs = true;
            }
            (None, None) => {}
            _ => return Err(invalid("missing paired embedded bitmap tables")),
        }
    }
    if let Some(data) = table(*b"EBSC") {
        drop(sfnt::bitmap::decode_scaling(data, &bl)?);
        checks.push("EBSC");
    }
    if let Some(data) = table(*b"sbix") {
        #[cfg(feature = "color")]
        {
            drop(sfnt::bitmap::color::decode_sbix(data, glyphs, &bl, false)?);
            checks.push("sbix");
            has_glyphs = true;
        }
        #[cfg(not(feature = "color"))]
        {
            let _ = data;
            return Err(Error::FeatureDisabled {
                format: "color".into(),
            });
        }
    }
    if !has_glyphs {
        return Err(invalid("no supported glyph-data table"));
    }
    Ok(FaceValidation {
        font_index: index,
        glyph_count: usize::from(glyphs),
        checks,
    })
}
fn name_budget(data: &[u8], allowance: usize) -> Result<()> {
    let count = usize::from(u16_at(data, 2)?);
    let mut used = product(
        if count == 0 { 0 } else { count.max(4) },
        2 * size_of::<sfnt::name::NameRecord>(),
        allowance,
    )?;
    let mut max_string = 0usize;
    for i in 0..count {
        let length = usize::from(u16_at(data, 6 + 12 * i + 8)?);
        max_string = max_string.max(length);
        charge(&mut used, length, allowance)?;
    }
    charge(&mut used, product(max_string, 4, allowance)?, allowance)?;
    if u16_at(data, 0)? == 1 {
        let start = 6 + 12 * count;
        let count = usize::from(u16_at(data, start)?);
        charge(
            &mut used,
            product(
                if count == 0 { 0 } else { count.max(4) },
                2 * size_of::<String>(),
                allowance,
            )?,
            allowance,
        )?;
        for i in 0..count {
            charge(
                &mut used,
                usize::from(u16_at(data, start + 2 + 4 * i)?)
                    .checked_mul(4)
                    .ok_or_else(|| limit_error(allowance))?,
                allowance,
            )?;
        }
    }
    Ok(())
}
fn glyf_limits(
    glyf: &[u8],
    loca: &[u8],
    glyphs: u16,
    format: i16,
    l: &sfnt::Limits,
    allowance: usize,
) -> Result<sfnt::glyf::Limits> {
    let width = match format {
        0 => 2,
        1 => 4,
        _ => return Err(invalid("invalid indexToLocFormat")),
    };
    let offset = |index: usize| -> Result<usize> {
        if width == 2 {
            Ok(usize::from(u16_at(loca, index * 2)?) * 2)
        } else {
            Ok(u32_at(loca, index * 4)? as usize)
        }
    };
    bytes(loca, 0, (usize::from(glyphs) + 1) * width)?;
    let mut owned = product(
        usize::from(glyphs),
        size_of::<sfnt::glyf::Glyph>() + 2 * size_of::<usize>() + 1,
        allowance,
    )?;
    charge(&mut owned, size_of::<usize>(), allowance)?;
    let (
        mut total_points,
        mut total_components,
        mut total_instructions,
        mut max_points,
        mut max_components,
        mut max_scratch,
    ) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for index in 0..usize::from(glyphs) {
        let start = offset(index)?;
        let end = offset(index + 1)?;
        let data = bytes(
            glyf,
            start,
            end.checked_sub(start)
                .ok_or_else(|| invalid("descending loca offsets"))?,
        )?;
        if data.is_empty() {
            continue;
        }
        bytes(data, 0, 10)?;
        let contours = u16_at(data, 0)? as i16;
        if contours >= 0 {
            let contours = contours as usize;
            let points = if contours == 0 {
                0
            } else {
                usize::from(u16_at(data, 10 + 2 * (contours - 1))?) + 1
            };
            let instruction_offset = 10 + 2 * contours;
            let instructions = usize::from(u16_at(data, instruction_offset)?);
            bytes(data, instruction_offset + 2, instructions)?;
            charge(&mut total_points, points, usize::MAX)?;
            charge(&mut total_instructions, instructions, usize::MAX)?;
            max_points = max_points.max(points);
            charge(
                &mut owned,
                product(points, size_of::<fontpixel::outline::Point>(), allowance)?,
                allowance,
            )?;
            charge(
                &mut owned,
                product(
                    contours,
                    size_of::<fontpixel::outline::Contour>(),
                    allowance,
                )?,
                allowance,
            )?;
            charge(&mut owned, instructions, allowance)?;
            // Concurrent ends, flags, Fontations points and point flags. The
            // final contour/point arrays above remain live during this scratch.
            max_scratch = max_scratch.max(
                product(points, 16, allowance)?
                    .checked_add(product(contours, size_of::<usize>(), allowance)?)
                    .ok_or_else(|| limit_error(allowance))?,
            );
        } else {
            let mut cursor = 10usize;
            let mut components = 0usize;
            let flags = loop {
                let flags = u16_at(data, cursor)?;
                bytes(data, cursor, 4)?;
                cursor += 4;
                components += 1;
                let args = if flags & 1 != 0 { 4 } else { 2 };
                let transform = if flags & 8 != 0 {
                    2
                } else if flags & 64 != 0 {
                    4
                } else if flags & 128 != 0 {
                    8
                } else {
                    0
                };
                bytes(data, cursor, args + transform)?;
                cursor += args + transform;
                if flags & 32 == 0 {
                    break flags;
                }
            };
            let instructions = if flags & 256 != 0 {
                let count = usize::from(u16_at(data, cursor)?);
                bytes(data, cursor + 2, count)?;
                count
            } else {
                0
            };
            charge(&mut total_components, components, usize::MAX)?;
            charge(&mut total_instructions, instructions, usize::MAX)?;
            max_components = max_components.max(components);
            // Vec::push geometric growth: both the old and new allocations may
            // coexist while growing; 4 slots minimum and a factor 4 dominate it.
            charge(
                &mut owned,
                product(
                    components.max(4),
                    4 * size_of::<sfnt::glyf::Component>(),
                    allowance,
                )?,
                allowance,
            )?;
            charge(&mut owned, instructions, allowance)?;
        }
    }
    charge(&mut owned, max_scratch, allowance)?;
    let mut result = sfnt::glyf::Limits {
        sfnt: l.clone(),
        max_total_points: total_points,
        max_total_components: total_components,
        max_total_instructions: total_instructions,
        max_components,
        ..Default::default()
    };
    result.geometry.max_points = max_points;
    Ok(result)
}
#[cfg(feature = "opentype")]
fn validate_cff(
    data: &[u8],
    upm: u16,
    glyphs: u16,
    l: &sfnt::Limits,
    allowance: usize,
) -> Result<()> {
    let metadata = product(
        usize::from(glyphs),
        size_of::<sfnt::cff::DecodedGlyph>() + 8 * size_of::<sfnt::cff::Command>() + 32,
        allowance,
    )?;
    let commands = sub(allowance, metadata)? / 2;
    let mut cl = sfnt::cff::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    cl.max_total_commands = cl
        .max_total_commands
        .min(commands / (2 * size_of::<sfnt::cff::Command>()));
    cl.max_commands = cl.max_commands.min(cl.max_total_commands);
    cl.max_total_dict_bytes = cl.max_total_dict_bytes.min(sub(allowance, metadata)? / 128);
    cl.max_dict_bytes = cl.max_dict_bytes.min(cl.max_total_dict_bytes);
    cl.max_total_program_bytes = cl.max_total_program_bytes.min(allowance.saturating_mul(16));
    cl.max_program_bytes = cl.max_program_bytes.min(cl.max_total_program_bytes);
    let cff = sfnt::cff::Cff::parse(data, upm, &cl)?;
    if cff.len() != usize::from(glyphs) {
        return Err(invalid("CFF glyph count differs from maxp"));
    }
    drop(cff.read_all(&cl)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glyf_counts_use_actual_storage_and_reject_small_allowance() {
        // One empty glyph: offsets, glyph slot and graph arrays still cost bytes.
        let loca = [0u8; 4];
        let resources = ResourceLimits::default();
        let l = limits(&resources, 4096);
        assert!(glyf_limits(&[], &loca, 1, 0, &l, 4096).is_ok());
        assert!(glyf_limits(&[], &loca, 1, 0, &l, 1).is_err());
    }
    #[test]
    fn name_aliases_charge_repeated_owned_payload() {
        let mut data = vec![0u8; 30];
        data[2..4].copy_from_slice(&2u16.to_be_bytes());
        data[4..6].copy_from_slice(&30u16.to_be_bytes());
        data[14..16].copy_from_slice(&1000u16.to_be_bytes());
        data[26..28].copy_from_slice(&1000u16.to_be_bytes());
        assert!(name_budget(&data, 1024).is_err());
        assert!(name_budget(&data, 8192).is_ok());
    }
}
