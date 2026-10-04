//! A bounded selector for LVGL's generated version/feature guards, not a C preprocessor.
use super::*;
struct Expression<'a> {
    tokens: Vec<Token<'a>>,
    at: usize,
    defines: &'a BTreeMap<String, i64>,
}
impl Expression<'_> {
    fn punct(&mut self, c: u8) -> bool {
        if self
            .tokens
            .get(self.at)
            .is_some_and(|v| v.kind == Kind::Punct(c))
        {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn pair(&mut self, a: u8, b: u8) -> bool {
        if self
            .tokens
            .get(self.at)
            .is_some_and(|v| v.kind == Kind::Punct(a))
            && self
                .tokens
                .get(self.at + 1)
                .is_some_and(|v| v.kind == Kind::Punct(b))
        {
            self.at += 2;
            true
        } else {
            false
        }
    }
    fn atom(&mut self, depth: usize) -> Result<i64> {
        if depth > 32 {
            return Err(invalid("LVGL conditional nesting exceeds 32"));
        }
        if self.punct(b'!') {
            return Ok(i64::from(self.atom(depth + 1)? == 0));
        }
        if self.punct(b'(') {
            let v = self.or(depth + 1)?;
            if !self.punct(b')') {
                return Err(invalid("unclosed LVGL conditional"));
            }
            return Ok(v);
        }
        if let Some(Token {
            kind: Kind::Ident(name),
            ..
        }) = self.tokens.get(self.at)
        {
            let name = *name;
            self.at += 1;
            if name == "LV_VERSION_CHECK" {
                if !self.punct(b'(') {
                    return Err(invalid("LVGL version check needs literal tuple"));
                }
                let a = lexer::integer(&self.tokens, &mut self.at)?;
                if !self.punct(b',') {
                    return Err(invalid("LVGL version tuple comma"));
                }
                let b = lexer::integer(&self.tokens, &mut self.at)?;
                if !self.punct(b',') {
                    return Err(invalid("LVGL version tuple comma"));
                }
                let c = lexer::integer(&self.tokens, &mut self.at)?;
                if !self.punct(b')') || [a, b, c].iter().any(|v| !(0..=65535).contains(v)) {
                    return Err(invalid("invalid LVGL version tuple"));
                }
                return Ok(i64::from((8, 3, 11) >= (a, b, c)));
            }
            return match name {
                "LVGL_VERSION_MAJOR" => Ok(8),
                "LVGL_VERSION_MINOR" => Ok(3),
                "LVGL_VERSION_PATCH" => Ok(11),
                _ => self
                    .defines
                    .get(name)
                    .copied()
                    .ok_or_else(|| invalid(format!("unknown LVGL conditional symbol {name}"))),
            };
        }
        lexer::integer(&self.tokens, &mut self.at)
    }
    fn compare(&mut self, depth: usize) -> Result<i64> {
        let a = self.atom(depth)?;
        let op = if self.pair(b'=', b'=') {
            Some(0)
        } else if self.pair(b'!', b'=') {
            Some(1)
        } else if self.pair(b'<', b'=') {
            Some(2)
        } else if self.pair(b'>', b'=') {
            Some(3)
        } else if self.punct(b'<') {
            Some(4)
        } else if self.punct(b'>') {
            Some(5)
        } else {
            None
        };
        if let Some(op) = op {
            let b = self.atom(depth)?;
            Ok(i64::from(match op {
                0 => a == b,
                1 => a != b,
                2 => a <= b,
                3 => a >= b,
                4 => a < b,
                _ => a > b,
            }))
        } else {
            Ok(a)
        }
    }
    fn and(&mut self, depth: usize) -> Result<i64> {
        let mut v = self.compare(depth)?;
        while self.pair(b'&', b'&') {
            let rhs = self.compare(depth)?;
            v = i64::from(v != 0 && rhs != 0);
        }
        Ok(v)
    }
    fn or(&mut self, depth: usize) -> Result<i64> {
        let mut v = self.and(depth)?;
        while self.pair(b'|', b'|') {
            let rhs = self.and(depth)?;
            v = i64::from(v != 0 || rhs != 0);
        }
        Ok(v)
    }
}
fn condition(s: &str, defs: &BTreeMap<String, i64>, b: &mut Budget) -> Result<bool> {
    let mut e = Expression {
        tokens: lexer::lex(s, b)?,
        at: 0,
        defines: defs,
    };
    let v = e.or(0)?;
    if e.at != e.tokens.len() {
        return Err(invalid("unsupported LVGL conditional expression"));
    }
    Ok(v != 0)
}
fn guard(s: &str) -> bool {
    s == "LV_LVGL_H_INCLUDE_SIMPLE"
        || (s.starts_with("LV_FONT_") && s.bytes().all(|v| v.is_ascii_alphanumeric() || v == b'_'))
}
pub(super) fn lvgl<'a>(tokens: Vec<Token<'a>>, b: &mut Budget) -> Result<Vec<Token<'a>>> {
    b.add(
        tokens
            .len()
            .saturating_mul(std::mem::size_of::<Token>() * 2),
    )?;
    let mut out = Vec::new();
    let mut defines = BTreeMap::from([("LV_LVGL_H_INCLUDE_SIMPLE".to_owned(), 1)]);
    let mut stack: Vec<(bool, bool, bool)> = Vec::new();
    let mut active = true;
    for token in tokens {
        let Kind::Directive(raw) = token.kind else {
            if active {
                out.push(token);
            }
            continue;
        };
        let text = raw
            .trim_start_matches('#')
            .trim()
            .split("/*")
            .next()
            .unwrap_or("")
            .trim();
        let mut words = text.splitn(2, char::is_whitespace);
        let cmd = words.next().unwrap_or("");
        let arg = words.next().unwrap_or("").trim();
        match cmd {
            "include" => {}
            "define" => {
                if active {
                    let mut pair = arg.split_whitespace();
                    let name = pair.next().ok_or_else(|| invalid("empty LVGL define"))?;
                    if !guard(name) {
                        return Err(invalid("only LVGL font feature defines are supported"));
                    }
                    let val = pair.next().unwrap_or("1");
                    if pair.next().is_some() || !["0", "1"].contains(&val) {
                        return Err(invalid("LVGL feature define must be 0 or 1"));
                    }
                    b.add(name.len() + 128)?;
                    defines.insert(name.into(), i64::from(val == "1"));
                }
            }
            "if" | "ifdef" | "ifndef" => {
                if stack.len() >= 32 {
                    return Err(invalid("LVGL directive nesting exceeds 32"));
                }
                let take = if !active {
                    false
                } else if cmd == "if" {
                    condition(arg, &defines, b)?
                } else {
                    if !guard(arg) {
                        return Err(invalid("unsupported LVGL feature guard"));
                    }
                    if cmd == "ifdef" {
                        defines.contains_key(arg)
                    } else {
                        !defines.contains_key(arg)
                    }
                };
                stack.push((active, take, false));
                active &= take;
            }
            "else" => {
                let Some((outer, taken, seen_else)) = stack.last_mut() else {
                    return Err(invalid("unmatched LVGL else"));
                };
                if *seen_else {
                    return Err(invalid("duplicate LVGL else"));
                }
                active = *outer && !*taken;
                *taken = true;
                *seen_else = true;
            }
            "elif" => {
                let Some((outer, taken, seen_else)) = stack.last_mut() else {
                    return Err(invalid("unmatched LVGL elif"));
                };
                if *seen_else {
                    return Err(invalid("LVGL elif after else"));
                }
                let take = *outer && !*taken && condition(arg, &defines, b)?;
                active = take;
                *taken |= take;
            }
            "endif" => {
                let Some((outer, _, _)) = stack.pop() else {
                    return Err(invalid("unmatched LVGL endif"));
                };
                active = outer;
            }
            _ => return Err(invalid("unsupported LVGL preprocessor directive")),
        }
    }
    if !stack.is_empty() {
        return Err(invalid("unterminated LVGL conditional"));
    }
    Ok(out)
}
