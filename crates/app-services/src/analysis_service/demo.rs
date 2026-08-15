//! Analysis demo data seeding.
//!
//! Populates a case with realistic fixture data for demonstration purposes.

use std::path::{Path, PathBuf};

use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance, EntryType,
    FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use uuid::Uuid;

use crate::active_case::ActiveCase;

use super::AnalysisServiceError;

/// Seed a case with analysis demo data (logical fixtures, EVTX, demo files).
pub fn seed_analysis_demo_data(active: &ActiveCase) -> Result<(), AnalysisServiceError> {
    let evidence_root = active.case_root.join("evidence").join("analysis-demo");
    if evidence_root.exists() {
        std::fs::remove_dir_all(&evidence_root)?;
    }
    std::fs::create_dir_all(&evidence_root)?;

    let fixture_root = repo_root()
        .join("testdata")
        .join("fixtures")
        .join("public-small");
    copy_dir_all(&fixture_root.join("logical"), &evidence_root)?;
    let evtx_src = fixture_root.join("evtx").join("system.evtx");
    let evtx_dest = evidence_root
        .join("Windows")
        .join("System32")
        .join("winevt")
        .join("Logs")
        .join("System.evtx");
    if let Some(parent) = evtx_dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&evtx_src, &evtx_dest).map_err(|e| {
        tracing::error!("Failed to copy public-small System.evtx fixture: {}", e);
        AnalysisServiceError::Io(e)
    })?;

    write_demo_file(
        &evidence_root.join("Users").join("alice").join("report.pdf"),
        b"%PDF-1.7\n% demo forensic report\n",
    )?;
    write_demo_file(
        &evidence_root
            .join("Users")
            .join("alice")
            .join("Downloads")
            .join("tool.exe"),
        b"MZdemo executable header",
    )?;
    write_demo_file(
        &evidence_root
            .join("Users")
            .join("alice")
            .join("Archive")
            .join("case-notes.zip"),
        b"PK\x03\x04demo zip payload",
    )?;

    let ds_id = DataSourceId(format!("demo-ds-{}", Uuid::new_v4()));
    let data_source = DataSource {
        id: ds_id.clone(),
        name: "Analysis Demo Logical Evidence".to_string(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: evidence_root.clone(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut entries = Vec::new();
    collect_demo_entries(&evidence_root, &evidence_root, &ds_id, None, &mut entries)?;
    persist_demo_source(active, &data_source, &entries)?;
    Ok(())
}

fn persist_demo_source(
    active: &ActiveCase,
    data_source: &DataSource,
    entries: &[FileEntry],
) -> Result<(), AnalysisServiceError> {
    let mut storage = DataSourceStorage::source_db(
        &data_source.id.0,
        Some(DataSourcePlatform::Windows.as_storage_str()),
        Some("analysis-demo".to_string()),
    );
    storage.import_state = "ready".to_string();

    let source_conn = crate::source_db::open_source_db(&active.case_root, &data_source.id)?;
    DataSourceRepo::new(&source_conn).upsert_source_local_metadata(&active.meta.id, data_source)?;
    FileRepo::new(&source_conn).insert_batch(entries)?;
    crate::source_db::checkpoint_source_db(&source_conn)?;
    drop(source_conn);

    active.with_conn(|case_conn| {
        DataSourceRepo::new(case_conn).insert_with_storage(&active.meta.id, data_source, &storage)
    })?;
    Ok(())
}

fn repo_root() -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = base.join("../..");
    candidate.canonicalize().unwrap_or(candidate)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), AnalysisServiceError> {
    if !src.is_dir() {
        return Err(AnalysisServiceError::InvalidInput(format!(
            "analysis demo fixture missing: {}",
            src.display()
        )));
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn write_demo_file(path: &Path, bytes: &[u8]) -> Result<(), AnalysisServiceError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn collect_demo_entries(
    root: &Path,
    path: &Path,
    data_source_id: &DataSourceId,
    parent_id: Option<FileEntryId>,
    entries: &mut Vec<FileEntry>,
) -> Result<(), AnalysisServiceError> {
    let mut children = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.path());

    for child in children {
        let child_path = child.path();
        let metadata = child.metadata()?;
        let relative = child_path
            .strip_prefix(root)
            .map_err(|_| {
                AnalysisServiceError::InvalidInput(
                    "demo entry path is outside evidence root".to_string(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let id = FileEntryId(format!("demo-file-{}", Uuid::new_v4()));
        let entry_type = if metadata.is_dir() {
            EntryType::Directory
        } else {
            EntryType::File
        };
        let name = child.file_name().to_string_lossy().to_string();
        let system = matches!(
            name.to_ascii_lowercase().as_str(),
            "$recycle.bin"
                | "system volume information"
                | "pagefile.sys"
                | "hiberfil.sys"
                | "swapfile.sys"
        );
        entries.push(FileEntry {
            id: id.clone(),
            parent_id: parent_id.clone(),
            data_source_id: data_source_id.clone(),
            path: relative,
            name: name.clone(),
            entry_type: entry_type.clone(),
            size: if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            },
            ext: child_path
                .extension()
                .map(|ext| ext.to_string_lossy().to_string()),
            deleted: false,
            hidden: name.starts_with('.') || system,
            system,
            encrypted: false,
            read_only: metadata.permissions().readonly(),
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        });
        if entry_type == EntryType::Directory {
            collect_demo_entries(root, &child_path, data_source_id, Some(id), entries)?;
        }
    }
    Ok(())
}
