use std::collections::BTreeMap;

use serde_json::json;
use transport::dto::TimelineEventDto;

pub fn get_timeline_events() -> Vec<TimelineEventDto> {
    vec![
        TimelineEventDto {
            id: "evt-001".into(),
            source_object_id: "file-001".into(),
            event_type: "file.accessed".into(),
            ts: "2025-02-16T16:02:12Z".into(),
            title: "访问可执行文件".into(),
            description: "用户访问了 Downloads/AnyDesk.exe".into(),
            attrs: BTreeMap::from([
                ("user".into(), json!("Alice")),
                ("source".into(), json!("shellbags")),
            ]),
        },
        TimelineEventDto {
            id: "evt-002".into(),
            source_object_id: "net-001".into(),
            event_type: "network.connection".into(),
            ts: "2025-02-16T14:13:55Z".into(),
            title: "建立外联".into(),
            description: "主机与 10.10.20.15:443 建立连接".into(),
            attrs: BTreeMap::from([
                ("protocol".into(), json!("tcp")),
                ("destination".into(), json!("10.10.20.15:443")),
            ]),
        },
    ]
}
