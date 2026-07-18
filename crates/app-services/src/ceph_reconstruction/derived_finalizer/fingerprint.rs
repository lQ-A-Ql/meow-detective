use domain::DataSourceId;
use persistence_sqlite::{
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseState,
    },
    DbError,
};
use sha2::{Digest, Sha256};

pub(super) const PROCESSING_PHASE_VERSION: u32 = 1;
const SOURCE_SCHEMA_015: &str = "source_015_ceph_bluestore_rbd_header_context";
const SOURCE_SCHEMA_016: &str = "source_016_file_partition_index";
pub(super) const CATALOG_POLICY_VERSION: &str = "rbd-filesystem-catalog-v2-xfs-macb";

pub(super) fn phase_input_fingerprint(seed: &str, phase: ProcessingPhase) -> String {
    phase_input_fingerprint_with_contract(
        seed,
        phase,
        phase_schema_dependency(phase),
        phase_policy_version(phase),
    )
}

pub(super) fn phase_input_fingerprint_with_contract(
    seed: &str,
    phase: ProcessingPhase,
    schema_dependency: &str,
    policy_version: &str,
) -> String {
    let mut hasher = Sha256::new();
    update_field(&mut hasher, b"derived-rbd-processing-phase");
    update_field(&mut hasher, seed.as_bytes());
    update_field(&mut hasher, schema_dependency.as_bytes());
    update_field(&mut hasher, phase.as_str().as_bytes());
    update_field(&mut hasher, &PROCESSING_PHASE_VERSION.to_le_bytes());
    update_field(&mut hasher, policy_version.as_bytes());
    hex::encode(hasher.finalize())
}

pub(super) fn phase_dependency_identity(label: &str, dependencies: &[&str]) -> String {
    let mut hasher = Sha256::new();
    update_field(&mut hasher, b"derived-rbd-phase-dependencies");
    update_field(&mut hasher, label.as_bytes());
    for dependency in dependencies {
        update_field(&mut hasher, dependency.as_bytes());
    }
    hex::encode(hasher.finalize())
}

pub(super) fn ready_phase_output_identity(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    phase: ProcessingPhase,
    identity_seed: &str,
) -> Result<Option<String>, DbError> {
    let expected_fingerprint = phase_input_fingerprint(identity_seed, phase);
    let Some(record) = DataSourceProcessingPhaseRepo::new(case_conn).find(data_source_id, phase)?
    else {
        return Ok(None);
    };
    if record.state != ProcessingPhaseState::Ready
        || record.version != PROCESSING_PHASE_VERSION
        || record.input_fingerprint != expected_fingerprint
    {
        return Ok(None);
    }

    let mut hasher = Sha256::new();
    update_field(&mut hasher, b"derived-rbd-phase-output");
    update_field(&mut hasher, phase.as_str().as_bytes());
    update_field(&mut hasher, record.input_fingerprint.as_bytes());
    update_field(&mut hasher, record.stats_json.as_bytes());
    Ok(Some(hex::encode(hasher.finalize())))
}

pub(super) fn load_catalog_identity(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    lineage_fingerprint: &str,
) -> Result<String, DbError> {
    let expected_fingerprint =
        phase_input_fingerprint(lineage_fingerprint, ProcessingPhase::Catalog);
    let record = DataSourceProcessingPhaseRepo::new(case_conn)
        .find(data_source_id, ProcessingPhase::Catalog)?
        .ok_or_else(|| DbError::System("derived-source catalog phase is missing".to_string()))?;
    if record.state != ProcessingPhaseState::Ready {
        return Err(DbError::System(format!(
            "derived-source catalog phase is not ready: {}",
            record.state
        )));
    }
    if record.version != PROCESSING_PHASE_VERSION
        || record.input_fingerprint != expected_fingerprint
    {
        return Err(DbError::System(
            "derived-source catalog identity is stale".to_string(),
        ));
    }

    let mut hasher = Sha256::new();
    update_field(&mut hasher, b"derived-rbd-catalog-identity");
    update_field(&mut hasher, lineage_fingerprint.as_bytes());
    update_field(&mut hasher, record.input_fingerprint.as_bytes());
    update_field(&mut hasher, record.stats_json.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

fn phase_policy_version(phase: ProcessingPhase) -> &'static str {
    match phase {
        ProcessingPhase::Catalog => CATALOG_POLICY_VERSION,
        ProcessingPhase::Graph => "source-file-graph-v1",
        ProcessingPhase::Platform => "registered-platform-v1",
        ProcessingPhase::Artifacts => crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
        ProcessingPhase::Timeline => "macb-only-timeline-v2",
        ProcessingPhase::Search => "budgeted-text-index-v2",
    }
}

pub(super) fn phase_schema_dependency(phase: ProcessingPhase) -> &'static str {
    match phase {
        ProcessingPhase::Catalog => SOURCE_SCHEMA_015,
        ProcessingPhase::Graph
        | ProcessingPhase::Platform
        | ProcessingPhase::Artifacts
        | ProcessingPhase::Timeline
        | ProcessingPhase::Search => SOURCE_SCHEMA_016,
    }
}

pub(in crate::ceph_reconstruction) fn phase_input_fingerprint_for_catalog(seed: &str) -> String {
    phase_input_fingerprint(seed, ProcessingPhase::Catalog)
}

pub(in crate::ceph_reconstruction) fn catalog_phase_is_current(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    lineage_fingerprint: &str,
) -> Result<bool, DbError> {
    let expected_fingerprint =
        phase_input_fingerprint(lineage_fingerprint, ProcessingPhase::Catalog);
    let record = DataSourceProcessingPhaseRepo::new(case_conn)
        .find(data_source_id, ProcessingPhase::Catalog)?;
    Ok(record.is_some_and(|record| {
        record.state == ProcessingPhaseState::Ready
            && record.version == PROCESSING_PHASE_VERSION
            && record.input_fingerprint == expected_fingerprint
    }))
}

fn update_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
