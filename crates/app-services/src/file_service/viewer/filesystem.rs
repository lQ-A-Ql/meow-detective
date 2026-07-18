use std::io::{Read, Seek, SeekFrom};

use persistence_sqlite::repositories::file_repo::FileRepo;

use crate::file_service::FileServiceError;

pub fn mft_partition_index_from_entry_id(entry_id: &str) -> Option<usize> {
    let mut parts = entry_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("mft"), Some(partition), Some(_record), None) => partition.parse().ok(),
        _ => None,
    }
}

pub(crate) fn resolve_partition_index_for_entry(
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
) -> Result<Option<usize>, FileServiceError> {
    if let Some(index) = repo.find_partition_index_by_id(&entry.id)? {
        return Ok(Some(index));
    }

    if let Some(index) = mft_partition_index_from_entry_id(&entry.id.0) {
        tracing::warn!(
            file_id = %entry.id.0,
            partition_index = index,
            "Preview routing is using the legacy MFT identifier fallback"
        );
        return Ok(Some(index));
    }

    let mut current = entry.clone();
    while let Some(parent_id) = &current.parent_id {
        let Some(parent) = repo.find_by_id(parent_id)? else {
            return Ok(None);
        };
        current = parent;
    }

    let resolved = current.name.strip_prefix("Partition ").and_then(|suffix| {
        suffix
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()
    });
    if let Some(index) = resolved {
        tracing::warn!(
            file_id = %entry.id.0,
            root_name = %current.name,
            partition_index = index,
            "Preview routing is using the legacy partition-root name fallback"
        );
    }
    Ok(resolved)
}

pub(crate) fn format_image_range_error(
    path: &str,
    reasons: &[String],
    fallback_error: Option<&str>,
) -> String {
    const MAX_REASONS: usize = 8;
    const MAX_REASON_LEN: usize = 120;
    const MAX_PATH_LEN: usize = 80;
    let display_path = bounded_text(path, MAX_PATH_LEN);
    let mut summary = reasons
        .iter()
        .take(MAX_REASONS)
        .map(|reason| bounded_text(reason, MAX_REASON_LEN))
        .collect::<Vec<_>>()
        .join("; ");
    if reasons.len() > MAX_REASONS {
        summary.push_str(&format!("; and {} more", reasons.len() - MAX_REASONS));
    }
    match fallback_error {
        Some(fallback) => format!(
            "Cannot open image-backed file '{}' from any partition. Attempts: {}. Fallback error: {}",
            display_path, summary, fallback
        ),
        None => format!(
            "Cannot open image-backed file '{}' from any partition. Attempts: {}",
            display_path, summary
        ),
    }
}

fn bounded_text(value: &str, max_len: usize) -> String {
    if value.len() > max_len {
        format!("{}...", &value[..max_len])
    } else {
        value.to_string()
    }
}

pub(crate) fn is_fat_filesystem_kind(kind: &str) -> bool {
    matches!(kind, "FAT" | "FAT32" | "FAT16" | "FAT12")
}

pub(crate) fn is_exfat_filesystem_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("exfat") || kind.to_ascii_uppercase().contains("EXFAT")
}

pub(crate) fn is_linux_filesystem_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("ext4")
        || kind.eq_ignore_ascii_case("xfs")
        || kind.eq_ignore_ascii_case("btrfs")
}

pub(crate) fn is_preview_image_filesystem_kind(kind: &str) -> bool {
    kind == "NTFS"
        || is_fat_filesystem_kind(kind)
        || is_exfat_filesystem_kind(kind)
        || is_linux_filesystem_kind(kind)
}

pub(crate) fn looks_like_exfat_boot_sector<R>(reader: &mut R, offset: u64) -> std::io::Result<bool>
where
    R: Read + Seek + ?Sized,
{
    let mut sector = [0u8; 512];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut sector)?;
    Ok(&sector[3..11] == b"EXFAT   " && sector[510] == 0x55 && sector[511] == 0xAA)
}
