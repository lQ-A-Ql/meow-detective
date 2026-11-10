use std::sync::{Arc, Mutex};

use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourcePlatform,
    DataSourceProvenance, DataSourceProvenanceStatus,
};

use super::linux_import::LinuxEvidenceSetImportPlan;
use super::mdraid::{
    discover_linux_evidence_set_mdraid, open_mdraid_reader, open_persisted_mdraid_reader,
    write_mdraid_descriptor, MdRaidArrayPlan, MdRaidPlanError,
};

pub fn materialize_mdraid_array(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
) -> Result<DataSource, MdRaidPlanError> {
    let source = register_mdraid_array(conn, case_root, case_id, plan)?;
    materialize_mdraid_source_catalog(conn, case_root, &source.id)?;
    Ok(source)
}

fn register_mdraid_array(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
) -> Result<DataSource, MdRaidPlanError> {
    let descriptor_path = write_mdraid_descriptor(case_root, plan)?;
    let source_id = DataSourceId(format!("mdraid-{}", hex::encode(plan.array_uuid)));
    let repo = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
    let sources = repo
        .find_by_case(case_id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    if let Some(existing) = sources
        .into_iter()
        .find(|source| source.id == source_id && source.kind == DataSourceKind::MdRaid)
    {
        let state = repo
            .find_storage(&source_id)
            .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?
            .map(|storage| storage.import_state);
        if state
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("pending"))
            || state
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("ready"))
        {
            return Ok(existing);
        }
        let source_db_path = crate::source_db::source_db_path(case_root, &source_id);
        for suffix in ["", "-wal", "-shm"] {
            let path = if suffix.is_empty() {
                source_db_path.clone()
            } else {
                std::path::PathBuf::from(format!("{}{}", source_db_path.display(), suffix))
            };
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(MdRaidPlanError::Io(error)),
            }
        }
        repo.delete_cascade_with_audit(&source_id, "Rebuild incomplete MD RAID catalog")
            .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    }
    let logical_size = open_mdraid_reader(plan)?.logical_size();
    let level_label = if plan.level == -1 {
        "linear".to_string()
    } else {
        plan.level.to_string()
    };
    let source = DataSource {
        id: source_id.clone(),
        name: format!("MD RAID{} {}", level_label, hex::encode(plan.array_uuid)),
        kind: DataSourceKind::MdRaid,
        source_path: descriptor_path.clone(),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: None,
            hash_status: DataSourceHashStatus::Unavailable,
            canonical_source_path: std::fs::canonicalize(&descriptor_path).ok(),
            evidence_size: Some(logical_size),
            reader_kind: Some("md_raid".to_string()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: Vec::new(),
        },
    };
    let storage = persistence_sqlite::repositories::datasource_repo::DataSourceStorage::source_db(
        &source.id.0,
        Some(DataSourcePlatform::Linux.as_storage_str()),
        Some("mdraid".to_string()),
    );
    repo.insert_with_storage(case_id, &source, &storage)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    Ok(source)
}

pub fn materialize_mdraid_source_catalog(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
) -> Result<(), MdRaidPlanError> {
    let cancel_token = std::sync::atomic::AtomicBool::new(false);
    materialize_mdraid_source_catalog_with_cancel(conn, case_root, source_id, &cancel_token)
}

pub fn materialize_mdraid_source_catalog_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
    cancel_token: &std::sync::atomic::AtomicBool,
) -> Result<(), MdRaidPlanError> {
    let repo = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
    let source = load_mdraid_source(&repo, case_root, source_id)?;
    if source.1 {
        return Ok(());
    }
    let source_conn = crate::source_db::open_source_db(case_root, &source.0.id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    if let Err(error) = enumerate_mdraid_catalog(&source.0, &source_conn, cancel_token) {
        let message = error.to_string();
        let _ = repo.update_import_state(&source.0.id, "failed", Some(&message));
        return Err(MdRaidPlanError::Io(std::io::Error::other(message)));
    }
    crate::source_db::checkpoint_source_db(&source_conn)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error.to_string())))?;
    repo.update_import_state(&source.0.id, "ready", None)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    Ok(())
}

