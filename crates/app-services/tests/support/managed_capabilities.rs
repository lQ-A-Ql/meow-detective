use crate::{
    active_case::ActiveCase, bitlocker_runtime::BitLockerUnlockRegistry, post_import_service,
    task_runtime::TaskManager,
};
use std::{sync::Arc, time::Duration};

pub fn run(
    active: &ActiveCase,
    events: Option<Arc<dyn post_import_service::ApplicationEventSink>>,
    timeout: Duration,
) {
    let runtime = post_import_service::PostImportRuntime {
        tasks: Arc::new(TaskManager::new()),
        bitlocker: Arc::new(BitLockerUnlockRegistry::default()),
        events,
    };
    assert!(
        post_import_service::enqueue_case_pending(&runtime, &active.meta.id, &active.case_root,)
            .unwrap()
            > 0
    );
    for (task, result) in runtime.tasks.wait_all(timeout) {
        result.unwrap_or_else(|error| panic!("{task}: {error}"));
    }
    assert_eq!(
        runtime.tasks.task_count(),
        0,
        "managed tasks exceeded timeout"
    );
    let connection = persistence_sqlite::open_existing(&active.db_path()).unwrap();
    let matrix = crate::capability_service::derive_capability_matrix(
        &connection,
        &active.case_root,
        &active.meta.id,
    )
    .unwrap();
    for row in matrix.sources.iter().filter(|row| {
        matches!(
            row.capability,
            transport::dto::CapabilityKindDto::Artifacts
                | transport::dto::CapabilityKindDto::Timeline
        )
    }) {
        assert!(
            matches!(
                row.state,
                transport::dto::CapabilityStateDto::Ready
                    | transport::dto::CapabilityStateDto::NotApplicable
            ),
            "managed capability did not complete: {row:?}"
        );
    }
}
