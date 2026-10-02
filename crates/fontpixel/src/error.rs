//! Typed operation failures; unsuccessful writes never return an output bundle.
use crate::Report;
use std::{fmt, io};

/// Result type used by the library.
pub type Result<T> = std::result::Result<T, Error>;

/// Capability being requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Content recognition.
    Detect,
    /// Decode source assets into the model.
    Read,
    /// Encode the model into output assets.
    Write,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Detect => "detect",
            Self::Read => "read",
            Self::Write => "write",
        })
    }
}

/// Structured library error. New format-specific variants may be added.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Unknown format identifier.
    UnknownFormat(String),
    /// Requested format was compiled out by Cargo features.
    FeatureDisabled {
        /// Format identifier.
        format: String,
    },
    /// The feature exists, but the requested implementation does not yet exist.
    NotImplemented {
        /// Format identifier.
        format: String,
        /// Missing capability.
        operation: Operation,
    },
    /// Multiple equally strong candidates; an explicit format is required.
    AmbiguousFormat {
        /// Candidate identifiers in deterministic order.
        candidates: Vec<String>,
    },
    /// No registered detector or filename hint recognized the input.
    UnrecognizedFormat,
    /// Invalid options, registration, asset paths, or data.
    InvalidInput(String),
    /// Configured resource bound exceeded.
    ResourceLimit {
        /// Resource category.
        resource: &'static str,
        /// Configured maximum.
        limit: usize,
    },
    /// Referenced sidecar was not supplied by the caller.
    MissingResource(String),
    /// Multiple output assets cannot be represented by a byte-vector API.
    MultipleAssets {
        /// Total asset count.
        count: usize,
    },
    /// Strict mode rejected lossy conversion before handing over output assets.
    StrictLoss(Report),
    /// A codec returned an error diagnostic instead of an explicit error.
    ReportedErrors(Report),
    /// An operating-system or caller-supplied resource I/O error.
    Io(io::Error),
}

impl Error {
    /// CLI exit convention: strict loss = 3, all other library failures = 1.
    pub fn exit_code(&self) -> u8 {
        if matches!(self, Self::StrictLoss(_)) {
            3
        } else {
            1
        }
    }

    /// Preserve diagnostics attached to a failed conversion.
    pub fn report(&self) -> Option<&Report> {
        match self {
            Self::StrictLoss(report) | Self::ReportedErrors(report) => Some(report),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFormat(id) => write!(f, "unknown format '{id}'"),
            Self::FeatureDisabled { format } => write!(
                f,
                "format '{format}' is disabled; enable the '{format}' Cargo feature"
            ),
            Self::NotImplemented { format, operation } => {
                write!(f, "{operation} for format '{format}' is not implemented")
            }
            Self::AmbiguousFormat { candidates } => write!(
                f,
                "ambiguous format ({}); specify --from/--to explicitly",
                candidates.join(", ")
            ),
            Self::UnrecognizedFormat => {
                f.write_str("format not recognized; specify --from explicitly")
            }
            Self::InvalidInput(message) => f.write_str(message),
            Self::ResourceLimit { resource, limit } => {
                write!(f, "{resource} exceeds configured limit {limit}")
            }
            Self::MissingResource(path) => write!(f, "missing input resource '{path}'"),
            Self::MultipleAssets { count } => {
                write!(f, "output has {count} assets; use the output-bundle API")
            }
            Self::StrictLoss(_) => {
                f.write_str("strict mode rejected information loss; no output assets returned")
            }
            Self::ReportedErrors(_) => f.write_str("operation reported errors; no output returned"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(error) = self {
            Some(error)
        } else {
            None
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
