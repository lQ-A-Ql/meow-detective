//! Project source-local partition identities into the public summary contract.
use crate::partition_query_service::PartitionQueryRepo as PartitionRepo;
use rusqlite::Connection;
use transport::dto::DataSourcePartitionDto;

use super::FileServiceError;

pub(super) fn summaries(
    conn: &Connection,
    source_id: &str,
) -> Result<(Vec<DataSourcePartitionDto>, Option<String>), FileServiceError> {
    let repo = PartitionRepo::new(conn);
    let partitions = repo
        .find_by_data_source(source_id)?
        .into_iter()
        .map(|item| DataSourcePartitionDto {
            index: item.partition_index,
            name: item.name,
            kind_label: item.kind_label,
            status: item.status,
            offset: item.offset,
            length: item.length,
            type_guid: item.type_guid,
            gpt_partition_uuid: item.gpt_partition_uuid,
            filesystem_uuid: item.filesystem_uuid,
            filesystem: item.filesystem,
            unlock_hint: item.unlock_hint,
        })
        .collect();
    let identities = repo.identities_for_data_source(source_id)?;
    Ok((partitions, identities.gpt_disk_guid))
}
