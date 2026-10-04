use super::*;
struct AsciiJson;
impl serde_json::ser::Formatter for AsciiJson {
    fn write_string_fragment<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> std::io::Result<()> {
        // serde_json separates C0 controls, quotes and backslashes before this
        // callback. DEL remains in fragments and still needs our ASCII escape.
        if fragment.is_ascii() && !fragment.as_bytes().contains(&0x7f) {
            return writer.write_all(fragment.as_bytes());
        }
        let mut start = 0;
        for (index, c) in fragment.char_indices() {
            if c.is_ascii() && !c.is_control() {
                continue;
            }
            // Keep unchanged ASCII together so bounded output checks happen per
            // run, rather than once for every metadata byte.
            if start < index {
                writer.write_all(&fragment.as_bytes()[start..index])?;
            }
            let mut utf16 = [0_u16; 2];
            for value in c.encode_utf16(&mut utf16) {
                write!(writer, "\\u{value:04x}")?;
            }
            start = index + c.len_utf8();
        }
        if start < fragment.len() {
            writer.write_all(&fragment.as_bytes()[start..])?;
        }
        Ok(())
    }
}
struct Output {
    bytes: Vec<u8>,
    max: usize,
    record_max: usize,
}
impl std::io::Write for Output {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("YAFF output limit"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    #[inline(always)]
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Output {
    fn text(&mut self, s: &str) -> Result<()> {
        self.write_all(s.as_bytes())
            .map_err(|_| limit("YAFF output bytes", self.max))
    }
    fn json(&mut self, key: &str, value: &impl Serialize, indent: &str) -> Result<()> {
        let maximum = self.max;
        self.max = self
            .max
            .min(self.bytes.len().saturating_add(self.record_max));
        self.text(indent)?;
        self.text(key)?;
        self.text(": ")?;
        let result = value.serialize(&mut serde_json::Serializer::with_formatter(
            &mut *self, AsciiJson,
        ));
        self.max = maximum;
        result.map_err(|_| limit("YAFF metadata output bytes", self.record_max))?;
        self.text("\n")
    }
    fn property(&mut self, k: &str, v: &str, indent: &str) -> Result<()> {
        // Unsupported text still survives inside the native extension.
        if !valid_text(v) || v.contains('\r') {
            return Ok(());
        }
        let maximum = self.max;
        self.max = self
            .max
            .min(self.bytes.len().saturating_add(self.record_max));
        self.text(indent)?;
        self.text(k)?;
        self.text(":")?;
        if v.contains('\n') {
            self.text("\n")?;
            for line in v.split('\n') {
                self.text(indent)?;
                self.text("    ")?;
                self.quoted(line)?;
                self.text("\n")?;
            }
        } else {
            self.text(" ")?;
            if k == "default-char" {
                self.text(v)?;
            } else {
                self.quoted(v)?;
            }
            self.text("\n")?;
        }
        self.max = maximum;
        Ok(())
    }
    fn quoted(&mut self, v: &str) -> Result<()> {
        if v.is_empty()
            || v.starts_with([' ', '\t', ':', '.', '@', '"'])
            || v.ends_with([' ', '\t', ':', '"'])
            || v == "-"
        {
            self.text("\"")?;
            self.text(v)?;
            self.text("\"")
        } else {
            self.text(v)
        }
    }
}
pub(super) fn write(set: &FontSet, options: &YaffWriteOptions) -> Result<(Vec<u8>, Report)> {
    set.validate(&options.limits)?;
    props_depth(&set.properties)?;
    let mut out = Output {
        bytes: Vec::new(),
        max: options.max_output_bytes,
        record_max: options.limits.max_table_bytes,
    };
    let empty = Font::default();
    let fonts = if set.fonts.is_empty() {
        std::slice::from_ref(&empty)
    } else {
        set.fonts.as_slice()
    };
    let mut budget = Budget {
        used: 0,
        max: options.limits.max_total_bytes,
        record_max: options.limits.max_table_bytes,
    };
    for (index, font) in fonts.iter().enumerate() {
        if font.slot_count() > options.limits.max_glyphs {
            return Err(limit("YAFF glyph slots", options.limits.max_glyphs));
        }
        if font
            .metrics
            .point_size
            .is_some_and(|v| v.denominator() == 0)
        {
            return Err(invalid("zero rational denominator"));
        }
        props_depth(&font.headers)?;
        props_depth(&font.properties)?;
        props_depth(&font.names.localized)?;
        if fonts.len() > 1 {
            out.text("---\n")?;
        }
        out.text("yaff: 1.0\n")?;
        if index == 0 {
            out.json(
                "fontpixel.container",
                &ContainerMetaRef {
                    version: 1,
                    properties: &set.properties,
                    source_format: &set.source_format,
                    empty: set.fonts.is_empty(),
                },
                "",
            )?;
        }
        out.json(
            "fontpixel.font",
            &FontMetaRef {
                version: 1,
                metrics: &font.metrics,
                names: &font.names,
                charset: &font.charset,
                source_format: &font.source_format,
                headers: &font.headers,
                properties: &font.properties,
                slots: font.slot_count(),
            },
            "",
        )?;
        for (k, v) in font_fields(font) {
            out.property(&k, &v, "")?;
        }
        let gray = font
            .glyphs()
            .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_)));
        if gray {
            out.text("levels: 256\n")?;
        }
        out.text("\n")?;
        for (id, glyph) in font.glyphs() {
            props_depth(&glyph.properties)?;
            budget.charge(128)?;
            if glyph
                .name
                .as_ref()
                .is_some_and(|s| s.len() > options.limits.max_table_bytes)
                || glyph.codepoints.len() > options.limits.max_table_bytes / 16
                || glyph
                    .unicode_sequences
                    .iter()
                    .map(Vec::len)
                    .try_fold(0_usize, usize::checked_add)
                    .is_none_or(|n| n > options.limits.max_table_bytes / 16)
            {
                return Err(limit("YAFF label bytes", options.limits.max_table_bytes));
            }
            for label in labels(glyph) {
                if label.len().saturating_add(1) > options.limits.max_table_bytes {
                    return Err(limit("YAFF label bytes", options.limits.max_table_bytes));
                }
                out.text(&label)?;
                out.text(":\n")?;
            }
            raster(&mut out, &glyph.bitmap, gray, options)?;
            out.text("\n")?;
            for (k, v) in glyph_fields(glyph, &font.metrics.glyph_defaults)? {
                out.property(&k, &v, "    ")?;
            }
            out.json(
                "fontpixel.glyph",
                &GlyphMetaRef {
                    version: 1,
                    id: id.index(),
                    codepoints: &glyph.codepoints,
                    unicode_sequences: &glyph.unicode_sequences,
                    name: &glyph.name,
                    metrics: &glyph.metrics,
                    gray: matches!(glyph.bitmap, GlyphBitmap::Gray8(_)),
                    properties: &glyph.properties,
                },
                "    ",
            )?;
            out.text("\n")?;
        }
    }
    Ok((out.bytes, Report::default()))
}

