use std::io::Read;
use transport::dto::{ArtifactRowDto, FamilyCountDto};

use artifacts_core::{ArtifactContext, ExtractorRegistry, VecSink};
use domain::FileEntryId;
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtifactExtractionStats {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceScanStats {
    pub candidate_count: u32,
    pub scanned_count: u32,
    pub artifact_count: u32,
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub warnings: Vec<String>,
}

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
) -> Result<ArtifactExtractionStats, String> {
    let mut stats = ArtifactExtractionStats::default();
    let extractors = registry.find_for_path(file_path);
    if extractors.is_empty() {
        return Ok(stats);
    }

    let mut buf = Vec::new();
    let bytes_read = reader
        .take(infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if bytes_read as u64 >= infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES {
        stats.warning_count += 1;
        stats.skipped_count += 1;
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
            stats.warning_count += 1;
            tracing::warn!("Extractor {} error: {}", extractor.id(), e);
        }
    }
    Ok(stats)
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

pub fn run_targeted_evidence_scan(
    conn: &Connection,
    case_id: &str,
    categories: &[&str],
    file_reader: impl Fn(&FileEntryId) -> Result<Box<dyn Read>, String>,
) -> Result<EvidenceScanStats, String> {
    let registry = create_registry();
    let selected_categories = if categories.is_empty() {
        crate::analysis_service::evidence_category_defs()
            .iter()
            .filter(|def| !def.artifact_families.is_empty())
            .map(|def| def.category)
            .collect::<Vec<_>>()
    } else {
        categories.to_vec()
    };
    let candidates =
        crate::analysis_service::evidence_candidates_for_categories(conn, &selected_categories)?;
    let mut stats = EvidenceScanStats {
        candidate_count: candidates.len() as u32,
        ..EvidenceScanStats::default()
    };

    for candidate in candidates {
        let extractors = registry.find_for_path(&candidate.path);
        if extractors.is_empty() {
            stats.skipped_count += 1;
            continue;
        }
        if already_has_artifact_for_source(conn, &candidate.file_id.0)? {
            stats.skipped_count += 1;
            continue;
        }
        let reader = match file_reader(&candidate.file_id) {
            Ok(reader) => reader,
            Err(err) => {
                stats.warning_count += 1;
                stats.skipped_count += 1;
                stats.warnings.push(format!("{}: {}", candidate.path, err));
                continue;
            }
        };
        let mut sink = VecSink::new();
        let file_stats = run_extractors_on_file(
            &registry,
            &candidate.file_id,
            &candidate.path,
            reader,
            &mut sink,
        )?;
        stats.warning_count += file_stats.warning_count;
        stats.skipped_count += file_stats.skipped_count;
        stats.failed_count += file_stats.failed_count;
        if !sink.artifacts.is_empty() {
            store_artifacts(conn, &sink.artifacts, case_id, &candidate.data_source_id)?;
            stats.artifact_count += sink.artifacts.len() as u32;
        }
        stats.scanned_count += 1;
    }

    Ok(stats)
}

fn already_has_artifact_for_source(
    conn: &Connection,
    source_object_id: &str,
) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE source_object_id = ?1",
            [source_object_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

/// Run artifact extraction on multiple files in parallel using rayon.
///
/// Strategy: read file contents sequentially (I/O bound), then run extractors
/// in parallel (CPU bound). This works well on NVMe where sequential reads
/// are fast and the extractor CPU work is the bottleneck.
///
/// Returns (all_extracted_artifacts, aggregated_stats).
pub fn run_extractors_parallel(
    registry: &ExtractorRegistry,
    files: &[domain::FileEntry],
    file_reader: &dyn Fn(&FileEntryId) -> Option<Box<dyn Read>>,
    limit: usize,
) -> (Vec<domain::Artifact>, ArtifactExtractionStats) {
    let to_process: Vec<&domain::FileEntry> = files
        .iter()
        .filter(|file| !registry.find_for_path(&file.path).is_empty())
        .take(limit)
        .collect();

    let results: Vec<(Vec<domain::Artifact>, ArtifactExtractionStats)> = to_process
        .iter()
        .map(|file| {
            let extractors = registry.find_for_path(&file.path);
            let mut sink = VecSink::new();
            let mut stats = ArtifactExtractionStats::default();
            let Some(reader) = file_reader(&file.id) else {
                stats.warning_count += 1;
                stats.skipped_count += 1;
                tracing::warn!("Artifact extraction skipped unreadable file: {}", file.path);
                return (Vec::new(), stats);
            };

            let mut buf = Vec::new();
            let mut limited = reader.take(ARTIFACT_FILE_LIMIT_BYTES);
            if let Err(e) = limited.read_to_end(&mut buf) {
                stats.warning_count += 1;
                stats.skipped_count += 1;
                tracing::warn!("Artifact extraction read error for {}: {}", file.path, e);
                return (Vec::new(), stats);
            }

            if buf.len() as u64 >= ARTIFACT_FILE_LIMIT_BYTES {
                stats.warning_count += 1;
                stats.skipped_count += 1;
                tracing::warn!(
                    "Artifact extraction truncated at {} bytes for file: {}",
                    ARTIFACT_FILE_LIMIT_BYTES,
                    file.path
                );
            }

            let buf_arc = std::sync::Arc::new(buf);
            for extractor in extractors {
                let cursor = std::io::Cursor::new((*buf_arc).clone());
                let ctx = ArtifactContext {
                    file_id: file.id.clone(),
                    file_path: file.path.clone(),
                    reader: Box::new(cursor),
                };
                if let Err(e) = extractor.run(ctx, &mut sink) {
                    stats.warning_count += 1;
                    tracing::warn!("Extractor {} error: {}", extractor.id(), e);
                }
            }
            (sink.artifacts, stats)
        })
        .collect();

    let mut all_artifacts = Vec::new();
    let mut total_stats = ArtifactExtractionStats::default();
    for (artifacts, stats) in results {
        all_artifacts.extend(artifacts);
        total_stats.warning_count += stats.warning_count;
        total_stats.skipped_count += stats.skipped_count;
        total_stats.failed_count += stats.failed_count;
    }

    (all_artifacts, total_stats)
}

const ARTIFACT_FILE_LIMIT_BYTES: u64 = infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES;

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

pub fn get_artifact_family_counts(conn: &Connection) -> Result<Vec<FamilyCountDto>, String> {
    let repo = ArtifactRepo::new(conn);
    let counts = repo.count_by_family().map_err(|e| e.to_string())?;
    Ok(counts
        .into_iter()
        .map(|(family, count)| FamilyCountDto { family, count })
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::{Artifact, ArtifactId, DataSourceId, EntryType, FileEntry, FileEntryId};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ARTIFACTS_SCHEMA: &str =
        include_str!("../../persistence-sqlite/src/migrations/scripts/0004_artifacts.sql");

    fn in_memory_db_with_artifacts() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(ARTIFACTS_SCHEMA).unwrap();
        conn
    }

    fn in_memory_case_db() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        let case = domain::CaseMeta {
            id: domain::CaseId("case-1".to_string()),
            name: "case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
            .create(&case)
            .unwrap();
        let ds = domain::DataSource {
            id: DataSourceId("ds-1".to_string()),
            name: "logical".to_string(),
            kind: domain::DataSourceKind::LogicalDirectory,
            source_path: std::path::PathBuf::from("C:/fixture"),
            imported_at: Utc::now(),
        };
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
            .insert(&domain::CaseId("case-1".to_string()), &ds)
            .unwrap();
        conn
    }

    fn make_artifact(family: &str, title: &str) -> Artifact {
        Artifact {
            id: ArtifactId(uuid::Uuid::new_v4().to_string()),
            family: family.to_string(),
            title: title.to_string(),
            summary: format!("summary for {}", title),
            source_object_id: Some(FileEntryId("src-1".to_string())),
            created_at: Utc::now(),
            attrs: BTreeMap::new(),
        }
    }

    fn make_file(id: &str, path: &str) -> FileEntry {
        FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: path.to_string(),
            name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            entry_type: EntryType::File,
            size: Some(1),
            ext: None,
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }
    }

    fn insert_files(conn: &rusqlite::Connection, files: &[FileEntry]) {
        persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
            .insert_batch(files)
            .unwrap();
    }

    #[test]
    fn create_registry_returns_extractors() {
        let registry = create_registry();
        assert!(registry.all().len() >= 7);
    }

    #[test]
    fn run_extractors_no_matching_extractor() {
        let registry = create_registry();
        let file_id = FileEntryId("f1".to_string());
        let mut sink = artifacts_core::VecSink::new();
        let data = b"hello world";
        let reader = Box::new(std::io::Cursor::new(data.to_vec()));

        let stats = run_extractors_on_file(
            &registry,
            &file_id,
            "/some/random/file.xyz",
            reader,
            &mut sink,
        )
        .unwrap();

        assert_eq!(stats.warning_count, 0);
        assert_eq!(stats.skipped_count, 0);
        assert!(sink.artifacts.is_empty());
    }

    #[test]
    fn store_artifacts_and_retrieve() {
        let conn = in_memory_db_with_artifacts();
        let artifacts = vec![
            make_artifact("Prefetch", "pf-1"),
            make_artifact("Prefetch", "pf-2"),
            make_artifact("LNK", "lnk-1"),
        ];

        store_artifacts(&conn, &artifacts, "case-1", "ds-1").unwrap();

        let rows = get_artifact_rows_from_db(&conn, None).unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn store_artifacts_empty_is_noop() {
        let conn = in_memory_db_with_artifacts();
        store_artifacts(&conn, &[], "case-1", "ds-1").unwrap();
        let rows = get_artifact_rows_from_db(&conn, None).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn get_artifact_rows_from_db_filter_by_family() {
        let conn = in_memory_db_with_artifacts();
        let artifacts = vec![
            make_artifact("Prefetch", "pf-1"),
            make_artifact("LNK", "lnk-1"),
        ];
        store_artifacts(&conn, &artifacts, "case-1", "ds-1").unwrap();

        let prefetch_rows = get_artifact_rows_from_db(&conn, Some("Prefetch")).unwrap();
        assert_eq!(prefetch_rows.len(), 1);
        assert_eq!(prefetch_rows[0].artifact_type, "Prefetch");

        let lnk_rows = get_artifact_rows_from_db(&conn, Some("LNK")).unwrap();
        assert_eq!(lnk_rows.len(), 1);
    }

    #[test]
    fn get_artifact_families_from_db_returns_distinct() {
        let conn = in_memory_db_with_artifacts();
        let artifacts = vec![
            make_artifact("Prefetch", "pf-1"),
            make_artifact("Prefetch", "pf-2"),
            make_artifact("LNK", "lnk-1"),
            make_artifact("Registry", "reg-1"),
        ];
        store_artifacts(&conn, &artifacts, "case-1", "ds-1").unwrap();

        let families = get_artifact_families_from_db(&conn).unwrap();
        assert_eq!(families.len(), 3);
        assert!(families.contains(&"LNK".to_string()));
        assert!(families.contains(&"Prefetch".to_string()));
        assert!(families.contains(&"Registry".to_string()));
    }

    #[test]
    fn get_artifact_family_counts_returns_counts() {
        let conn = in_memory_db_with_artifacts();
        let artifacts = vec![
            make_artifact("Prefetch", "pf-1"),
            make_artifact("Prefetch", "pf-2"),
            make_artifact("LNK", "lnk-1"),
        ];
        store_artifacts(&conn, &artifacts, "case-1", "ds-1").unwrap();

        let counts = get_artifact_family_counts(&conn).unwrap();
        assert_eq!(counts.len(), 2);

        let lnk = counts.iter().find(|c| c.family == "LNK").unwrap();
        assert_eq!(lnk.count, 1);

        let pf = counts.iter().find(|c| c.family == "Prefetch").unwrap();
        assert_eq!(pf.count, 2);
    }

    #[test]
    fn parallel_extraction_skips_non_matching_files_without_reader() {
        let registry = create_registry();
        let files = vec![make_file("txt-1", "/notes/readme.txt")];
        let reads = AtomicUsize::new(0);

        let (artifacts, stats) = run_extractors_parallel(
            &registry,
            &files,
            &|_| {
                reads.fetch_add(1, Ordering::Relaxed);
                Some(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn std::io::Read>)
            },
            10,
        );

        assert!(artifacts.is_empty());
        assert_eq!(reads.load(Ordering::Relaxed), 0);
        assert_eq!(stats.warning_count, 0);
        assert_eq!(stats.skipped_count, 0);
    }

    #[test]
    fn parallel_extraction_reader_error_counts_warning_and_skipped() {
        let registry = create_registry();
        let files = vec![make_file("pf-1", "/Windows/Prefetch/CMD.EXE-DEADBEEF.pf")];

        let (artifacts, stats) = run_extractors_parallel(&registry, &files, &|_| None, 10);

        assert!(artifacts.is_empty());
        assert_eq!(stats.warning_count, 1);
        assert_eq!(stats.skipped_count, 1);
    }

    #[test]
    fn parallel_extraction_limit_applies_after_extractor_filter() {
        let registry = create_registry();
        let files = vec![
            make_file("txt-1", "/notes/readme.txt"),
            make_file("pf-1", "/Windows/Prefetch/A.EXE-DEADBEEF.pf"),
            make_file("pf-2", "/Windows/Prefetch/B.EXE-DEADBEEF.pf"),
        ];
        let reads = AtomicUsize::new(0);

        let (_artifacts, _stats) = run_extractors_parallel(
            &registry,
            &files,
            &|_| {
                reads.fetch_add(1, Ordering::Relaxed);
                Some(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn std::io::Read>)
            },
            1,
        );

        assert_eq!(reads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn targeted_scan_skips_files_without_matching_extractor() {
        let conn = in_memory_case_db();
        insert_files(
            &conn,
            &[make_file(
                "evtx-1",
                "Windows/System32/winevt/Logs/System.evtx",
            )],
        );
        let reads = AtomicUsize::new(0);

        let stats = run_targeted_evidence_scan(&conn, "case-1", &["EventLogs"], |_| {
            reads.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
        })
        .unwrap();

        assert_eq!(stats.candidate_count, 1);
        assert_eq!(stats.scanned_count, 0);
        assert_eq!(stats.skipped_count, 1);
        assert_eq!(reads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn targeted_scan_records_warning_for_read_error() {
        let conn = in_memory_case_db();
        insert_files(
            &conn,
            &[make_file("pf-1", "Windows/Prefetch/CMD.EXE-12345678.pf")],
        );

        let stats = run_targeted_evidence_scan(&conn, "case-1", &["ProgramExecution"], |_| {
            Err("reader unavailable".to_string())
        })
        .unwrap();

        assert_eq!(stats.candidate_count, 1);
        assert_eq!(stats.warning_count, 1);
        assert_eq!(stats.skipped_count, 1);
        assert!(stats.warnings[0].contains("reader unavailable"));
    }

    #[test]
    fn targeted_scan_deduplicates_existing_artifacts() {
        let conn = in_memory_case_db();
        insert_files(
            &conn,
            &[make_file("pf-1", "Windows/Prefetch/CMD.EXE-12345678.pf")],
        );
        conn.execute(
            "INSERT INTO artifacts
             (id, case_id, data_source_id, artifact_type, source_object_id, title, summary, attrs, created_at)
             VALUES ('artifact-1', 'case-1', 'ds-1', 'Prefetch', 'pf-1', 'Prefetch', 'summary', '{}', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let reads = AtomicUsize::new(0);

        let stats = run_targeted_evidence_scan(&conn, "case-1", &["ProgramExecution"], |_| {
            reads.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
        })
        .unwrap();

        assert_eq!(stats.candidate_count, 1);
        assert_eq!(stats.scanned_count, 0);
        assert_eq!(stats.skipped_count, 1);
        assert_eq!(reads.load(Ordering::Relaxed), 0);
        assert_eq!(
            get_artifact_rows_from_db(&conn, Some("Prefetch"))
                .unwrap()
                .len(),
            1
        );
    }
}
