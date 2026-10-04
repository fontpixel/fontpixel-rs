//! Reconstructed W6.5 selection implementation; currently untested.
use crate::charset::merge_categories::{CJK_RANGES, GEOMETRIC_RANGES, is_scalar};
use crate::{Error, Result};
use std::{fmt, str::FromStr};

/// Inclusive Unicode scalar range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CodepointRange {
    /// First included scalar.
    pub start: u32,
    /// Last included scalar.
    pub end: u32,
}
/// Union of selected ranges, with exclusions applied last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    expression: String,
    ranges: Vec<CodepointRange>,
}
const ALL: [CodepointRange; 2] = [
    CodepointRange {
        start: 0,
        end: 0xd7ff,
    },
    CodepointRange {
        start: 0xe000,
        end: 0x10ffff,
    },
];
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn normalize(mut ranges: Vec<CodepointRange>) -> Vec<CodepointRange> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut out: Vec<CodepointRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = out.last_mut()
            && range.start <= last.end + 1
        {
            last.end = last.end.max(range.end);
            continue;
        }
        out.push(range);
    }
    out
}
pub(super) fn subtract(
    source: &[CodepointRange],
    excluded: &[CodepointRange],
) -> Vec<CodepointRange> {
    let mut result = Vec::new();
    let mut index = 0;
    for &range in source {
        let mut cursor = range.start;
        while index < excluded.len() && excluded[index].end < cursor {
            index += 1;
        }
        let mut i = index;
        while i < excluded.len() && excluded[i].start <= range.end {
            let cut = excluded[i];
            if cursor < cut.start {
                result.push(CodepointRange {
                    start: cursor,
                    end: cut.start - 1,
                });
            }
            cursor = cursor.max(cut.end + 1);
            if cursor > range.end {
                break;
            }
            i += 1;
        }
        if cursor <= range.end {
            result.push(CodepointRange {
                start: cursor,
                end: range.end,
            });
        }
    }
    result
}
fn atom(term: &str) -> Result<Vec<CodepointRange>> {
    let ranges = match term.to_ascii_lowercase().as_str() {
        "all" => ALL.to_vec(),
        "cjk" => CJK_RANGES
            .iter()
            .map(|r| CodepointRange {
                start: r.start,
                end: r.end,
            })
            .collect(),
        "geometric" => GEOMETRIC_RANGES
            .iter()
            .map(|r| CodepointRange {
                start: r.start,
                end: r.end,
            })
            .collect(),
        "western" => subtract(&ALL, &atom("cjk")?),
        _ => {
            let value = term
                .strip_prefix("U+")
                .or_else(|| term.strip_prefix("u+"))
                .ok_or_else(|| invalid(format!("unknown merge selection {term:?}")))?;
            let mut fields = value.split('-');
            let parse = |text: &str| -> Result<u32> {
                let text = text
                    .strip_prefix("U+")
                    .or_else(|| text.strip_prefix("u+"))
                    .unwrap_or(text);
                if text.is_empty() || text.len() > 6 || !text.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(invalid(format!("invalid Unicode range in {term:?}")));
                }
                u32::from_str_radix(text, 16)
                    .map_err(|_| invalid(format!("invalid Unicode range in {term:?}")))
            };
            let start = parse(fields.next().unwrap_or_default())?;
            let end = fields.next().map(parse).transpose()?.unwrap_or(start);
            if fields.next().is_some() || start > end || !is_scalar(start) || !is_scalar(end) {
                return Err(invalid(format!("invalid Unicode scalar range {term:?}")));
            }
            subtract(
                &[CodepointRange { start, end }],
                &[CodepointRange {
                    start: 0xd800,
                    end: 0xdfff,
                }],
            )
        }
    };
    Ok(ranges)
}
impl Selection {
    /// Parse categories/ranges; 64 KiB and 1,024 terms maximum.
    pub fn parse(expression: &str) -> Result<Self> {
        if expression.len() > 65536 {
            return Err(Error::ResourceLimit {
                resource: "merge selection bytes",
                limit: 65536,
            });
        }
        if expression.split(',').count() > 1024 {
            return Err(Error::ResourceLimit {
                resource: "merge selection terms",
                limit: 1024,
            });
        }
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        let mut positive = false;
        for term in expression.split(',').map(str::trim) {
            let (exclude, name) = term
                .strip_prefix('-')
                .map_or((false, term), |v| (true, v.trim()));
            if name.is_empty() {
                return Err(invalid("merge selection contains an empty term"));
            }
            if exclude {
                excluded.extend(atom(name)?);
            } else {
                positive = true;
                included.extend(atom(name)?);
            }
        }
        if !positive {
            included.extend(ALL);
        }
        let ranges = subtract(&normalize(included), &normalize(excluded));
        Ok(Self {
            expression: expression.trim().into(),
            ranges,
        })
    }
    /// Whether this scalar is selected.
    pub fn contains(&self, codepoint: u32) -> bool {
        let index = self.ranges.partition_point(|range| range.end < codepoint);
        self.ranges
            .get(index)
            .is_some_and(|range| range.start <= codepoint)
    }
    /// Normalized disjoint intervals.
    pub fn ranges(&self) -> &[CodepointRange] {
        &self.ranges
    }
    /// Original expression without outer whitespace.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    /// Exact count, including selected unassigned positions.
    pub fn scalar_count(&self) -> u64 {
        self.ranges
            .iter()
            .map(|range| u64::from(range.end - range.start) + 1)
            .sum()
    }
}
impl FromStr for Selection {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}
impl fmt::Display for Selection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.expression)
    }
}
