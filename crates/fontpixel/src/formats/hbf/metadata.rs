use super::*;
use crate::model::{PropertyValue, Spacing};

pub(super) fn text<'a>(font: &'a Font, key: &str) -> Result<Option<&'a str>> {
    match font.properties.get(key) {
        None => Ok(None),
        Some(PropertyValue::Text(s)) => Ok(Some(s)),
        _ => Err(invalid(format!("{key} must be a quoted string"))),
    }
}
fn number(font: &Font, key: &str) -> Result<Option<i64>> {
    match font.properties.get(key) {
        None => Ok(None),
        Some(PropertyValue::Integer(n)) => Ok(Some(*n)),
        Some(PropertyValue::Unsigned(n)) => Ok(Some(
            i64::try_from(*n).map_err(|_| invalid(format!("{key} overflows")))?,
        )),
        _ => Err(invalid(format!("{key} must be an integer"))),
    }
}
pub(super) fn scheme(s: &str) -> (&str, bool) {
    if s.eq_ignore_ascii_case("Unicode 1.1") || s.eq_ignore_ascii_case("Unicode") {
        ("unicode", true)
    } else if s.eq_ignore_ascii_case("GB2312-1980") {
        ("gb2312", false)
    } else if s.eq_ignore_ascii_case("Big5") {
        ("big5", false)
    } else if s.eq_ignore_ascii_case("JISX0208-1990") {
        ("jisx0208", false)
    } else if s.eq_ignore_ascii_case("KSC5601-1987") {
        ("ksc5601", false)
    } else {
        (s, false)
    }
}
pub(super) fn check_code(font: &Font, cp: u32) -> Result<()> {
    if matches!(
        font.charset.name.as_deref(),
        Some("gb2312" | "jisx0208" | "ksc5601")
    ) {
        let (low, high) = if font.charset.gl == Some(true) {
            (0x21, 0x7e)
        } else {
            (0xa1, 0xfe)
        };
        if !(low..=high).contains(&(cp >> 8)) || !(low..=high).contains(&(cp & 255)) {
            return Err(invalid("code is outside the declared GL/GR 94x94 charset"));
        }
    }
    Ok(())
}
pub(super) fn project(
    font: &mut Font,
    code_scheme: &str,
    ranges: &[header::Range],
    bounds: Bounds,
    report: &mut Report,
) -> Result<bool> {
    let (name, unicode) = scheme(code_scheme);
    font.charset.name = Some(name.into());
    font.charset.registry = text(font, "CHARSET_REGISTRY")?.map(str::to_owned);
    font.charset.encoding = text(font, "CHARSET_ENCODING")?.map(str::to_owned);
    font.charset.code_ranges = ranges.iter().map(|r| (r.first, r.last)).collect();
    if matches!(name, "gb2312" | "jisx0208" | "ksc5601") {
        let first = ranges
            .first()
            .ok_or_else(|| invalid("empty code ranges"))?
            .first;
        font.charset.gl = Some(first < 0x8000);
        for range in ranges {
            check_code(font, range.first)?;
            check_code(font, range.last)?;
        }
    } else if !matches!(name, "unicode" | "big5") {
        report.push(ReportEntry {
            level: Level::Info,
            impact: Impact::None,
            code: "hbf.explicit_charset".into(),
            location: Location {
                font_index: Some(0),
                ..Default::default()
            },
            message: format!("Code scheme {code_scheme:?} is retained without Unicode remapping"),
        });
    }
    macro_rules! name {
        ($field:ident,$key:literal) => {
            font.names.$field = text(font, $key)?.map(str::to_owned);
        };
    }
    name!(family, "FAMILY_NAME");
    name!(style, "ADD_STYLE_NAME");
    name!(full, "FULL_NAME");
    name!(postscript, "POSTSCRIPT_NAME");
    name!(version, "FONT_VERSION");
    name!(copyright, "COPYRIGHT");
    name!(foundry, "FOUNDRY");
    name!(license, "LICENSE");
    name!(license_url, "LICENSE_URL");
    font.metrics.pixel_size = number(font, "PIXEL_SIZE")?
        .map(|v| u32::try_from(v).map_err(|_| invalid("PIXEL_SIZE overflow")))
        .transpose()?;
    font.metrics.ascent = Some(
        number(font, "FONT_ASCENT")?.unwrap_or(
            bounds
                .y
                .checked_add(i64::from(bounds.height))
                .ok_or_else(|| invalid("ascent overflow"))?,
        ),
    );
    font.metrics.descent = Some(
        number(font, "FONT_DESCENT")?.unwrap_or(
            bounds
                .y
                .checked_neg()
                .ok_or_else(|| invalid("descent overflow"))?,
        ),
    );
    font.metrics.underline_position = number(font, "UNDERLINE_POSITION")?;
    font.metrics.underline_thickness = number(font, "UNDERLINE_THICKNESS")?;
    font.metrics.spacing = match text(font, "SPACING")? {
        None => None,
        Some(s) if s.eq_ignore_ascii_case("p") => Some(Spacing::Proportional),
        Some(s) if s.eq_ignore_ascii_case("m") => Some(Spacing::Monospaced),
        Some(s) if s.eq_ignore_ascii_case("c") => Some(Spacing::CharacterCell),
        _ => return Err(invalid("invalid SPACING")),
    };
    let default =
        number(font, "DEFAULT_CHAR")?.ok_or_else(|| invalid("DEFAULT_CHAR is required"))?;
    let default = u32::try_from(default).map_err(|_| invalid("DEFAULT_CHAR overflow"))?;
    if default > 0xffffff
        || (unicode && char::from_u32(default).is_none())
        || (code_scheme.eq_ignore_ascii_case("Unicode 1.1") && default > 0xffff)
    {
        return Err(invalid("invalid DEFAULT_CHAR"));
    }
    font.metrics.default_character = Some(if unicode {
        Codepoint::Unicode(default)
    } else {
        Codepoint::Legacy(default)
    });
    Ok(unicode)
}
