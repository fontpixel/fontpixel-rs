use super::subset::{Count, Out, index as put_index};
use super::*;
use crate::working::Vector;
use std::collections::BTreeSet;

fn name(s: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 127
        || s.bytes()
            .any(|b| !(33..=126).contains(&b) || b"[](){}<>/%".contains(&b))
    {
        return Err(invalid("invalid CFF PostScript name"));
    }
    Ok(())
}
fn int(out: &mut Vec<u8>, n: i32) {
    match n {
        -107..=107 => out.push((n + 139) as u8),
        108..=1131 => {
            out.push(((n - 108) / 256 + 247) as u8);
            out.push(((n - 108) % 256) as u8);
        }
        -1131..=-108 => {
            out.push(((-n - 108) / 256 + 251) as u8);
            out.push(((-n - 108) % 256) as u8);
        }
        -32768..=32767 => {
            out.push(28);
            out.extend((n as i16).to_be_bytes());
        }
        _ => {
            out.push(29);
            out.extend(n.to_be_bytes());
        }
    }
}
fn fixed(x: f64) -> Result<f64> {
    coordinate(x)?;
    let q = (x * 65536.).round();
    if q < i32::MIN as f64 || q > i32::MAX as f64 {
        return Err(invalid("CFF fixed-point rounding overflow"));
    }
    Ok(q / 65536.)
}
fn number(out: &mut Vec<u8>, n: f64) -> Result<f64> {
    let q = fixed(n)?;
    if q.fract() == 0. {
        int(out, q as i32);
    } else {
        out.push(255);
        out.extend(((q * 65536.) as i32).to_be_bytes());
    }
    Ok(q)
}
fn offset(out: &mut Vec<u8>, n: usize) -> Result<()> {
    let n = i32::try_from(n).map_err(|_| invalid("CFF offset overflow"))?;
    out.push(29);
    out.extend(n.to_be_bytes());
    Ok(())
}
fn real(out: &mut Vec<u8>, x: f64) {
    out.push(30);
    let s = format!("{x:.18}");
    let mut n = s
        .trim_end_matches('0')
        .trim_end_matches('.')
        .bytes()
        .map(|c| match c {
            b'.' => 10,
            b'-' => 14,
            _ => c - b'0',
        })
        .collect::<Vec<_>>();
    n.push(15);
    if n.len() % 2 != 0 {
        n.push(15);
    }
    out.extend(n.chunks(2).map(|p| p[0] * 16 + p[1]));
}
#[cfg(test)]
pub(super) fn index(items: &[Vec<u8>], limits: &Limits) -> Result<Vec<u8>> {
    to16(items.len())?;
    if items.is_empty() {
        return Ok(vec![0, 0]);
    }
    let total = items.iter().try_fold(1usize, |n, item| {
        n.checked_add(item.len())
            .ok_or_else(|| invalid("CFF INDEX overflow"))
    })?;
    let offsize = if total <= 0xff {
        1
    } else if total <= 0xffff {
        2
    } else if total <= 0xffffff {
        3
    } else {
        4
    };
    let len = 3usize
        .checked_add((items.len() + 1) * offsize)
        .and_then(|n| n.checked_add(total - 1))
        .ok_or_else(|| invalid("CFF INDEX size overflow"))?;
    limits.sfnt.table(len)?;
    let mut out = Vec::with_capacity(len);
    super::super::p16(&mut out, items.len() as u16);
    out.push(offsize as u8);
    let mut pos = 1usize;
    for item_len in items.iter().map(Vec::len).chain(std::iter::once(0)) {
        let p = u32::try_from(pos).map_err(|_| invalid("CFF INDEX offset overflow"))?;
        out.extend(&p.to_be_bytes()[4 - offsize..]);
        pos += item_len;
    }
    for item in items {
        out.extend(item);
    }
    Ok(out)
}
#[derive(Default)]
struct Bounds(Option<[f64; 4]>);
impl Bounds {
    fn point(&mut self, p: Point) {
        if let Some(b) = &mut self.0 {
            b[0] = b[0].min(p.0);
            b[1] = b[1].min(p.1);
            b[2] = b[2].max(p.0);
            b[3] = b[3].max(p.1);
        } else {
            self.0 = Some([p.0, p.1, p.0, p.1]);
        }
    }
}
// Compute actual cubic extrema without flattening or including off-curve controls.
fn curve_bounds(bounds: &mut Bounds, p: Point, c: Point, d: Point, q: Point) {
    bounds.point(p);
    bounds.point(q);
    for (a, b, cc, dd) in [(p.0, c.0, d.0, q.0), (p.1, c.1, d.1, q.1)] {
        let aa = -a + 3. * b - 3. * cc + dd;
        let bb = 2. * (a - 2. * b + cc);
        let c0 = b - a;
        let mut ts = [f64::NAN; 2];
        if aa.abs() < 1e-14 {
            if bb != 0. {
                ts[0] = -c0 / bb;
            }
        } else {
            let disc = bb * bb - 4. * aa * c0;
            if disc >= 0. {
                ts = [
                    (-bb + disc.sqrt()) / (2. * aa),
                    (-bb - disc.sqrt()) / (2. * aa),
                ];
            }
        }
        for t in ts {
            if t > 0. && t < 1. {
                let u = 1. - t;
                bounds.point((
                    u * u * u * p.0 + 3. * u * u * t * c.0 + 3. * u * t * t * d.0 + t * t * t * q.0,
                    u * u * u * p.1 + 3. * u * u * t * c.1 + 3. * u * t * t * d.1 + t * t * t * q.1,
                ));
            }
        }
    }
}
struct PathWriter<'a> {
    /// Shared program buffer; this glyph's program starts at `start`.
    out: &'a mut Vec<u8>,
    start: usize,
    current: Point,
    open: bool,
    bounds: &'a mut Bounds,
    commands: usize,
    limits: &'a Limits,
    // First axis operator and operand count in the current alternating run.
    // Type 2 permits up to 48 stack entries; cap at 40 after width consumption.
    line_run: Option<(u8, usize)>,
}
impl PathWriter<'_> {
    fn delta(&mut self, target: Point) -> Result<Point> {
        coordinate(target.0)?;
        coordinate(target.1)?;
        let dx = number(self.out, target.0 - self.current.0)?;
        let dy = number(self.out, target.1 - self.current.1)?;
        self.current = (self.current.0 + dx, self.current.1 + dy);
        Ok(self.current)
    }
    fn command(&mut self, c: Command) -> Result<()> {
        add_budget(
            &mut self.commands,
            1,
            self.limits.max_commands,
            "CFF commands",
        )?;
        match c {
            Command::MoveTo(p) => {
                self.line_run = None;
                if self.open {
                    return Err(invalid("CFF path requires explicit Close before MoveTo"));
                }
                let dx = fixed(p.0 - self.current.0)?;
                let dy = fixed(p.1 - self.current.1)?;
                coordinate(p.0)?;
                coordinate(p.1)?;
                if dx == 0. {
                    number(self.out, dy)?;
                    self.out.push(4);
                } else if dy == 0. {
                    number(self.out, dx)?;
                    self.out.push(22);
                } else {
                    number(self.out, dx)?;
                    number(self.out, dy)?;
                    self.out.push(21);
                }
                self.current = (self.current.0 + dx, self.current.1 + dy);
                self.open = true;
            }
            Command::LineTo(p) => {
                if !self.open {
                    return Err(invalid("CFF line outside path"));
                }
                self.bounds.point(self.current);
                coordinate(p.0)?;
                coordinate(p.1)?;
                let dx = fixed(p.0 - self.current.0)?;
                let dy = fixed(p.1 - self.current.1)?;
                if dx == 0. || dy == 0. {
                    let (operator, value) = if dx == 0. { (7, dy) } else { (6, dx) };
                    let run = self.line_run.filter(|&(first, count)| {
                        count < 40 && operator == if count % 2 == 0 { first } else { 13 - first }
                    });
                    if let Some((first, count)) = run {
                        self.out.pop(); // Replace the previous run terminator.
                        number(self.out, value)?;
                        self.out.push(first);
                        self.line_run = Some((first, count + 1));
                    } else {
                        number(self.out, value)?;
                        self.out.push(operator);
                        self.line_run = Some((operator, 1));
                    }
                } else {
                    self.line_run = None;
                    number(self.out, dx)?;
                    number(self.out, dy)?;
                    self.out.push(5);
                }
                self.current = (self.current.0 + dx, self.current.1 + dy);
                self.bounds.point(self.current);
            }
            Command::CurveTo(c, d, p) => {
                self.line_run = None;
                if !self.open {
                    return Err(invalid("CFF curve outside path"));
                }
                let start = self.current;
                let c = self.delta(c)?;
                let d = self.delta(d)?;
                let p = self.delta(p)?;
                self.out.push(8);
                curve_bounds(self.bounds, start, c, d, p);
            }
            Command::Close => {
                self.line_run = None;
                if !self.open {
                    return Err(invalid("CFF Close outside path"));
                }
                self.open = false;
            }
        }
        bounded(
            self.out.len() - self.start,
            self.limits.max_program_bytes,
            "CFF program bytes",
        )?;
        Ok(())
    }
}
fn circle(o: DotOptions) -> Result<(Vec<u8>, f64, f64)> {
    if !o.cell_size.is_finite()
        || !o.diameter.is_finite()
        || o.cell_size <= 0.
        || o.diameter <= 0.
        || o.diameter > 1.5 * o.cell_size
    {
        return Err(invalid("invalid CFF dot cell/diameter"));
    }
    let r = fixed(o.diameter / 2.)?;
    let center = fixed(o.cell_size / 2.)?;
    if r == 0. {
        return Err(invalid("CFF dot diameter rounds to zero"));
    }
    let k = fixed(0.5522847498 * r)?;
    let rem = r - k;
    let mut out = Vec::new();
    for values in [
        [0., k, -rem, rem, -k, 0.],
        [-k, 0., -rem, -rem, 0., -k],
        [0., -k, rem, -rem, k, 0.],
        [k, 0., rem, rem, 0., k],
    ] {
        for v in values {
            number(&mut out, v)?;
        }
        out.push(8);
    }
    out.push(11);
    Ok((out, center, r))
}
pub(super) fn encode(font: &Font, l: &Limits) -> Result<Encoded> {
    let budget = Budget::new(usize::MAX, "outline working bytes");
    let has_dots = font
        .glyphs
        .iter()
        .any(|g| matches!(g.outline, Outline::Dots(_)));
    let mut encoder = Encoder::new(
        &font.postscript_name,
        &font.full_name,
        font.units_per_em,
        font.dots,
        l,
        &budget,
    )?;
    encoder.preamble(
        font.glyphs.len(),
        font.glyphs.first().map(|g| g.name == ".notdef"),
        has_dots,
    )?;
    for g in &font.glyphs {
        encoder.glyph(g)?;
    }
    Ok(encoder.tables()?.into_parts().0)
}

