mod event_kind;
mod file_activity;
mod timestamp_policy;

pub use event_kind::TimelineEventKind;
pub use file_activity::project_file_activity;
pub use timestamp_policy::is_meaningful_timestamp;

pub fn retain_supported_events(events: &mut Vec<domain::TimelineEvent>) {
    events.retain(|event| TimelineEventKind::parse(&event.event_type).is_some());
}

#[cfg(test)]
#[path = "../tests/unit/timeline.rs"]
mod tests;
