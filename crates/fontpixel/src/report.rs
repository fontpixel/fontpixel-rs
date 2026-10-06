//! Structured diagnostics shared by reading, writing, and the CLI.
use std::fmt;

/// Diagnostic severity; information loss is tracked independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Level {
    /// Informational note.
    Info,
    /// Recoverable anomaly.
    Warning,
    /// Error recorded before an operation failed.
    Error,
}

/// Effect on source information. Strict mode rejects `Loss` at any severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Impact {
    /// No source information changed.
    None,
    /// Representation repaired without losing information.
    Repair,
    /// Information lost, approximated, or irreversibly changed.
    Loss,
}

/// Optional location; glyph index and codepoint need not both be known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Location {
    /// Strike/font index in a collection.
    pub font_index: Option<usize>,
    /// Original glyph index, including unencoded glyphs.
    pub glyph_index: Option<usize>,
    /// Original code (not assumed to be a Unicode scalar).
    pub codepoint: Option<u32>,
    /// Source asset, if known.
    pub asset: Option<String>,
}

/// A stable machine-readable code plus a human-readable explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ReportEntry {
    /// Severity.
    pub level: Level,
    /// Effect on information.
    pub impact: Impact,
    /// Stable dotted code, e.g. `encoding.out_of_range`.
    pub code: String,
    /// Affected source location.
    pub location: Location,
    /// Human-readable description.
    pub message: String,
}

/// Diagnostics in deterministic encounter order; no generated timestamps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Report {
    /// Entries in source/operation order.
    pub entries: Vec<ReportEntry>,
}

impl Report {
    /// Add an entry without reordering earlier diagnostics.
    pub fn push(&mut self, entry: ReportEntry) {
        self.entries.push(entry);
    }

    /// Append another report, preserving the two encounter orders.
    pub fn extend(&mut self, other: Self) {
        self.entries.extend(other.entries);
    }

    /// Whether the operation dropped or approximated any information.
    pub fn has_loss(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.impact == Impact::Loss)
    }

    /// Whether any entry represents an error.
    pub fn has_errors(&self) -> bool {
        self.entries.iter().any(|entry| entry.level == Level::Error)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in &self.entries {
            writeln!(
                f,
                "{:?} [{}] {:?}: {}",
                entry.level, entry.code, entry.impact, entry.message
            )?;
        }
        Ok(())
    }
}