/// Exact Type 2 program length [`Encoder`] writes for `g`: the advance,
/// `PathWriter` moves (one omitted zero operand), alternating h/v line runs of
/// up to 40 operands sharing one operator, other lines and curves with all
/// operands, each shared dot as a move plus `callgsubr`, then `endchar`.
/// Deltas follow the writer's rounded current point. Each program's bytes are
/// admitted with this length before they are written.
pub(crate) fn program_len(g: &Glyph, dots: DotOptions) -> usize {
    fn fixed(x: f64) -> f64 {
        (x * 65536.).round() / 65536.
    }
    // Bytes `number()` writes: 1/2/3-byte integers, else 5 bytes.
    fn operand(v: f64) -> usize {
        let q = fixed(v);
        if !q.is_finite() || q.fract() != 0. {
            5
        } else if (-107. ..=107.).contains(&q) {
            1
        } else if (-1131. ..=1131.).contains(&q) {
            2
        } else if (-32768. ..=32767.).contains(&q) {
            3
        } else {
            5
        }
    }
    fn delta(at: &mut (f64, f64), p: (f64, f64)) -> (f64, f64) {
        let d = (fixed(p.0 - at.0), fixed(p.1 - at.1));
        *at = (at.0 + d.0, at.1 + d.1);
        d
    }
    fn moved(d: (f64, f64)) -> usize {
        1 + if d.0 == 0. {
            operand(d.1)
        } else if d.1 == 0. {
            operand(d.0)
        } else {
            operand(d.0) + operand(d.1)
        }
    }
    let mut bytes = operand(g.advance) + 1;
    let mut at = (0., 0.);
    match &g.outline {
        Outline::Path(path) => {
            let mut run: Option<(u8, usize)> = None;
            for &c in path {
                let added = match c {
                    Command::MoveTo(p) => {
                        run = None;
                        moved(delta(&mut at, p))
                    }
                    Command::LineTo(p) => {
                        let (dx, dy) = delta(&mut at, p);
                        if dx == 0. || dy == 0. {
                            let (operator, value) = if dx == 0. { (7, dy) } else { (6, dx) };
                            let joined = run.filter(|&(first, count)| {
                                count < 40
                                    && operator == if count % 2 == 0 { first } else { 13 - first }
                            });
                            if let Some((first, count)) = joined {
                                run = Some((first, count + 1));
                                operand(value)
                            } else {
                                run = Some((operator, 1));
                                operand(value) + 1
                            }
                        } else {
                            run = None;
                            operand(dx) + operand(dy) + 1
                        }
                    }
                    Command::CurveTo(a, b, p) => {
                        run = None;
                        let mut n = 1;
                        for t in [a, b, p] {
                            let (dx, dy) = delta(&mut at, t);
                            n += operand(dx) + operand(dy);
                        }
                        n
                    }
                    Command::Close => {
                        run = None;
                        0
                    }
                };
                bytes = bytes.saturating_add(added);
            }
        }
        Outline::Dots(origins) => {
            let r = fixed(dots.diameter / 2.);
            let center = fixed(dots.cell_size / 2.);
            for &(x, y) in origins {
                let d = delta(&mut at, (x + center + r, y + center));
                bytes = bytes.saturating_add(moved(d) + 2);
            }
        }
    }
    bytes
}

