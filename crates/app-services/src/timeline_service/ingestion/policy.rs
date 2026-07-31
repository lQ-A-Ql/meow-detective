pub(crate) fn retain_registry_events(events: &mut Vec<domain::TimelineEvent>) {
    events.retain(|event| {
        timeline::TimelineEventKind::parse(&event.event_type)
            .is_some_and(timeline::TimelineEventKind::is_registry)
    });
}
