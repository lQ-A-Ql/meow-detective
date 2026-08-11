use std::path::PathBuf;

use domain::{CaseId, DataSourceId};
use evidence_core::FileSystemReader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo, partition_repo::PartitionRepo,
};
use rusqlite::Connection;
use transport::dto::{EmulationBootRouteDto, EmulationInstallDto, EmulationPreflightDto};

use super::{prepare_physical_mount_source, MountServiceError, PreparedPhysicalImageKind};
use crate::source_db::{self, ReadySourceError};

/// Upper bound for the partition scan; real images have a handful of
/// partitions and indices may be sparse when system partitions are skipped.
const MAX_PREFLIGHT_PARTITIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedEmulationSource {
    pub source_path: PathBuf,
    pub image_kind: PreparedPhysicalImageKind,
    pub parent_sha256: [u8; 32],
}

pub fn prepare_emulation_source(
    case_conn: &Connection,
    data_source_id: &DataSourceId,
) -> Result<PreparedEmulationSource, MountServiceError> {
    let prepared = prepare_physical_mount_source(case_conn, data_source_id)?;
    let parent_sha256 = parse_source_sha256(&prepared.source_binding)?;
    Ok(PreparedEmulationSource {
        source_path: prepared.source_path,
        image_kind: prepared.image_kind,
        parent_sha256,
    })
}

