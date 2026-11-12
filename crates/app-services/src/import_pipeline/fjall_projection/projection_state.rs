use persistence_source::fjall::{encode_key, KeyPart, PersistMode, VersionedEnvelope};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use transport::CommandError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AnalysisProjectionRevision {
    pub(super) source_id: String,
    pub(super) artifact_revision: u64,
    pub(super) artifact_high_water: i64,
    pub(super) timeline_revision: u64,
    pub(super) timeline_high_water: i64,
    #[serde(default)]
    pub(super) change_high_water: u64,
    #[serde(default)]
    pub(super) change_log_initialized: bool,
}

pub(super) fn publish_projection_marker(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
    include_catalog: bool,
    analysis_revision: Option<&AnalysisProjectionRevision>,
) -> Result<String, CommandError> {
    let (family, kind) = if include_catalog {
        ("projection", "source_projection_complete")
    } else {
        ("analysis_projection", "analysis_projection_complete")
    };
    let marker = match analysis_revision {
        Some(revision) => VersionedEnvelope::from_serializable(
            kind,
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            revision,
        ),
        None => Ok(VersionedEnvelope::new(
            kind,
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            source_id.as_bytes().to_vec(),
        )),
    }
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let key = encode_key(family, &[KeyPart::Text(source_id.into())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut receipt = source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if include_catalog {
        let revision = analysis_revision
            .ok_or_else(|| CommandError::internal("Import projection revision is missing"))?;
        let marker = VersionedEnvelope::from_serializable(
            "analysis_projection_complete",
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            revision,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        let key = encode_key("analysis_projection", &[KeyPart::Text(source_id.into())])
            .map_err(|error| CommandError::internal(error.to_string()))?;
        receipt = source
            .write_envelope(
                persistence_source::fjall::KeyspaceName::Meta,
                key,
                &marker,
                PersistMode::SyncAll,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    if analysis_revision.is_some() {
        write_analysis_projection_state(source, source_id, "analysis_projection_complete")?;
    }
    Ok(hex::encode(receipt.batch_digest))
}

pub(super) fn analysis_projection_revision(
    source_connection: &Connection,
    source_id: &str,
) -> Result<AnalysisProjectionRevision, CommandError> {
    let artifact_repo =
        persistence_source::repositories::artifact_repo::ArtifactRepo::new(source_connection);
    let artifact_revision = artifact_repo
        .cursor_revision()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let artifact_high_water = artifact_repo
        .snapshot_high_water()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let timeline_repo =
        persistence_source::repositories::timeline_repo::TimelineRepo::new(source_connection);
    let timeline_revision = timeline_repo
        .cursor_revision()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let timeline_high_water = timeline_repo
        .snapshot_high_water()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let change_high_water = artifact_revision.max(timeline_revision);
    Ok(AnalysisProjectionRevision {
        source_id: source_id.to_owned(),
        artifact_revision,
        artifact_high_water,
        timeline_revision,
        timeline_high_water,
        change_high_water,
        change_log_initialized: false,
    })
}

pub(super) fn analysis_projection_marker(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<Option<AnalysisProjectionRevision>, CommandError> {
    let key = encode_key(
        "analysis_projection",
        &[KeyPart::Text(source_id.to_owned())],
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let snapshot = source.snapshot();
    let Some(envelope) = source
        .read_envelope(
            &snapshot,
            persistence_source::fjall::KeyspaceName::Meta,
            &key,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?
    else {
        return Ok(None);
    };
    if envelope.record_kind != "analysis_projection_complete" || envelope.source_id != source_id {
        return Ok(None);
    }
    envelope
        .decode_payload()
        .map(Some)
        .map_err(|error| CommandError::internal(error.to_string()))
}

pub(super) fn ensure_analysis_projection_building(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    ensure_projection_building(source, source_id, "analysis_projection_state")
}

pub(super) fn write_analysis_projection_state(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
    record_kind: &str,
) -> Result<(), CommandError> {
    let key = encode_key(
        "analysis_projection_state",
        &[KeyPart::Text(source_id.to_owned())],
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let marker = VersionedEnvelope::new(
        record_kind,
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

pub(super) fn ensure_projection_building(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
    family: &str,
) -> Result<(), CommandError> {
    let building_kind = if family == "projection" {
        "source_projection_building"
    } else {
        "analysis_projection_building"
    };
    let key = encode_key(family, &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let snapshot = source.snapshot();
    let current = source
        .read_envelope(
            &snapshot,
            persistence_source::fjall::KeyspaceName::Meta,
            &key,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if current.is_some_and(|envelope| {
        envelope.record_kind == building_kind && envelope.source_id == source_id
    }) {
        return Ok(());
    }
    let marker = VersionedEnvelope::new(
        building_kind,
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

pub(super) fn catalog_projection_is_published(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<bool, CommandError> {
    let snapshot = source.snapshot();
    let key = encode_key("projection", &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(source
        .read_envelope(
            &snapshot,
            persistence_source::fjall::KeyspaceName::Meta,
            &key,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?
        .is_some_and(|envelope| {
            (envelope.record_kind == "source_projection_complete"
                || envelope.record_kind == "catalog_projection_seeded")
                && envelope.source_id == source_id
        }))
}