/// Canonical CFF1 encoder that takes glyphs one at a time, so a caller can
/// drop each glyph's outline as soon as it is encoded ([`Font::encode`] uses
/// it for a whole font).
///
/// Only the encoded programs, the glyph names (in order, and once more in the
/// duplicate check) and their end offsets are kept, each charged on `budget`
/// before it grows. [`finish`](Self::finish) lays the table out once into a
/// byte counter and writes it once into an exactly sized, admitted buffer,
/// with no per-item or INDEX copies. A per-glyph validation error is held, and
/// stops encoding, until `finish` has run the checks `Font::encode` makes
/// before its glyph loop, so bytes and error precedence are `Font::encode`'s.
pub(crate) struct Encoder<'a> {
    postscript_name: &'a str,
    full_name: &'a str,
    units_per_em: u16,
    dots: DotOptions,
    l: &'a Limits,
    budget: &'a Budget,
    /// The shared dot subroutine, when `dots` are valid.
    circle: Option<(Vec<u8>, f64, f64)>,
    _circle: Lease,
    has_dots: bool,
    first_notdef: Option<bool>,
    count: usize,
    failed: Option<crate::Error>,
    stopped: bool,
    seen: BTreeSet<String>,
    seen_heap: usize,
    seen_lease: Lease,
    /// Names of glyphs 1.. (glyph 0's name is not a CFF string), concatenated.
    names: Vector<u8>,
    name_ends: Vector<usize>,
    programs: Vector<u8>,
    program_ends: Vector<usize>,
    bounds: Bounds,
    total_commands: usize,
    total_program: usize,
    total_table: usize,
}

