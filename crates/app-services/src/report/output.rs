use super::ReportError;
use persistence_sqlite::repositories::report_repo::{ReportRecord, ReportRepo};
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) fn prepare_report_output(
    output_dir: &Path,
    file_name: &str,
    overwrite: bool,
) -> Result<PathBuf, ReportError> {
    fs::create_dir_all(output_dir)?;
    let path = output_dir.join(file_name);
    if path.exists() && !overwrite {
        return Err(ReportError::Other(format!(
            "report output already exists: {} (set overwrite=true to replace it)",
            file_name
        )));
    }
    Ok(path)
}

pub(crate) fn write_report_atomically(
    final_path: &Path,
    overwrite: bool,
    write_fn: impl FnOnce(&mut std::fs::File) -> Result<(), ReportError>,
) -> Result<(), ReportError> {
    let parent = final_path.parent().ok_or_else(|| {
        ReportError::Other("report output path must have a parent directory".to_string())
    })?;
    let temp_name = format!(
        ".{}.{}.tmp",
        final_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("report"),
        Uuid::new_v4()
    );
    let temp_path = parent.join(temp_name);
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;

    let write_result = write_fn(&mut temp_file)
        .and_then(|_| temp_file.flush().map_err(ReportError::Io))
        .and_then(|_| temp_file.sync_all().map_err(ReportError::Io));
    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    drop(temp_file);

    if overwrite && final_path.exists() {
        fs::remove_file(final_path).map_err(|err| {
            let _ = fs::remove_file(&temp_path);
            ReportError::Io(err)
        })?;
    }
    fs::rename(&temp_path, final_path).map_err(|err| {
        let _ = fs::remove_file(&temp_path);
        ReportError::Io(err)
    })
}

pub(crate) fn persist_report_record(
    conn: &Connection,
    case_id: &str,
    template_id: &str,
    file_name: &str,
    status: &str,
) -> Result<(), ReportError> {
    ReportRepo::new(conn).insert(&ReportRecord {
        id: Uuid::new_v4().to_string(),
        case_id: case_id.to_string(),
        template_id: template_id.to_string(),
        file_name: file_name.to_string(),
        created_by: String::new(),
        status: status.to_string(),
        progress: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    })?;
    Ok(())
}
