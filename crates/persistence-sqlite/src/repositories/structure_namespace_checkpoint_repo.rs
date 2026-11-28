mod record;

pub use record::{NamespaceCheckpointRecord, NamespacePageCommit};

use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Row, Transaction, TransactionBehavior,
};

use crate::{DbError, DbResult};

pub struct StructureNamespaceCheckpointRepo<'a> {
    conn: &'a Connection,
}

struct StoredCheckpoint {
    record: NamespaceCheckpointRecord,
    last_page_identity: Option<String>,
}

impl<'a> StructureNamespaceCheckpointRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_pending(&self, record: &NamespaceCheckpointRecord) -> DbResult<bool> {
        record.validate()?;
        if record.next_page != 0 {
            return Err(invalid(
                "checkpoint insertion requires uncommitted initial state",
            ));
        }
        let transaction = self.begin_write()?;
        let inserted = self.conn.execute(
            "INSERT INTO structure_namespace_checkpoints (
                case_id,run_id,target_id,snapshot_id,query_sha256,ordering_id,attempt_id,
                owner_id,next_page,next_ordinal,verified_objects,eof
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?9,0)
             ON CONFLICT(case_id,run_id,target_id) DO NOTHING",
            params![
                record.case_id,
                record.run_id,
                record.target_id,
                record.snapshot_id,
                record.query_sha256,
                record.ordering_id,
                record.attempt_id,
                record.owner_id,
                0u64.to_be_bytes().as_slice(),
            ],
        )? == 1;
        if !inserted {
            let current = self
                .find_stored(&record.case_id, &record.run_id, &record.target_id)?
                .ok_or_else(|| invalid("checkpoint disappeared during insertion"))?;
            require_definition(&current.record, record)?;
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
        target_id: &str,
    ) -> DbResult<Option<NamespaceCheckpointRecord>> {
        Ok(self
            .find_stored(case_id, run_id, target_id)?
            .map(|stored| stored.record))
    }

    /// Returns true for a new page and false for an identical retry or a lost CAS.
    pub fn commit_page(
        &self,
        expected: &NamespaceCheckpointRecord,
        page: &NamespacePageCommit,
    ) -> DbResult<bool> {
        expected.validate()?;
        page.validate()?;
        if page.first_ordinal != expected.next_ordinal {
            return Err(invalid(
                "page first ordinal differs from its expected checkpoint",
            ));
        }
        let next_page = expected
            .next_page
            .checked_add(1)
            .ok_or_else(|| invalid("checkpoint page number overflow"))?;
        let next_ordinal = page
            .first_ordinal
            .checked_add(page.object_count)
            .ok_or_else(|| invalid("checkpoint object ordinal overflow"))?;
        let verified = expected
            .verified_objects
            .checked_add(page.object_count)
            .ok_or_else(|| invalid("checkpoint verified object count overflow"))?;
        let identity = page.identity(expected.next_page)?;
        let transaction = self.begin_write()?;
        let Some(stored) =
            self.find_stored(&expected.case_id, &expected.run_id, &expected.target_id)?
        else {
            return Ok(false);
        };
        require_definition(&stored.record, expected)?;
        if stored.record.attempt_id != expected.attempt_id
            || stored.record.owner_id != expected.owner_id
        {
            return Ok(false);
        }
        if stored.record.next_page == next_page {
            if stored.last_page_identity.as_deref() == Some(identity.as_str())
                && stored.record.next_ordinal == next_ordinal
                && stored.record.verified_objects == verified
                && stored.record.next_cursor == page.next_cursor
                && stored.record.eof == page.eof
                && stored.record.last_page_sha256.as_deref() == Some(page.page_sha256.as_str())
                && stored.record.output_receipt.as_deref() == Some(page.output_receipt.as_str())
            {
                return Ok(false);
            }
            return Err(invalid(
                "a different page was already committed at this page number",
            ));
        }
        if stored.record.eof {
            return Err(invalid(
                "a completed namespace checkpoint cannot accept another page",
            ));
        }
        if stored.record != *expected {
            return Ok(false);
        }
        let committed =
            self.update_page(expected, page, next_page, next_ordinal, verified, &identity)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(committed)
    }

    fn begin_write(&self) -> DbResult<Option<Transaction<'_>>> {
        self.conn
            .is_autocommit()
            .then(|| Transaction::new_unchecked(self.conn, TransactionBehavior::Immediate))
            .transpose()
            .map_err(Into::into)
    }

    fn find_stored(
        &self,
        case: &str,
        run: &str,
        target: &str,
    ) -> DbResult<Option<StoredCheckpoint>> {
        record::require_text(case, "case_id")?;
        record::require_text(run, "run_id")?;
        record::require_text(target, "target_id")?;
        let stored = self
            .conn
            .query_row(
                "SELECT case_id,run_id,target_id,snapshot_id,query_sha256,ordering_id,attempt_id,
                owner_id,next_page,next_ordinal,verified_objects,next_cursor,eof,
                last_page_sha256,output_receipt,last_page_identity
             FROM structure_namespace_checkpoints WHERE case_id=?1 AND run_id=?2 AND target_id=?3",
                params![case, run, target],
                read_record,
            )
            .optional()?;
        if let Some(stored) = &stored {
            stored.record.validate()?;
            if stored.record.next_page == 0 && stored.last_page_identity.is_some()
                || stored.record.next_page != 0 && stored.last_page_identity.is_none()
            {
                return Err(invalid("checkpoint last page identity is inconsistent"));
            }
        }
        Ok(stored)
    }

    #[allow(clippy::too_many_arguments)]
    fn update_page(
        &self,
        expected: &NamespaceCheckpointRecord,
        page: &NamespacePageCommit,
        next_page: u64,
        next_ordinal: u64,
        verified: u64,
        identity: &str,
    ) -> DbResult<bool> {
        Ok(self.conn.execute(
            "UPDATE structure_namespace_checkpoints SET next_page=?9,next_ordinal=?10,
                verified_objects=?11,next_cursor=?12,eof=?13,last_page_sha256=?14,
                output_receipt=?15,last_page_identity=?16
             WHERE case_id=?1 AND run_id=?2 AND target_id=?3 AND attempt_id=?4 AND owner_id=?5
                AND next_page=?6 AND next_ordinal=?7 AND verified_objects=?8 AND eof=0",
            params![
                expected.case_id,
                expected.run_id,
                expected.target_id,
                expected.attempt_id,
                expected.owner_id,
                expected.next_page.to_be_bytes().as_slice(),
                expected.next_ordinal.to_be_bytes().as_slice(),
                expected.verified_objects.to_be_bytes().as_slice(),
                next_page.to_be_bytes().as_slice(),
                next_ordinal.to_be_bytes().as_slice(),
                verified.to_be_bytes().as_slice(),
                page.next_cursor,
                page.eof,
                page.page_sha256,
                page.output_receipt,
                identity,
            ],
        )? == 1)
    }
}