impl<'a> Encoder<'a> {
    pub(crate) fn new(
        postscript_name: &'a str,
        full_name: &'a str,
        units_per_em: u16,
        dots: DotOptions,
        l: &'a Limits,
        budget: &'a Budget,
    ) -> Result<Self> {
        let circle = circle(dots).ok();
        let circle_lease = budget.reserve(circle.as_ref().map_or(0, |c| c.0.capacity()))?;
        Ok(Self {
            postscript_name,
            full_name,
            units_per_em,
            dots,
            l,
            budget,
            circle,
            _circle: circle_lease,
            has_dots: false,
            first_notdef: None,
            count: 0,
            failed: None,
            stopped: false,
            seen: BTreeSet::new(),
            seen_heap: 0,
            seen_lease: budget.reserve(0)?,
            names: Vector::new(budget)?,
            name_ends: Vector::new(budget)?,
            programs: Vector::new(budget)?,
            program_ends: Vector::new(budget)?,
            bounds: Bounds::default(),
            total_commands: 0,
            total_program: 0,
            total_table: full_name.len(),
        })
    }

    /// The checks `Font::encode` makes before encoding any glyph.
    fn preamble(&self, count: usize, first_notdef: Option<bool>, has_dots: bool) -> Result<()> {
        let l = self.l;
        l.sfnt.glyphs(count)?;
        name(self.postscript_name)?;
        if !(16..=16384).contains(&self.units_per_em) {
            return Err(invalid("invalid CFF units per em"));
        }
        if first_notdef != Some(true) {
            return Err(invalid("CFF GID 0 must be .notdef"));
        }
        if self.full_name.is_empty() || self.full_name.bytes().any(|b| !(32..=126).contains(&b)) {
            return Err(invalid("CFF full name must be printable ASCII"));
        }
        l.sfnt.table(self.full_name.len())?;
        // SID 391 is FullName; glyph names begin at 392. SID 65000+ is reserved.
        if count > 64609 {
            return Err(invalid("too many custom CFF glyph-name SIDs"));
        }
        if has_dots {
            circle(self.dots)?;
        }
        Ok(())
    }

