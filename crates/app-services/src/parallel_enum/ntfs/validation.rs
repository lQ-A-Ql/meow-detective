use rusqlite::{params, Connection};

use super::super::{batch_sink::mft_entry_id, error::ParallelEnumError};

pub(in crate::parallel_enum) fn validate_mft_staging_shape(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
) -> Result<(), ParallelEnumError> {
    let root_id = mft_entry_id(partition_index, 5);
    let id_pattern = format!("mft:{partition_index}:%");
    let total = count_rows(
        conn,
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1 AND id LIKE ?2",
        data_source_id,
        &id_pattern,
    )?;
    let root_count = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries
             WHERE data_source_id = ?1 AND id = ?2
               AND parent_id IS NULL AND entry_type = 'directory' COLLATE NOCASE",
            params![data_source_id, root_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    let orphan_count = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries child
             WHERE child.data_source_id = ?1 AND child.id LIKE ?2
               AND child.parent_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM file_entries parent
                    WHERE parent.data_source_id = child.data_source_id
                      AND parent.id = child.parent_id
               )",
            params![data_source_id, id_pattern],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    let reachable = conn
        .query_row(
            "WITH RECURSIVE reachable(id) AS (
                 SELECT id FROM file_entries
                  WHERE data_source_id = ?1 AND id = ?2
                 UNION
                 SELECT child.id FROM file_entries child
                 JOIN reachable parent ON child.parent_id = parent.id
                  WHERE child.data_source_id = ?1 AND child.id LIKE ?3
             )
             SELECT COUNT(*) FROM reachable",
            params![data_source_id, root_id, id_pattern],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;

    if total == 0 || root_count != 1 || orphan_count != 0 || reachable != total {
        return Err(ParallelEnumError::MftParams(format!(
            "MFT catalog tree is incomplete: total={total}, roots={root_count}, reachable={reachable}, orphans={orphan_count}"
        )));
    }
    Ok(())
}

fn count_rows(
    conn: &Connection,
    query: &str,
    data_source_id: &str,
    id_pattern: &str,
) -> Result<i64, String> {
    conn.query_row(query, params![data_source_id, id_pattern], |row| row.get(0))
        .map_err(|error| error.to_string())
}
