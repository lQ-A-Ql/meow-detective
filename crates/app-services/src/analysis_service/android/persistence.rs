use domain::DataSourceId;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::model::{AndroidFact, AndroidPackage};
use crate::analysis_service::AnalysisServiceError;

const PARSER: &str = "android.system-packages.v1";

pub(super) fn persist_android_analysis(
    connection: &Connection,
    data_source_id: &DataSourceId,
    facts: &[AndroidFact],
    packages: &[AndroidPackage],
) -> Result<(), AnalysisServiceError> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute(
        "DELETE FROM android_system_facts WHERE data_source_id = ?1",
        [data_source_id.0.as_str()],
    )?;
    transaction.execute(
        "DELETE FROM android_packages WHERE data_source_id = ?1",
        [data_source_id.0.as_str()],
    )?;
    for fact in facts {
        transaction.execute(
            "INSERT INTO android_system_facts
             (id, data_source_id, field_key, field_value, confidence, source_file_id, source_path, parser, source_rank, warning)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                &data_source_id.0,
                fact.field,
                &fact.value,
                fact.confidence,
                &fact.source_file_id,
                &fact.source_path,
                PARSER,
                fact.source_rank,
                &fact.warning,
            ],
        )?;
    }
    for package in packages {
        transaction.execute(
            "INSERT INTO android_packages
             (id, data_source_id, package_name, app_name, version_code, install_time, update_time, installer, uid, code_path, source_file_id, source_path, parser, warning)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                Uuid::new_v4().to_string(),
                &data_source_id.0,
                &package.package_name,
                &package.version_code,
                &package.install_time,
                &package.update_time,
                &package.installer,
                package.uid,
                &package.code_path,
                &package.source_file_id,
                &package.source_path,
                PARSER,
                &package.warning,
            ],
        )?;
    }
    transaction.commit()?;
    Ok(())
}