    /// Encode the next glyph. Only ledger errors are returned here; a
    /// validation error is held for [`finish`](Self::finish) and stops encoding.
    pub(crate) fn glyph(&mut self, g: &Glyph) -> Result<()> {
        let i = self.count;
        self.count += 1;
        if i == 0 {
            self.first_notdef = Some(g.name == ".notdef");
        }
        let dots = matches!(g.outline, Outline::Dots(_));
        self.has_dots |= dots;
        if self.stopped {
            return Ok(());
        }
        if dots && self.circle.is_none() {
            // `finish` reports the invalid dot options first.
            self.stopped = true;
            return Ok(());
        }
        // Admit everything this glyph adds before writing it.
        self.programs.reserve(program_len(g, self.dots))?;
        self.program_ends.reserve(1)?;
        if i != 0 {
            self.names.reserve(g.name.len())?;
            self.name_ends.reserve(1)?;
        }
        let heap = self.seen_heap.saturating_add(g.name.len());
        self.seen_lease.grow_to(
            self.budget
                .tree_bytes::<String, ()>(self.seen.len() + 1)?
                .saturating_add(heap),
        )?;
        let start = self.programs.len();
        match self.encode_glyph(i, g)? {
            Ok(()) => {
                // `clone` allocates exactly the name's length (in `heap`).
                self.seen.insert(g.name.clone());
                self.seen_heap = heap;
                if i != 0 {
                    self.names.extend(g.name.as_bytes())?;
                    self.name_ends.push(self.names.len())?;
                }
                self.program_ends.push(self.programs.len())
            }
            Err(error) => {
                self.programs.append_with(|out| out.truncate(start))?;
                self.failed = Some(error);
                self.stopped = true;
                Ok(())
            }
        }
    }

