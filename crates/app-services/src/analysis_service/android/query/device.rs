use std::collections::BTreeMap;

use domain::DataSourceId;
use rusqlite::Connection;
use transport::dto::{AnalysisParseStatusDto, AndroidDeviceFactDto, AndroidDeviceInfoDto};

use crate::analysis_service::AnalysisServiceError;

pub(crate) fn get_android_device_info(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> Result<AndroidDeviceInfoDto, AnalysisServiceError> {
    let mut statement = connection.prepare(
        "SELECT field_key, field_value, confidence, source_file_id, source_path, parser, warning
         FROM android_system_facts
         WHERE data_source_id = ?1
         ORDER BY source_rank ASC, field_key ASC",
    )?;
    let facts = statement
        .query_map([data_source_id.0.as_str()], |row| {
            Ok(AndroidDeviceFactDto {
                field: row.get(0)?,
                value: row.get(1)?,
                confidence: row.get(2)?,
                source_file_id: row.get(3)?,
                source_path: row.get(4)?,
                parser: row.get(5)?,
                warning: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
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
