mod encoding;
mod event;
pub mod merkle;

pub const LEDGER_SCHEMA_VERSION: &str = "ledger-v1";

pub use event::{ForensicLedgerEvent, LedgerScope, LEDGER_GENESIS_HASH};
pub use merkle::{
    merkle_proof, merkle_root, verify_merkle_proof, MerkleError, MerkleProof, MerkleProofStep,
};
