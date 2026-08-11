//! Boot-path risk annotations for Linux emulation preflight: the GPT/ESP
//! fallback analysis and the XFS dirty-log detection. Both annotate the
//! preflight install list with `boot_risk_notes` consumed by the UI.

use domain::DataSourceId;
use evidence_core::FileSystemReader;
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use transport::dto::EmulationInstallDto;

use super::emulation::EvidenceContext;
use super::emulation_linux::open_linux_volume_reader;
use crate::datasource_service::open_evidence_reader;

/// XFS log annotation for the boot path: a volume captured with pending
/// log transactions blocks the RHEL/CentOS GRUB builds before the kernel is
/// even reached. Every install on the disk is annotated when ANY XFS volume
/// is dirty — the separate-/boot layout makes a non-root XFS volume just as
/// boot-critical. Reads stay bounded by the snapshot limit and failures are
/// silent (no evidence, no annotation).
pub(crate) fn annotate_xfs_log_risk(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    partitions: &PartitionRepo<'_>,
    data_source_id: &DataSourceId,
    installs: &mut [EmulationInstallDto],
) {
    use fs_xfs::log::{assess_log_state, XfsLogState, XFS_LOG_MAX_SNAPSHOT_BYTES};

    if installs.is_empty() {
        return;
    }
    let context = EvidenceContext {
        source_path: source_path.to_path_buf(),
        kind: source_kind.clone(),
    };
    let records = partitions
        .find_by_data_source(&data_source_id.0)
        .unwrap_or_default();
    let mut any_dirty = false;
    for record in records
        .iter()
        .filter(|record| record.filesystem.as_deref() == Some("XFS"))
    {
        let Some(reader) = open_linux_volume_reader(&context, record) else {
            continue;
        };
        let Ok(xfs) = fs_xfs::XfsReader::open(reader, 0) else {
            continue;
        };
        let dirty = xfs
            .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
            .map(|snapshot| assess_log_state(&snapshot) == XfsLogState::Dirty)
            .unwrap_or(false);
        if dirty {
            any_dirty = true;
            break;
        }
    }
    if !any_dirty {
        return;
    }
    for install in installs.iter_mut() {
        if !install
            .boot_risk_notes
            .iter()
            .any(|note| note == "xfs-log-dirty")
        {
            install.boot_risk_notes.push("xfs-log-dirty".to_string());
        }
    }
}

/// Disk-level boot-path annotation for GPT images. A fresh VM has an empty
/// NVRAM, so a GPT disk can only boot through GRUB's BIOS boot partition or
/// the ESP fallback loader `\EFI\BOOT\BOOTX64.EFI`; when neither exists the
/// firmware drops into its boot manager, which the `no-efi-fallback` note
/// surfaces to the investigator (the session-level EFI fallback installation
/// remediates it). MBR disks boot legacy and need no annotation.
pub(crate) fn annotate_boot_path_risk(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    installs: &mut [EmulationInstallDto],
) {
    if installs.is_empty() {
        return;
    }
    let missing = gpt_disk_missing_boot_paths(source_path, source_kind);
    if missing != Some(true) {
        return;
    }
    for install in installs.iter_mut() {
        if !install
            .boot_risk_notes
            .iter()
            .any(|note| note == "no-efi-fallback")
        {
            install.boot_risk_notes.push("no-efi-fallback".to_string());
        }
    }
}

/// `Some(true)` when the disk is GPT and neither a BIOS boot partition nor an
/// ESP fallback loader exists; `None` when the disk layout could not be
/// determined (no annotation without evidence).
fn gpt_disk_missing_boot_paths(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
) -> Option<bool> {
    use evidence_core::volume::gpt::{
        classify_partition_type, parse_gpt_entries, parse_gpt_header, GptPartitionType,
    };
    use std::io::{Read, Seek, SeekFrom};

    const BIOS_BOOT_PARTITION: [u8; 16] = *b"Hah!IdontNeedEFI";
    let mut reader = open_evidence_reader(source_path, source_kind).ok()?;
    let mut header = [0u8; 512];
    reader.seek(SeekFrom::Start(512)).ok()?;
    reader.read_exact(&mut header).ok()?;
    let header = parse_gpt_header(&header)?;
    let count = header.partition_count.min(4096);
    let entry_size = header.entry_size.clamp(128, 4096);
    let byte_len = count as usize * entry_size as usize;
    reader
        .seek(SeekFrom::Start(
            header.partition_entry_lba.checked_mul(512)?,
        ))
        .ok()?;
    let mut entries = vec![0u8; byte_len];
    reader.read_exact(&mut entries).ok()?;
    let partitions = parse_gpt_entries(&entries, entry_size, count);
    if partitions
        .iter()
        .any(|partition| partition.type_guid == BIOS_BOOT_PARTITION)
    {
        return Some(false);
    }
    let esp = partitions.iter().find(|partition| {
        classify_partition_type(&partition.type_guid) == GptPartitionType::EfiSystem
    })?;
    let esp_offset = esp.start_lba.checked_mul(512)?;
    let esp_length = esp
        .end_lba
        .checked_sub(esp.start_lba)?
        .checked_add(1)?
        .checked_mul(512)?;
    let window =
        evidence_core::PartitionWindowReader::new(reader, esp_offset, Some(esp_length)).ok()?;
    let fs = fs_fat::FatReader::open(Box::new(window), 0).ok()?;
    Some(fs.open_file("EFI/BOOT/BOOTX64.EFI").is_err())
}
