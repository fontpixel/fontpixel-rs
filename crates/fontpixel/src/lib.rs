//! Infrastructure for a bitmap-font library under development.
//!
//! No built-in font reader, writer, bitmap model, or charset table is implemented
//! yet. The catalogue reports planned formats separately from actual capabilities.
//! [`Registry`] is generic over the model, so the phase-1 `FontSet` can be added
//! without introducing an empty pretend font type at the skeleton stage.
#![forbid(unsafe_code)]

pub mod error;
pub mod formats;
pub mod report;
pub mod resources;

pub use error::{Error, Operation, Result};
pub use formats::{ReadOptions, Registry, WriteOptions};
pub use report::{Report, ReportEntry};
pub use resources::{
    Input, OutputAsset, OutputBundle, ResourceLimits, ResourcePath, ResourceResolver,
};
