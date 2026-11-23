//! Shared image-container inspection and bounded statistical image analysis.
//!
//! The crate is deliberately independent of the application: it accepts the
//! complete file bytes, decodes them row by row, and returns a triage report.
//! It never writes files or database rows.

mod analyze;
mod decode;
mod limits;
mod scan;
mod stats;
mod types;
mod warnings;

/// Shared BMP parser used by metadata inspection and pixel analysis.
pub use tinybmp as bmp;

pub use analyze::analyze;
pub use limits::StegLimits;
pub use types::{
    PlaneKind, StegAssessment, StegBand, StegBlockReport, StegFormat, StegMethod, StegMethodReport,
    StegPlaneReport, StegReport, StegSeverity, StegStatus, StegWarning,
};

#[cfg(test)]
#[path = "../tests/unit/unit_tests.rs"]
mod tests;
