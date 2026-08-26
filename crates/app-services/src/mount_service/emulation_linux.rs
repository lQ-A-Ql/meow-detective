//! Linux installation probing for the emulation preflight.
//!
//! Detection runs against the import-built file catalog first
//! (`/etc/os-release`), then each found install is enriched from the
//! filesystem itself: distro identity, bootable kernel, fstab and init.
//! LVM logical volumes are read through `fs-lvm`'s `LvReader`. The probe
//! feeds the VMware guestOS mapping and the boot-risk annotations shown in
//! the UI. Probing is deliberately lenient: a doubtful install is listed
//! with risk notes rather than silently dropped.

use domain::DataSourceId;
use evidence_core::{EvidenceReader, FileSystemReader};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    file_repo::FileRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use rusqlite::Connection;
use transport::dto::{EmulationInstallDto, EmulationInstallPlatformDto};

use super::emulation::EvidenceContext;
use super::emulation_linux_controller::LinuxControllerEvidence;
use super::MountServiceError;
use crate::datasource_service::open_evidence_reader;

pub(crate) const MAX_LINUX_PRECHECK_PARTITIONS: usize = 64;

/// Catalog-only detection: a partition holding `/etc/os-release` (or the
/// `/usr/lib/os-release` fallback location) is a Linux system root
/// candidate. Fields that need file contents stay empty here and are filled
/// by the filesystem enrichment.
pub(crate) fn linux_installs_from_catalog(
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
) -> Result<Vec<EmulationInstallDto>, MountServiceError> {
    let mut installs = Vec::new();
    for partition_index in 0..MAX_LINUX_PRECHECK_PARTITIONS {
        if repo
            .find_root_for_partition(data_source_id, partition_index)?
            .is_none()
        {
            continue;
        }
        let has_os_release = path_present(repo, data_source_id, partition_index, "etc/os-release")?
            || path_present(repo, data_source_id, partition_index, "usr/lib/os-release")?;
        if !has_os_release {
            continue;
        }
        installs.push(linux_install_skeleton(partition_index as u32));
    }
    Ok(installs)
}

pub(crate) fn linux_install_skeleton(partition_index: u32) -> EmulationInstallDto {
    EmulationInstallDto {
        partition_index,
        platform: EmulationInstallPlatformDto::Linux,
        osdata_present: false,
        sam_present: false,
        utilman_bypass_available: false,
        osdata_empty: None,
        os_release_pretty_name: None,
        kernel_present: None,
        fstab_present: None,
        boot_risk_notes: Vec::new(),
    }
}

fn path_present(
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
    partition_index: usize,
    path: &str,
) -> Result<bool, MountServiceError> {
    Ok(repo
        .find_by_partition_and_path_ci(data_source_id, partition_index, path)?
        .is_some())
}

/// Ground-truth enrichment from the filesystem. Non-fatal per install: a
/// partition that cannot be opened keeps its catalog-only verdict. A
/// separate /boot partition is the standard server layout, so a root
/// without its own kernel image is only flagged when no other partition of
/// the image carries one either.
pub(crate) fn enrich_linux_installs_from_fs(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    partitions: &PartitionRepo<'_>,
    data_source_id: &DataSourceId,
    installs: &mut [EmulationInstallDto],
) {
    let context = EvidenceContext {
        source_path: source_path.to_path_buf(),
        kind: source_kind.clone(),
    };
    for install in installs.iter_mut() {
        match probe_linux_install(
            &context,
            partitions,
            data_source_id,
            install.partition_index,
        ) {
            Some(probe) => {
                install.os_release_pretty_name = probe.pretty_name;
                install.kernel_present = Some(probe.kernel_present);
                install.fstab_present = Some(probe.fstab_present);
                install.boot_risk_notes = probe.risk_notes;
            }
            None => {
                tracing::warn!(
                    partition_index = install.partition_index,
                    "linux install fs probe: partition could not be opened"
                );
            }
        }
    }
    if installs
        .iter()
        .all(|install| install.kernel_present != Some(false))
    {
        return;
    }
    // Second pass: a kernel on ANY Linux partition satisfies the boot
    // requirement (separate /boot partition layout).
    let all_partitions = partitions
        .find_by_data_source(&data_source_id.0)
        .unwrap_or_default();
    let kernel_elsewhere = all_partitions.iter().any(|record| {
        if installs
            .iter()
            .any(|install| install.partition_index == record.partition_index)
        {
            return false;
        }
        open_linux_fs(&context, record)
            .map(|fs| {
                // A dedicated boot partition carries the kernels at its own
                // root; a shared root carries them under /boot.
                let at_boot = fs.list_children("boot").unwrap_or_default();
                let at_root = fs.list_children("").unwrap_or_default();
                at_boot.iter().chain(at_root.iter()).any(|node| {
                    !node.is_dir
                        && (node.name.starts_with("vmlinuz") || node.name.starts_with("bzImage"))
                })
            })
            .unwrap_or(false)
    });
    if kernel_elsewhere {
        for install in installs.iter_mut() {
            if install.kernel_present == Some(false) {
                install.kernel_present = Some(true);
                install.boot_risk_notes.retain(|note| note != "no-kernel");
            }
        }
    }
}

