//! Project source-local partition identities into the public summary contract.
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use rusqlite::Connection;
use transport::dto::DataSourcePartitionDto;

use super::FileServiceError;

pub(super) fn summaries(
    conn: &Connection,
    source_id: &str,
) -> Result<(Vec<DataSourcePartitionDto>, Option<String>), FileServiceError> {
    let repo = PartitionRepo::new(conn);
    let identities = repo.identities_for_data_source(source_id)?;
    let partitions = repo
        .find_by_data_source(source_id)?
        .into_iter()
        .map(|item| {
            let identity = identities.partitions.get(&item.partition_index);
            DataSourcePartitionDto {
                index: item.partition_index,
                name: item.name,
                kind_label: item.kind_label,
                status: item.status,
                offset: item.offset,
                length: item.length,
                type_guid: item.type_guid,
                gpt_partition_uuid: identity
                    .and_then(|identity| identity.gpt_partition_uuid.clone()),
                filesystem_uuid: identity.and_then(|identity| identity.filesystem_uuid.clone()),
                filesystem: item.filesystem,
                unlock_hint: item.unlock_hint,
            }
        })
        .collect();
    Ok((partitions, identities.gpt_disk_guid))
}
