use super::reader::RegistryHiveReader;
use super::*;
use crate::registry::RegistryError;

mod appcompat;
mod mount_points;
mod recent_files;
mod shell_folders;
mod user_assist;

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lookup/ntuser.rs"]
mod tests;

pub use appcompat::extract_appcompat_layers_from_ntuser_hive;
pub(crate) use recent_files::decode_pidl_path;

// ── NTUSER.DAT field extraction ──────────────────────────────────────────────

pub fn extract_ntuser_fields(
    bytes: &(impl crate::registry::RegistryHiveSource + ?Sized),
    hive_path: &str,
) -> Result<NtuserInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = NtuserInfo::default();
    let parser = "registry.ntuser";

    info.run_keys = recent_files::extract_run_keys(&hive, hive_path, parser, &mut info.warnings);
    info.recent_docs =
        recent_files::extract_recent_docs(&hive, hive_path, parser, &mut info.warnings);
    info.ua_entries =
        user_assist::extract_user_assist(&hive, hive_path, parser, &mut info.warnings);
    info.typed_urls =
        shell_folders::extract_typed_urls(&hive, hive_path, parser, &mut info.warnings);
    info.word_wheel_query =
        shell_folders::extract_word_wheel_query(&hive, hive_path, parser, &mut info.warnings);
    info.mount_points =
        mount_points::extract_mount_points(&hive, hive_path, parser, &mut info.warnings);
    info.open_save_mru =
        recent_files::extract_open_save_mru(&hive, hive_path, parser, &mut info.warnings);
    info.last_visited_mru =
        recent_files::extract_last_visited_mru(&hive, hive_path, parser, &mut info.warnings);
    info.run_mru = recent_files::extract_run_mru(&hive, hive_path, parser, &mut info.warnings);
    info.default_browser = shell_folders::extract_default_browser(&hive);

    Ok(info)
}

/// Extract fields from the page-recovered NTUSER hive.
pub fn extract_ntuser_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<NtuserInfo, RegistryError> {
    let snapshot = crate::registry::RegistrySnapshot::prepare(bytes, Some(txlog_data), None)?;
    let mut info = extract_ntuser_fields(&snapshot, hive_path)?;
    info.txlog_applied = snapshot.logs_applied();
    Ok(info)
}
