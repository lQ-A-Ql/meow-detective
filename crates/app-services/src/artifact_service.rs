use std::collections::BTreeMap;

use serde_json::json;
use transport::dto::ArtifactRowDto;

pub fn get_artifact_families() -> Vec<String> {
    vec!["recent_docs".into(), "autoruns".into(), "browser_history".into()]
}

pub fn get_artifact_rows(family: Option<String>) -> Vec<ArtifactRowDto> {
    let rows = vec![
        ArtifactRowDto {
            id: "artifact-001".into(),
            artifact_type: "recent_docs".into(),
            title: "Recent Docs - report.docx".into(),
            summary: "最近打开文档".into(),
            source_object_id: Some("file-010".into()),
            created_at: "2025-02-16T09:22:10Z".into(),
            attrs: BTreeMap::from([
                ("path".into(), json!("C:/Users/Alice/Documents/report.docx")),
                ("source".into(), json!("automaticdestinations-ms")),
            ]),
        },
        ArtifactRowDto {
            id: "artifact-002".into(),
            artifact_type: "autoruns".into(),
            title: "Run Key - Updater".into(),
            summary: "登录时自启项".into(),
            source_object_id: Some("reg-010".into()),
            created_at: "2025-02-15T07:10:00Z".into(),
            attrs: BTreeMap::from([
                (
                    "key".into(),
                    json!("HKCU/Software/Microsoft/Windows/CurrentVersion/Run"),
                ),
                ("value".into(), json!("Updater.exe")),
            ]),
        },
    ];

    rows.into_iter()
        .filter(|row| family.as_ref().is_none_or(|selected| &row.artifact_type == selected))
        .collect()
}
