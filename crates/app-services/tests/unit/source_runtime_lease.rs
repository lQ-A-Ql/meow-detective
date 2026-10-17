use super::{invalidate_case, invalidate_source};
use crate::source_runtime::{open_factory, SourceIdentity, SourceRuntime};
use domain::{DataSourceId, DataSourceKind};

fn runtime(temp: &tempfile::TempDir) -> SourceRuntime {
    let path = temp.path().join("source.raw");
    std::fs::write(&path, [0; 512]).unwrap();
    SourceRuntime::new(
        SourceIdentity {
            case_id: temp.path().to_string_lossy().into_owned(),
            data_source_id: DataSourceId("source".into()),
            revision: 9,
            source_kind: DataSourceKind::Raw,
            path: path.clone(),
        },
        open_factory(&path, &DataSourceKind::Raw).unwrap(),
    )
}

#[test]
fn superseded_generation_cannot_open_or_commit_even_when_factory_survives() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime(&temp);
    let old = runtime.publication_lease();
    let old_factory = runtime.factory();
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER PRIMARY KEY)")
        .unwrap();
    let fence = old.fence_connection(&c);
    c.execute_batch("BEGIN; INSERT INTO output VALUES(1)")
        .unwrap();
    runtime.bump_generation();
    assert!(old_factory.open_reader().is_err());
    assert!(old.validate().is_err());
    assert!(c.execute_batch("COMMIT").is_err());
    assert!(c.is_autocommit());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM output", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    drop(fence);
    let _fresh = runtime.publication_lease().fence_connection(&c);
    c.execute_batch("INSERT INTO output VALUES(2)").unwrap();
}

#[test]
fn physical_member_change_between_begin_and_commit_rolls_back_output() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime(&temp);
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER PRIMARY KEY)")
        .unwrap();
    let lease = runtime.publication_lease();
    let _fence = lease.fence_connection(&c);
    c.execute_batch("BEGIN; INSERT INTO output VALUES(1)")
        .unwrap();
    std::fs::write(runtime.source_path(), [0; 1024]).unwrap();
    assert!(c.execute_batch("COMMIT").is_err());
    assert!(c.is_autocommit());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM output", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn invalidation_inside_sql_callback_never_waits_for_the_current_publisher() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime(&temp);
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER PRIMARY KEY)")
        .unwrap();
    let case_id = runtime.identity().case_id.clone();
    c.create_scalar_function(
        "retire_source",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        move |_| {
            invalidate_source(&case_id, "source");
            Ok(0)
        },
    )
    .unwrap();
    c.execute_batch(
        "CREATE TRIGGER retire BEFORE INSERT ON output BEGIN SELECT retire_source(); END",
    )
    .unwrap();
    let lease = runtime.publication_lease();
    let _fence = lease.fence_connection(&c);
    assert!(c.execute_batch("INSERT INTO output VALUES(1)").is_err());
    assert!(lease.validate().is_err());
}

#[test]
fn case_invalidation_cancels_only_its_live_leases_and_weak_entries_do_not_own_runtime() {
    let first_temp = tempfile::tempdir().unwrap();
    let second_temp = tempfile::tempdir().unwrap();
    let first = runtime(&first_temp);
    let second = runtime(&second_temp);
    let old = first.publication_lease();
    invalidate_case(&first.identity().case_id);
    assert!(old.validate().is_err());
    assert!(first.open_reader().is_err());
    assert!(second.open_reader().is_ok());
    drop(first);
    assert!(old.validate().is_err());
    drop(old);
    let registry = super::EPOCHS.lock().unwrap_or_else(|p| p.into_inner());
    assert!(registry
        .get(&(
            first_temp.path().to_string_lossy().into_owned(),
            "source".into()
        ))
        .and_then(std::sync::Weak::upgrade)
        .is_none());
}

#[test]
fn actual_preview_lifecycle_invalidates_an_import_scope_without_open_preview_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime(&temp);
    let registry = crate::file_service::PreviewRuntimeRegistry::default();
    registry
        .invalidate_source(&runtime.identity().case_id, "source")
        .unwrap();
    assert!(runtime.publication_lease().validate().is_err());
}