    /// Validate and write one glyph. The outer error is the ledger's; the
    /// inner one is a validation error that `Font::encode` would report.
    fn encode_glyph(&mut self, i: usize, g: &Glyph) -> Result<Result<()>> {
        let l = self.l;
        let checked = (|| {
            name(&g.name)?;
            if self.seen.contains(&g.name) {
                return Err(invalid("duplicate CFF glyph name"));
            }
            if i != 0 {
                add_budget(
                    &mut self.total_table,
                    g.name.len(),
                    l.sfnt.resources.max_table_bytes,
                    "CFF table bytes",
                )?;
            }
            if g.advance < 0. {
                return Err(invalid("negative CFF advance"));
            }
            Ok(())
        })();
        if checked.is_err() {
            return Ok(checked);
        }
        let (circle_len, center, r) = self
            .circle
            .as_ref()
            .map_or((0, 0., 0.), |(c, center, r)| (c.len(), *center, *r));
        let bounds = &mut self.bounds;
        let written = self.programs.append_with(|out| -> Result<(usize, usize)> {
            let start = out.len();
            let mut w = PathWriter {
                out,
                start,
                current: (0., 0.),
                open: false,
                bounds,
                commands: 0,
                limits: l,
                line_run: None,
            };
            number(w.out, g.advance)?;
            match &g.outline {
                Outline::Path(commands) => {
                    bounded(commands.len(), l.max_commands, "CFF commands")?;
                    for &c in commands {
                        w.command(c)?;
                    }
                    if w.open {
                        return Err(invalid("unclosed CFF path"));
                    }
                }
                Outline::Dots(origins) => {
                    bounded(
                        origins.len().saturating_mul(6),
                        l.max_commands,
                        "CFF dot commands",
                    )?;
                    for &(x, y) in origins {
                        let target = (x + center + r, y + center);
                        w.command(Command::MoveTo(target))?;
                        int(w.out, -107);
                        w.out.push(29);
                        w.open = false;
                        w.bounds.point((w.current.0 - 2. * r, w.current.1 - r));
                        w.bounds.point((w.current.0, w.current.1 + r));
                        coordinate(w.current.0 - 2. * r)?;
                        coordinate(w.current.1 - r)?;
                        coordinate(w.current.1 + r)?;
                        add_budget(&mut w.commands, 5, l.max_commands, "CFF commands")?;
                    }
                }
            }
            w.out.push(14);
            Ok((w.out.len() - start, w.commands))
        })?;
        Ok(written
            .and_then(|(length, commands)| self.after_program(g, length, commands, circle_len)))
    }

    /// Whole-font totals once a glyph's program is written.
    fn after_program(
        &mut self,
        g: &Glyph,
        length: usize,
        commands: usize,
        circle_len: usize,
    ) -> Result<()> {
        let l = self.l;
        add_budget(
            &mut self.total_commands,
            commands,
            l.max_total_commands,
            "CFF total commands",
        )?;
        let expanded = length
            .checked_add(match &g.outline {
                Outline::Dots(d) => d.len().saturating_mul(circle_len + 1),
                _ => 0,
            })
            .ok_or_else(|| invalid("CFF work overflow"))?;
        bounded(expanded, l.max_program_bytes, "CFF program bytes")?;
        add_budget(
            &mut self.total_program,
            expanded,
            l.max_total_program_bytes,
            "CFF total program bytes",
        )?;
        add_budget(
            &mut self.total_table,
            length,
            l.sfnt.resources.max_table_bytes,
            "CFF table bytes",
        )
    }

    /// Run the held checks, then lay out and write the table. The returned
    /// lease charges exactly the table and `maxp`; every other buffer of this
    /// encoder has dropped (and been refunded) when this returns.
    pub(crate) fn finish(self) -> Result<Owned<Encoded>> {
        self.preamble(self.count, self.first_notdef, self.has_dots)?;
        self.tables()
    }

    fn program(&self, i: usize) -> &[u8] {
        let start = if i == 0 { 0 } else { self.program_ends[i - 1] };
        &self.programs[start..self.program_ends[i]]
    }

