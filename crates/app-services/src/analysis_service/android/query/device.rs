use std::collections::BTreeMap;

use domain::DataSourceId;
use persistence_sqlite::repositories::android_analysis_repo::AndroidAnalysisRepo;
use rusqlite::Connection;
use transport::dto::{AnalysisParseStatusDto, AndroidDeviceFactDto, AndroidDeviceInfoDto};

use crate::analysis_service::AnalysisServiceError;

pub(crate) fn get_android_device_info(
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
