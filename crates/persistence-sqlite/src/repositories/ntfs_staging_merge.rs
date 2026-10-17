//! Completed validated numeric catalogs generate external ids only at final merge.
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn merge_completed_numeric(
    main: &Connection,
    staging: &Connection,
    source: &str,
    partition: usize,
    root: &str,
) -> rusqlite::Result<Option<u64>> {
    let numeric: bool = staging.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='file_entries' AND type='view')",
        [],
        |r| r.get(0),
    )?;
    if !numeric {
        return Ok(None);
    }
    let meta = |key: &str| {
        staging
            .query_row(
                "SELECT value FROM ntfs_staging_meta WHERE key=?1",
                [key],
                |r| r.get::<_, String>(0),
            )
            .optional()
    };
    if meta("source_id")?.as_deref() != Some(source)
        || meta("partition")?.as_deref() != Some(partition.to_string().as_str())
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "numeric staging identity mismatch".into(),
        ));
    }
    if meta("tree_validated")?.as_deref() != Some("1")
        || meta("paths_complete")?.as_deref() != Some("1")
    {
        return Ok(None);
    }
    let done: bool = staging.query_row(
        "SELECT EXISTS(SELECT 1 FROM staging_meta WHERE key='status' AND value='done')",
        [],
        |r| r.get(0),
    )?;
    if !done {
        return Ok(None);
    }
    let rows = main.execute(
        include_str!("ntfs_staging/merge.sql"),
        params![root, partition as i64],
    )?;
    Ok(Some(rows as u64))
}
