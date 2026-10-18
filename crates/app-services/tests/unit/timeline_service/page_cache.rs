use crate::timeline_service::{projection::with_timeline_page_cache, TimelineServiceError};
use rusqlite::{
    hooks::{AuthAction, AuthContext, Authorization},
    Connection,
};

fn cache_size(conn: &Connection) -> i64 {
    conn.pragma_query_value(None, "cache_size", |row| row.get(0))
        .unwrap()
}

#[test]
fn temporary_cache_is_restored_after_success_error_and_cancellation() {
    for previous in [321_i64, -2048] {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "cache_size", previous).unwrap();
        let result = with_timeline_page_cache(&conn, || {
            assert_eq!(cache_size(&conn), -1_000_000);
            Ok(17)
        })
        .unwrap();
        assert_eq!(result, 17);
        assert_eq!(cache_size(&conn), previous);
        for error in [
            TimelineServiceError::Cancelled,
            TimelineServiceError::Other("failed".into()),
        ] {
            let result = with_timeline_page_cache(&conn, || Err::<(), _>(error));
            assert!(result.is_err());
            assert_eq!(cache_size(&conn), previous);
        }
    }
}

#[test]
fn temporary_cache_is_restored_during_unwinding() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "cache_size", 321).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = with_timeline_page_cache(&conn, || -> Result<(), TimelineServiceError> {
            assert_eq!(cache_size(&conn), -1_000_000);
            panic!("test projection unwinding");
        });
    }));
    assert!(result.is_err());
    assert_eq!(cache_size(&conn), 321);
}

#[test]
fn cache_restore_failure_cannot_be_reported_as_success() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "cache_size", 321).unwrap();
    let result = with_timeline_page_cache(&conn, || {
        conn.authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::Pragma {
                pragma_name: "cache_size",
                pragma_value: Some("321"),
            } => Authorization::Deny,
            _ => Authorization::Allow,
        }));
        Ok(())
    });
    assert!(result.is_err());
    conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
}
