//! Offline LSA secret decryption (NT6 / Vista+ format) from the SECURITY hive.
//!
//! The decryption chain mirrors the documented offline format: BootKey ->
//! `Policy\PolEKList` -> LSA key -> per-secret `CurrVal` values. Secret bytes
//! stay wrapped in [`zeroize::Zeroizing`] and are never logged.

mod keys;
mod secrets;
mod types;

pub use secrets::{decrypt_lsa_secrets, LsaDecryptedSecret, LsaDecryptedSecrets};
pub use types::{DpapiSystemKeys, TbalSecret};

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lsa.rs"]
mod tests;
