use domain::DataSourceId;
use persistence_sqlite::repositories::android_analysis_repo::{
    AndroidAnalysisRepo, AndroidFactRecord, AndroidPackageRecord,
};
use rusqlite::Connection;

use super::model::{AndroidFact, AndroidPackage};
use crate::analysis_service::AnalysisServiceError;

const PARSER: &str = "android.system-packages.v1";

pub(super) fn persist_android_analysis(
    connection: &Connection,
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
    Ok(())
}