fn load_mdraid_source(
    repo: &persistence_sqlite::repositories::datasource_repo::DataSourceRepo<'_>,
    _case_root: &std::path::Path,
    source_id: &DataSourceId,
) -> Result<(DataSource, bool), MdRaidPlanError> {
    let case_id = repo
        .case_id(source_id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    let ready = repo
        .find_storage(source_id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?
        .is_some_and(|storage| storage.import_state.eq_ignore_ascii_case("ready"));
    let source = repo
        .find_by_case(&case_id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?
        .into_iter()
        .find(|source| source.id == *source_id && source.kind == DataSourceKind::MdRaid)
        .ok_or_else(|| {
            MdRaidPlanError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("MD RAID data source '{}' was not found", source_id.0),
            ))
        })?;
    Ok((source, ready))
}

fn enumerate_mdraid_catalog(
    source: &DataSource,
    source_conn: &rusqlite::Connection,
    cancel_token: &std::sync::atomic::AtomicBool,
) -> Result<(), MdRaidPlanError> {
    let started = std::time::Instant::now();
    let descriptor_path = source.source_path.clone();
    let reader = open_persisted_mdraid_reader(&descriptor_path)?;
    let filesystem = fs_btrfs::BtrfsReader::open(
        Box::new(reader) as Box<dyn evidence_core::EvidenceReader>,
        0,
    )
    .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    if filesystem.has_unreplayed_log() {
        tracing::warn!(
            data_source_id = %source.id.0,
            "Btrfs log tree is not replayed; committed filesystem trees are used"
        );
    }
    let reader_factory: Arc<
        dyn Fn() -> persistence_sqlite::DbResult<Box<dyn evidence_core::FileSystemReader>>
            + Send
            + Sync,
    > = Arc::new(move || {
        let reader = open_persisted_mdraid_reader(&descriptor_path)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        fs_btrfs::BtrfsReader::open(
            Box::new(reader) as Box<dyn evidence_core::EvidenceReader>,
            0,
        )
        .map(|filesystem| Box::new(filesystem) as Box<dyn evidence_core::FileSystemReader>)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
    });
    let progress_state = Mutex::new((started, 0u32));
    let progress = |progress_pct: u32| {
        if let Ok(mut state) = progress_state.lock() {
            let elapsed = state.0.elapsed();
            if progress_pct != state.1 || elapsed.as_secs() >= 5 {
                tracing::info!(
                    data_source_id = %source.id.0,
                    progress_pct,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "Persisted MD RAID Btrfs file tree enumeration progress"
                );
                state.0 = std::time::Instant::now();
                state.1 = progress_pct;
            }
        }
    };
    let worker_count = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(2)
        .clamp(2, 4);
    let _budget = crate::resource_budget::global_pool()
        .acquire(
            crate::resource_budget::Resources {
                bytes: crate::resource_budget::ENUMERATION_BYTES,
                readers: worker_count as u64,
                workers: worker_count as u64,
                catalogs: 1,
                ..Default::default()
            },
            Some(cancel_token),
        )
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error.to_string())))?;
    crate::file_service::enumerate_filesystem_parallel_with_factory(
        source_conn,
        &source.id,
        worker_count,
        reader_factory,
        None,
        Some(&progress),
        Some(cancel_token),
    )
    .map(|_| ())
    .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error.to_string())))
}

pub fn register_linux_evidence_set_mdraid(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<DataSource>, MdRaidPlanError> {
    discover_linux_evidence_set_mdraid(plan)?
        .iter()
        .map(|array| register_mdraid_array(conn, case_root, case_id, array))
        .collect()
}

pub fn materialize_linux_evidence_set_mdraid(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<DataSource>, MdRaidPlanError> {
    discover_linux_evidence_set_mdraid(plan)?
        .iter()
        .map(|array| materialize_mdraid_array(conn, case_root, case_id, array))
        .collect()
}
