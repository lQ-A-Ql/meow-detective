use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use chrono::{DateTime, Utc};
use serde_json::Value;

pub fn extract_macos_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let normalized = normalize_evidence_path(&candidate.path);

    if normalized.contains("/.fseventsd/") {
        extract_fsevents(candidate, bytes, &mut outcome);
    } else if normalized.contains("com.apple.launchservices") {
        extract_launch_services(candidate, bytes, &mut outcome);
    } else if normalized.contains("com.apple.recentitems") || normalized.ends_with(".sfl2") {
        extract_recent_items(candidate, bytes, &mut outcome);
    } else if normalized.contains("/launchservices.quarantineevents")
        || normalized.contains("quarantineevents")
    {
        extract_quarantine_events(candidate, bytes, &mut outcome);
    } else if normalized.contains("/.spotlight-v100/") || normalized.ends_with(".store.db") {
        extract_spotlight(candidate, bytes, &mut outcome);
    } else if normalized.contains("/var/db/diagnostics/") || normalized.ends_with(".tracev3") {
        extract_unified_log(candidate, bytes, &mut outcome);
    } else {
        outcome.warnings.push(format!(
            "{} 未被识别为已支持的 macOS 痕迹类型",
            candidate.path
        ));
    }

    outcome
}

fn extract_fsevents(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    match artifacts_macos::parse_fsevents_log(bytes) {
        Ok(events) => {
            for event in events {
                let mut attrs = base_attrs(candidate);
                attrs.insert("path".to_string(), Value::String(event.path.clone()));
                attrs.insert(
                    "eventType".to_string(),
                    Value::String(format!("{:?}", event.event_type)),
                );
                attrs.insert(
                    "timestamp".to_string(),
                    Value::String(event.timestamp.clone()),
                );

                outcome.artifacts.push(make_artifact(
                    "MacFSEvent",
                    format!(
                        "FSEvent {:?}: {}",
                        event.event_type,
                        truncate(&event.path, 80)
                    ),
                    format!("{:?} {}", event.event_type, event.path),
                    candidate,
                    "macos.fsevents",
                    attrs.clone(),
                ));

                if let Some(ts) = parse_iso_timestamp(&event.timestamp) {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "macos.fsevents",
                        ts,
                        format!("FSEvent {:?}", event.event_type),
                        event.path,
                        attrs,
                        "macos.fsevents",
                    ));
                }
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} FSEvents parse failed: {}", candidate.path, err)),
    }
}

