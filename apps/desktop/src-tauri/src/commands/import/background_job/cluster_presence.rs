use super::types::BackgroundLinuxClusterImportJob;
use app_services::cluster_service;

pub(super) fn assess_cephfs_presence(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxClusterImportJob,
) {
    match cluster_service::assess_linux_cluster_cephfs_presence(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan.cluster_id,
    ) {
        Ok(assessment) => {
            tracing::info!(
                cluster_id = %job.plan.cluster_id,
                state = %assessment.state,
                source_count = assessment.source_count,
                filesystem_count = assessment.filesystem_count,
                diagnostics = assessment.diagnostics.len(),
                "Linux cluster CephFS presence assessed; namespace reconstruction remains gated"
            );
        }
        Err(error) => {
            tracing::warn!(
                cluster_id = %job.plan.cluster_id,
                error = %error,
                "Linux cluster CephFS presence assessment was unavailable; no CephFS source was created"
            );
        }
    }
}
