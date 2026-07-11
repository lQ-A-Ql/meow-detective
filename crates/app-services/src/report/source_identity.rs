use domain::DataSourceId;
use transport::dto::{ArtifactRowDto, TimelineEventDto};

use super::ReportError;

pub(crate) fn artifact_data_source_id(
    artifact: &ArtifactRowDto,
) -> Result<DataSourceId, ReportError> {
    let data_source_id = parse_data_source_id("artifact id", &artifact.id)?;
    if let Some(source_object_id) = &artifact.source_object_id {
        ensure_same_source(
            "artifact source object id",
            source_object_id,
            &data_source_id,
        )?;
    }
    Ok(data_source_id)
}

pub(crate) fn timeline_data_source_id(
    event: &TimelineEventDto,
) -> Result<DataSourceId, ReportError> {
    let data_source_id = parse_data_source_id("timeline event id", &event.id)?;
    if !event.source_object_id.is_empty() {
        ensure_same_source(
            "timeline source object id",
            &event.source_object_id,
            &data_source_id,
        )?;
    }
    Ok(data_source_id)
}

fn ensure_same_source(
    label: &str,
    global_id: &str,
    expected: &DataSourceId,
) -> Result<(), ReportError> {
    let actual = parse_data_source_id(label, global_id)?;
    if actual != *expected {
        return Err(ReportError::Other(format!(
            "report record crosses data source boundaries: expected '{}', found '{}'",
            expected.0, actual.0
        )));
    }
    Ok(())
}

fn parse_data_source_id(label: &str, global_id: &str) -> Result<DataSourceId, ReportError> {
    crate::source_db::parse_source_scoped_id(label, global_id)
        .map(|(data_source_id, _)| data_source_id)
        .map_err(|error| ReportError::Other(error.to_string()))
}