fn extract_launch_services(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_macos::parse_launch_services_plist(bytes) {
        Ok(services) => {
            for service in services {
                let mut attrs = base_attrs(candidate);
                attrs.insert("label".to_string(), Value::String(service.label.clone()));
                attrs.insert(
                    "bundleId".to_string(),
                    Value::String(service.bundle_id.clone()),
                );
                attrs.insert("path".to_string(), Value::String(service.path.clone()));
                attrs.insert("kind".to_string(), Value::String(service.kind.clone()));

                outcome.artifacts.push(make_artifact(
                    "MacLaunchService",
                    format!("LaunchService: {}", service.label),
                    format!("{} {} ({})", service.kind, service.bundle_id, service.path),
                    candidate,
                    "macos.launch_services",
                    attrs,
                ));
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} LaunchServices parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_recent_items(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_macos::parse_recent_items_plist(bytes) {
        Ok(items) => {
            for item in items {
                let mut attrs = base_attrs(candidate);
                attrs.insert("name".to_string(), Value::String(item.name.clone()));
                attrs.insert("path".to_string(), Value::String(item.path.clone()));
                attrs.insert(
                    "kind".to_string(),
                    Value::String(format!("{:?}", item.kind)),
                );
                if let Some(ts) = &item.last_used {
                    attrs.insert("lastUsed".to_string(), Value::String(ts.clone()));
                }

                outcome.artifacts.push(make_artifact(
                    "MacRecentItem",
                    format!("Recent item: {}", item.name),
                    format!("{:?} {} ({})", item.kind, item.name, item.path),
                    candidate,
                    "macos.recent_items",
                    attrs.clone(),
                ));

                if let Some(ts) = item.last_used.as_deref().and_then(parse_iso_timestamp) {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "macos.recent_item",
                        ts,
                        format!("Recent item used: {}", item.name),
                        item.path,
                        attrs,
                        "macos.recent_items",
                    ));
                }
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} RecentItems parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_quarantine_events(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_macos::parse_quarantine_events(bytes) {
        Ok(entries) => {
            for entry in entries {
                let mut attrs = base_attrs(candidate);
                attrs.insert("url".to_string(), Value::String(entry.url.clone()));
                attrs.insert(
                    "originBundle".to_string(),
                    Value::String(entry.origin_bundle.clone()),
                );
                attrs.insert("agent".to_string(), Value::String(entry.agent.clone()));
                attrs.insert(
                    "timestamp".to_string(),
                    Value::String(entry.timestamp.clone()),
                );

                outcome.artifacts.push(make_artifact(
                    "MacQuarantineEvent",
                    format!("Quarantine: {}", truncate(&entry.url, 80)),
                    format!(
                        "Downloaded by {} from {} at {}",
                        entry.agent, entry.url, entry.timestamp
                    ),
                    candidate,
                    "macos.quarantine",
                    attrs.clone(),
                ));

                if let Some(ts) = parse_iso_timestamp(&entry.timestamp) {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "macos.quarantine",
                        ts,
                        format!("Quarantine download: {}", truncate(&entry.url, 80)),
                        entry.url,
                        attrs,
                        "macos.quarantine",
                    ));
                }
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} QuarantineEvents parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_spotlight(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    match artifacts_macos::parse_spotlight_store(bytes) {
        Ok(entries) => {
            for entry in entries {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "filePath".to_string(),
                    Value::String(entry.file_path.clone()),
                );
                attrs.insert(
                    "displayName".to_string(),
                    Value::String(entry.display_name.clone()),
                );
                attrs.insert("kind".to_string(), Value::String(entry.kind.clone()));
                attrs.insert(
                    "contentType".to_string(),
                    Value::String(entry.content_type.clone()),
                );
                if !entry.dates.is_empty() {
                    attrs.insert(
                        "dates".to_string(),
                        Value::Array(entry.dates.iter().cloned().map(Value::String).collect()),
                    );
                }
                if !entry.authors.is_empty() {
                    attrs.insert(
                        "authors".to_string(),
                        Value::Array(entry.authors.iter().cloned().map(Value::String).collect()),
                    );
                }

                outcome.artifacts.push(make_artifact(
                    "MacSpotlightEntry",
                    format!("Spotlight: {}", entry.display_name),
                    format!("{} ({})", entry.file_path, entry.content_type),
                    candidate,
                    "macos.spotlight",
                    attrs.clone(),
                ));

                for date_str in &entry.dates {
                    if let Some(ts) = parse_iso_timestamp(date_str) {
                        outcome.timeline_events.push(make_timeline_event(
                            &candidate.file_id,
                            "macos.spotlight",
                            ts,
                            format!("Spotlight indexed: {}", entry.display_name),
                            entry.file_path.clone(),
                            attrs.clone(),
                            "macos.spotlight",
                        ));
                    }
                }
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} Spotlight parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_unified_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_macos::parse_tracev3(bytes) {
        Ok(entries) => {
            for entry in entries {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "timestamp".to_string(),
                    Value::String(entry.timestamp.clone()),
                );
                attrs.insert("process".to_string(), Value::String(entry.process.clone()));
                attrs.insert("message".to_string(), Value::String(entry.message.clone()));
                attrs.insert(
                    "activityId".to_string(),
                    Value::String(entry.activity_id.clone()),
                );
                attrs.insert(
                    "threadId".to_string(),
                    Value::String(entry.thread_id.clone()),
                );

                outcome.artifacts.push(make_artifact(
                    "MacUnifiedLogEntry",
                    format!("UnifiedLog: {}", truncate(&entry.message, 80)),
                    format!("[{}] {}", entry.process, entry.message),
                    candidate,
                    "macos.unified_log",
                    attrs.clone(),
                ));

                if let Some(ts) = parse_iso_timestamp(&entry.timestamp) {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "macos.unified_log",
                        ts,
                        format!("Unified log: {}", truncate(&entry.message, 80)),
                        entry.process,
                        attrs,
                        "macos.unified_log",
                    ));
                }
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} UnifiedLog parse failed: {}",
            candidate.path, err
        )),
    }
}

fn parse_iso_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    }
}
