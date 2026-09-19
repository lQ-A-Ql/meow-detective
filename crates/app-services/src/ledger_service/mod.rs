//! Read, verify, and seal the local forensic ledger.

mod dto_conversion;
pub mod error;
mod operations;

pub use error::LedgerServiceError;
pub use operations::{get_proof, get_snapshot, seal_next_batch};

#[cfg(test)]
#[path = "../../tests/unit/ledger_service/mod.rs"]
mod tests;
