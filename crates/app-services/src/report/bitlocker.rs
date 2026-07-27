use std::path::Path;

use domain::CaseId;
use rusqlite::Connection;
use serde_json::{json, Value};
use transport::commands::ExportScopeDto;

use super::{BitLockerReportContext, ReportError};
use crate::bitlocker_service::BitLockerReportEntry;

pub(crate) fn current_inventory(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    scope: &ExportScopeDto,
    context: Option<BitLockerReportContext<'_>>,
) -> Result<Vec<BitLockerReportEntry>, ReportError> {
    if !scope.file_system_metadata {
        return Ok(Vec::new());
    }
    context
        .map(|context| {
            crate::bitlocker_service::collect_report_inventory(
                case_conn,
                case_root,
                case_id,
                context.runtimes,
            )
            .map_err(|_| ReportError::Other("BitLocker inventory collection failed".to_string()))
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

pub(crate) fn report_rows(entries: &[BitLockerReportEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| {
            format!(
                "bitlocker dataSourceId={} partitionIndex={} partitionName={} encryptionMethod={} encryptionMethodCode={} decryptable={} unlocked={} storedKeyAvailable={} supportsPassword={} supportsRecoveryPassword={} protectors={} plaintextFilesystem={} inspectionErrorCode={}",
                entry.data_source_id,
                entry.partition_index,
                entry.partition_name,
                option_text(entry.encryption_method.as_deref()),
                option_value(entry.encryption_method_code),
                option_value(entry.decryptable),
                option_value(entry.unlocked),
                option_value(entry.stored_key_available),
                option_value(entry.supports_password),
                option_value(entry.supports_recovery_password),
                entry
                    .protectors
                    .iter()
                    .map(|protector| format!(
                        "{}:{}:{}:{}",
                        protector.code, protector.kind, protector.label, protector.unlockable
                    ))
                    .collect::<Vec<_>>()
                    .join(" | "),
                option_text(entry.plaintext_filesystem.as_deref()),
                option_text(entry.inspection_error_code),
            )
        })
        .collect()
}

pub(crate) fn json_section(entries: &[BitLockerReportEntry]) -> Value {
    json!({
        "volumes": entries.iter().map(|entry| json!({
            "dataSourceId": entry.data_source_id,
            "partitionIndex": entry.partition_index,
            "partitionName": entry.partition_name,
            "encryptionMethod": entry.encryption_method,
            "encryptionMethodCode": entry.encryption_method_code,
            "decryptable": entry.decryptable,
            "unlocked": entry.unlocked,
            "storedKeyAvailable": entry.stored_key_available,
            "supportsPassword": entry.supports_password,
            "supportsRecoveryPassword": entry.supports_recovery_password,
            "protectors": entry.protectors.iter().map(|protector| json!({
                "code": protector.code,
                "kind": protector.kind,
                "label": protector.label,
                "unlockable": protector.unlockable,
            })).collect::<Vec<_>>(),
            "plaintextFilesystem": entry.plaintext_filesystem,
            "inspectionErrorCode": entry.inspection_error_code,
        })).collect::<Vec<_>>(),
    })
}

fn option_text(value: Option<&str>) -> &str {
    value.unwrap_or("unknown")
}

fn option_value(value: Option<impl ToString>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
