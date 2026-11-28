use std::collections::{BTreeMap, BTreeSet};

use domain::structure::ActivityState;
use persistence_sqlite::repositories::structure_activity_repo::{
    ActivityRecord, StructureActivityRepo,
};

use super::{ActivityDag, RuntimeError, RuntimeStart};

pub(super) fn register(
    start: &RuntimeStart<'_>,
    dag: &ActivityDag,
    keys: &BTreeSet<String>,
) -> Result<BTreeMap<String, ActivityState>, RuntimeError> {
    let transaction = start
        .connection
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    let repository = StructureActivityRepo::new(&transaction);
    let mut states = BTreeMap::new();
    for key in keys {
        let definition = dag.get(key).expect("inserted activity is absent");
        let record = ActivityRecord {
            case_id: start.request.case_id.clone(),
            run_id: start.run_id.into(),
            work_key: key.clone(),
            attempt_id: start.attempt_id.into(),
            owner_id: start.owner_id.into(),
            definition_sha256: definition.definition_digest()?,
            input_capability_ids: definition.work.input_capability_versions.clone(),
            target_id: definition
                .target
                .identity()
                .map_err(|_| RuntimeError::InvalidDefinition)?,
            state: ActivityState::Pending,
            output_receipt: None,
            failure: None,
        };
        repository.insert_pending(&record)?;
        let stored = repository
            .get(&record.case_id, &record.run_id, key)?
            .ok_or(RuntimeError::LostOwner)?;
        if stored.attempt_id != start.attempt_id || stored.owner_id != start.owner_id {
            return Err(RuntimeError::LostOwner);
        }
        if stored.state != ActivityState::Pending {
            return Err(RuntimeError::InvalidDefinition);
        }
        states.insert(key.clone(), stored.state);
    }
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(states)
}