    /// String INDEX item `i`: the full name, then the glyph names from 1.
    fn string(&self, i: usize) -> &[u8] {
        if i == 0 {
            return self.full_name.as_bytes();
        }
        let start = if i == 1 { 0 } else { self.name_ends[i - 2] };
        &self.names[start..self.name_ends[i - 1]]
    }

    fn tables(mut self) -> Result<Owned<Encoded>> {
        if let Some(error) = self.failed.take() {
            return Err(error);
        }
        let l = self.l;
        let count = self.count;
        let b = self.bounds.0.unwrap_or([0.; 4]);
        let mut bbox = [0i16; 4];
        for (i, x) in b.iter().enumerate() {
            let x = if i < 2 { x.floor() } else { x.ceil() };
            if x < i16::MIN as f64 || x > i16::MAX as f64 {
                return Err(invalid("CFF bounds exceed head int16"));
            }
            bbox[i] = x as i16;
        }
        // `preamble` has checked that the dot subroutine exists when used.
        let circle: &[u8] = match (&self.circle, self.has_dots) {
            (Some((c, _, _)), true) => c,
            _ => &[],
        };
        let globals = usize::from(self.has_dots);
        // The two top DICT drafts are small.
        let _top_scratch = self
            .budget
            .reserve(4096usize.saturating_add(self.postscript_name.len().saturating_mul(4)))?;
        let ps = self.postscript_name.as_bytes();
        let strings = |o: &mut dyn Out| -> Result<()> {
            put_index(
                o,
                count,
                |i| Ok(self.string(i).len()),
                |o, i| o.put(self.string(i)),
                l,
            )
        };
        let programs = |o: &mut dyn Out| -> Result<()> {
            put_index(
                o,
                count,
                |i| Ok(self.program(i).len()),
                |o, i| o.put(self.program(i)),
                l,
            )
        };
        let single = |o: &mut dyn Out, n: usize, item: &[u8]| -> Result<()> {
            put_index(o, n, |_| Ok(item.len()), |o, _| o.put(item), l)
        };
        // Lay out every INDEX in `Font::encode`'s order, with its checks.
        let name_len = layout(|o| single(o, 1, ps))?;
        let string_len = layout(|o| strings(o))?;
        let global_len = layout(|o| single(o, globals, circle))?;
        let cs_len = layout(|o| programs(o))?;
        let charset_len = 1 + 2 * (count - 1);
        let private = [139, 20, 139, 21];
        let top = |base: usize| -> Result<Vec<u8>> {
            let mut d = Vec::new();
            int(&mut d, 391);
            d.push(2);
            for v in bbox {
                int(&mut d, i32::from(v));
            }
            d.push(5);
            for v in [
                1. / f64::from(self.units_per_em),
                0.,
                0.,
                1. / f64::from(self.units_per_em),
                0.,
                0.,
            ] {
                if v == 0. {
                    int(&mut d, 0);
                } else {
                    real(&mut d, v);
                }
            }
            d.extend([12, 7]);
            offset(&mut d, base)?;
            d.push(15);
            offset(&mut d, base + charset_len)?;
            d.push(17);
            int(&mut d, private.len() as i32);
            offset(&mut d, base + charset_len + cs_len)?;
            d.push(18);
            Ok(d)
        };
        let draft = top(0)?;
        let base = 4 + name_len + layout(|o| single(o, 1, &draft))? + string_len + global_len;
        drop(draft);
        let dict = top(base)?;
        let top_len = layout(|o| single(o, 1, &dict))?;
        let size = 4 + name_len + top_len + string_len + global_len + charset_len + cs_len + 4;
        l.sfnt.table(size)?;
        let mut data = Vector::<u8>::with_capacity(self.budget, size)?;
        data.put(&[1, 0, 4, 4])?;
        single(&mut data, 1, ps)?;
        single(&mut data, 1, &dict)?;
        strings(&mut data)?;
        single(&mut data, globals, circle)?;
        data.put(&[0])?;
        for i in 1..count {
            data.put(&((391 + i) as u16).to_be_bytes())?;
        }
        programs(&mut data)?;
        data.put(&private)?;
        debug_assert_eq!(data.len(), size);
        let (data, mut lease) = data.into_parts();
        let mut maxp = Vector::<u8>::with_capacity(self.budget, 6)?;
        maxp.put(&0x5000u32.to_be_bytes())?;
        maxp.put(&(count as u16).to_be_bytes())?;
        let (maxp, mut maxp_lease) = maxp.into_parts();
        lease.merge(&mut maxp_lease)?;
        Ok(Owned::from_parts(
            Encoded {
                data,
                bounds: bbox,
                maxp,
            },
            lease,
        ))
    }
}

