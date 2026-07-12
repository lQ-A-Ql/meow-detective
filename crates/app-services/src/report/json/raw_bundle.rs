use super::{RawExportBundle, ReportError};
use rusqlite::Connection;
use sha2::Digest;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawExportManifestEntry {
    file_id: String,
    data_source_id: String,
    relative_source_path: String,
    exported_relative_path: String,
    size: Option<u64>,
    sha256: Option<String>,
    deleted: bool,
    hidden: bool,
    system: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawExportManifest {
    case_id: String,
    generated_from_report: String,
    exported_count: usize,
    skipped_count: usize,
    skipped_files: Vec<String>,
    files: Vec<RawExportManifestEntry>,
}

#[derive(Default)]
struct ExportAccumulator {
    manifest_entries: Vec<RawExportManifestEntry>,
    hash_lines: Vec<String>,
    skipped_files: Vec<String>,
}

pub(super) fn export_raw_file_bundle(
    conn: &Connection,
    output_dir: &Path,
    case_id: &str,
    report_file_name: &str,
    overwrite: bool,
) -> Result<RawExportBundle, ReportError> {
    let bundle_dir_name = bundle_dir_name_from_report(report_file_name);
    let bundle_dir = output_dir.join(&bundle_dir_name);
    prepare_bundle_directory(&bundle_dir, overwrite)?;
    let entries = collect_exportable_file_entries(conn)?;
    let exported = export_legacy_entries(conn, &bundle_dir, entries)?;
    write_bundle_metadata(
        bundle_dir,
        bundle_dir_name,
        case_id,
        report_file_name,
        exported,
    )
}

pub(super) fn export_raw_file_bundle_for_case(
    conn: &Connection,
    case_root: &Path,
    output_dir: &Path,
    case_id: &str,
    report_file_name: &str,
    overwrite: bool,
) -> Result<RawExportBundle, ReportError> {
    let bundle_dir_name = bundle_dir_name_from_report(report_file_name);
    let bundle_dir = output_dir.join(&bundle_dir_name);
    prepare_bundle_directory(&bundle_dir, overwrite)?;
    let entries = collect_exportable_file_entries_for_case(conn, case_root, case_id)?;
    let exported = export_source_entries(conn, case_root, case_id, &bundle_dir, entries)?;
    write_bundle_metadata(
        bundle_dir,
        bundle_dir_name,
        case_id,
        report_file_name,
        exported,
    )
}

fn export_legacy_entries(
    conn: &Connection,
    bundle_dir: &Path,
    entries: Vec<domain::FileEntry>,
) -> Result<ExportAccumulator, ReportError> {
    let export_root = bundle_dir.join("files");
    fs::create_dir_all(&export_root)?;
    let mut exported = ExportAccumulator::default();

    for entry in entries {
        let mut reader = match crate::file_service::open_file_content_by_id(conn, &entry.id) {
            Ok(reader) => reader,
            Err(error) => {
                exported.skip(&entry, &error.to_string());
                continue;
            }
        };
        let export_rel = PathBuf::from(entry.data_source_id.0.clone()).join(format!(
            "{}-{}",
            entry.id.0,
            sanitize_bundle_component(&entry.name)
        ));
        let export_path = export_root.join(&export_rel);
        create_parent(&export_path)?;
        let (total_bytes, sha256) = copy_and_hash(&mut reader, &export_path)?;
        exported.push_legacy(entry, export_rel, total_bytes, sha256);
    }
    Ok(exported)
}

fn export_source_entries(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
    bundle_dir: &Path,
    entries: Vec<domain::FileEntry>,
) -> Result<ExportAccumulator, ReportError> {
    let export_root = bundle_dir.join("files");
    fs::create_dir_all(&export_root)?;
    let mut exported = ExportAccumulator::default();

    for entry in entries {
        let global_file_id =
            crate::source_db::GlobalFileId::new(entry.data_source_id.clone(), entry.id.clone())
                .encode();
        let export_rel = PathBuf::from(entry.data_source_id.0.clone()).join(format!(
            "{}-{}",
            sanitize_bundle_component(&global_file_id.0),
            sanitize_bundle_component(&entry.name)
        ));
        let export_path = export_root.join(&export_rel);
        create_parent(&export_path)?;
        let extracted = match crate::file_service::extract_file_to_destination_for_case(
            conn,
            case_root,
            &domain::CaseId(case_id.to_string()),
            &global_file_id.0,
            &export_path,
            false,
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                remove_partial_export(&export_path);
                exported.skip(&entry, &error.to_string());
                continue;
            }
        };
        let sha256 = hash_file(&export_path)?;
        exported.push_source(entry, global_file_id.0, export_rel, extracted, sha256);
    }
    Ok(exported)
}

