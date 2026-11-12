use domain::DataSourceId;
use persistence_source::fjall::{ParserRecord, PersistMode};
use persistence_sqlite::repositories::android_analysis_repo::{
    AndroidAnalysisRepo, AndroidFactRecord, AndroidPackageRecord,
};
use rusqlite::Connection;
use std::path::Path;

use super::model::{AndroidFact, AndroidPackage};
use crate::analysis_service::AnalysisServiceError;

const PARSER: &str = "android.system-packages.v1";

pub(super) fn persist_android_analysis(
    connection: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    facts: &[AndroidFact],
    packages: &[AndroidPackage],
) -> Result<(), AnalysisServiceError> {
    let fact_records = facts
        .iter()
        .map(|fact| AndroidFactRecord {
            field: fact.field.to_string(),
            value: fact.value.clone(),
            confidence: fact.confidence.to_string(),
            source_file_id: fact.source_file_id.clone(),
            source_path: fact.source_path.clone(),
            source_rank: fact.source_rank,
            parser: PARSER.to_string(),
            warning: fact.warning.clone(),
        })
        .collect::<Vec<_>>();
    let package_records = packages
        .iter()
        .map(|package| AndroidPackageRecord {
            package_name: package.package_name.clone(),
            app_name: None,
            version_code: package.version_code.clone(),
            install_time: package.install_time.clone(),
            update_time: package.update_time.clone(),
            installer: package.installer.clone(),
            uid: package.uid,
            code_path: package.code_path.clone(),
            source_file_id: package.source_file_id.clone(),
            source_path: package.source_path.clone(),
            parser: PARSER.to_string(),
            warning: package.warning.clone(),
        })
        .collect::<Vec<_>>();
    AndroidAnalysisRepo::new(connection).replace_for_source(
        data_source_id,
        &fact_records,
        &package_records,
    )?;
    if let Ok(source) =
        source_kernel::source_db::open_registered_fjall_source_read_write(case_root, data_source_id)
    {
        persist_fjall_records(&source, data_source_id, &fact_records, &package_records)?;
    }
    Ok(())
}

fn persist_fjall_records(
    source: &source_kernel::source_db::FjallReadWriteSourceStorageHandle,
    data_source_id: &DataSourceId,
    fact_records: &[AndroidFactRecord],
    package_records: &[AndroidPackageRecord],
) -> Result<(), AnalysisServiceError> {
    let mut records = Vec::with_capacity(fact_records.len().saturating_add(package_records.len()));
    for fact in fact_records {
        let payload = serde_json::json!({"field": fact.field, "value": fact.value, "confidence": fact.confidence, "sourceFileId": fact.source_file_id, "sourcePath": fact.source_path, "parser": fact.parser, "warning": fact.warning});
        let record = ParserRecord {
            id: format!("android-fact:{}:{}", fact.field, fact.source_file_id),
            data_source_id: data_source_id.0.clone(),
            family: "android".into(),
            kind: "system_fact".into(),
            status: "ready".into(),
            summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
            payload,
        };
        records.push(record);
    }
    for package in package_records {
        let payload = serde_json::json!({"packageName": package.package_name, "appName": package.app_name, "versionCode": package.version_code, "installTime": package.install_time, "updateTime": package.update_time, "installer": package.installer, "uid": package.uid, "codePath": package.code_path, "sourceFileId": package.source_file_id, "sourcePath": package.source_path, "parser": package.parser, "warning": package.warning});
        let record = ParserRecord {
            id: format!(
                "android-package:{}:{}",
                package.package_name, package.source_file_id
            ),
            data_source_id: data_source_id.0.clone(),
            family: "android".into(),
            kind: "package".into(),
            status: "ready".into(),
            summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
            payload,
        };
        records.push(record);
    }
    source
        .parser_upsert_batch(&records, PersistMode::Buffer)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    source
        .persist(PersistMode::SyncAll)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    Ok(())
}