/// Length of what `write` puts into an [`Out`], with the same checks.
fn layout(write: impl FnOnce(&mut Count) -> Result<()>) -> Result<usize> {
    let mut count = Count(0);
    write(&mut count)?;
    Ok(count.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        let square = |x: f64| {
            Outline::Path(vec![
                Command::MoveTo((x, 0.)),
                Command::LineTo((x + 100., 0.)),
                Command::LineTo((x + 100., 100.)),
                Command::LineTo((x, 100.)),
                Command::Close,
            ])
        };
        let mut glyphs = vec![Glyph {
            name: ".notdef".into(),
            advance: 500.,
            outline: Outline::Path(Vec::new()),
        }];
        for i in 0..40 {
            glyphs.push(Glyph {
                name: format!("g{i}"),
                advance: 100. * f64::from(i),
                outline: if i % 3 == 0 {
                    Outline::Dots(vec![(0., 0.), (f64::from(i) * 10., 300.)])
                } else {
                    square(f64::from(i) * 7.5)
                },
            });
        }
        Font {
            postscript_name: "Stream".into(),
            full_name: "Stream Test".into(),
            units_per_em: 1000,
            glyphs,
            dots: DotOptions::default(),
        }
    }

    /// Stream `font` through an encoder on a ledger of `maximum` bytes.
    fn stream(font: &Font, maximum: usize) -> Result<(Encoded, usize)> {
        let l = Limits::default();
        let budget = Budget::new(maximum, "test CFF bytes");
        let mut encoder = Encoder::new(
            &font.postscript_name,
            &font.full_name,
            font.units_per_em,
            font.dots,
            &l,
            &budget,
        )?;
        for g in &font.glyphs {
            encoder.glyph(g)?;
        }
        let (encoded, lease) = encoder.finish()?.into_parts();
        // Only the table and maxp stay charged.
        assert_eq!(
            lease.bytes(),
            encoded.data.capacity() + encoded.maxp.capacity()
        );
        drop(lease);
        Ok((encoded, budget.peak()))
    }

    #[test]
    fn streamed_tables_match_whole_font_encoding_at_an_exact_ledger() {
        let font = font();
        let whole = font.encode(&Limits::default()).unwrap();
        assert_eq!(whole.data.capacity(), whole.data.len());
        let (streamed, peak) = stream(&font, usize::MAX).unwrap();
        assert_eq!(streamed, whole);
        assert_eq!(stream(&font, peak).unwrap().0, whole);
        assert!(matches!(
            stream(&font, peak - 1),
            Err(crate::Error::ResourceLimit { .. })
        ));
    }

    #[test]
    fn glyph_errors_wait_for_the_whole_font_checks() {
        let mut font = font();
        font.glyphs[3].name = "g1".into();
        let l = Limits::default();
        assert_eq!(
            font.encode(&l).unwrap_err().to_string(),
            invalid("duplicate CFF glyph name").to_string()
        );
        // A whole-font check failure precedes the earlier glyph's error.
        font.units_per_em = 8;
        let expected = font.encode(&l).unwrap_err().to_string();
        assert_eq!(expected, invalid("invalid CFF units per em").to_string());
        assert_eq!(stream(&font, usize::MAX).unwrap_err().to_string(), expected);
    }
}
