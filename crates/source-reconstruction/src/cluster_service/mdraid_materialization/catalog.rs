use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

use domain::DataSource;
use evidence_access::{datasource_service::ImageFilesystemKind, file_service};

use super::MdRaidPlanError;

pub(super) fn enumerate_mdraid_catalog(
    _case_root: &Path,
    source: &DataSource,
    source_conn: &rusqlite::Connection,
    cancel: &AtomicBool,
) -> Result<(), MdRaidPlanError> {
    let workers = source_kernel::resource_budget::default_cpu_budget().clamp(1, 4);
    let _budget = acquire_budget(source, workers, cancel)?;
    let probe = evidence_access::mdraid::probe_mdraid_filesystems(&source.source_path)?;
    if !probe
        .candidates
        .iter()
        .any(|candidate| candidate.kind != ImageFilesystemKind::LvmPool)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "MD RAID has no readable filesystem",
        )
        .into());
    }
    persistence_source::detail::open_store_for_write(source_conn)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let storage = source_kernel::source_db::open_fjall_source_for_build(source_conn, &source.id)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    evidence_access::partition_storage_service::replace_from_probe(
        &storage,
        &source.id,
        &probe.partitions,
        probe.gpt_disk_guid.as_deref(),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    for candidate in probe
        .candidates
        .iter()
        .filter(|candidate| candidate.kind != ImageFilesystemKind::LvmPool)
    {
        let index = candidate
            .partition_index
            .ok_or_else(|| std::io::Error::other("MD filesystem partition identity is missing"))?;
        let name = import_core::import_pipeline::partition::format_partition_root_name(candidate);
        let root = file_service::insert_partition_placeholder_root(
            source_conn,
            &source.id,
            index,
            &name,
            "pending",
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        let descriptor = source.source_path.clone();
        let candidate = candidate.clone();
        let factory = Arc::new(move || {
            evidence_access::mdraid::open_mdraid_filesystem(&descriptor, &candidate)
                .map(|fs| fs as Box<dyn evidence_core::FileSystemReader>)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        });
        let result = file_service::enumerate_partition_parallel_with_factory(
            source_conn,
            &root,
            workers,
            factory,
            &name,
            None,
            Some(cancel),
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        if result.incomplete_catalog_diagnostic_count() != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MD filesystem enumeration is incomplete",
            )
            .into());
        }
    }
    Ok(())
}

fn acquire_budget(
    source: &DataSource,
    workers: usize,
    cancel: &AtomicBool,
) -> Result<source_kernel::resource_budget::ResourcePermit, MdRaidPlanError> {
    let plan: evidence_access::mdraid::MdRaidArrayPlan =
        serde_json::from_slice(&std::fs::read(&source.source_path)?)?;
    let physical_readers = plan
        .members
        .iter()
        .map(|member| match &member.origin {
            evidence_access::mdraid::MdRaidMemberOrigin::Direct { .. } => 1,
            evidence_access::mdraid::MdRaidMemberOrigin::Lvm { identity, .. } => {
                identity.pv_sources.len() as u64
            }
        })
        .sum::<u64>();
    source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: source_kernel::resource_budget::ENUMERATION_BYTES,
                readers: physical_readers * (workers as u64 + 1),
                workers: workers as u64,
                catalogs: 1,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(|error| std::io::Error::other(error.to_string()).into())
}
