use std::collections::BTreeMap;
use serde_json::json;
use transport::dto::TimelineEventDto;

use domain::FileEntry;
use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rusqlite::Connection;

pub fn project_and_store_macb(conn: &Connection, files: &[FileEntry]) -> Result<u64, String> {
    let repo = TimelineRepo::new(conn);
    let mut all_events = Vec::new();
    for file in files {
        let events = timeline::project_file_macb(file);
        all_events.extend(events);
    }
    let count = all_events.len() as u64;
    if !all_events.is_empty() {
        repo.insert_batch(&all_events).map_err(|e| e.to_string())?;
    }
    Ok(count)
}

pub fn query_timeline(conn: &Connection, offset: u64, limit: u32) -> Result<Vec<TimelineEventDto>, String> {
    let repo = TimelineRepo::new(conn);
    let events = repo.query(offset, limit).map_err(|e| e.to_string())?;
    let dtos: Vec<TimelineEventDto> = events.into_iter().map(|ev| TimelineEventDto {
        id: ev.id.0,
        source_object_id: ev.source_object_id,
        event_type: ev.event_type,
        ts: ev.timestamp.to_rfc3339(),
        title: ev.title,
        description: ev.description,
        attrs: ev.attrs,
    }).collect();
    Ok(dtos)
}

pub fn get_timeline_events() -> Vec<TimelineEventDto> {
    vec![
        TimelineEventDto { id: "evt-001".into(), source_object_id: "file-001".into(), event_type: "file.accessed".into(), ts: "2025-02-16T16:02:12Z".into(), title: "访问可执行文件".into(), description: "用户访问了 Downloads/AnyDesk.exe".into(), attrs: BTreeMap::from([("user".into(), json!("Alice")), ("source".into(), json!("shellbags"))]) },
        TimelineEventDto { id: "evt-002".into(), source_object_id: "net-001".into(), event_type: "network.connection".into(), ts: "2025-02-16T14:13:55Z".into(), title: "建立外联".into(), description: "主机与 10.10.20.15:443 建立连接".into(), attrs: BTreeMap::from([("protocol".into(), json!("tcp")), ("destination".into(), json!("10.10.20.15:443"))]) },
    ]
}
