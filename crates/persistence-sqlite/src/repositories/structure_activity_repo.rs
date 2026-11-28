mod record;

pub use record::ActivityRecord;

use domain::structure::{ActivityState, TargetFailure};
use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Row, Transaction, TransactionBehavior,
};
use serde::de::DeserializeOwned;

use crate::{DbError, DbResult};

pub struct StructureActivityRepo<'a> {
    conn: &'a Connection,
}

impl<'a> StructureActivityRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_pending(&self, record: &ActivityRecord) -> DbResult<bool> {
        record.validate()?;
        if record.state != ActivityState::Pending {
            return Err(invalid("activity insertion requires the pending state"));
        }
        let transaction = if self.conn.is_autocommit() {
            Some(Transaction::new_unchecked(
                self.conn,
                TransactionBehavior::Immediate,
            )?)
        } else {
            None
        };
        let inserted = self.conn.execute(
            "INSERT INTO structure_activities (
                case_id, run_id, work_key, attempt_id, owner_id, definition_sha256,
                input_capability_ids_json, target_id, state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')
             ON CONFLICT(case_id, run_id, work_key) DO NOTHING",
            params![
                record.case_id,
                record.run_id,
                record.work_key,
                record.attempt_id,
                record.owner_id,
                record.definition_sha256,
                serde_json::to_string(&record.input_capability_ids)
                    .map_err(|_| invalid("activity input references cannot be serialized"))?,
                record.target_id,
            ],
        )? == 1;
        if !inserted {
            let prior = self
                .get(&record.case_id, &record.run_id, &record.work_key)?
                .ok_or_else(|| invalid("existing activity is unavailable during insertion"))?;
            if prior.definition_sha256 != record.definition_sha256
                || prior.input_capability_ids != record.input_capability_ids
                || prior.target_id != record.target_id
            {
                return Err(invalid(
                    "activity work key already has a different definition",
                ));
            }
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(inserted)
    }

    pub fn get(
        &self,
        case_id: &str,
        run_id: &str,
        work_key: &str,
    ) -> DbResult<Option<ActivityRecord>> {
        record::validate_key(case_id, run_id, work_key)?;
        let record = self
            .conn
            .query_row(
                "SELECT case_id, run_id, work_key, attempt_id, owner_id, definition_sha256,
                    input_capability_ids_json, target_id, state, output_receipt, failure_json
                 FROM structure_activities WHERE case_id=?1 AND run_id=?2 AND work_key=?3",
                params![case_id, run_id, work_key],
                read_record,
            )
            .optional()?;
        if let Some(record) = &record {
            record.validate()?;
        }
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn transition(
        &self,
        case_id: &str,
        run_id: &str,
        work_key: &str,
        attempt_id: &str,
        owner_id: &str,
        expected: ActivityState,
        next: ActivityState,
        receipt: Option<&str>,
        failure: Option<&TargetFailure>,
    ) -> DbResult<bool> {
        record::validate_key(case_id, run_id, work_key)?;
        record::require_text(attempt_id, "attempt_id")?;
        record::require_text(owner_id, "owner_id")?;
        expected
            .transition(next)
            .map_err(|_| invalid("activity state transition is not allowed"))?;
        record::validate_outcome(next, receipt, failure)?;
        let failure_json = failure
            .map(serde_json::to_string)
            .transpose()
            .map_err(|_| invalid("activity failure cannot be serialized"))?;
        Ok(self.conn.execute(
            "UPDATE structure_activities SET state=?7, output_receipt=?8, failure_json=?9
             WHERE case_id=?1 AND run_id=?2 AND work_key=?3 AND attempt_id=?4
                AND owner_id=?5 AND state=?6",
            params![
                case_id,
                run_id,
                work_key,
                attempt_id,
                owner_id,
                state_name(expected),
                state_name(next),
                receipt,
                failure_json,
            ],
        )? == 1)
    }
}

fn read_record(row: &Row<'_>) -> rusqlite::Result<ActivityRecord> {
    let state: String = row.get(8)?;
    let failure: Option<String> = row.get(10)?;
    Ok(ActivityRecord {
        case_id: row.get(0)?,
        run_id: row.get(1)?,
        work_key: row.get(2)?,
        attempt_id: row.get(3)?,
        owner_id: row.get(4)?,
        definition_sha256: row.get(5)?,
        input_capability_ids: decode_json(6, &row.get::<_, String>(6)?)?,
        target_id: row.get(7)?,
        state: serde_json::from_value(serde_json::Value::String(state)).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(8, Type::Text, Box::new(error))
        })?,
        output_receipt: row.get(9)?,
        failure: failure
            .as_deref()
            .map(|value| decode_json(10, value))
            .transpose()?,
    })
}

fn decode_json<T: DeserializeOwned>(column: usize, value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn state_name(state: ActivityState) -> &'static str {
    match state {
        ActivityState::Pending => "pending",
        ActivityState::Ready => "ready",
        ActivityState::Admitted => "admitted",
        ActivityState::Running => "running",
        ActivityState::Succeeded => "succeeded",
        ActivityState::Failed => "failed",
        ActivityState::Blocked => "blocked",
        ActivityState::Cancelled => "cancelled",
    }
}

fn invalid(message: &str) -> DbError {
    DbError::System(message.to_owned())
}
