//! Read-only Windows DPAPI primitives used by offline browser extraction.
//!
//! This module is deliberately byte-oriented. It does not open files, query
//! registries, or emit decrypted material to logs; callers own evidence I/O
//! and decide which user/profile context is valid.

mod algorithms;
mod app_bound;
mod blob;
mod chrome;
mod error;
mod master_key;

#[cfg(test)]
pub(crate) use app_bound::{
    content_requires_cng, parse_cng_private_key, unwrap_app_bound_key, unwrap_direct_key_blob,
    AppBoundScheme, CHROME_147_XOR_CONSTANT, KNOWN_APP_BOUND_KEYS,
};
pub use app_bound::{
    parse_chrome_key_blob, parse_cng_system_key_file, ChromeKeyBlob, CngSystemKeyFile,
};
pub use blob::{parse_dpapi_blob, DpapiBlob};
pub(crate) use chrome::BrowserDecryption;
pub use chrome::{ChromiumDecryptor, ChromiumFamily};
pub use error::DpapiError;
pub use master_key::{
    decrypt_master_key_file, derive_user_prekeys, derive_user_prekeys_from_password_sha1,
    parse_masterkey_file, DecryptedMasterKey, MasterKeyFile,
};

#[cfg(test)]
#[path = "../../tests/unit/dpapi_app_bound.rs"]
mod tests;
