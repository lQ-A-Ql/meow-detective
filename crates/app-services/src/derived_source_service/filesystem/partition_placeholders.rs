//! Create catalog placeholder roots before derived-source enumeration.
use super::{ensure_not_cancelled, DerivedSourceResult};
use crate::{datasource_service::PartitionRecord, file_service};
use domain::DataSourceId;
use std::{collections::HashMap, sync::atomic::AtomicBool};

pub(super) fn seed_placeholders(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    partitions: &[PartitionRecord],
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<HashMap<usize, domain::FileEntryId>> {
    let mut placeholders = HashMap::new();
    for partition in partitions {
        ensure_not_cancelled(cancel_token)?;
        let root = file_service::insert_partition_placeholder_root(
            conn,
            data_source_id,
            partition.index,
            &partition.name,
            "supported",
        )?;
        placeholders.insert(partition.index, root);
    }
    Ok(placeholders)
}
