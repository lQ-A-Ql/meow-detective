use crate::timeline_service::{
    wal_policy::with_wal_policy, TimelineProjectionPolicy, TimelineServiceError,
};
use rusqlite::{
    hooks::{AuthAction, AuthContext, Authorization},
    Connection,
};

fn pages(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))
        .unwrap()
}

#[test]
fn policy_restores_after_success_error_cancellation_and_unwind() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "wal_autocheckpoint", 1000)
        .unwrap();
    assert_eq!(
        with_wal_policy(&conn, Some(16384), || {
            assert_eq!(pages(&conn), 16384);
            Ok(17)
        })
        .unwrap(),
        17
    );
    assert_eq!(pages(&conn), 1000);
    for error in [
        TimelineServiceError::Cancelled,
        TimelineServiceError::Other("failed".into()),
    ] {
        assert!(with_wal_policy(&conn, Some(32768), || Err::<(), _>(error)).is_err());
        assert_eq!(pages(&conn), 1000);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = with_wal_policy(
            &conn,
            Some(16384),
            || -> Result<(), TimelineServiceError> {
                panic!("test WAL policy unwind");
            },
        );
    }));
    assert!(result.is_err());
    assert_eq!(pages(&conn), 1000);
}

#[test]
fn restore_failure_is_not_success_and_preserves_primary_error() {
    for cancel in [false, true] {
        let conn = Connection::open_in_memory().unwrap();
        let result = with_wal_policy(&conn, Some(16384), || {
            conn.authorizer(Some(|context: AuthContext<'_>| match context.action {
                AuthAction::Pragma {
                    pragma_name: "wal_autocheckpoint",
                    pragma_value: Some("1000"),
                } => Authorization::Deny,
                _ => Authorization::Allow,
            }));
            if cancel {
                Err(TimelineServiceError::Cancelled)
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        if cancel {
            assert!(matches!(result, Err(TimelineServiceError::Cancelled)));
        }
        conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    }
}

#[test]
fn invalid_policy_is_rejected_before_database_work() {
    for policy in [
        TimelineProjectionPolicy {
            cache_kib: 0,
            ..Default::default()
        },
        TimelineProjectionPolicy {
            cache_kib: 1024 * 1024 + 1,
            ..Default::default()
        },
        TimelineProjectionPolicy {
            graph_batch_rows: 0,
            ..Default::default()
        },
        TimelineProjectionPolicy {
            graph_batch_rows: 100001,
            ..Default::default()
        },
        TimelineProjectionPolicy {
            wal_autocheckpoint_pages: Some(0),
            ..Default::default()
        },
        TimelineProjectionPolicy {
            wal_autocheckpoint_pages: Some(65537),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            policy.validate(),
            Err(TimelineServiceError::InvalidInput(_))
        ));
    }
}
