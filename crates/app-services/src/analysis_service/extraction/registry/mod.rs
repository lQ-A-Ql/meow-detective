mod amcache;
mod appcompat;
mod context;
mod dispatch;
mod entry;
mod extractors;
mod ntuser;
mod sam;
mod security;
mod shared;
mod software;
mod system;
mod txlog;
mod usrclass;
mod warnings;

use super::ExtractionOutcome;

pub use entry::extract_registry_candidate;

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/registry/mod.rs"]
mod tests;
