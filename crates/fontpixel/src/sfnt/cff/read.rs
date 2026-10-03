use super::*;
use read_fonts::ps::{
    cff::{
        charset::Charset,
        dict::{self, Entry, Operator, Token},
    },
    string::Sid,
};
use std::collections::BTreeSet;

/// Borrowed, structurally validated single-font CFF1 table.
/// Glyph programs are checked when decoded; call `read_all` to validate all.
pub struct Cff<'a> {
    pub(super) raw: CffFontRef<'a>,
    subfonts: Vec<Subfont>,
    locals: Vec<Index<'a>>,
    widths: Vec<(f64, f64)>,
    selectors: Vec<u16>,
    identifiers: Vec<u16>,
}
/// Validate complete monotonic 1-based CFF1 INDEX offsets before Fontations lookup.
pub(super) fn index_at<'a>(data: &'a [u8], off: usize) -> Result<(Index<'a>, usize)> {
    let count = u16_at(data, off)? as usize;
    if count == 0 {
        return Ok((Index::Empty, off + 2));
    }
    let size = *bytes(data, off + 2, 1)?.first().unwrap() as usize;
    if !(1..=4).contains(&size) {
        return Err(invalid("invalid CFF INDEX offSize"));
    }
    let offsets = bytes(data, off + 3, (count + 1) * size)?;
    let mut prev = 0usize;
    for (i, b) in offsets.chunks(size).enumerate() {
        let n = b.iter().fold(0usize, |v, &b| (v << 8) | usize::from(b));
        if (i == 0 && n != 1) || n < prev || n == 0 {
            return Err(invalid("invalid CFF INDEX offsets"));
        }
        prev = n;
    }
    let start = off + 3 + (count + 1) * size;
    bytes(data, start, prev - 1)?;
    let end = start + prev - 1;
    let index = Index::new(&data[off..end], false).map_err(ps_error)?;
    Ok((index, end))
}
pub(super) fn dictionary(data: &[u8], l: &Limits, total: &mut usize) -> Result<Vec<Entry>> {
    bounded(data.len(), l.max_dict_bytes, "CFF DICT bytes")?;
    add_budget(
        total,
        data.len(),
        l.max_total_dict_bytes,
        "CFF total DICT bytes",
    )?;
    let mut n = 0usize;
    let mut operators = 0;
    let mut operands = Vec::new();
    let mut seen = BTreeSet::new();
    for token in dict::tokens(data) {
        match token.map_err(ps_error)? {
            Token::Operand(value, _) => {
                operands.push(value);
                n += 1;
                bounded(n, 48, "CFF DICT stack")?;
            }
            Token::Operator(op) => {
                use Operator::*;
                let valid = match op {
                    FontMatrix => n == 6,
                    FontBbox => n == 4,
                    PrivateDictRange => n == 2,
                    Ros => n == 3,
                    BlueValues | FamilyBlues => n <= 14 && n.is_multiple_of(2),
                    OtherBlues | FamilyOtherBlues => n <= 10 && n.is_multiple_of(2),
                    StemSnapH | StemSnapV => n <= 12,
                    Xuid => n > 0,
                    SyntheticBase | BaseFontBlend | Blend | VariationStoreOffset
                    | VariationStoreIndex => false,
                    _ => n == 1,
                };
                if !valid || !seen.insert(format!("{op:?}")) {
                    return Err(invalid(
                        "invalid, duplicate or unsupported CFF DICT operator",
                    ));
                }
                if matches!(op, DefaultWidthX | NominalWidthX) {
                    use read_fonts::ps::cff::stack::Number;
                    let value = match operands[0] {
                        Number::I32(n) => f64::from(n),
                        Number::Fixed(n) => n.to_f64(),
                    };
                    coordinate(value)?;
                }
                operands.clear();
                n = 0;
                operators += 1;
            }
            _ => return Err(invalid("unknown/reserved CFF DICT operator")),
        }
    }
    if n != 0 {
        return Err(invalid("unterminated CFF DICT operands"));
    }
    let entries = dict::entries(data, None)
        .map(|e| e.map_err(ps_error))
        .collect::<Result<Vec<_>>>()?;
    if entries.len() != operators {
        return Err(invalid("invalid CFF DICT values"));
    }
    for e in &entries {
        match e {
            Entry::CharstringType(n) if *n != 2 => {
                return Err(invalid("CFF Type 1 charstrings unsupported"));
            }
            Entry::PaintType(n) if *n != 0 => return Err(invalid("stroked CFF unsupported")),
            _ => {}
        }
    }
    Ok(entries)
}
fn private<'a>(
    data: &'a [u8],
    range: std::ops::Range<usize>,
    l: &Limits,
    total: &mut usize,
) -> Result<(Index<'a>, (f64, f64))> {
    let dict = data
        .get(range.clone())
        .ok_or_else(|| invalid("CFF Private DICT outside table"))?;
    let entries = dictionary(dict, l, total)?;
    let mut local = Index::Empty;
    let mut widths = (0., 0.);
    for e in entries {
        match &e {
            Entry::DefaultWidthX(w) => widths.0 = w.to_f64(),
            Entry::NominalWidthX(w) => widths.1 = w.to_f64(),
            _ => {}
        }
        if let Entry::SubrsOffset(off) = e {
            if off < range.len() {
                return Err(invalid("CFF local subrs overlap Private DICT"));
            }
            local = index_at(
                data,
                range
                    .start
                    .checked_add(off)
                    .ok_or_else(|| invalid("CFF subrs offset overflow"))?,
            )?
            .0;
        }
    }
    Ok((local, widths))
}
fn charset(data: &[u8], offset: usize, count: usize, is_cid: bool) -> Result<Vec<u16>> {
    if offset <= 2 {
        if is_cid {
            return Err(invalid("CID CFF requires a custom charset"));
        }
        let charset = Charset::new(FontData::new(data), offset, count as u32)
            .ok_or_else(|| invalid("invalid CFF charset"))?;
        return (0..count)
            .map(|i| {
                charset
                    .string_id(GlyphId::new(i as u32))
                    .map(|s| s.to_u16())
                    .ok_or_else(|| invalid("predefined CFF charset too short"))
            })
            .collect();
    }
    let format = bytes(data, offset, 1)?[0];
    let mut cursor = offset + 1;
    let mut ids = vec![0];
    match format {
        0 => {
            for _ in 1..count {
                ids.push(u16_at(data, cursor)?);
                cursor += 2;
            }
        }
        1 | 2 => {
            while ids.len() < count {
                let first = u16_at(data, cursor)? as usize;
                cursor += 2;
                let left = if format == 1 {
                    let x = bytes(data, cursor, 1)?[0] as usize;
                    cursor += 1;
                    x
                } else {
                    let x = u16_at(data, cursor)? as usize;
                    cursor += 2;
                    x
                };
                if first + left > 65535 || ids.len() + left + 1 > count {
                    return Err(invalid("CFF charset range overflow"));
                }
                ids.extend((first..=first + left).map(|v| v as u16));
            }
        }
        _ => return Err(invalid("unsupported CFF charset format")),
    }
    let mut seen = BTreeSet::new();
    for &id in &ids {
        if !seen.insert(id) {
            return Err(invalid("duplicate CFF charset identifier"));
        }
    }
    Ok(ids)
}
fn encoding(data: &[u8], offset: usize, ids: &[u16]) -> Result<()> {
    if offset <= 1 {
        return Ok(());
    }
    let flag = bytes(data, offset, 1)?[0];
    let format = flag & 127;
    let mut cur = offset + 1;
    let count = bytes(data, cur, 1)?[0] as usize;
    cur += 1;
    let mut codes = BTreeSet::new();
    match format {
        0 => {
            for &code in bytes(data, cur, count)? {
                if !codes.insert(code) {
                    return Err(invalid("duplicate CFF encoding code"));
                }
            }
            cur += count;
        }
        1 => {
            for _ in 0..count {
                let r = bytes(data, cur, 2)?;
                cur += 2;
                let last = u16::from(r[0]) + u16::from(r[1]);
                if last > 255 {
                    return Err(invalid("CFF encoding range overflow"));
                }
                for code in u16::from(r[0])..=last {
                    if !codes.insert(code as u8) {
                        return Err(invalid("duplicate CFF encoding code"));
                    }
                }
            }
        }
        _ => return Err(invalid("invalid CFF encoding format")),
    }
    if codes.len() >= ids.len() {
        return Err(invalid("CFF encoding exceeds glyph count"));
    }
    if flag & 128 != 0 {
        let count = bytes(data, cur, 1)?[0] as usize;
        cur += 1;
        let valid = ids.iter().copied().collect::<BTreeSet<_>>();
        for _ in 0..count {
            let record = bytes(data, cur, 3)?;
            cur += 3;
            let sid = u16::from_be_bytes([record[1], record[2]]);
            if !valid.contains(&sid) {
                return Err(invalid("CFF supplement SID missing from charset"));
            }
        }
    }
    Ok(())
}
fn selectors(data: &[u8], off: usize, count: usize, fd_count: usize) -> Result<Vec<u16>> {
    let format = bytes(data, off, 1)?[0];
    let mut result = Vec::with_capacity(count);
    match format {
        0 => result.extend(bytes(data, off + 1, count)?.iter().map(|n| u16::from(*n))),
        3 => {
            let ranges = u16_at(data, off + 1)? as usize;
            if ranges == 0 || ranges > count {
                return Err(invalid("invalid CFF FDSelect range count"));
            }
            let records = bytes(data, off + 3, ranges * 3)?;
            let sentinel = u16_at(data, off + 3 + ranges * 3)? as usize;
            if sentinel != count {
                return Err(invalid("CFF FDSelect sentinel mismatch"));
            }
            for i in 0..ranges {
                let start = u16::from_be_bytes([records[i * 3], records[i * 3 + 1]]) as usize;
                let end = if i + 1 == ranges {
                    sentinel
                } else {
                    u16::from_be_bytes([records[i * 3 + 3], records[i * 3 + 4]]) as usize
                };
                if start != result.len() || end <= start || end > count {
                    return Err(invalid("invalid CFF FDSelect range"));
                }
                result.resize(end, u16::from(records[i * 3 + 2]));
            }
        }
        _ => return Err(invalid("unsupported CFF1 FDSelect format")),
    }
    if result.iter().any(|n| usize::from(*n) >= fd_count) {
        return Err(invalid("CFF FDSelect references missing FD"));
    }
    Ok(result)
}
impl<'a> Cff<'a> {
    /// Parse a CFF1 table, with the containing font's units per em.
    pub fn parse(data: &'a [u8], units_per_em: u16, l: &Limits) -> Result<Self> {
        l.sfnt.table(data.len())?;
        bounded(
            data.len(),
            l.sfnt.resources.max_input_bytes,
            "CFF input bytes",
        )?;
        if !(16..=16384).contains(&units_per_em) {
            return Err(invalid("invalid CFF units per em"));
        }
        let h = bytes(data, 0, 4)?;
        if h[0] != 1 || h[2] < 4 || !(1..=4).contains(&h[3]) {
            return Err(invalid("invalid or unsupported CFF1 header"));
        }
        let (names, end) = index_at(data, h[2] as usize)?;
        let (top, end) = index_at(data, end)?;
        let (strings, end) = index_at(data, end)?;
        let (global, _) = index_at(data, end)?;
        if names.count() != 1 || top.count() != 1 {
            return Err(invalid("OpenType CFF requires exactly one font"));
        }
        let mut dict_bytes = 0;
        let entries = dictionary(
            top.get(0).ok_or_else(|| invalid("missing CFF Top DICT"))?,
            l,
            &mut dict_bytes,
        )?;
        let (mut cs_off, mut charset_off, mut encoding_off, mut fdarray, mut fdselect, mut is_cid) =
            (None, 0, 0, None, None, false);
        let mut private_range = None;
        for e in &entries {
            match e {
                Entry::CharstringsOffset(n) => cs_off = Some(*n),
                Entry::Charset(n) => charset_off = *n,
                Entry::Encoding(n) => encoding_off = *n,
                Entry::FdArrayOffset(n) => fdarray = Some(*n),
                Entry::FdSelectOffset(n) => fdselect = Some(*n),
                Entry::Ros { .. } => is_cid = true,
                Entry::PrivateDictRange(r) => private_range = Some(r.clone()),
                _ => {}
            }
        }
        let cs = index_at(
            data,
            cs_off.ok_or_else(|| invalid("missing CFF CharStrings"))?,
        )?
        .0;
        let count = cs.count() as usize;
        l.sfnt.glyphs(count)?;
        let identifiers = charset(data, charset_off, count, is_cid)?;
        if !is_cid {
            for id in &identifiers {
                if *id > 64999 || (*id >= 391 && u32::from(*id) - 391 >= strings.count()) {
                    return Err(invalid("CFF charset SID out of range"));
                }
            }
            encoding(data, encoding_off, &identifiers)?;
        }
        // Check all SID-bearing dictionary entries, including CID registry/ordering.
        let check_sid = |s: Sid| -> Result<()> {
            let id = s.to_u16();
            if id > 64999 || (id >= 391 && u32::from(id) - 391 >= strings.count()) {
                Err(invalid("CFF DICT SID out of range"))
            } else {
                Ok(())
            }
        };
        for e in &entries {
            match e {
                Entry::Version(s)
                | Entry::Notice(s)
                | Entry::FullName(s)
                | Entry::FamilyName(s)
                | Entry::Weight(s)
                | Entry::Copyright(s)
                | Entry::PostScript(s)
                | Entry::BaseFontName(s)
                | Entry::FontName(s) => check_sid(*s)?,
                Entry::Ros {
                    registry, ordering, ..
                } => {
                    check_sid(*registry)?;
                    check_sid(*ordering)?;
                }
                _ => {}
            }
        }
        let mut locals = Vec::new();
        let mut widths = Vec::new();
        let selection = if is_cid {
            let fds = index_at(data, fdarray.ok_or_else(|| invalid("missing CFF FDArray"))?)?.0;
            if fds.count() == 0 || fds.count() > 256 {
                return Err(invalid("CFF FDArray count outside 1..=256"));
            }
            for i in 0..fds.count() {
                let dict = dictionary(
                    fds.get(i as usize)
                        .ok_or_else(|| invalid("invalid CFF Font DICT"))?,
                    l,
                    &mut dict_bytes,
                )?;
                let range = dict
                    .into_iter()
                    .find_map(|e| {
                        if let Entry::PrivateDictRange(r) = e {
                            Some(r)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| invalid("missing FD Private DICT"))?;
                let (local, width) = private(data, range, l, &mut dict_bytes)?;
                locals.push(local);
                widths.push(width);
            }
            selectors(
                data,
                fdselect.ok_or_else(|| invalid("missing CFF FDSelect"))?,
                count,
                fds.count() as usize,
            )?
        } else {
            if fdarray.is_some() || fdselect.is_some() {
                return Err(invalid("CFF FD structures without ROS"));
            }
            let (local, width) = private(
                data,
                private_range.ok_or_else(|| invalid("missing CFF Private DICT"))?,
                l,
                &mut dict_bytes,
            )?;
            locals.push(local);
            widths.push(width);
            vec![0; count]
        };
        // Fontations is used only after independent structural preflight.
        let raw = CffFontRef::new_cff(data, 0, Some(i32::from(units_per_em))).map_err(ps_error)?;
        if raw.num_glyphs() != count as u32 || raw.global_subrs().count() != global.count() {
            return Err(invalid("CFF reader count mismatch"));
        }
        let mut subfonts = Vec::with_capacity(locals.len());
        for i in 0..locals.len() {
            subfonts.push(raw.subfont(i as u16, &[]).map_err(ps_error)?);
        }
        Ok(Self {
            raw,
            subfonts,
            locals,
            widths,
            selectors: selection,
            identifiers,
        })
    }
    /// Parse from an sfnt font, checking CFF/maxp count and head units per em.
    pub fn from_sfnt(font: &super::super::Sfnt<'a>, l: &Limits) -> Result<Self> {
        if font.flavor() != super::super::Flavor::Cff {
            return Err(invalid("CFF requires OTTO sfnt flavor"));
        }
        let head = font
            .table(*b"head")
            .ok_or_else(|| invalid("missing head"))?;
        let maxp = font
            .table(*b"maxp")
            .ok_or_else(|| invalid("missing maxp"))?;
        if bytes(maxp, 0, 4)? != [0, 0, 0x50, 0] {
            return Err(invalid("CFF requires maxp 0.5"));
        }
        let cff = Self::parse(
            font.table(*b"CFF ")
                .ok_or_else(|| invalid("missing CFF table"))?,
            u16_at(head, 18)?,
            l,
        )?;
        if u32::from(u16_at(maxp, 4)?) != cff.raw.num_glyphs() {
            return Err(invalid("CFF/maxp glyph count mismatch"));
        }
        Ok(cff)
    }
    pub(super) fn subfont_selector(&self, gid: u16) -> Result<u8> {
        self.selectors
            .get(usize::from(gid))
            .copied()
            .and_then(|v| u8::try_from(v).ok())
            .ok_or_else(|| invalid("CFF FDSelect outside uint8"))
    }
    pub(super) fn validate_selected(&self, glyphs: &[u16], l: &Limits) -> Result<()> {
        let (mut programs, mut commands) = (0, 0);
        for &gid in glyphs {
            let fd = usize::from(self.subfont_selector(gid)?);
            let program = self
                .raw
                .charstrings()
                .get(usize::from(gid))
                .ok_or_else(|| invalid("missing CFF charstring"))?;
            let (p, c, _) =
                validate::program(program, &self.locals[fd], self.raw.global_subrs(), l)?;
            add_budget(
                &mut programs,
                p,
                l.max_total_program_bytes,
                "CFF total program bytes",
            )?;
            add_budget(&mut commands, c, l.max_total_commands, "CFF total commands")?;
        }
        Ok(())
    }
    /// Original borrowed bytes, including unsupported metadata not projected here.
    pub fn data(&self) -> &'a [u8] {
        self.raw.data()
    }
    /// Number of glyphs including `.notdef`.
    pub fn len(&self) -> usize {
        self.selectors.len()
    }
    /// Always false for a successfully parsed OpenType CFF font.
    pub fn is_empty(&self) -> bool {
        self.selectors.is_empty()
    }
    /// Whether the charset identifiers are CIDs rather than SIDs.
    pub fn is_cid(&self) -> bool {
        self.raw.is_cid()
    }
    /// PostScript glyph name, or None for CID fonts or an invalid glyph ID.
    pub fn glyph_name(&self, gid: u16) -> Option<&'a [u8]> {
        if self.is_cid() {
            None
        } else {
            self.raw
                .string(Sid::new(*self.identifiers.get(gid as usize)?))
        }
    }
    /// Charset SID or CID for the glyph.
    pub fn identifier(&self, gid: u16) -> Option<u16> {
        self.identifiers.get(gid as usize).copied()
    }
    /// Number of shared global subroutines.
    pub fn global_subr_count(&self) -> usize {
        self.raw.global_subrs().count() as usize
    }
    /// Borrowed Fontations reader, for explicit raw metadata inspection.
    pub fn fontations(&self) -> &CffFontRef<'a> {
        &self.raw
    }
    /// Evaluate one glyph, checking program and geometry budgets first.
    pub fn glyph(&self, gid: u16, l: &Limits) -> Result<DecodedGlyph> {
        self.decode(gid, l, &mut 0, &mut 0)
    }
    /// Read every glyph with cumulative program and geometry budgets.
    pub fn read_all(&self, l: &Limits) -> Result<Vec<DecodedGlyph>> {
        let (mut programs, mut commands) = (0, 0);
        let mut out = Vec::with_capacity(self.len());
        for i in 0..self.len() {
            out.push(self.decode(i as u16, l, &mut programs, &mut commands)?);
        }
        Ok(out)
    }
    pub(crate) fn decode(
        &self,
        gid: u16,
        l: &Limits,
        total_program: &mut usize,
        total_commands: &mut usize,
    ) -> Result<DecodedGlyph> {
        let fd = *self
            .selectors
            .get(gid as usize)
            .ok_or_else(|| invalid("CFF glyph ID out of range"))? as usize;
        let sub = &self.subfonts[fd];
        let program = self
            .raw
            .charstrings()
            .get(gid as usize)
            .ok_or_else(|| invalid("missing CFF charstring"))?;
        let checked = validate::program(program, &self.locals[fd], self.raw.global_subrs(), l)?;
        add_budget(
            total_program,
            checked.0,
            l.max_total_program_bytes,
            "CFF total program bytes",
        )?;
        add_budget(
            total_commands,
            checked.1,
            l.max_total_commands,
            "CFF total commands",
        )?;
        let design_width = checked
            .2
            .map(|w| w + self.widths[fd].1)
            .unwrap_or(self.widths[fd].0);
        coordinate(design_width)?;
        let expected_width = checked
            .2
            .map(|w| w + sub.nominal_width().to_f64())
            .unwrap_or_else(|| sub.default_width().unwrap_or(Fixed::ZERO).to_f64());
        coordinate(expected_width)?;
        let mut sink = Sink {
            commands: Vec::with_capacity(checked.1),
            max: checked.1,
            overflow: false,
        };
        let width = self
            .raw
            .evaluate_charstring(sub, GlyphId::from(gid), &[], &mut sink)
            .map_err(ps_error)?
            .unwrap_or(Fixed::ZERO);
        if width.to_f64() != expected_width {
            return Err(invalid("CFF width evaluation mismatch"));
        }
        if sink.overflow {
            return Err(invalid("CFF evaluator exceeded validated command count"));
        }
        // Preserve fractional design units. Fontations' draw() uses FreeType-like
        // integer rounding; this geometry layer deliberately does not grid fit.
        let t = self.raw.transform(sub, None);
        let a = t.matrix.elements().map(|x| x.to_f64());
        let scale = t.scale.map_or(1., |s| s.to_f64() / 64.);
        let transform = |p: Point| -> Result<Point> {
            let q = (
                (a[0] * p.0 + a[2] * p.1 + a[4]) * scale,
                (a[1] * p.0 + a[3] * p.1 + a[5]) * scale,
            );
            coordinate(q.0)?;
            coordinate(q.1)?;
            Ok(q)
        };
        for c in &mut sink.commands {
            *c = match *c {
                Command::MoveTo(p) => Command::MoveTo(transform(p)?),
                Command::LineTo(p) => Command::LineTo(transform(p)?),
                Command::CurveTo(a, b, p) => {
                    Command::CurveTo(transform(a)?, transform(b)?, transform(p)?)
                }
                Command::Close => Command::Close,
            };
        }
        let advance = (design_width * a[0] + a[4]) * scale;
        coordinate(advance)?;
        Ok(DecodedGlyph {
            advance,
            commands: sink.commands,
        })
    }
}
struct Sink {
    commands: Vec<Command>,
    max: usize,
    overflow: bool,
}
impl Sink {
    fn push(&mut self, c: Command) {
        if self.commands.len() < self.max {
            self.commands.push(c);
        } else {
            self.overflow = true;
        }
    }
}
impl CommandSink for Sink {
    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.push(Command::MoveTo((x.to_f64(), y.to_f64())));
    }
    fn line_to(&mut self, x: Fixed, y: Fixed) {
        self.push(Command::LineTo((x.to_f64(), y.to_f64())));
    }
    fn curve_to(&mut self, a: Fixed, b: Fixed, c: Fixed, d: Fixed, x: Fixed, y: Fixed) {
        self.push(Command::CurveTo(
            (a.to_f64(), b.to_f64()),
            (c.to_f64(), d.to_f64()),
            (x.to_f64(), y.to_f64()),
        ));
    }
    fn close(&mut self) {
        self.push(Command::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(program: Vec<u8>, private: Vec<u8>) -> Vec<u8> {
        let l = Limits::default();
        let name = super::super::write::index(&[b"Test".to_vec()], &l).unwrap();
        let cs = super::super::write::index(&[program], &l).unwrap();
        let dict = |offset: usize| {
            let mut d = vec![29];
            d.extend((offset as i32).to_be_bytes());
            d.push(17);
            d.push((private.len() + 139) as u8);
            d.push(29);
            d.extend(((offset + cs.len()) as i32).to_be_bytes());
            d.push(18);
            super::super::write::index(&[d], &l).unwrap()
        };
        let base = 4 + name.len() + dict(0).len() + 4;
        let mut out = vec![1, 0, 4, 4];
        for v in [name, dict(base), vec![0, 0], vec![0, 0], cs, private] {
            out.extend(v);
        }
        out
    }
    #[test]
    fn private_fractional_widths_are_not_grid_rounded() {
        // defaultWidthX 300.5, nominalWidthX 100.25; no external fixture required.
        let private = vec![30, 0x30, 0x0a, 0x5f, 20, 30, 0x10, 0x0a, 0x25, 0xff, 21];
        let l = Limits::default();
        for (program, width) in [(vec![14], 300.5), (vec![239, 14], 200.25)] {
            let data = fixture(program, private.clone());
            let font = Cff::parse(&data, 1000, &l).unwrap();
            assert_eq!(font.glyph(0, &l).unwrap().advance, width);
        }
    }
    #[test]
    fn private_width_overflow_and_bad_dict_are_errors() {
        let l = Limits::default();
        for private in [
            vec![29, 0, 1, 0, 0, 20],
            vec![139],
            vec![12],
            vec![139, 139, 20],
            vec![139, 20, 139, 20],
            vec![255, 20],
        ] {
            assert!(Cff::parse(&fixture(vec![14], private), 1000, &l).is_err());
        }
        let data = fixture(vec![140, 14], vec![28, 0x7f, 0xff, 21]);
        assert!(Cff::parse(&data, 1000, &l).unwrap().glyph(0, &l).is_err());
    }
    #[test]
    fn index_offsets_and_header_are_checked() {
        let l = Limits::default();
        let valid = fixture(vec![14], vec![]);
        for (offset, value) in [(0, 2), (2, 3), (3, 0), (6, 0), (7, 0), (8, 0)] {
            let mut d = valid.clone();
            d[offset] = value;
            assert!(Cff::parse(&d, 1000, &l).is_err(), "{offset}");
        }
        let data = [0, 2, 1, 1, 3, 2, 0, 0];
        assert!(index_at(&data, 0).is_err());
    }
}
