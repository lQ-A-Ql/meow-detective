use domain::DataSourceId;

use crate::ceph_reconstruction::derived_finalizer::DerivedFinalizationReport;

pub(super) fn log_finalization_report(
    data_source_id: &DataSourceId,
    report: &DerivedFinalizationReport,
) {
    let failed = report.failed_count();
    let deferred = report.deferred_count();
    if failed > 0 {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            failed_phases = failed,
            deferred_phases = deferred,
            "RBD derived source is ready, but post-catalog processing is incomplete"
        );
    } else {
        tracing::info!(
            data_source_id = %data_source_id.0,
            deferred_phases = deferred,
            "RBD derived source post-catalog processing completed"
        );
    }
}
