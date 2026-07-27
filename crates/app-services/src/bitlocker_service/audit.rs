use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};

pub(crate) struct BitLockerAudit<'a> {
    pub case_id: &'a str,
    pub data_source_id: &'a str,
    pub partition_index: u32,
    pub metadata_fingerprint: Option<&'a str>,
    pub operation: &'a str,
    pub outcome: &'a str,
    pub error_code: Option<&'a str>,
}

pub(crate) fn record(conn: &rusqlite::Connection, entry: BitLockerAudit<'_>) {
    let action = match entry.operation {
        "lock" => AuditAction::BitLockerLock,
        "catalogImport" => AuditAction::BitLockerCatalogImport,
        _ => AuditAction::BitLockerUnlock,
    };
    let details = serde_json::json!({
        "dataSourceId": entry.data_source_id,
        "partitionIndex": entry.partition_index,
        "metadataFingerprint": entry.metadata_fingerprint,
        "operation": entry.operation,
        "outcome": entry.outcome,
        "errorCode": entry.error_code,
    });
    let serialized = serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string());
    if let Err(error) = AuditRepo::new(conn).log(
        Some(entry.case_id),
        "system",
        &action,
        Some(entry.data_source_id),
        &serialized,
    ) {
        tracing::warn!(
            data_source_id = entry.data_source_id,
            partition_index = entry.partition_index,
            operation = entry.operation,
            %error,
            "Failed to record BitLocker audit event"
        );
    }
}
