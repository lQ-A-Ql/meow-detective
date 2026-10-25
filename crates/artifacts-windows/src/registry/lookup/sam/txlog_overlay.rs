use super::extraction::extract_sam_fields;
use crate::registry::{RegistryError, RegistrySnapshot};

/// Extract SAM records after applying validated LOG1/LOG2 pages.
pub fn extract_sam_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> Result<super::super::SamInfo, RegistryError> {
    let snapshot = RegistrySnapshot::prepare(bytes, txlog1, txlog2)?;
    let mut info = extract_sam_fields(snapshot.bytes(), hive_path, boot_key)?;
    info.txlog_applied = snapshot.logs_applied();
    Ok(info)
}
