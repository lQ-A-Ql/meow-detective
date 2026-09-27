use std::sync::Arc;

use domain::DataSourceId;

use super::super::schedule_linux_artifact_analysis;
use super::types::MemberCoordinator;

impl MemberCoordinator<'_, '_> {
    pub(super) fn schedule_linux_artifacts(&self, member_index: usize) {
        let Some(task_manager) = self.task_manager.as_ref() else {
            return;
        };
        let source_id = self
            .connection
            .query_row(
                "SELECT data_source_id FROM linux_import_set_members
                 WHERE import_set_id = ?1 AND member_index = ?2",
                rusqlite::params![self.job.plan.import_set_id, member_index as u32],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
            .map(DataSourceId);
        let Some(source_id) = source_id else {
            tracing::warn!(
                member_index,
                "Linux member has no data source for deferred artifacts"
            );
            return;
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