/// Read-only pre-flight: locates the operating system installations and
/// their bypass feasibility. The probe branches on the data source platform
/// recorded at import: Windows installs are located from their registry
/// hives (OSDATA/SAM/utilman), Linux installs from `/etc/os-release`. An
/// `unknown` platform is probed for Windows first, then Linux — the
/// persisted platform is never rewritten.
pub fn emulation_preflight(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<EmulationPreflightDto, MountServiceError> {
    prepare_physical_mount_source(case_conn, data_source_id)?;
    let source =
        source_db::open_ready_source_read_only_by_id(case_conn, case_root, case_id, data_source_id)
            .map_err(preflight_source_error)?;
    let platform = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .map(|storage| storage.platform)
        .unwrap_or_default();
    let repo = FileRepo::new(&source.connection);
    match platform.as_str() {
        "linux" => linux_preflight(case_conn, &source, &repo, data_source_id),
        "windows" => windows_preflight(case_conn, &source, &repo, data_source_id),
        _ => {
            let mut dto = windows_preflight(case_conn, &source, &repo, data_source_id)?;
            if dto.installs.is_empty() {
                dto = linux_preflight(case_conn, &source, &repo, data_source_id)?;
            }
            Ok(dto)
        }
    }
}

fn windows_preflight(
    case_conn: &Connection,
    source: &crate::source_db::ReadySourceConnection,
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
) -> Result<EmulationPreflightDto, MountServiceError> {
    let mut installs = Vec::new();
    for partition_index in 0..MAX_PREFLIGHT_PARTITIONS {
        // Partition indices are not guaranteed dense: an import may skip
        // system partitions, so a missing root must not end the scan.
        if repo
            .find_root_for_partition(data_source_id, partition_index)?
            .is_none()
        {
            continue;
        }
        if !path_present(
            repo,
            data_source_id,
            partition_index,
            "Windows/System32/config/SYSTEM",
        )? {
            continue;
        }
        let utilman = path_present(
            repo,
            data_source_id,
            partition_index,
            "Windows/System32/utilman.exe",
        )?;
        let cmd = path_present(
            repo,
            data_source_id,
            partition_index,
            "Windows/System32/cmd.exe",
        )?;
        installs.push(EmulationInstallDto {
            partition_index: partition_index as u32,
            platform: transport::dto::EmulationInstallPlatformDto::Windows,
            osdata_present: path_present(
                repo,
                data_source_id,
                partition_index,
                "Windows/System32/config/OSDATA",
            )?,
            sam_present: path_present(
                repo,
                data_source_id,
                partition_index,
                "Windows/System32/config/SAM",
            )?,
            utilman_bypass_available: utilman && cmd,
            osdata_empty: None,
            os_release_pretty_name: None,
            kernel_present: None,
            fstab_present: None,
            boot_risk_notes: Vec::new(),
        });
    }
    let partition_count =
        PartitionRepo::new(&source.connection).count_by_data_source(&data_source_id.0)?;
    if partition_count as usize > MAX_PREFLIGHT_PARTITIONS {
        tracing::warn!(
            partition_count,
            max = MAX_PREFLIGHT_PARTITIONS,
            "emulation preflight partition scan truncated"
        );
    }
    enrich_installs_from_fs(case_conn, data_source_id, source, &mut installs);
    let recommended_boot_route = if installs.iter().any(|install| install.osdata_present) {
        EmulationBootRouteDto::RecoveryMedia
    } else {
        EmulationBootRouteDto::DirectSystem
    };
    Ok(EmulationPreflightDto {
        data_source_id: data_source_id.0.clone(),
        installs,
        recommended_boot_route,
        maintenance_tool_available: false,
    })
}

/// Linux pre-flight: catalog detection plus a filesystem enrichment that
/// reads the distro identity and boot prerequisites. Direct boot is always
/// the recommended route — no OSDATA-style blocker exists and the
/// maintenance CD is a Windows PE concept.
fn linux_preflight(
    case_conn: &Connection,
    source: &crate::source_db::ReadySourceConnection,
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
) -> Result<EmulationPreflightDto, MountServiceError> {
    let mut installs = super::emulation_linux::linux_installs_from_catalog(repo, data_source_id)?;
    let partitions = PartitionRepo::new(&source.connection);
    let partition_count = partitions.count_by_data_source(&data_source_id.0)?;
    if partition_count as usize > super::emulation_linux::MAX_LINUX_PRECHECK_PARTITIONS {
        tracing::warn!(
            partition_count,
            max = super::emulation_linux::MAX_LINUX_PRECHECK_PARTITIONS,
            "linux preflight partition scan truncated"
        );
    }
    let ds_repo = DataSourceRepo::new(case_conn);
    match ds_repo
        .source_path(data_source_id)
        .and_then(|path| ds_repo.source_kind(data_source_id).map(|kind| (path, kind)))
    {
        Ok((path, kind)) => {
            super::emulation_linux::enrich_linux_installs_from_fs(
                std::path::Path::new(&path),
                &kind,
                &partitions,
                data_source_id,
                &mut installs,
            );
            super::emulation_linux_boot::annotate_boot_path_risk(
                std::path::Path::new(&path),
                &kind,
                &mut installs,
            );
            super::emulation_linux_boot::annotate_xfs_log_risk(
                std::path::Path::new(&path),
                &kind,
                &partitions,
                data_source_id,
                &mut installs,
            );
        }
        Err(error) => {
            tracing::warn!(error = %error, "linux preflight: source metadata unavailable");
        }
    }
    Ok(EmulationPreflightDto {
        data_source_id: data_source_id.0.clone(),
        installs,
        recommended_boot_route: EmulationBootRouteDto::DirectSystem,
        maintenance_tool_available: false,
    })
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

/// The file catalog is inode-keyed: hard-linked system binaries such as
/// `cmd.exe` and `Utilman.exe` appear only under their WinSxS payload name,
/// and OSDATA/SAM probes can likewise miss, so a catalog miss must fall back
/// to the NTFS directory index (ground truth) before declaring a capability
/// absent. The same listing also decides whether a present OSDATA directory
/// is empty. Failures here are non-fatal — the catalog verdict stands.
fn enrich_installs_from_fs(
    case_conn: &Connection,
    data_source_id: &DataSourceId,
    source: &crate::source_db::ReadySourceConnection,
    installs: &mut [EmulationInstallDto],
) {
    if installs.is_empty() {
        return;
    }
    let repo = DataSourceRepo::new(case_conn);
    let (source_path, source_kind) = match repo
        .source_path(data_source_id)
        .and_then(|path| repo.source_kind(data_source_id).map(|kind| (path, kind)))
    {
        Ok(parts) => parts,
        Err(error) => {
            tracing::warn!(error = %error, "install fs fallback: source metadata unavailable");
            return;
        }
    };
    let context = repo_context(&source_path, &source_kind);
    let partitions = PartitionRepo::new(&source.connection);
    for install in installs.iter_mut() {
        let Some(probe) = probe_install_from_fs(
            &context,
            &partitions,
            data_source_id,
            install.partition_index,
        ) else {
            tracing::warn!(
                partition_index = install.partition_index,
                "install fs fallback: partition could not be probed"
            );
            continue;
        };
        install.utilman_bypass_available |= probe.utilman_bypass_available;
        install.osdata_present |= probe.osdata_present;
        install.sam_present |= probe.sam_present;
        if install.osdata_present {
            install.osdata_empty = probe.osdata_empty;
        }
    }
}

pub(crate) struct EvidenceContext {
    pub(crate) source_path: std::path::PathBuf,
    pub(crate) kind: domain::DataSourceKind,
}

fn repo_context(source_path: &str, source_kind: &domain::DataSourceKind) -> EvidenceContext {
    EvidenceContext {
        source_path: PathBuf::from(source_path),
        kind: source_kind.clone(),
    }
}

/// Ground-truth capability probe from the NTFS directory index.
struct FsInstallProbe {
    utilman_bypass_available: bool,
    osdata_present: bool,
    /// `Some` only when OSDATA exists as a directory whose children could be
    /// listed.
    osdata_empty: Option<bool>,
    sam_present: bool,
}

/// Returns `None` when the partition is unavailable for a filesystem-level
/// probe.
fn probe_install_from_fs(
    context: &EvidenceContext,
    partitions: &PartitionRepo<'_>,
    data_source_id: &DataSourceId,
    partition_index: u32,
) -> Option<FsInstallProbe> {
    let record = partitions
        .find_by_data_source_and_index(&data_source_id.0, partition_index as usize)
        .ok()??;
    let reader =
        crate::datasource_service::open_evidence_reader(&context.source_path, &context.kind)
            .ok()?;
    let length = (record.length > 0).then_some(record.length);
    let window = evidence_core::PartitionWindowReader::new(reader, record.offset, length).ok()?;
    let fs = fs_ntfs::NtfsReader::open(Box::new(window), 0).ok()?;
    let system32 = fs.list_children("Windows/System32").ok()?;
    let config = fs.list_children("Windows/System32/config").ok()?;
    let present = |nodes: &[evidence_core::FsNode], name: &str| {
        nodes
            .iter()
            .any(|node| !node.is_dir && node.name.eq_ignore_ascii_case(name))
    };
    let osdata_node = config
        .iter()
        .find(|node| node.name.eq_ignore_ascii_case("OSDATA"));
    let osdata_empty = match osdata_node {
        Some(node) if node.is_dir => fs
            .list_children("Windows/System32/config/OSDATA")
            .ok()
            .map(|children| children.is_empty()),
        _ => None,
    };
    Some(FsInstallProbe {
        utilman_bypass_available: present(&system32, "utilman.exe")
            && present(&system32, "cmd.exe"),
        osdata_present: osdata_node.is_some(),
        osdata_empty,
        sam_present: present(&config, "SAM"),
    })
}

fn preflight_source_error(error: ReadySourceError) -> MountServiceError {
    match error {
        ReadySourceError::Db(error) => MountServiceError::Database(error),
        ReadySourceError::NotFound { .. } => MountServiceError::NotFound("data source".to_string()),
        other => MountServiceError::SourceNotReady(other.to_string()),
    }
}

fn parse_source_sha256(value: &str) -> Result<[u8; 32], MountServiceError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MountServiceError::InvalidSourceFingerprint);
    }
    let decoded = hex::decode(value).map_err(|_| MountServiceError::InvalidSourceFingerprint)?;
    decoded
        .try_into()
        .map_err(|_| MountServiceError::InvalidSourceFingerprint)
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/emulation.rs"]
mod tests;
