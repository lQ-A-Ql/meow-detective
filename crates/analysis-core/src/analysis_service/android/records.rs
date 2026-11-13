use super::model::{AndroidFact, AndroidPackage};
use crate::analysis_service::AnalysisServiceError;
use domain::DataSourceId;
use persistence_source::fjall::ParserRecord;
use serde::{Deserialize, Serialize};
use transport::dto::{AndroidDeviceFactDto, AndroidPackageDto};

pub(super) const FAMILY: &str = "android";
pub(super) const PARSER: &str = "android.system-packages.v1";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredFact {
    pub source_rank: i64,
    #[serde(flatten)]
    pub fact: AndroidDeviceFactDto,
}

pub(super) fn fact_record(
    fact: &AndroidFact,
    source: &DataSourceId,
) -> Result<ParserRecord, AnalysisServiceError> {
    let payload = StoredFact {
        source_rank: fact.source_rank,
        fact: AndroidDeviceFactDto {
            field: fact.field.into(),
            value: fact.value.clone(),
            confidence: fact.confidence.into(),
            source_file_id: fact.source_file_id.clone(),
            source_path: fact.source_path.clone(),
            parser: PARSER.into(),
            warning: fact.warning.clone(),
        },
    };
    record(
        source,
        format!("android-fact:{}:{}", fact.field, fact.source_file_id),
        "system_fact",
        &payload,
    )
}

pub(super) fn package_record(
    package: &AndroidPackage,
    source: &DataSourceId,
) -> Result<ParserRecord, AnalysisServiceError> {
    let payload = AndroidPackageDto {
        package_name: package.package_name.clone(),
        app_name: None,
        version_code: package.version_code.clone(),
        install_time: package.install_time.clone(),
        update_time: package.update_time.clone(),
        installer: package.installer.clone(),
        uid: package.uid,
        user_id: None,
        user_state: None,
        code_path: package.code_path.clone(),
        source_file_id: package.source_file_id.clone(),
        source_path: package.source_path.clone(),
        parser: PARSER.into(),
        icon_data_url: None,
        warning: package.warning.clone(),
    };
    record(
        source,
        format!(
            "android-package:{}:{}",
            package.package_name, package.source_file_id
        ),
        "package",
        &payload,
    )
}

fn record(
    source: &DataSourceId,
    id: String,
    kind: &str,
    payload: &impl Serialize,
) -> Result<ParserRecord, AnalysisServiceError> {
    let payload = serde_json::to_value(payload)
        .map_err(|error| AnalysisServiceError::Extraction(error.to_string()))?;
    Ok(ParserRecord {
        id,
        data_source_id: source.0.clone(),
        family: FAMILY.into(),
        kind: kind.into(),
        status: "ready".into(),
        summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
        payload,
    })
}

pub(super) fn read_fact(
    record: ParserRecord,
    source: &DataSourceId,
) -> Result<StoredFact, AnalysisServiceError> {
    validate_record(&record, source, "system_fact")?;
    let result: StoredFact = serde_json::from_value(record.payload).map_err(|error| {
        AnalysisServiceError::Extraction(format!("invalid Android fact payload: {error}"))
    })?;
    if result.source_rank < 0
        || result.fact.field.is_empty()
        || result.fact.source_file_id.is_empty()
        || result.fact.parser != PARSER
    {
        return Err(AnalysisServiceError::Extraction(
            "invalid Android fact identity".into(),
        ));
    }
    Ok(result)
}

pub(super) fn read_package(
    record: ParserRecord,
    source: &DataSourceId,
) -> Result<AndroidPackageDto, AnalysisServiceError> {
    validate_record(&record, source, "package")?;
    let result: AndroidPackageDto = serde_json::from_value(record.payload).map_err(|error| {
        AnalysisServiceError::Extraction(format!("invalid Android package payload: {error}"))
    })?;
    if result.package_name.is_empty() || result.source_file_id.is_empty() || result.parser != PARSER
    {
        return Err(AnalysisServiceError::Extraction(
            "invalid Android package identity".into(),
        ));
    }
    Ok(result)
}

fn validate_record(
    record: &ParserRecord,
    source: &DataSourceId,
    kind: &str,
) -> Result<(), AnalysisServiceError> {
    if record.data_source_id != source.0
        || record.family != FAMILY
        || record.kind != kind
        || record.status != "ready"
    {
        return Err(AnalysisServiceError::Extraction(
            "Android parser record ownership or kind is invalid".into(),
        ));
    }
    Ok(())
}