fn require_definition(
    actual: &NamespaceCheckpointRecord,
    expected: &NamespaceCheckpointRecord,
) -> DbResult<()> {
    if actual.snapshot_id != expected.snapshot_id
        || actual.query_sha256 != expected.query_sha256
        || actual.ordering_id != expected.ordering_id
    {
        return Err(invalid(
            "checkpoint snapshot, query or ordering conflicts with its identity",
        ));
    }
    Ok(())
}

fn read_record(row: &Row<'_>) -> rusqlite::Result<StoredCheckpoint> {
    Ok(StoredCheckpoint {
        record: NamespaceCheckpointRecord {
            case_id: row.get(0)?,
            run_id: row.get(1)?,
            target_id: row.get(2)?,
            snapshot_id: row.get(3)?,
            query_sha256: row.get(4)?,
            ordering_id: row.get(5)?,
            attempt_id: row.get(6)?,
            owner_id: row.get(7)?,
            next_page: read_u64(row, 8)?,
            next_ordinal: read_u64(row, 9)?,
            verified_objects: read_u64(row, 10)?,
            next_cursor: row.get(11)?,
            eof: row.get(12)?,
            last_page_sha256: row.get(13)?,
            output_receipt: row.get(14)?,
        },
        last_page_identity: row.get(15)?,
    })
}

fn read_u64(row: &Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let bytes: Vec<u8> = row.get(column)?;
    let encoded: [u8; 8] = bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            Type::Blob,
            "invalid u64 checkpoint counter".into(),
        )
    })?;
    Ok(u64::from_be_bytes(encoded))
}

fn invalid(message: &str) -> DbError {
    DbError::System(message.into())
}
