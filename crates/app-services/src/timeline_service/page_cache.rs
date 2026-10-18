//! Restore the connection-local Timeline cache on return and during unwinding.
use rusqlite::Connection;

use super::TimelineServiceError;

pub(super) fn with_timeline_page_cache<T>(
    connection: &Connection,
    operation: impl FnOnce() -> Result<T, TimelineServiceError>,
) -> Result<T, TimelineServiceError> {
    let previous = connection.pragma_query_value(None, "cache_size", |row| row.get(0))?;
    let mut guard = PageCacheGuard {
        connection,
        previous,
        restore_pending: true,
    };
    connection.pragma_update(None, "cache_size", -1_000_000)?;
    let result = operation();
    let restored = guard.restore();
    match result {
        Ok(value) => restored.map(|()| value).map_err(Into::into),
        Err(error) => {
            if let Err(restore_error) = restored {
                tracing::warn!(%restore_error, "Timeline cache restoration failed after projection error");
            }
            Err(error)
        }
    }
}

struct PageCacheGuard<'a> {
    connection: &'a Connection,
    previous: i64,
    restore_pending: bool,
}

impl PageCacheGuard<'_> {
    fn restore(&mut self) -> rusqlite::Result<()> {
        self.connection
            .pragma_update(None, "cache_size", self.previous)?;
        self.restore_pending = false;
        Ok(())
    }
}

impl Drop for PageCacheGuard<'_> {
    fn drop(&mut self) {
        if self.restore_pending {
            if let Err(error) = self.restore() {
                tracing::warn!(%error, "Timeline cache restoration failed while draining guard");
            }
        }
    }
}