fn copy_and_hash(reader: &mut impl Read, export_path: &Path) -> Result<(u64, String), ReportError> {
    let temp_path = temporary_export_path(export_path);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total_bytes = 0u64;
    let copy_result = (|| {
        loop {
            let read = reader.read(&mut buffer).map_err(ReportError::Io)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            total_bytes = total_bytes.saturating_add(read as u64);
        }
        output.flush()?;
        output.sync_all()?;
        Ok::<_, ReportError>((total_bytes, format!("{:x}", hasher.finalize())))
    })();
    drop(output);

    match copy_result {
        Ok(result) => {
            fs::rename(&temp_path, export_path).map_err(|error| {
                remove_partial_export(&temp_path);
                ReportError::Io(error)
            })?;
            Ok(result)
        }
        Err(error) => {
            remove_partial_export(&temp_path);
            Err(error)
        }
    }
}

fn hash_file(path: &Path) -> Result<String, ReportError> {
    let mut input = OpenOptions::new().read(true).open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(ReportError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

impl ExportAccumulator {
    fn push_legacy(
        &mut self,
        entry: domain::FileEntry,
        export_rel: PathBuf,
        total_bytes: u64,
        sha256: String,
    ) {
        self.push_hash_line(&sha256, &export_rel);
        self.manifest_entries.push(RawExportManifestEntry {
            file_id: entry.id.0,
            data_source_id: entry.data_source_id.0,
            relative_source_path: entry.path,
            exported_relative_path: normalize_manifest_path(
                &PathBuf::from("files").join(&export_rel),
            ),
            size: entry.size.or(Some(total_bytes)),
            sha256: Some(sha256),
            deleted: entry.deleted,
            hidden: entry.hidden,
            system: entry.system,
        });
    }

    fn push_source(
        &mut self,
        entry: domain::FileEntry,
        global_file_id: String,
        export_rel: PathBuf,
        extracted: u64,
        sha256: String,
    ) {
        self.push_hash_line(&sha256, &export_rel);
        self.manifest_entries.push(RawExportManifestEntry {
            file_id: global_file_id,
            data_source_id: entry.data_source_id.0,
            relative_source_path: entry.path,
            exported_relative_path: normalize_manifest_path(
                &PathBuf::from("files").join(&export_rel),
            ),
            size: entry.size.or(Some(extracted)),
            sha256: Some(sha256),
            deleted: entry.deleted,
            hidden: entry.hidden,
            system: entry.system,
        });
    }

    fn push_hash_line(&mut self, sha256: &str, export_rel: &Path) {
        self.hash_lines.push(format!(
            "{}  {}",
            sha256,
            normalize_manifest_path(&PathBuf::from("files").join(export_rel))
        ));
    }

    fn skip(&mut self, entry: &domain::FileEntry, error: &str) {
        tracing::warn!(
            file_id = %entry.id.0,
            data_source_id = %entry.data_source_id.0,
            "Raw report export skipped unreadable file: {}",
            error
        );
        self.skipped_files.push(format!(
            "{}:{}",
            entry.data_source_id.0,
            sanitize_bundle_component(&entry.id.0)
        ));
    }
}

fn write_bundle_metadata(
    bundle_dir: PathBuf,
    bundle_dir_name: String,
    case_id: &str,
    report_file_name: &str,
    exported: ExportAccumulator,
) -> Result<RawExportBundle, ReportError> {
    let manifest = RawExportManifest {
        case_id: case_id.to_string(),
        generated_from_report: report_file_name.to_string(),
        exported_count: exported.manifest_entries.len(),
        skipped_count: exported.skipped_files.len(),
        skipped_files: exported.skipped_files.clone(),
        files: exported.manifest_entries,
    };
    let manifest_file_name = "manifest.json".to_string();
    let hashes_file_name = "SHA256SUMS.txt".to_string();
    fs::write(
        bundle_dir.join(&manifest_file_name),
        serde_json::to_vec_pretty(&manifest).map_err(|err| ReportError::Other(err.to_string()))?,
    )?;
    fs::write(
        bundle_dir.join(&hashes_file_name),
        exported.hash_lines.join("\n"),
    )?;
    Ok(RawExportBundle {
        bundle_dir_name,
        manifest_file_name,
        hashes_file_name,
        exported_count: manifest.exported_count,
        skipped_count: manifest.skipped_count,
        skipped_files: manifest.skipped_files,
    })
}

fn collect_exportable_file_entries(
    conn: &Connection,
) -> Result<Vec<domain::FileEntry>, ReportError> {
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
         FROM file_entries
         WHERE entry_type = 'file'
         ORDER BY data_source_id ASC, path ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let entry_type: String = row.get(5)?;
        Ok(domain::FileEntry {
            id: domain::FileEntryId(row.get::<_, String>(0)?),
            parent_id: row.get::<_, Option<String>>(1)?.map(domain::FileEntryId),
            data_source_id: domain::DataSourceId(row.get::<_, String>(2)?),
            path: row.get(3)?,
            name: row.get(4)?,
            entry_type: if entry_type.eq_ignore_ascii_case("directory") {
                domain::EntryType::Directory
            } else {
                domain::EntryType::File
            },
            size: row.get(6)?,
            ext: row.get(7)?,
            deleted: row.get::<_, i32>(8)? != 0,
            hidden: row.get::<_, i32>(9)? != 0,
            system: row.get::<_, i32>(10)? != 0,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: row.get(15)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(ReportError::from)
}

fn collect_exportable_file_entries_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<Vec<domain::FileEntry>, ReportError> {
    let mut entries = Vec::new();
    for (_source_id, source_conn) in super::super::open_ready_source_connections(
        conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
    )? {
        entries.extend(collect_exportable_file_entries(&source_conn)?);
    }
    entries.sort_by(|left, right| {
        left.data_source_id
            .0
            .cmp(&right.data_source_id.0)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    Ok(entries)
}

fn create_parent(path: &Path) -> Result<(), ReportError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn temporary_export_path(export_path: &Path) -> PathBuf {
    let file_name = export_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export");
    export_path.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()))
}

fn remove_partial_export(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                file = %path.file_name().and_then(|name| name.to_str()).unwrap_or("export"),
                "Failed to remove partial raw report export: {}",
                error
            );
        }
    }
}

fn bundle_dir_name_from_report(report_file_name: &str) -> String {
    let stem = Path::new(report_file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export");
    format!("{stem}-bundle")
}

fn prepare_bundle_directory(bundle_dir: &Path, overwrite: bool) -> Result<(), ReportError> {
    if bundle_dir.exists() {
        if !overwrite {
            return Err(ReportError::Other(format!(
                "raw export bundle already exists: {} (set overwrite=true to replace it)",
                bundle_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("bundle")
            )));
        }
        fs::remove_dir_all(bundle_dir)?;
    }
    fs::create_dir_all(bundle_dir)?;
    Ok(())
}

pub(crate) fn sanitize_bundle_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if sanitized.trim().is_empty() {
        "file".to_string()
    } else {
        sanitized
    }
}

fn normalize_manifest_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
#[path = "../../../tests/unit/report/raw_bundle.rs"]
mod tests;
