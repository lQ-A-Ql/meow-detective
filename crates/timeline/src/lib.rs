mod event_kind;
mod file_activity;
mod projection_policy;
mod timestamp_policy;

pub use event_kind::TimelineEventKind;
pub use file_activity::project_file_activity;
pub use projection_policy::{TimelineProjectionPolicy, TimelineProjectionPolicyError};
pub use timestamp_policy::is_meaningful_timestamp;

pub fn retain_supported_events(events: &mut Vec<domain::TimelineEvent>) {
    events.retain(|event| TimelineEventKind::parse(&event.event_type).is_some());
}

pub fn retain_analysis_events(events: &mut Vec<domain::TimelineEvent>) {
    events.retain(|event| {
        TimelineEventKind::parse(&event.event_type)
            .is_some_and(TimelineEventKind::is_analysis_event)
    });
}

#[cfg(test)]
#[path = "../tests/unit/timeline.rs"]
mod tests;
