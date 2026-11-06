use crate::{repositories::fingerprint_repo::ForensicFingerprintRepo, DbResult};
use domain::FileEntry;
use rusqlite::{Connection, OptionalExtension};

pub(super) fn files(conn: &Connection, entries: &[FileEntry]) -> DbResult<()> {
    let Some(first) = entries.first() else {
        return Ok(());
    };
    let case: Option<String> = conn
        .query_row(
            "SELECT case_id FROM data_sources WHERE id = ?1",
            [&first.data_source_id.0],
            |row| row.get(0),
        )
        .optional()?;
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        persist(&tx, entries, case.as_deref())?;
        tx.commit()?;
    } else {
        persist(conn, entries, case.as_deref())?;
    }
    Ok(())
}

fn persist(conn: &Connection, entries: &[FileEntry], case: Option<&str>) -> DbResult<()> {
    let fingerprints = ForensicFingerprintRepo::new(conn);
    for entry in entries {
        fingerprints
            .upsert_in_transaction(&domain::ForensicFingerprint::for_file_entry(entry, case))?;
    }
    Ok(())
}

pub(super) fn artifacts(
    conn: &Connection,
    records: &[persistence_fjall::ArtifactRecord],
    case: &str,
    source: &str,
) -> DbResult<()> {
    let fingerprints = ForensicFingerprintRepo::new(conn);
    for record in records {
        let artifact = crate::repositories::artifact_repo::decode_fjall_artifact(record.clone())?;
        fingerprints.upsert_in_transaction(&domain::ForensicFingerprint::for_artifact(
            &artifact, case, source,
        ))?;
    }
    Ok(())
}
