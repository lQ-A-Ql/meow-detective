//! Email evidence extraction (EML/EMLX, mbox, PST/OST).
//!
//! This module was split from a single ~1800 line file into focused submodules:
//!
//! - `eml`: single-message EML/EMLX parsing (`parse_email_message`) and extraction
//! - `mbox`: mbox container extraction
//! - `pst`: PST/OST container extraction
//! - `shared`: helpers used by more than one of the above (body preview truncation)

mod dispatch;
mod eml;
mod mbox;
mod pst;
mod shared;

pub(super) use dispatch::extract_email_candidate;

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/email/mod.rs"]
mod tests;
