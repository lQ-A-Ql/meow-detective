use std::collections::HashSet;

use rusqlite::Connection;

use crate::connection::DbResult;

pub(super) fn current_version(
    conn: &Connection,
    case_migrations: &[(&str, &str)],
    source_migrations: &[(&str, &str)],
) -> DbResult<Option<String>> {
    let has_table: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(None);
    }

    let mut statement = conn.prepare("SELECT name FROM schema_migrations ORDER BY id")?;
    let applied = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if applied.is_empty() {
        return Ok(None);
    }
    let registry = if applied.iter().any(|name| name.starts_with("source_")) {
        source_migrations
    } else {
        case_migrations
    };
    let known = registry
        .iter()
        .map(|(name, _)| *name)
        .collect::<HashSet<_>>();
    if let Some(unknown) = applied
        .iter()
        .rev()
        .find(|name| !known.contains(name.as_str()))
    {
        return Ok(Some(unknown.clone()));
    }
    let applied = applied.iter().map(String::as_str).collect::<HashSet<_>>();
    Ok(registry
        .iter()
        .rev()
        .find(|(name, _)| applied.contains(name))
        .map(|(name, _)| (*name).to_string()))
}
