use std::path::PathBuf;

use super::model::LoadedEntity;
use crate::entity_resolution::EntityResolutionError;

pub(super) fn load_entities(
    db_paths: &[PathBuf],
) -> Result<Vec<LoadedEntity>, EntityResolutionError> {
    let mut entities = Vec::new();
    for (database_index, path) in db_paths.iter().enumerate() {
        let conn = persistence_sqlite::connection::open_existing(path).map_err(|error| {
            EntityResolutionError::Other(format!("failed to open {}: {error}", path.display()))
        })?;
        let mut statement = conn.prepare(
            "SELECT id, case_id, entity_type, canonical_value
             FROM resolved_entities ORDER BY case_id, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(LoadedEntity {
                entity_id: row.get(0)?,
                case_id: row.get(1)?,
                entity_type: row.get(2)?,
                canonical_value: row.get(3)?,
                database_index,
            })
        })?;
        entities.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    entities.sort_by(|left, right| {
        left.database_index
            .cmp(&right.database_index)
            .then_with(|| left.case_id.cmp(&right.case_id))
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });
    Ok(entities)
}
