use crate::analysis_service::{android::records, AnalysisServiceError};
use domain::DataSourceId;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};
use transport::dto::{AnalysisParseStatusDto, AndroidDeviceInfoDto};

pub(crate) fn get_android_device_info(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<AndroidDeviceInfoDto, AnalysisServiceError> {
    let mut stored = Vec::new();
    super::visit_current_kind(case_root, data_source_id, "system_fact", |record| {
        stored.push(records::read_fact(record, data_source_id)?);
        Ok(())
    })?;
    stored.sort_by(|a, b| {
        a.source_rank
            .cmp(&b.source_rank)
            .then_with(|| a.fact.field.cmp(&b.fact.field))
            .then_with(|| a.fact.source_file_id.cmp(&b.fact.source_file_id))
    });
    let mut fields = HashSet::new();
    let facts = stored
        .into_iter()
        .filter(|item| fields.insert(item.fact.field.clone()))
        .map(|item| item.fact)
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