#[test]
fn closing_and_reactivating_preview_case_never_revalidates_the_old_import_lease() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime(&temp);
    let lease = runtime.publication_lease();
    let registry = crate::file_service::PreviewRuntimeRegistry::default();
    assert!(registry
        .retire_case_and_drain(&runtime.identity().case_id, std::time::Duration::ZERO)
        .unwrap());
    registry
        .reactivate_case(&runtime.identity().case_id)
        .unwrap();
    assert!(lease.validate().is_err());
    assert!(runtime.factory().open_reader().is_err());
}

#[test]
fn superseded_failure_cannot_overwrite_newer_ready_even_after_new_runtime_drops() {
    let temp = tempfile::tempdir().unwrap();
    let old = runtime(&temp);
    let lease = old.publication_lease();
    let new = SourceRuntime::new(
        old.identity().clone(),
        open_factory(old.source_path(), &DataSourceKind::Raw).unwrap(),
    );
    assert!(!lease.owns_failure_state());
    drop(new);
    assert!(!lease.owns_failure_state());
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(state TEXT); INSERT INTO output VALUES('ready')")
        .unwrap();
    let _fence = lease.failure_fence_connection(&c);
    assert!(c.execute_batch("UPDATE output SET state='failed'").is_err());
    assert_eq!(
        c.query_row("SELECT state FROM output", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ready"
    );
}

#[test]
fn preview_refresh_preserves_import_lease_but_source_retirement_revokes_it() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime(&temp);
    let registry = crate::file_service::PreviewRuntimeRegistry::default();
    assert!(registry
        .retire_source_preview_only_and_drain(
            &runtime.identity().case_id,
            "source",
            std::time::Duration::ZERO
        )
        .unwrap());
    runtime.publication_lease().validate().unwrap();
    registry
        .reactivate_source(&runtime.identity().case_id, "source")
        .unwrap();
    registry
        .retire_source_and_drain(
            &runtime.identity().case_id,
            "source",
            std::time::Duration::ZERO,
        )
        .unwrap();
    assert!(runtime.publication_lease().validate().is_err());
}

#[test]
fn scoped_raw_reader_survives_workspace_and_factory_then_returns_its_bytes() {
    use crate::resource_budget::{ResourcePool, Resources, MIB};
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime(&temp);
    let root = ResourcePool::new(Resources {
        bytes: 1536 * MIB,
        workers: 4,
        ..Default::default()
    });
    let workspace = root
        .workspace(&std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    runtime.publication_lease().bind_pool(workspace.pool());
    let reader = runtime.open_reader().unwrap();
    let resident = runtime.factory().reader_memory_bound();
    assert_eq!(workspace.pool().snapshot().0.bytes, resident);
    drop(workspace);
    assert_eq!(root.snapshot().0.bytes, resident);
    drop(runtime);
    assert_eq!(root.snapshot().0.bytes, resident);
    drop(reader);
    assert_eq!(root.snapshot().0, Resources::default());
}
#[test]
fn factory_e01_plan_is_reused_inside_workspace_without_metadata_reparse() {
    use crate::resource_budget::{ResourcePool, Resources, MIB};
    let path = testing::fixtures::tiny_e01_image();
    let temp = tempfile::tempdir().unwrap();
    let case = temp.path().to_string_lossy().into_owned();
    let factory = crate::source_runtime::factory::open_factory_for_case(
        &path,
        &DataSourceKind::E01,
        &std::sync::atomic::AtomicBool::new(false),
        &case,
    )
    .unwrap();
    let runtime = SourceRuntime::new(
        SourceIdentity {
            case_id: case.clone(),
            data_source_id: DataSourceId("e01".into()),
            revision: 0,
            source_kind: DataSourceKind::E01,
            path: path.clone(),
        },
        factory,
    );
    let root = ResourcePool::new(Resources {
        bytes: 1536 * MIB,
        workers: 4,
        ..Default::default()
    });
    let workspace = root
        .workspace(&std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    runtime.publication_lease().bind_pool(workspace.pool());
    let a = crate::open_e01_reader_cached(&path, &case).unwrap();
    assert_eq!(
        workspace.pool().snapshot().0.bytes,
        runtime.factory().reader_memory_bound()
    );
    drop(workspace);
    let b = runtime.open_reader().unwrap();
    assert_eq!(
        root.snapshot().0.bytes,
        runtime.factory().reader_memory_bound()
    );
    drop(a);
    assert_eq!(root.snapshot().0.bytes, 0);
    drop(b);
    crate::clear_e01_reader_cache_for_case(&case);
}
