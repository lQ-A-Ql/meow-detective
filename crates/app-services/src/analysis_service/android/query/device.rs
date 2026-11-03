use std::collections::BTreeMap;

use domain::DataSourceId;
use persistence_fjall::ScanDirection;
use persistence_sqlite::repositories::android_analysis_repo::AndroidAnalysisRepo;
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{AnalysisParseStatusDto, AndroidDeviceFactDto, AndroidDeviceInfoDto};

use crate::analysis_service::AnalysisServiceError;

pub(crate) fn get_android_device_info(
    connection: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<AndroidDeviceInfoDto, AnalysisServiceError> {
    if let Ok(source) =
        crate::source_db::open_registered_fjall_source_read_only(case_root, data_source_id)
    {
        let snapshot = source.snapshot();
        let complete = source
            .source_projection_complete(&snapshot, &data_source_id.0)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        if complete {
            if let Some(result) = read_fjall_device(&source, data_source_id)? {
                return Ok(result);
            }
        }
    }
    read_sqlite_device(connection, data_source_id)
}

fn read_fjall_device(
    source: &crate::source_db::FjallReadOnlySourceStorageHandle,
    data_source_id: &DataSourceId,
) -> Result<Option<AndroidDeviceInfoDto>, AnalysisServiceError> {
    let snapshot = source.snapshot();
    let page = source
        .parser_list_by_kind(
            &snapshot,
            &data_source_id.0,
            "system_fact",
            None,
            ScanDirection::Forward,
            1000,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    if page.records.is_empty() {
        return Ok(None);
    }
    if page.next_cursor.is_some() {
        return Err(AnalysisServiceError::Other(
            "Android system facts exceed the bounded parser query limit".to_owned(),
        ));
    }
    let facts = page
        .records
        .into_iter()
        .filter_map(|record| {
            let value = record.payload;
            Some(AndroidDeviceFactDto {
                field: value.get("field")?.as_str()?.to_owned(),
                value: value.get("value")?.as_str()?.to_owned(),
                confidence: value.get("confidence")?.as_str()?.to_owned(),
                source_file_id: value.get("sourceFileId")?.as_str()?.to_owned(),
                source_path: value.get("sourcePath")?.as_str()?.to_owned(),
                parser: value.get("parser")?.as_str()?.to_owned(),
                warning: value
                    .get("warning")
                    .and_then(|value| value.as_str().map(str::to_owned)),
            })
        })
        .collect::<Vec<_>>();
    let values = facts
        .iter()
        .map(|fact| (fact.field.as_str(), fact.value.clone()))
        .collect::<BTreeMap<_, _>>();
    let warnings = facts
        .iter()
        .filter_map(|fact| fact.warning.clone())
        .collect();
    Ok(Some(AndroidDeviceInfoDto {
        status: AnalysisParseStatusDto::Parsed,
        model: values.get("model").cloned(),
        manufacturer: values.get("manufacturer").cloned(),
        android_version: values.get("androidVersion").cloned(),
        sdk_int: values.get("sdkInt").cloned(),
        build_id: values.get("buildId").cloned(),
        build_fingerprint: values.get("buildFingerprint").cloned(),
        serial_number: values.get("serialNumber").cloned(),
        android_id: values.get("androidId").cloned(),
        imei: None,
        facts,
        warnings,
    }))
}

fn read_sqlite_device(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> Result<AndroidDeviceInfoDto, AnalysisServiceError> {
    let facts = AndroidAnalysisRepo::new(connection)
        .list_facts(data_source_id)?
        .into_iter()
        .map(|fact| AndroidDeviceFactDto {
            field: fact.field,
            value: fact.value,
            confidence: fact.confidence,
            source_file_id: fact.source_file_id,
            source_path: fact.source_path,
            parser: fact.parser,
            warning: fact.warning,
        })
        .collect::<Vec<_>>();
    let values = facts
        .iter()
        .map(|fact| (fact.field.as_str(), fact.value.clone()))
        .collect::<BTreeMap<_, _>>();
    let status = if facts.is_empty() {
        AnalysisParseStatusDto::NotParsed
    } else {
        AnalysisParseStatusDto::Parsed
    };
    let warnings = facts
        .iter()
        .filter_map(|fact| fact.warning.clone())
        .collect();
    Ok(AndroidDeviceInfoDto {
        status,
        model: values.get("model").cloned(),
        manufacturer: values.get("manufacturer").cloned(),
        android_version: values.get("androidVersion").cloned(),
        sdk_int: values.get("sdkInt").cloned(),
        build_id: values.get("buildId").cloned(),
        build_fingerprint: values.get("buildFingerprint").cloned(),
        serial_number: values.get("serialNumber").cloned(),
        android_id: values.get("androidId").cloned(),
        imei: None,
        facts,
        warnings,
    })
}
