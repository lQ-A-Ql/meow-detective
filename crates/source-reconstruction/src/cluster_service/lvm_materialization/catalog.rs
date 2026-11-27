use std::sync::{atomic::AtomicBool, Arc};

use domain::DataSource;
use evidence_access::{file_service, lvm};

use super::LvmPlanError;

pub(super) fn enumerate_lvm_catalog(
    source: &DataSource,
    source_conn: &rusqlite::Connection,
    cancel: &AtomicBool,
) -> Result<(), LvmPlanError> {
    let probe = lvm::probe_lvm_filesystems(&source.source_path, cancel)?;
    if probe.candidates.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "LVM logical volume has no readable filesystem",
        )
        .into());
    }
    let workers = enumeration_workers(&source.source_path, cancel)?;
    persistence_source::detail::open_store_for_write(source_conn)?;
    let storage = source_kernel::source_db::open_fjall_source_for_build(source_conn, &source.id)?;
    evidence_access::partition_storage_service::replace_from_probe(
        &storage,
        &source.id,
        &probe.partitions,
        probe.gpt_disk_guid.as_deref(),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    for candidate in &probe.candidates {
        let index = candidate
            .partition_index
            .ok_or_else(|| std::io::Error::other("LVM filesystem partition identity is missing"))?;
        let name = import_core::import_pipeline::partition::format_partition_root_name(candidate);
        let root = file_service::insert_partition_placeholder_root(
            source_conn,
            &source.id,
            index,
            &name,
            "pending",
        )?;
        let descriptor = source.source_path.clone();
        let candidate = candidate.clone();
        let factory = Arc::new(move || {
            lvm::open_lvm_filesystem(&descriptor, &candidate)
                .map(|filesystem| filesystem as Box<dyn evidence_core::FileSystemReader>)
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
        )?;
        if result.incomplete_catalog_diagnostic_count() != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "LVM filesystem enumeration is incomplete",
            )
            .into());
        }
    }
    Ok(())
}

fn enumeration_workers(
    descriptor_path: &std::path::Path,
    cancel: &AtomicBool,
) -> Result<usize, LvmPlanError> {
    let plan = lvm::read_lvm_descriptor(descriptor_path)?;
    let pool = source_kernel::resource_budget::global_pool();
    let used = pool.snapshot().0;
    let parse = source_kernel::source_runtime::resources::raw_parse_request();
    let metadata_workers = pool.limits().metadata.saturating_sub(used.metadata) / parse.metadata;
    let memory_workers = pool
        .limits()
        .bytes
        .saturating_sub(used.bytes)
        .saturating_sub(source_kernel::resource_budget::ENUMERATION_BYTES)
        / parse.bytes;
    let workers = source_kernel::resource_budget::default_cpu_budget()
        .clamp(1, 4)
        .min(metadata_workers.min(memory_workers).max(1) as usize);
    let readers = plan.sources.len() as u64;
    let _permit = source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: source_kernel::resource_budget::ENUMERATION_BYTES
                    + lvm::lvm_reader_content_bytes(&plan).saturating_mul(workers as u64 + 1),
                readers: readers * (workers as u64 + 1),
                workers: workers as u64,
                catalogs: 1,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(workers)
}
