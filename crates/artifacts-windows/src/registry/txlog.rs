//! Registry transaction log (.LOG1 / .LOG2) parsing and merge orchestration.

mod merge;
mod parser;
mod types;

pub(crate) use merge::parse_and_merge_txlogs;
pub use parser::parse_transaction_log;
pub use types::{RegistryTransaction, RegistryTransactionOperation, TxLogParseResult};

#[cfg(test)]
#[path = "../../tests/unit/registry/txlog.rs"]
mod tests;
