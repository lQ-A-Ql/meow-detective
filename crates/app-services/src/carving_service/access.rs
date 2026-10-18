use domain::{CaseId, DataSourceId, DataSourceKind};
use evidence_core::SharedReaderFactory;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo,
};
use rusqlite::Connection;
use std::path::Path;
use transport::{dto::CarvedArtifactDto, CommandError};

pub(super) fn decimal(value: &str) -> Result<u64, CommandError> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(CommandError::invalid_input("Invalid decimal byte address"));
    }
    value
        .parse()
        .map_err(|_| CommandError::invalid_input("Byte address overflow"))
}
pub(crate) fn source_revision(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    source: &str,
) -> Result<String, CommandError> {
    let source_meta = DataSourceRepo::new(conn)
        .find_by_case(case)
        .map_err(CommandError::from_service_error)?
        .into_iter()
        .find(|entry| entry.id.0 == source)
        .ok_or_else(|| CommandError::not_found("Carving source"))?;
    if !matches!(source_meta.kind, DataSourceKind::Raw | DataSourceKind::E01) {
        return Err(CommandError::invalid_input(
            "Carving requires a RAW/E01 logical disk view",
        ));
    }
    let target = crate::digest_service::DigestTarget {
        data_source_id: Some(source),
        file_id: None,
        partition_index: None,
    };
    let identity = crate::digest_service::identity::snapshot(conn, root, case, &target)
        .map_err(CommandError::from_typed_service_error)?;
    Ok(format!(
        "{}:{}",
        evidence_core::carving::PNG_VALIDATOR_VERSION,
        identity.revision
    ))
}
pub(super) fn factory(
    conn: &Connection,
    case: &CaseId,
    source: &str,
) -> Result<SharedReaderFactory, CommandError> {
    let source = DataSourceRepo::new(conn)
        .find_by_case(case)
        .map_err(CommandError::from_service_error)?
        .into_iter()
        .find(|s| s.id.0 == source)
        .ok_or_else(|| CommandError::not_found("Carving source"))?;
    let storage = DataSourceRepo::new(conn)
        .find_storage(&source.id)
        .map_err(CommandError::from_service_error)?;
    if storage.is_none_or(|s| s.import_state != "ready") {
        return Err(CommandError::conflict("Carving source is not ready"));
    }
    if !matches!(source.kind, DataSourceKind::Raw | DataSourceKind::E01) {
        return Err(CommandError::invalid_input(
            "Carving requires a RAW/E01 logical disk view",
        ));
    }
    crate::source_runtime::open_factory(&source.source_path, &source.kind)
        .map_err(|_| CommandError::internal("Cannot open carving evidence view"))
}
pub(crate) fn validate_request(
    request: &transport::dto::PngCarvingRequestDto,
) -> Result<(), CommandError> {
    if request.data_source_id.is_empty() || request.data_source_id.len() > 256 {
        return Err(CommandError::invalid_input("Invalid carving source"));
    }
    if let Some(offset) = &request.offset {
        decimal(offset)?;
    }
    if let Some(length) = &request.length {
        if !(1..=1024 * 1024 * 1024).contains(&decimal(length)?) {
            return Err(CommandError::invalid_input(
                "Carving range exceeds its per-task budget",
            ));
        }
    }
    Ok(())
}

pub(super) fn prepare(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    request: &transport::dto::PngCarvingRequestDto,
) -> Result<(SharedReaderFactory, evidence_core::carving::CarveScope), CommandError> {
    validate_request(request)?;
    let revision = source_revision(conn, root, case, &request.data_source_id)?;
    let factory = factory(conn, case, &request.data_source_id)?;
    let offset = request
        .offset
        .as_deref()
        .map(decimal)
        .transpose()?
        .unwrap_or(0);
    let remaining = factory
        .logical_size()
        .checked_sub(offset)
        .ok_or_else(|| CommandError::invalid_input("Carving offset outside source"))?;
    let length = request
        .length
        .as_deref()
        .map(decimal)
        .transpose()?
        .unwrap_or(remaining.min(64 * 1024 * 1024));
    let scope = evidence_core::carving::CarveScope {
        source_id: request.data_source_id.clone(),
        revision,
        view: "logicalDisk".into(),
        source_length: factory.logical_size(),
        offset,
        length,
    };
    scope.end().map_err(super::run::core_error)?;
    Ok((factory, scope))
}
pub(super) fn dto(
    artifact: &domain::Artifact,
    source: &DataSourceId,
) -> Result<CarvedArtifactDto, CommandError> {
    let value = |key: &str| {
        artifact
            .attrs
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| CommandError::security("Carving provenance is incomplete"))
    };
    if artifact.family != "carving.png"
        || artifact.extractor_id.as_deref() != Some("carving.png")
        || value("dataSourceId")? != source.0
    {
        return Err(CommandError::security("Carving artifact source mismatch"));
    }
    Ok(CarvedArtifactDto {
        artifact_id: crate::source_db::encode_source_scoped_id(source, &artifact.id.0),
        data_source_id: source.0.clone(),
        source_revision: value("sourceRevision")?,
        view: value("view")?,
        offset: value("offset")?,
        length: value("length")?,
        sha256: value("sha256")?,
        validator_version: value("validatorVersion")?,
    })
}
pub(super) fn load(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    id: &str,
) -> Result<CarvedArtifactDto, CommandError> {
    let (source, local) = crate::source_db::parse_source_scoped_id("Carved artifact", id)
        .map_err(CommandError::from_service_error)?;
    let source_db = crate::source_db::open_ready_source_read_only_by_id(conn, root, case, &source)
        .map_err(CommandError::from_service_error)?;
    let artifact = ArtifactRepo::new(&source_db.connection)
        .find_by_id(&local)
        .map_err(CommandError::from_service_error)?
        .ok_or_else(|| CommandError::not_found("Carved artifact"))?;
    let result = dto(&artifact, &source)?;
    if result.view != "logicalDisk"
        || result.validator_version != evidence_core::carving::PNG_VALIDATOR_VERSION
        || result.source_revision != source_revision(conn, root, case, &source.0)?
    {
        return Err(CommandError::security("Carving source revision changed"));
    }
    Ok(result)
}