pub(crate) struct LinuxFsProbe {
    pretty_name: Option<String>,
    kernel_present: bool,
    fstab_present: bool,
    risk_notes: Vec<String>,
}
fn probe_linux_install(
    context: &EvidenceContext,
    partitions: &PartitionRepo<'_>,
    data_source_id: &DataSourceId,
    partition_index: u32,
) -> Option<LinuxFsProbe> {
    let record = partitions
        .find_by_data_source_and_index(&data_source_id.0, partition_index as usize)
        .ok()??;
    let fs = open_linux_fs(context, &record)?;
    probe_linux_fs_root(fs.as_ref(), record.filesystem.as_deref().unwrap_or(""))
}

/// Probe an opened Linux root filesystem: distro identity, bootable kernel,
/// fstab and init presence, plus structured boot-risk annotations.
pub(crate) fn probe_linux_fs_root(
    fs: &dyn FileSystemReader,
    fs_label: &str,
) -> Option<LinuxFsProbe> {
    let etc = fs.list_children("etc").ok()?;
    // /sbin/init is a symlink chain on usrmerge/systemd layouts; check the
    // physical target too in case the reader does not follow symlinks.
    let has_init = has_file(fs, "sbin/init")
        || has_file(fs, "lib/systemd/systemd")
        || has_file(fs, "usr/lib/systemd/systemd")
        || has_file(fs, "bin/init");
    let pretty_name = read_os_release_pretty_name(fs);
    let kernel_present = fs
        .list_children("boot")
        .map(|children| {
            children.iter().any(|node| {
                !node.is_dir
                    && (node.name.starts_with("vmlinuz") || node.name.starts_with("bzImage"))
            })
        })
        .unwrap_or(false);
    let fstab_present = etc.iter().any(|node| !node.is_dir && node.name == "fstab");
    let mut risk_notes = Vec::new();
    if !kernel_present {
        risk_notes.push("no-kernel".to_string());
    }
    if !fstab_present {
        risk_notes.push("no-fstab".to_string());
    }
    if !has_init {
        risk_notes.push("no-init".to_string());
    }
    if fs_label == "Btrfs" {
        risk_notes.push("btrfs-root".to_string());
    }
    Some(LinuxFsProbe {
        pretty_name,
        kernel_present,
        fstab_present,
        risk_notes,
    })
}

fn has_file(fs: &dyn FileSystemReader, path: &str) -> bool {
    fs.open_file(path).is_ok()
}

fn read_os_release_pretty_name(fs: &dyn FileSystemReader) -> Option<String> {
    for path in ["etc/os-release", "usr/lib/os-release"] {
        if let Ok(content) = fs.read_file_range(path, 0, 64 * 1024) {
            if let Some(name) = parse_os_release_pretty_name(&content) {
                return Some(name);
            }
        }
    }
    None
}

/// Parse `PRETTY_NAME` out of os-release content (shell-like `KEY="value"`
/// lines per the freedesktop specification).
pub(crate) fn parse_os_release_pretty_name(content: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(content);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            let value = value.trim();
            let unquoted = value
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .unwrap_or(value);
            if !unquoted.is_empty() {
                return Some(unquoted.to_string());
            }
        }
    }
    None
}

/// Parse the os-release `ID` field (unquoted lowercase token).
pub(crate) fn parse_os_release_id(content: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(content);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("ID=") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Map an os-release `ID` to the VMware guestid. Unknown or missing distros
/// fall back to the generic 64-bit Linux 5.x profile.
pub fn linux_guest_os_id(distro_id: Option<&str>) -> &'static str {
    match distro_id.unwrap_or("") {
        "ubuntu" => "ubuntu-64",
        "debian" => "debian12-64",
        "rhel" => "rhel8-64",
        "centos" => "centos-64",
        "ol" | "oraclelinux" => "oraclelinux-64",
        _ => "other5xlinux-64",
    }
}

fn open_linux_fs(
    context: &EvidenceContext,
    record: &DataSourcePartitionRecord,
) -> Option<Box<dyn FileSystemReader>> {
    let fs_label = record.filesystem.as_deref()?;
    let reader = open_linux_volume_reader(context, record)?;
    open_fs_by_label(reader, fs_label)
}

