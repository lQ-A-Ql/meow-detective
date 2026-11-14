use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourcePlatform,
    DataSourceProvenance, DataSourceProvenanceStatus,
};

use super::linux_import::LinuxEvidenceSetImportPlan;
use ::evidence_access::mdraid::{
    discover_linux_evidence_set_mdraid, open_mdraid_reader, open_persisted_mdraid_reader,
    write_mdraid_descriptor, MdRaidArrayPlan, MdRaidDiscoveryMember, MdRaidPlanError,
};
use persistence_source::fjall::{
    encode_key, KeyPart, KeyspaceName, PartitionRecord, PartitionRecordStore, PersistMode,
    VersionedEnvelope,
};
use sha2::{Digest, Sha256};

pub fn materialize_mdraid_array(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
) -> Result<DataSource, MdRaidPlanError> {
    let cancel_token = AtomicBool::new(false);
    materialize_mdraid_array_with_cancel(conn, case_root, case_id, plan, &cancel_token)
}

fn materialize_mdraid_array_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
    cancel_token: &AtomicBool,
) -> Result<DataSource, MdRaidPlanError> {
    if cancel_token.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    let source = register_mdraid_array(conn, case_root, case_id, plan)?;
    materialize_mdraid_source_catalog_with_cancel(conn, case_root, &source.id, cancel_token)?;
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
        let source_db_path = source_kernel::source_db::source_db_path(case_root, &source_id);
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
    let cancel_token = AtomicBool::new(false);
    materialize_mdraid_source_catalog_with_cancel(conn, case_root, source_id, &cancel_token)
}

pub fn materialize_mdraid_source_catalog_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
    cancel_token: &AtomicBool,
) -> Result<(), MdRaidPlanError> {
    let repo = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
    let (source, case_id) = load_mdraid_source(&repo, source_id)?;
    let source_conn = source_kernel::source_db::open_source_db(case_root, &source.id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&case_id, &source)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    if mdraid_catalog_is_consumable(&repo, &source_conn, case_root, &source) {
        return Ok(());
    }
    if cancel_token.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    if let Err(error) = enumerate_mdraid_catalog(&source, &source_conn, cancel_token) {
        if cancel_token.load(Ordering::Acquire) {
            let _ = repo.update_import_state(&source.id, "pending", Some("cancelled"));
            return Err(MdRaidPlanError::Cancelled);
        }
        let message = error.to_string();
        let _ = repo.update_import_state(&source.id, "failed", Some(&message));
        return Err(MdRaidPlanError::Io(std::io::Error::other(message)));
    }
    if cancel_token.load(Ordering::Acquire) {
        let _ = repo.update_import_state(&source.id, "pending", Some("cancelled"));
        return Err(MdRaidPlanError::Cancelled);
    }
    publish_mdraid_catalog_generation(&source, &source_conn)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    source_kernel::source_db::checkpoint_source_db(&source_conn)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error.to_string())))?;
    repo.update_import_state(&source.id, "ready", None)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
    Ok(())
}

fn mdraid_catalog_is_consumable(
    repo: &persistence_sqlite::repositories::datasource_repo::DataSourceRepo<'_>,
    source_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source: &DataSource,
) -> bool {
    let state_ready = repo
        .find_storage(&source.id)
        .ok()
        .flatten()
        .is_some_and(|storage| storage.import_state.eq_ignore_ascii_case("ready"));
    let location_present = persistence_source::repositories::file_repo::FileRepo::new(source_conn)
        .find_data_source_location(&source.id)
        .ok()
        .flatten()
        .is_some();
    let projection_complete =
        source_kernel::source_db::open_registered_fjall_source_read_only(case_root, &source.id)
            .ok()
            .is_some_and(|native| {
                let snapshot = native.snapshot();
                native
                    .source_projection_complete(&snapshot, &source.id.0)
                    .unwrap_or(false)
            });
    state_ready && location_present && projection_complete
}

