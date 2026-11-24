use crate::timeline_service::{
    rebuild_timeline_projection_with_progress, TimelineProjectionPolicy, TimelineServiceError,
};
use import_core::post_import_service::{
    CapabilityOutcome, TimelineProjectionExecutor, TimelineProjectionRequest,
};
use transport::{dto::TimelineProjectionStateDto, ServiceErrorCategory};

pub(super) struct SourceTimelineProjection;

impl TimelineProjectionExecutor for SourceTimelineProjection {
    fn project(&self, request: TimelineProjectionRequest<'_>) -> CapabilityOutcome {
        let result = rebuild_timeline_projection_with_progress(
            request.conn,
            request.case_root,
            request.case_id,
            Some(request.source_id),
            TimelineProjectionPolicy::default(),
            request.cancel,
            request.progress,
        );
        match result {
            Ok(status) => match status
                .sources
                .iter()
                .find(|source| source.data_source_id == request.source_id)
            {
                Some(source) if source.state == TimelineProjectionStateDto::Complete => {
                    CapabilityOutcome {
                        timeline_events: source.inserted_events,
                        ..CapabilityOutcome::terminal("ready")
                    }
                }
                Some(source) if source.state == TimelineProjectionStateDto::Deferred => {
                    CapabilityOutcome {
                        detail: source.reason.clone(),
                        ..CapabilityOutcome::terminal("deferred")
                    }
                }
                _ => CapabilityOutcome::failed("TIMELINE_NOT_READY"),
            },
            Err(TimelineServiceError::Cancelled) => CapabilityOutcome::terminal("cancelled"),
            Err(error) => {
                tracing::error!(error = %error, "post-import timeline projection failed");
                CapabilityOutcome::failed(error.code().unwrap_or("TIMELINE_REBUILD_FAILED"))
            }
        }
    }
}
