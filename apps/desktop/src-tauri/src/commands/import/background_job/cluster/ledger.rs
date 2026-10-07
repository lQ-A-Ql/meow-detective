use super::super::types::BackgroundLinuxEvidenceSetImportJob;

pub(super) fn record_cluster_phase(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxEvidenceSetImportJob,
    phase: &str,
    success: bool,
    error: Option<&str>,
    ready_count: u32,
    failed_count: u32,
) {
    if let Err(record_error) = app_services::cluster_service::record_linux_import_phase(
        connection,
        &job.case_root,
        app_services::cluster_service::LinuxImportPhaseInput {
            case_id: &job.case_id,
            import_set_id: &job.plan.import_set_id,
            phase,
            success,
            error,
            ready_count,
            failed_count,
        },
    ) {
        tracing::warn!(
            import_set_id = %job.plan.import_set_id,
            phase,
            error = %record_error,
            "Failed to record Linux evidence-set phase ledger entry"
        );
    }
}
