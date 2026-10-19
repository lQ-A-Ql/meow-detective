//! Signed portable program updates. Evidence and case databases are outside this API.
mod error;
mod executable;
mod helper;
mod process;
mod transaction;
mod verification;
pub use error::UpdateError;
pub use executable::{read_metadata, validate_candidate, METADATA_MAGIC};
pub use helper::run_if_requested;
pub use transaction::reject_links;
pub use transaction::{apply, clean_workspace, recover, rollback_pending};
pub use verification::{hash_file, verify_signature};
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const CASE_SCHEMA: &str = env!("MEOW_CASE_SCHEMA");
pub const PUBLIC_KEY: &str = match option_env!("MEOW_UPDATER_PUBLIC_KEY") {
    Some(key) => key,
    None => "",
};
