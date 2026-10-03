use super::*;
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
    p16(&mut out, items.len() as u16);
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
    out: Vec<u8>,
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
        let dx = number(&mut self.out, target.0 - self.current.0)?;
        let dy = number(&mut self.out, target.1 - self.current.1)?;
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
                    number(&mut self.out, dy)?;
                    self.out.push(4);
                } else if dy == 0. {
                    number(&mut self.out, dx)?;
                    self.out.push(22);
                } else {
                    number(&mut self.out, dx)?;
                    number(&mut self.out, dy)?;
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
                        number(&mut self.out, value)?;
                        self.out.push(first);
                        self.line_run = Some((first, count + 1));
                    } else {
                        number(&mut self.out, value)?;
                        self.out.push(operator);
                        self.line_run = Some((operator, 1));
                    }
                } else {
                    self.line_run = None;
                    number(&mut self.out, dx)?;
                    number(&mut self.out, dy)?;
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
            self.out.len(),
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
    l.sfnt.glyphs(font.glyphs.len())?;
    name(&font.postscript_name)?;
    if !(16..=16384).contains(&font.units_per_em) {
        return Err(invalid("invalid CFF units per em"));
    }
    if font.glyphs[0].name != ".notdef" {
        return Err(invalid("CFF GID 0 must be .notdef"));
    }
    if font.full_name.is_empty() || font.full_name.bytes().any(|b| !(32..=126).contains(&b)) {
        return Err(invalid("CFF full name must be printable ASCII"));
    }
    l.sfnt.table(font.full_name.len())?;
    // SID 391 is FullName; glyph names begin at 392. SID 65000+ is reserved.
    if font.glyphs.len() > 64609 {
        return Err(invalid("too many custom CFF glyph-name SIDs"));
    }
    let has_dots = font
        .glyphs
        .iter()
        .any(|g| matches!(g.outline, Outline::Dots(_)));
    let (circle, center, r) = if has_dots {
        circle(font.dots)?
    } else {
        (Vec::new(), 0., 0.)
    };
    let mut seen = BTreeSet::new();
    let mut strings = vec![font.full_name.as_bytes().to_vec()];
    let mut charstrings = Vec::with_capacity(font.glyphs.len());
    let mut bounds = Bounds::default();
    let mut total_commands = 0;
    let mut total_program = 0;
    let mut total_table = font.full_name.len();
    for (i, g) in font.glyphs.iter().enumerate() {
        name(&g.name)?;
        if !seen.insert(&g.name) {
            return Err(invalid("duplicate CFF glyph name"));
        }
        if i != 0 {
            add_budget(
                &mut total_table,
                g.name.len(),
                l.sfnt.resources.max_table_bytes,
                "CFF table bytes",
            )?;
            strings.push(g.name.as_bytes().to_vec());
        }
        if g.advance < 0. {
            return Err(invalid("negative CFF advance"));
        }
        let mut w = PathWriter {
            out: Vec::new(),
            current: (0., 0.),
            open: false,
            bounds: &mut bounds,
            commands: 0,
            limits: l,
            line_run: None,
        };
        number(&mut w.out, g.advance)?;
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
                    int(&mut w.out, -107);
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
        add_budget(
            &mut total_commands,
            w.commands,
            l.max_total_commands,
            "CFF total commands",
        )?;
        let expanded = w
            .out
            .len()
            .checked_add(match &g.outline {
                Outline::Dots(d) => d.len().saturating_mul(circle.len() + 1),
                _ => 0,
            })
            .ok_or_else(|| invalid("CFF work overflow"))?;
        bounded(expanded, l.max_program_bytes, "CFF program bytes")?;
        add_budget(
            &mut total_program,
            expanded,
            l.max_total_program_bytes,
            "CFF total program bytes",
        )?;
        add_budget(
            &mut total_table,
            w.out.len(),
            l.sfnt.resources.max_table_bytes,
            "CFF table bytes",
        )?;
        charstrings.push(w.out);
    }
    let b = bounds.0.unwrap_or([0.; 4]);
    let mut bbox = [0i16; 4];
    for (i, x) in b.iter().enumerate() {
        let x = if i < 2 { x.floor() } else { x.ceil() };
        if x < i16::MIN as f64 || x > i16::MAX as f64 {
            return Err(invalid("CFF bounds exceed head int16"));
        }
        bbox[i] = x as i16;
    }
    let name_index = index(&[font.postscript_name.as_bytes().to_vec()], l)?;
    let string_index = index(&strings, l)?;
    let global = index(&if has_dots { vec![circle] } else { vec![] }, l)?;
    let cs = index(&charstrings, l)?;
    let mut charset = vec![0];
    for i in 1..font.glyphs.len() {
        p16(&mut charset, (391 + i) as u16);
    }
    let private = vec![139, 20, 139, 21];
    let top = |base: usize| -> Result<Vec<u8>> {
        let mut d = Vec::new();
        int(&mut d, 391);
        d.push(2);
        for v in bbox {
            int(&mut d, i32::from(v));
        }
        d.push(5);
        for v in [
            1. / f64::from(font.units_per_em),
            0.,
            0.,
            1. / f64::from(font.units_per_em),
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
        offset(&mut d, base + charset.len())?;
        d.push(17);
        int(&mut d, private.len() as i32);
        offset(&mut d, base + charset.len() + cs.len())?;
        d.push(18);
        index(&[d], l)
    };
    let base = 4 + name_index.len() + top(0)?.len() + string_index.len() + global.len();
    let parts = [
        name_index,
        top(base)?,
        string_index,
        global,
        charset,
        cs,
        private,
    ];
    let size = 4 + parts.iter().map(Vec::len).sum::<usize>();
    l.sfnt.table(size)?;
    let mut data = Vec::with_capacity(size);
    data.extend([1, 0, 4, 4]);
    for part in parts {
        data.extend(part);
    }
    let mut maxp = Vec::new();
    p32(&mut maxp, 0x5000);
    p16(&mut maxp, font.glyphs.len() as u16);
    Ok(Encoded {
        data,
        bounds: bbox,
        maxp,
    })
}
