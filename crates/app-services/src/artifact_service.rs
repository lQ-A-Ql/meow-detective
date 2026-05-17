use serde_json::json;
use std::collections::BTreeMap;
use std::io::Read;
use transport::dto::ArtifactRowDto;

use artifacts_core::{ArtifactContext, ExtractorRegistry, VecSink};
use domain::FileEntryId;

pub fn create_registry() -> ExtractorRegistry {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(artifacts_windows::PrefetchExtractor));
    registry.register(Box::new(artifacts_windows::LnkExtractor));
    registry.register(Box::new(artifacts_windows::RecycleBinExtractor));
    registry.register(Box::new(artifacts_windows::RegistryExtractor));
    registry
}

pub fn run_extractors_on_file(
    registry: &ExtractorRegistry,
    file_id: &FileEntryId,
    file_path: &str,
    mut reader: Box<dyn Read>,
    sink: &mut VecSink,
) -> Result<(), String> {
    let extractors = registry.find_for_path(file_path);
    if extractors.is_empty() {
        return Ok(());
    }

    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    for extractor in extractors {
        let cursor = std::io::Cursor::new(buf.clone());
        let run_ctx = ArtifactContext {
            file_id: file_id.clone(),
            file_path: file_path.to_string(),
            reader: Box::new(cursor),
        };
        if let Err(e) = extractor.run(run_ctx, sink) {
            eprintln!("Extractor {} error: {}", extractor.id(), e);
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn artifact_to_dto(a: &domain::Artifact) -> ArtifactRowDto {
    ArtifactRowDto {
        id: a.id.0.clone(),
        artifact_type: a.family.clone(),
        title: a.title.clone(),
        summary: a.summary.clone(),
        source_object_id: a.source_object_id.as_ref().map(|id| id.0.clone()),
        created_at: a.created_at.to_rfc3339(),
        attrs: a.attrs.clone(),
    }
}

pub fn get_artifact_families() -> Vec<String> {
    vec![
        "Prefetch".into(),
        "LNK".into(),
        "RecycleBin".into(),
        "Registry".into(),
        "recent_docs".into(),
        "autoruns".into(),
        "browser_history".into(),
    ]
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
                ("path".into(), json!("C:/.../report.docx")),
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
                ("key".into(), json!("HKCU/.../Run")),
                ("value".into(), json!("Updater.exe")),
            ]),
        },
    ];
    rows.into_iter()
        .filter(|row| {
            family
                .as_ref()
                .is_none_or(|selected| &row.artifact_type == selected)
        })
        .collect()
}