/// The volume-level reader beneath the filesystem: a partition window for
/// direct partitions, the LV reader for LVM logical volumes.
pub(super) fn open_linux_volume_reader(
    context: &EvidenceContext,
    record: &DataSourcePartitionRecord,
) -> Option<Box<dyn EvidenceReader>> {
    if record.lvm_lv_name.is_some() {
        return open_lvm_lv_reader(context, record);
    }
    let reader = open_evidence_reader(&context.source_path, &context.kind).ok()?;
    let length = (record.length > 0).then_some(record.length);
    let window = evidence_core::PartitionWindowReader::new(reader, record.offset, length).ok()?;
    Some(Box::new(window))
}

fn open_lvm_lv_reader(
    context: &EvidenceContext,
    record: &DataSourcePartitionRecord,
) -> Option<Box<dyn EvidenceReader>> {
    let pv_offsets: Vec<u64> = record
        .lvm_pv_offsets_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())?;
    if pv_offsets.is_empty() {
        return None;
    }
    // One reader per PV, even when every PV lives on the same disk image.
    let readers = pv_offsets
        .iter()
        .map(|_| {
            open_evidence_reader(&context.source_path, &context.kind)
                .map(|reader| reader as Box<dyn EvidenceReader>)
                .ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let pool = fs_lvm::LvmPool::discover(readers, pv_offsets).ok()?;
    let volumes = pool.list_volumes();
    let lv_index = volumes.iter().position(|volume| {
        Some(volume.name.as_str()) == record.lvm_lv_name.as_deref()
            && Some(volume.uuid.as_str()) == record.lvm_lv_uuid.as_deref()
    })?;
    let lv = pool.open_volume(lv_index).ok()?;
    Some(Box::new(lv))
}

fn open_fs_by_label(
    reader: Box<dyn EvidenceReader>,
    fs_label: &str,
) -> Option<Box<dyn FileSystemReader>> {
    match fs_label {
        "Ext4" => fs_ext4::Ext4Reader::open(reader, 0)
            .ok()
            .map(|fs| Box::new(fs) as Box<dyn FileSystemReader>),
        "XFS" => fs_xfs::XfsReader::open(reader, 0)
            .ok()
            .map(|fs| Box::new(fs) as Box<dyn FileSystemReader>),
        "Btrfs" => fs_btrfs::BtrfsReader::open(reader, 0)
            .ok()
            .map(|fs| Box::new(fs) as Box<dyn FileSystemReader>),
        _ => None,
    }
}

/// The guest profile a Linux data source should boot with: the VMware
/// guestid derived from the distro's os-release ID.
pub fn linux_guest_profile(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    data_source_id: &DataSourceId,
) -> Result<LinuxGuestProfile, MountServiceError> {
    let source = crate::source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )
    .map_err(|error| MountServiceError::SourceNotReady(error.to_string()))?;
    let partitions = PartitionRepo::new(&source.connection);
    let repo = DataSourceRepo::new(case_conn);
    let (source_path, source_kind) = repo
        .source_path(data_source_id)
        .and_then(|path| repo.source_kind(data_source_id).map(|kind| (path, kind)))?;
    let all_partitions = partitions
        .find_by_data_source(&data_source_id.0)
        .unwrap_or_default();
    let context = EvidenceContext {
        source_path: std::path::PathBuf::from(source_path),
        kind: source_kind,
    };
    // Read the distro id and initramfs controller evidence from Linux
    // filesystems. A dedicated /boot partition is included in this scan.
    let mut distro_id = None;
    let mut controller_evidence = LinuxControllerEvidence::default();
    for record in &all_partitions {
        let Some(fs_label) = record.filesystem.as_deref() else {
            continue;
        };
        if !matches!(fs_label, "Ext4" | "XFS" | "Btrfs") {
            continue;
        }
        let Some(fs) = open_linux_fs(&context, record) else {
            continue;
        };
        let current_evidence = super::emulation_linux_controller::inspect_filesystem(fs.as_ref());
        controller_evidence.merge(current_evidence);
        if distro_id.is_none() {
            for path in ["etc/os-release", "usr/lib/os-release"] {
                if let Ok(content) = fs.read_file_range(path, 0, 64 * 1024) {
                    distro_id = parse_os_release_id(&content);
                    if distro_id.is_some() {
                        break;
                    }
                }
            }
        }
        if distro_id.is_some() && controller_evidence.ide_is_decisive() {
            break;
        }
    }
    let controller = controller_evidence.decision();
    Ok(LinuxGuestProfile {
        guest_os: linux_guest_os_id(distro_id.as_deref()).to_string(),
        disk_adapter: controller.adapter,
        disk_adapter_reason: controller.reason.to_string(),
    })
}

pub struct LinuxGuestProfile {
    pub guest_os: String,
    pub disk_adapter: evidence_emulation::VmdkAdapter,
    pub disk_adapter_reason: String,
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/emulation_linux.rs"]
mod tests;
