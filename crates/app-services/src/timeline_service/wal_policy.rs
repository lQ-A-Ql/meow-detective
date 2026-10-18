//! Connection-local WAL policy; existing source durability settings are retained.
use super::TimelineServiceError;
use rusqlite::Connection;

pub(super) fn with_wal_policy<T>(
    conn: &Connection,
    pages: Option<u32>,
    operation: impl FnOnce() -> Result<T, TimelineServiceError>,
) -> Result<T, TimelineServiceError> {
    let Some(pages) = pages else {
        return operation();
    };
    let previous = conn.pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))?;
    let mut guard = WalPolicyGuard {
        conn,
        previous,
        pending: true,
    };
    conn.pragma_update(None, "wal_autocheckpoint", pages)?;
    let result = operation();
    let restored = guard.restore();
    match result {
        Ok(value) => restored.map(|()| value).map_err(Into::into),
        Err(error) => {
            if let Err(restore_error) = restored {
                tracing::warn!(%restore_error, "WAL policy restoration failed after projection error");
            }
            Err(error)
        }
    }
}
struct WalPolicyGuard<'a> {
    conn: &'a Connection,
    previous: u32,
    pending: bool,
}
impl WalPolicyGuard<'_> {
    fn restore(&mut self) -> rusqlite::Result<()> {
        self.conn
            .pragma_update(None, "wal_autocheckpoint", self.previous)?;
        self.pending = false;
        Ok(())
    }
}
impl Drop for WalPolicyGuard<'_> {
    fn drop(&mut self) {
        if self.pending {
            if let Err(error) = self.restore() {
                tracing::warn!(%error, "WAL policy restoration failed during drain");
            }
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/timeline_service/wal_policy.rs"]
mod tests;
