use super::{DataSourceError, Result};
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourcePlatform,
    DataSourceProvenance, DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::io::Read;
use std::path::Path;

pub fn attach_data_source(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    name: &str,
    source_path: &Path,
    kind: DataSourceKind,
    platform: DataSourcePlatform,
) -> Result<DataSource> {
    attach_data_source_with_storage(conn, case_id, name, source_path, kind, platform, None)
}

pub fn attach_data_source_with_storage(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    name: &str,
    source_path: &Path,
    kind: DataSourceKind,
    platform: DataSourcePlatform,
    profile: Option<String>,
) -> Result<DataSource> {
    if platform == DataSourcePlatform::Unknown {
        return Err(DataSourceError::UnsupportedPlatform(platform.to_string()));
    }
    let platform = Some(platform.as_storage_str());
    let id = DataSourceId(uuid::Uuid::new_v4().to_string());
    let provenance = build_attach_provenance(source_path, &kind);
    let ds = DataSource {
        id: id.clone(),
        name: name.to_string(),
        kind,
        source_path: source_path.to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance,
    };

    let storage = DataSourceStorage::source_db(&ds.id.0, platform, profile);
    DataSourceRepo::new(conn).insert_with_storage(case_id, &ds, &storage)?;
    Ok(ds)
}

fn build_attach_provenance(source_path: &Path, kind: &DataSourceKind) -> DataSourceProvenance {
    if *kind == DataSourceKind::LocalDisk {
        return match evidence_core::LocalDiskReader::open(source_path) {
            Ok(reader) => DataSourceProvenance {
                source_hash_sha256: None,
                hash_status: DataSourceHashStatus::Pending,
                canonical_source_path: Some(source_path.to_path_buf()),
                evidence_size: Some(reader.len()),
                reader_kind: Some(kind.to_string()),
                provenance_status: DataSourceProvenanceStatus::Recorded,
                warnings: Vec::new(),
            },
            Err(error) => DataSourceProvenance {
                source_hash_sha256: None,
                hash_status: DataSourceHashStatus::Unknown,
                canonical_source_path: Some(source_path.to_path_buf()),
                evidence_size: None,
                reader_kind: Some(kind.to_string()),
                provenance_status: DataSourceProvenanceStatus::Partial,
                warnings: vec![format!("local disk probe failed: {error}")],
            },
        };
    }
    let mut warnings = Vec::new();
    let canonical_source_path = match std::fs::canonicalize(source_path) {
        Ok(path) => Some(path),
        Err(err) => {
            warnings.push(format!(
                "canonicalize failed for {}: {}",
                source_path.display(),
                err
            ));
            None
        }
    };
    let metadata = match std::fs::metadata(source_path) {
        Ok(metadata) => Some(metadata),
        Err(err) => {
            warnings.push(format!(
                "metadata unavailable for {}: {}",
                source_path.display(),
                err
            ));
            None
        }
    };
    let evidence_size = metadata
        .as_ref()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len());
    let hash_status = if metadata.as_ref().is_some_and(|metadata| metadata.is_file()) {
        DataSourceHashStatus::Pending
    } else if metadata.as_ref().is_some_and(|metadata| metadata.is_dir()) {
        DataSourceHashStatus::Unavailable
    } else {
        DataSourceHashStatus::Unknown
    };
    let provenance_status = if canonical_source_path.is_some() && metadata.is_some() {
        DataSourceProvenanceStatus::Recorded
    } else {
        DataSourceProvenanceStatus::Partial
    };

    DataSourceProvenance {
        source_hash_sha256: None,
        hash_status,
        canonical_source_path,
        evidence_size,
        reader_kind: Some(kind.to_string()),
        provenance_status,
        warnings,
    }
}

pub fn classify_data_source_path(source_path: &Path) -> Result<DataSourceKind> {
    #[cfg(windows)]
    if evidence_core::LocalDiskReader::is_supported_path(source_path) {
        return Ok(DataSourceKind::LocalDisk);
    }
    let metadata = std::fs::metadata(source_path)?;
    if metadata.is_dir() {
        return Ok(DataSourceKind::LogicalDirectory);
    }

    if has_e01_magic(source_path)? || has_e01_name(source_path) {
        Ok(DataSourceKind::E01)
    } else if has_android_sparse_magic(source_path)? {
        Ok(DataSourceKind::AndroidSparse)
    } else if has_archive_magic(source_path)? || has_archive_name(source_path) {
        Ok(DataSourceKind::LogicalArchive)
    } else {
        Ok(DataSourceKind::Raw)
    }
}

fn has_e01_magic(source_path: &Path) -> Result<bool> {
    let mut file = std::fs::File::open(source_path)?;
    let mut magic = [0u8; 8];
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(&magic == b"EVF\x09\x0d\x0a\xff\x00" || &magic[0..3] == b"EVF"),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn has_e01_name(source_path: &Path) -> bool {
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "e01" | "ewf") {
        return true;
    }

    source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains(".e01."))
        .unwrap_or(false)
}

fn has_android_sparse_magic(source_path: &Path) -> Result<bool> {
    let mut file = std::fs::File::open(source_path)?;
    let mut magic = [0u8; 4];
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(u32::from_le_bytes(magic) == image_android::SPARSE_MAGIC),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn has_archive_magic(source_path: &Path) -> Result<bool> {
    let mut file = std::fs::File::open(source_path)?;
    let mut header = [0u8; 512];
    let bytes_read = file.read(&mut header)?;
    if bytes_read >= 2 && header[..2] == [0x1f, 0x8b] {
        return Ok(true);
    }
    Ok(bytes_read >= 263
        && (&header[257..262] == b"ustar"
            || &header[257..263] == b"ustar\0"
            || &header[257..263] == b"ustar "))
}

fn has_archive_name(source_path: &Path) -> bool {
    let name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    name.ends_with(".tar")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".gz")
        || name.ends_with(".gzip")
}
