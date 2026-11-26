use super::{ReportError, ReportExportContext};
use persistence_source::fjall::KeyspaceName;
use persistence_sqlite::repositories::datasource_repo::DataSourceStorage;
use transport::dto::{ReportDocumentDto, ReportSourceDto, ReportSourceSummaryDto};

pub(super) fn load(context: &ReportExportContext<'_>) -> Result<ReportDocumentDto, ReportError> {
    let mut sources = Vec::new();
    for (source, storage) in
        crate::source_db::ready_data_sources(context.connection, &context.case.id)?
    {
        if context
            .request
            .data_source_id
            .as_ref()
            .is_some_and(|id| id != &source.id.0)
        {
            continue;
        }
        super::ensure_active(context.cancel)?;
        sources.push(read_source(context, source, storage)?);
    }
    if context.request.data_source_id.is_some() && sources.is_empty() {
        return Err(ReportError::NotFound(
            "report data source is not ready in the active case".into(),
        ));
    }
    Ok(ReportDocumentDto {
        case_id: context.case.id.0.clone(),
        case_name: context.case.name.clone(),
        case_number: context.case.number.clone(),
        examiner: context.case.examiner.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        scope: context.request.scope.clone(),
        sources,
    })
}

fn read_source(
    context: &ReportExportContext<'_>,
    source: domain::DataSource,
    storage: DataSourceStorage,
) -> Result<ReportSourceDto, ReportError> {
    domain::DataSourcePlatform::parse_explicit(&storage.platform)
        .map_err(|error| ReportError::Unsupported(error.to_string()))?;
    let handle =
        crate::source_db::open_registered_fjall_source_read_only(context.case_root, &source.id)
            .map_err(|error| ReportError::Other(error.to_string()))?;
    let snapshot = handle.snapshot();
    handle
        .require_projection_complete(&snapshot, &source.id.0)
        .map_err(|error| ReportError::Other(error.to_string()))?;
    let key = analysis_core::analysis_service::report_summary::summary_key(&source.id.0)
        .map_err(|error| ReportError::Other(error.to_string()))?;
    let envelope = handle
        .read_envelope(&snapshot, KeyspaceName::Meta, &key)
        .map_err(|error| ReportError::Other(error.to_string()))?;
    let summary_available = envelope.is_some();
    let mut summary: ReportSourceSummaryDto = match envelope {
        Some(envelope) => {
            if envelope.record_kind != "report_summary"
                || envelope.source_id != source.id.0
                || envelope.schema_version != 1
            {
                return Err(ReportError::Other("invalid report summary envelope".into()));
            }
            envelope
                .decode_payload()
                .map_err(|error| ReportError::Other(error.to_string()))?
        }
        None => Default::default(),
    };
    let scope = &context.request.scope;
    if !scope.operating_system {
        summary.operating_system = Default::default();
    }
    if !scope.hardware {
        summary.hardware = Default::default();
    }
    if !scope.browsers {
        summary.browsers = Default::default();
    }
    if !scope.email {
        summary.email = Default::default();
    }
    Ok(ReportSourceDto {
        data_source_id: source.id.0,
        name: source.name,
        platform: storage.platform,
        generation: handle
            .source_root()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ReportError::Other("source generation name is invalid".into()))?
            .to_owned(),
        hash_status: serde_json::to_value(&source.provenance.hash_status)
            .map_err(|error| ReportError::Other(error.to_string()))?
            .as_str()
            .ok_or_else(|| ReportError::Other("source hash status is invalid".into()))?
            .to_owned(),
        summary_available,
        summary,
    })
}
