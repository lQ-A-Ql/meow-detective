use std::io::Read;
use transport::dto::ArtifactRowDto;

use artifacts_core::{ArtifactContext, ExtractorRegistry, VecSink};
use domain::FileEntryId;
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;

pub fn create_registry() -> ExtractorRegistry {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(artifacts_windows::PrefetchExtractor));
    registry.register(Box::new(artifacts_windows::LnkExtractor));
    registry.register(Box::new(artifacts_windows::RecycleBinExtractor));
    registry.register(Box::new(artifacts_windows::RegistryExtractor));
    registry.register(Box::new(artifacts_windows::JumpListExtractor));
    registry.register(Box::new(artifacts_windows::SruExtractor));
    registry.register(Box::new(artifacts_windows::ThumbcacheExtractor));
    registry
}

pub fn run_extractors_on_file(
    registry: &ExtractorRegistry,
    file_id: &FileEntryId,
    file_path: &str,
    reader: Box<dyn Read>,
    sink: &mut VecSink,
) -> Result<(), String> {
    let extractors = registry.find_for_path(file_path);
    if extractors.is_empty() {
        return Ok(());
    }

    let mut buf = Vec::new();
    // Guard: limit artifact extraction to 50 MB per file to prevent OOM
    const ARTIFACT_FILE_LIMIT: u64 = 50 * 1024 * 1024;
    let bytes_read = reader
        .take(ARTIFACT_FILE_LIMIT)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if bytes_read as u64 >= ARTIFACT_FILE_LIMIT {
        tracing::warn!(
            "Artifact extraction truncated at {} bytes for file: {}",
            bytes_read,
            file_path
        );
    }

    // 克隆缓冲区一次，供所有提取器使用
    let buf = std::sync::Arc::new(buf);
    for extractor in extractors {
        let cursor = std::io::Cursor::new((*buf).clone());
        let run_ctx = ArtifactContext {
            file_id: file_id.clone(),
            file_path: file_path.to_string(),
            reader: Box::new(cursor),
        };
        if let Err(e) = extractor.run(run_ctx, sink) {
            tracing::warn!("Extractor {} error: {}", extractor.id(), e);
        }
    }
    Ok(())
}

pub fn store_artifacts(
    conn: &Connection,
    artifacts: &[domain::Artifact],
    case_id: &str,
    data_source_id: &str,
) -> Result<(), String> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let repo = ArtifactRepo::new(conn);
    repo.insert_batch(artifacts, case_id, data_source_id)
        .map_err(|e| e.to_string())
}

pub fn get_artifact_families_from_db(conn: &Connection) -> Result<Vec<String>, String> {
    let repo = ArtifactRepo::new(conn);
    repo.families().map_err(|e| e.to_string())
}

pub fn get_artifact_rows_from_db(
    conn: &Connection,
    family: Option<&str>,
) -> Result<Vec<ArtifactRowDto>, String> {
    let repo = ArtifactRepo::new(conn);
    let artifacts = repo.list_by_family(family).map_err(|e| e.to_string())?;
    Ok(artifacts.iter().map(artifact_to_dto).collect())
}

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