fn raster(
    out: &mut Output,
    bitmap: &GlyphBitmap,
    gray: bool,
    options: &YaffWriteOptions,
) -> Result<()> {
    let width = bitmap.width() as usize;
    let height = bitmap.height() as usize;
    let empty = width == 0 || height == 0;
    let length = if empty {
        width.checked_add(6)
    } else {
        width
            .checked_mul(if gray { 2 } else { 1 })
            .and_then(|n| n.checked_add(5))
    }
    .ok_or_else(|| limit("YAFF raster row bytes", options.limits.max_table_bytes))?;
    if length > options.limits.max_table_bytes || length > options.max_output_bytes {
        return Err(limit(
            "YAFF raster row bytes",
            options.limits.max_table_bytes.min(options.max_output_bytes),
        ));
    }
    let mut row = Vec::new();
    row.try_reserve_exact(length)
        .map_err(|_| limit("YAFF raster row allocation", length))?;
    if empty {
        row.extend_from_slice(b"    ");
        row.resize(length - 1, b'-');
        row.push(b'\n');
        for _ in 0..=height {
            out.write_all(&row)
                .map_err(|_| limit("YAFF output bytes", out.max))?;
        }
        return Ok(());
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for y in 0..height {
        row.clear();
        row.extend_from_slice(b"    ");
        for x in 0..width {
            let value = match bitmap {
                GlyphBitmap::Mono(b) => {
                    ((b.as_bytes()[y * width.div_ceil(8) + x / 8] >> (7 - x % 8)) & 1) * 255
                }
                GlyphBitmap::Gray8(b) => b.as_bytes()[y * width + x],
            };
            if gray {
                match value {
                    0 => row.extend_from_slice(b".."),
                    255 => row.extend_from_slice(b"@@"),
                    _ => {
                        row.push(HEX[usize::from(value >> 4)]);
                        row.push(HEX[usize::from(value & 15)]);
                    }
                }
            } else {
                row.push(if value == 0 { b'.' } else { b'@' });
            }
        }
        row.push(b'\n');
        out.write_all(&row)
            .map_err(|_| limit("YAFF output bytes", out.max))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(max: usize, record_max: usize) -> Output {
        Output {
            bytes: Vec::new(),
            max,
            record_max,
        }
    }

    #[test]
    fn write_all_matches_whole_slice_write_at_output_boundaries() {
        for maximum in [0, 1, 3, 4, 8] {
            let mut actual = output(maximum, usize::MAX);
            let mut previous = output(maximum, usize::MAX);
            for fragment in [b"".as_slice(), b"a", b"bc", b"d", b"", b"efgh"] {
                let result = actual.write_all(fragment);
                let expected = previous.write(fragment).map(|length| {
                    assert_eq!(length, fragment.len());
                });
                assert_eq!(
                    result.err().map(|e| (e.kind(), e.to_string())),
                    expected.err().map(|e| (e.kind(), e.to_string())),
                    "cap {maximum}, fragment {fragment:?}"
                );
                assert_eq!(actual.bytes, previous.bytes);
                assert!(actual.bytes.len() <= maximum);
            }
        }
    }

    // Retain the previous default Write::write_all loop as an independent
    // reference while using the unchanged whole-slice write implementation.
    struct PreviousWriteAll(Output);
    impl std::io::Write for PreviousWriteAll {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }

    #[test]
    fn bounded_json_fragments_match_previous_write_all_loop() {
        for value in ["", "plain ASCII", "\u{7f}", "A\0\n\t\"\\é\u{7f}😀Z"] {
            let mut reference = Vec::new();
            value
                .serialize(&mut serde_json::Serializer::with_formatter(
                    &mut reference,
                    AsciiJson,
                ))
                .unwrap();
            for record_max in 0..=reference.len() + 1 {
                for maximum in [0, reference.len() - 1, reference.len(), usize::MAX] {
                    let mut actual = output(maximum, record_max);
                    let mut previous = PreviousWriteAll(output(maximum, record_max));
                    // The same record-local bound used while Output::json serializes.
                    actual.max = actual.max.min(actual.record_max);
                    previous.0.max = previous.0.max.min(previous.0.record_max);
                    let result = value.serialize(&mut serde_json::Serializer::with_formatter(
                        &mut actual,
                        AsciiJson,
                    ));
                    let expected = value.serialize(&mut serde_json::Serializer::with_formatter(
                        &mut previous,
                        AsciiJson,
                    ));
                    assert_eq!(
                        result.err().map(|e| e.to_string()),
                        expected.err().map(|e| e.to_string()),
                        "output cap {maximum}, record cap {record_max}, value {value:?}"
                    );
                    assert_eq!(actual.bytes, previous.0.bytes);
                    assert!(actual.bytes.len() <= maximum.min(record_max));
                }
            }
        }
    }
}
