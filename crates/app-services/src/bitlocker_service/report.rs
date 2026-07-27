use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::partition_repo::{DataSourcePartitionRecord, PartitionRepo};
use rusqlite::Connection;
use transport::ServiceErrorCategory;

use super::{
    inspect_bitlocker_volume, source::is_bitlocker_partition, BitLockerRuntimeContext,
    BitLockerServiceError,
};

pub(crate) struct BitLockerReportEntry {
    pub(crate) data_source_id: String,
    pub(crate) partition_index: u32,
    pub(crate) partition_name: String,
    pub(crate) encryption_method: Option<String>,
    pub(crate) encryption_method_code: Option<u16>,
    pub(crate) decryptable: Option<bool>,
    pub(crate) unlocked: Option<bool>,
    pub(crate) stored_key_available: Option<bool>,
    pub(crate) supports_password: Option<bool>,
    pub(crate) supports_recovery_password: Option<bool>,
    pub(crate) protectors: Vec<BitLockerReportProtector>,
    pub(crate) plaintext_filesystem: Option<String>,
    pub(crate) inspection_error_code: Option<&'static str>,
}

pub(crate) struct BitLockerReportProtector {
    pub(crate) code: u16,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) unlockable: bool,
}

pub(crate) fn collect_report_inventory(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<Vec<BitLockerReportEntry>, BitLockerServiceError> {
    let mut entries = Vec::new();
    for (source, _) in crate::source_db::ready_data_sources(case_conn, case_id)? {
        let ready = crate::source_db::open_ready_source_read_only_by_id(
            case_conn, case_root, case_id, &source.id,
        )?;
        let partitions = PartitionRepo::new(&ready.connection).find_by_data_source(&source.id.0)?;
        entries.extend(
            partitions
                .into_iter()
                .filter(is_bitlocker_partition)
                .map(|partition| {
                    inspect_partition(
                        case_conn, case_root, case_id, &source.id, partition, runtimes,
                    )
                }),
        );
    }
    entries.sort_by(|left, right| {
        left.data_source_id
            .cmp(&right.data_source_id)
            .then(left.partition_index.cmp(&right.partition_index))
    });
    Ok(entries)
}

fn inspect_partition(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition: DataSourcePartitionRecord,
    runtimes: BitLockerRuntimeContext<'_>,
) -> BitLockerReportEntry {
    match inspect_bitlocker_volume(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition.partition_index,
        runtimes,
    ) {
        Ok(status) => BitLockerReportEntry {
            data_source_id: status.data_source_id,
            partition_index: status.partition_index,
            partition_name: partition.name,
            encryption_method: Some(status.encryption_method),
            encryption_method_code: Some(status.encryption_method_code),
            decryptable: Some(status.decryptable),
            unlocked: Some(status.unlocked),
            stored_key_available: Some(status.stored_key_available),
            supports_password: Some(status.supports_password),
            supports_recovery_password: Some(status.supports_recovery_password),
            protectors: status
                .protectors
                .into_iter()
                .map(|protector| BitLockerReportProtector {
                    code: protector.code,
                    kind: protector.kind,
                    label: protector.label,
                    unlockable: protector.unlockable,
                })
                .collect(),
            plaintext_filesystem: status.plaintext_filesystem,
            inspection_error_code: None,
        },
        Err(error) => BitLockerReportEntry {
            data_source_id: data_source_id.0.clone(),
            partition_index: partition.partition_index,
            partition_name: partition.name,
            encryption_method: None,
            encryption_method_code: None,
            decryptable: None,
            unlocked: None,
            stored_key_available: None,
            supports_password: None,
            supports_recovery_password: None,
            protectors: Vec::new(),
            plaintext_filesystem: None,
            inspection_error_code: Some(error.code().unwrap_or("BITLOCKER_INSPECTION_FAILED")),
        },
    }
}
