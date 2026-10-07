use std::sync::Arc;

use super::super::schedule_linux_artifact_analysis;
use super::types::MemberCoordinator;

impl MemberCoordinator<'_, '_> {
    pub(super) fn schedule_linux_artifacts(&self, member_index: usize) {
        let Some(task_manager) = self.task_manager.as_ref() else {
            return;
        };
        let source_id = match app_services::cluster_service::get_linux_import_member_source_id(
            self.connection,
            &self.job.case_id,
            &self.job.plan.import_set_id,
            member_index,
        ) {
            Ok(Some(source_id)) => source_id,
            Ok(None) => {
                tracing::warn!(
                    member_index,
                    "Linux member has no data source for deferred artifacts"
                );
                return;
            }
            Err(error) => {
                tracing::warn!(member_index, error = %error, "Failed to resolve Linux member for deferred artifacts");
                return;
            }
        };
        match schedule_linux_artifact_analysis(
            &self.job.case_root,
            &self.job.case_id,
            &source_id,
            self.app,
            Arc::clone(task_manager),
        ) {
            Ok(true) => {
                tracing::info!(member_index, data_source_id = %source_id.0, "Deferred Linux artifact analysis scheduled")
            }
            Ok(false) => {
                tracing::debug!(member_index, data_source_id = %source_id.0, "Deferred Linux artifact analysis not needed")
            }
            Err(error) => {
                tracing::warn!(member_index, data_source_id = %source_id.0, error = %error.message, "Failed to schedule deferred Linux artifact analysis")
            }
        }
    }
}
