//! Read-only Windows DPAPI primitives used by offline browser extraction.
//!
//! This module is deliberately byte-oriented. It does not open files, query
//! registries, or emit decrypted material to logs; callers own evidence I/O
//! and decide which user/profile context is valid.

mod algorithms;
mod blob;
mod chrome;
mod error;
mod master_key;

pub use blob::{parse_dpapi_blob, DpapiBlob};
pub use chrome::{BrowserDecryption, ChromiumDecryptor, ChromiumValueKind};
pub use error::DpapiError;
pub use master_key::{
    decrypt_master_key_file, derive_user_prekeys, parse_masterkey_file, DecryptedMasterKey,
    MasterKeyFile,
};
