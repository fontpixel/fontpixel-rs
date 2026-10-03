//! Strict Type 2 preflight. Fontations remains the path evaluator; this pass
//! rejects permissive recovery, bounds repeated work, and checks coordinate sums.
use super::*;

pub(super) fn program(
    data: &[u8],
    local: &Index<'_>,
    global: &Index<'_>,
    limits: &Limits,
) -> Result<(usize, usize, Option<f64>)> {
    let mut v = Validator {
        local,
        global,
        limits,
        stack: Vec::with_capacity(48),
        current: (0., 0.),
        width_seen: false,
        width_value: None,
        stems: 0,
        open: false,
        work: 0,
        commands: 0,
    };
    v.run(data, 0)?;
    Ok((v.work, v.commands, v.width_value))
}
struct Validator<'a, 'b> {
    local: &'a Index<'b>,
    global: &'a Index<'b>,
    limits: &'a Limits,
    stack: Vec<f64>,
    current: Point,
    width_seen: bool,
    width_value: Option<f64>,
    stems: usize,
    open: bool,
    work: usize,
    commands: usize,
}
impl Validator<'_, '_> {
    fn command(&mut self) -> Result<()> {
        add_budget(
            &mut self.commands,
            1,
            self.limits.max_commands,
            "CFF commands",
        )
    }
    fn point(&mut self, dx: f64, dy: f64) -> Result<()> {
        coordinate(dx)?;
        coordinate(dy)?;
        let p = (self.current.0 + dx, self.current.1 + dy);
        coordinate(p.0)?;
        coordinate(p.1)?;
        self.current = p;
        Ok(())
    }
    fn line(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.point(dx, dy)?;
        self.command()
    }
    fn curve(&mut self, d: [f64; 6]) -> Result<()> {
        for p in d.as_chunks::<2>().0 {
            self.point(p[0], p[1])?;
        }
        self.command()
    }
    fn width(&mut self, present: bool) {
        if !self.width_seen && present {
            self.width_value = Some(self.stack.remove(0));
        }
        self.width_seen = true;
    }
    fn args(&self, ok: bool) -> Result<()> {
        if ok {
            Ok(())
        } else {
            Err(invalid("invalid CFF Type 2 operand count"))
        }
    }
    fn run(&mut self, data: &[u8], depth: usize) -> Result<()> {
        bounded(depth, 10, "CFF subroutine depth")?;
        add_budget(
            &mut self.work,
            data.len().saturating_add(1),
            self.limits.max_program_bytes,
            "CFF program bytes",
        )?;
        let mut i = 0;
        while i < data.len() {
            let op = data[i];
            i += 1;
            let number = match op {
                32..=246 => Some(f64::from(op) - 139.),
                247..=250 => {
                    let b = bytes(data, i, 1)?[0];
                    i += 1;
                    Some((u16::from(op) - 247) as f64 * 256. + f64::from(b) + 108.)
                }
                251..=254 => {
                    let b = bytes(data, i, 1)?[0];
                    i += 1;
                    Some(-((u16::from(op) - 251) as f64 * 256. + f64::from(b) + 108.))
                }
                28 => {
                    let b = bytes(data, i, 2)?;
                    i += 2;
                    Some(f64::from(i16::from_be_bytes([b[0], b[1]])))
                }
                255 => {
                    let b = bytes(data, i, 4)?;
                    i += 4;
                    Some(f64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])) / 65536.)
                }
                _ => None,
            };
            if let Some(n) = number {
                bounded(self.stack.len() + 1, 48, "CFF operand stack")?;
                self.stack.push(n);
                continue;
            }
            match op {
                10 | 29 => {
                    let n = self
                        .stack
                        .pop()
                        .ok_or_else(|| invalid("missing CFF subroutine index"))?;
                    if n.fract() != 0. {
                        return Err(invalid("fractional CFF subroutine index"));
                    }
                    let index = if op == 10 { self.local } else { self.global };
                    let idx = n as i64 + i64::from(index.subr_bias());
                    let sub = index
                        .get(
                            usize::try_from(idx)
                                .map_err(|_| invalid("negative CFF subroutine index"))?,
                        )
                        .ok_or_else(|| invalid("missing CFF subroutine"))?;
                    self.run(sub, depth + 1)?;
                }
                11 => {
                    if depth == 0 || i != data.len() {
                        return Err(invalid("unexpected CFF return/trailing bytes"));
                    }
                    return Ok(());
                }
                14 => {
                    if depth != 0 || i != data.len() {
                        return Err(invalid(
                            "CFF endchar in subroutine or trailing bytes unsupported",
                        ));
                    }
                    self.width(self.stack.len() == 1);
                    self.args(self.stack.is_empty())?;
                    if self.open {
                        self.command()?;
                        self.open = false;
                    }
                    return Ok(());
                }
                1 | 3 | 18 | 23 | 19 | 20 => {
                    self.width(self.stack.len() % 2 == 1);
                    let n = self.stack.len();
                    self.args(n.is_multiple_of(2) && (n != 0 || matches!(op, 19 | 20)))?;
                    self.stems += n / 2;
                    bounded(self.stems, 96, "CFF stem hints")?;
                    // Stem cumulative arithmetic must not wrap the Fontations fixed type.
                    let mut v = 0.;
                    for pair in self.stack.as_chunks::<2>().0 {
                        v += pair[0];
                        coordinate(v)?;
                        v += pair[1];
                        coordinate(v)?;
                    }
                    self.stack.clear();
                    if matches!(op, 19 | 20) {
                        let len = self.stems.div_ceil(8);
                        bytes(data, i, len)?;
                        i += len;
                    }
                }
                4 | 21 | 22 => {
                    let n = if op == 21 { 2 } else { 1 };
                    self.width(self.stack.len() == n + 1);
                    self.args(self.stack.len() == n)?;
                    let (dx, dy) = match op {
                        4 => (0., self.stack[0]),
                        22 => (self.stack[0], 0.),
                        _ => (self.stack[0], self.stack[1]),
                    };
                    self.point(dx, dy)?;
                    if self.open {
                        self.command()?;
                    }
                    self.command()?;
                    self.open = true;
                    self.stack.clear();
                }
                12 => {
                    let sub = bytes(data, i, 1)?[0];
                    i += 1;
                    if sub == 12 {
                        self.args(self.stack.len() >= 2)?;
                        let b = self.stack.pop().unwrap();
                        let a = self.stack.pop().unwrap();
                        if b == 0. {
                            return Err(invalid("CFF division by zero"));
                        }
                        coordinate(a / b)?;
                        let n = (Fixed::from_f64(a) / Fixed::from_f64(b)).to_f64();
                        self.stack.push(n);
                        continue;
                    }
                    if sub == 0 {
                        self.args(self.stack.is_empty())?;
                        continue;
                    }
                    if !self.open {
                        return Err(invalid("CFF flex outside contour"));
                    }
                    let s = std::mem::take(&mut self.stack);
                    match sub {
                        34 => {
                            self.args(s.len() == 7)?;
                            self.curve([s[0], 0., s[1], s[2], s[3], 0.])?;
                            self.curve([s[4], 0., s[5], -s[2], s[6], 0.])?;
                        }
                        35 => {
                            self.args(s.len() == 13)?;
                            self.curve(s[0..6].try_into().unwrap())?;
                            self.curve(s[6..12].try_into().unwrap())?;
                        }
                        36 => {
                            self.args(s.len() == 9)?;
                            self.curve([s[0], s[1], s[2], s[3], s[4], 0.])?;
                            self.curve([s[5], 0., s[6], s[7], s[8], -(s[1] + s[3] + s[7])])?;
                        }
                        37 => {
                            self.args(s.len() == 11)?;
                            let dx = s[0] + s[2] + s[4] + s[6] + s[8];
                            let dy = s[1] + s[3] + s[5] + s[7] + s[9];
                            // The evaluator compares these distances in signed 16.16.
                            // Absolute control points can fit while this subtraction
                            // wraps and chooses the wrong final flex axis.
                            coordinate(dx)?;
                            coordinate(dy)?;
                            let (x, y) = if dx.abs() > dy.abs() {
                                (s[10], -dy)
                            } else {
                                (-dx, s[10])
                            };
                            self.curve(s[0..6].try_into().unwrap())?;
                            self.curve([s[6], s[7], s[8], s[9], x, y])?;
                        }
                        _ => {
                            return Err(invalid(
                                "unsupported CFF Type 2 arithmetic/transient/legacy operator",
                            ));
                        }
                    }
                }
                5..=8 | 24..=27 | 30 | 31 => {
                    if !self.open {
                        return Err(invalid("CFF drawing outside contour"));
                    }
                    let s = std::mem::take(&mut self.stack);
                    let n = s.len();
                    match op {
                        5 => {
                            self.args(n >= 2 && n.is_multiple_of(2))?;
                            for p in s.as_chunks::<2>().0 {
                                self.line(p[0], p[1])?;
                            }
                        }
                        6 | 7 => {
                            self.args(n > 0)?;
                            for (j, &d) in s.iter().enumerate() {
                                if (j % 2 == 0) == (op == 6) {
                                    self.line(d, 0.)?;
                                } else {
                                    self.line(0., d)?;
                                }
                            }
                        }
                        8 => {
                            self.args(n >= 6 && n.is_multiple_of(6))?;
                            for p in s.as_chunks::<6>().0 {
                                self.curve(*p)?;
                            }
                        }
                        24 => {
                            self.args(n >= 8 && (n - 2).is_multiple_of(6))?;
                            for p in s[..n - 2].as_chunks::<6>().0 {
                                self.curve(*p)?;
                            }
                            self.line(s[n - 2], s[n - 1])?;
                        }
                        25 => {
                            self.args(n >= 8 && (n - 6).is_multiple_of(2))?;
                            for p in s[..n - 6].as_chunks::<2>().0 {
                                self.line(p[0], p[1])?;
                            }
                            self.curve(s[n - 6..].try_into().unwrap())?;
                        }
                        26 | 27 => {
                            self.args(n >= 4 && (n.is_multiple_of(4) || n % 4 == 1))?;
                            let mut j = n % 4;
                            let mut extra = if j == 1 { s[0] } else { 0. };
                            while j < n {
                                let p = &s[j..j + 4];
                                let d = if op == 26 {
                                    [extra, p[0], p[1], p[2], 0., p[3]]
                                } else {
                                    [p[0], extra, p[1], p[2], p[3], 0.]
                                };
                                self.curve(d)?;
                                extra = 0.;
                                j += 4;
                            }
                        }
                        30 | 31 => {
                            self.args(n >= 4 && (n.is_multiple_of(4) || n % 4 == 1))?;
                            let mut horizontal = op == 31;
                            let mut j = 0;
                            while n - j >= 4 {
                                let p = &s[j..j + 4];
                                let last = n - j == 5;
                                let extra = if last { s[j + 4] } else { 0. };
                                let d = if horizontal {
                                    [p[0], 0., p[1], p[2], extra, p[3]]
                                } else {
                                    [0., p[0], p[1], p[2], p[3], extra]
                                };
                                self.curve(d)?;
                                horizontal = !horizontal;
                                j += if last { 5 } else { 4 };
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                _ => return Err(invalid("reserved or unsupported CFF Type 2 operator")),
            }
        }
        Err(invalid("CFF program missing endchar/return"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(p: &[u8], subs: &[Vec<u8>]) -> Result<(usize, usize, Option<f64>)> {
        let l = Limits::default();
        let bytes = super::super::write::index(subs, &l)?;
        let global = Index::new(&bytes, false).map_err(ps_error)?;
        program(p, &Index::Empty, &global, &l)
    }
    #[test]
    fn malformed_type2_is_not_recovered() {
        for p in [
            vec![],
            vec![139],
            vec![139, 12],
            vec![0, 14],
            vec![11],
            vec![14, 14],
            vec![139, 139, 5, 14],
            vec![139, 139, 21, 139, 8, 14],
            vec![139, 139, 21, 139, 139, 139, 24, 14],
            vec![139, 139, 1, 19],
            vec![139, 139, 21, 139, 139, 12, 10, 14], // unsupported add
            vec![139, 139, 21, 139, 139, 12, 12, 14], // div zero
            vec![139, 139, 139, 139, 14],             // seac
            vec![139; 49],
        ] {
            assert!(run(&p, &[]).is_err(), "{p:?}");
        }
    }
    #[test]
    fn cyclic_and_missing_subroutines_fail() {
        assert!(run(&[32, 29, 14], &[]).is_err());
        assert!(run(&[32, 29, 14], &[vec![32, 29, 11]]).is_err());
        assert!(run(&[32, 29, 14], &[vec![14]]).is_err());
        assert!(run(&[32, 29, 14], &[vec![]]).is_err());
        assert!(run(&[32, 29, 14], &[vec![11]]).is_ok());
    }
    #[test]
    fn point_sums_cannot_wrap() {
        assert!(run(&[28, 0x7f, 0xff, 139, 21, 140, 6, 14], &[]).is_err());
        assert!(run(&[28, 0x80, 0, 139, 21, 138, 6, 14], &[]).is_err());
    }
    #[test]
    fn hints_and_cross_call_operand_stacks() {
        let p = [
            248, 136, 139, 149, 1, 19, 0x80, 139, 139, 21, 239, 139, 32, 29, 14,
        ];
        assert_eq!(run(&p, &[vec![5, 11]]).unwrap().2, Some(500.));
        // Width supplied only at endchar, or omitted => Private default.
        assert_eq!(run(&[248, 136, 14], &[]).unwrap().2, Some(500.));
        assert_eq!(run(&[14], &[]).unwrap().2, None);
    }
    #[test]
    fn repeated_empty_calls_consume_budget() {
        let l = Limits {
            max_program_bytes: 20,
            ..Default::default()
        };
        let b = super::super::write::index(&[vec![11]], &l).unwrap();
        let ix = Index::new(&b, false).unwrap();
        assert!(
            program(
                &[32, 29, 32, 29, 32, 29, 32, 29, 32, 29, 14],
                &Index::Empty,
                &ix,
                &l
            )
            .is_err()
        );
    }
    #[test]
    fn flex_axis_distances_cannot_wrap_even_when_points_fit() {
        let mut p = vec![28, 0x8a, 0xd0, 28, 0xd8, 0xf0, 21]; // (-30000,-10000)
        for _ in 0..5 {
            p.extend([28, 0x2e, 0xe0, 28, 0x0f, 0xa0]);
        } // (+12000,+4000)
        p.extend([28, 0xfc, 0x18, 12, 37, 14]); // flex1: dx=60000, dy=20000
        assert!(run(&p, &[]).is_err());
    }
}