fn publish_mdraid_catalog_generation(
    source: &DataSource,
    source_conn: &rusqlite::Connection,
) -> Result<(), String> {
    let store = persistence_source::detail::open_store_for_write(source_conn)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MD RAID source detail storage is missing".to_string())?;
    let partition = PartitionRecord {
        id: "partition:0".to_string(),
        data_source_id: source.id.0.clone(),
        partition_index: 0,
        name: "Btrfs root".to_string(),
        kind_label: "MD RAID".to_string(),
        status: "supported".to_string(),
        type_guid: None,
        offset: 0,
        length: source.provenance.evidence_size.unwrap_or_default(),
        filesystem: Some("Btrfs".to_string()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
        gpt_disk_guid: None,
        gpt_partition_uuid: None,
        filesystem_uuid: None,
    };
    PartitionRecordStore::from_source_store((*store).clone())
        .replace_by_source(&source.id.0, &[partition], PersistMode::Buffer)
        .map_err(|error| error.to_string())?;
    let (keyspace, key, marker) =
        evidence_access::partition_storage_service::partition_projection_marker(&source.id.0)
            .map_err(|error| error.to_string())?;
    store
        .write_envelope(keyspace, key, &marker, PersistMode::Buffer)
        .map_err(|error| error.to_string())?;
    let projection_key = encode_key("projection", &[KeyPart::Text(source.id.0.clone())])
        .map_err(|error| error.to_string())?;
    let projection_marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source.id.0),
        source.id.0.clone(),
        1,
        source.id.0.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            KeyspaceName::Meta,
            projection_key,
            &projection_marker,
            PersistMode::Buffer,
        )
        .map_err(|error| error.to_string())?;
    store
        .database()
        .persist(PersistMode::SyncAll)
        .map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    digest.update(b"mdraid-btrfs-catalog-v1");
    digest.update(source.id.0.as_bytes());
    digest.update(
        source
            .provenance
            .evidence_size
            .unwrap_or_default()
            .to_le_bytes(),
    );
    persistence_source::fjall::replace_source_generation(
        &store,
        &source.id.0,
        "mdraid-btrfs-catalog-v1",
        &hex::encode(digest.finalize()),
    )
    .map_err(|error| error.to_string())?;
    persistence_source::fjall::verify_source_generation_with_control(
        &store,
        &source.id.0,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .map_err(|error| error.to_string())?;
    persistence_source::detail::bind_generation(source_conn, "source.fjall")
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_mdraid_source(
    repo: &persistence_sqlite::repositories::datasource_repo::DataSourceRepo<'_>,
    source_id: &DataSourceId,
) -> Result<(DataSource, CaseId), MdRaidPlanError> {
    let case_id = repo
        .case_id(source_id)
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error)))?;
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
    Ok((source, case_id))
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
    let _budget = source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: source_kernel::resource_budget::ENUMERATION_BYTES,
                readers: worker_count as u64,
                workers: worker_count as u64,
                catalogs: 1,
                ..Default::default()
            },
            Some(cancel_token),
        )
        .map_err(|error| MdRaidPlanError::Io(std::io::Error::other(error.to_string())))?;
    evidence_access::file_service::enumerate_filesystem_parallel_with_factory(
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

fn discover_evidence_set_arrays(
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    let members = plan
        .members
        .iter()
        .map(|member| MdRaidDiscoveryMember {
            source_path: &member.source_path,
            source_kind: &member.source_kind,
        })
        .collect::<Vec<_>>();
    discover_linux_evidence_set_mdraid(&plan.root_path, &members)
}

pub fn materialize_linux_evidence_set_mdraid(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<DataSource>, MdRaidPlanError> {
    let cancel_token = AtomicBool::new(false);
    materialize_linux_evidence_set_mdraid_with_cancel(conn, case_root, case_id, plan, &cancel_token)
}

pub fn materialize_linux_evidence_set_mdraid_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LinuxEvidenceSetImportPlan,
    cancel_token: &AtomicBool,
) -> Result<Vec<DataSource>, MdRaidPlanError> {
    if cancel_token.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    discover_evidence_set_arrays(plan)?
        .iter()
        .map(|array| {
            materialize_mdraid_array_with_cancel(conn, case_root, case_id, array, cancel_token)
        })
        .collect()
}
