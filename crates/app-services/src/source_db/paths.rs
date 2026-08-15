use super::identity::is_safe_data_source_id;
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use std::path::{Path, PathBuf};

const SOURCES_DIR_NAME: &str = "sources";
const STAGING_DIR_NAME: &str = "staging";
const SOURCE_DB_FILE_NAME: &str = "source.db";
const SOURCE_INDEX_DIR_NAME: &str = "index";

pub fn source_dir(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    case_root.join(SOURCES_DIR_NAME).join(&data_source_id.0)
}

pub fn source_db_path(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    source_dir(case_root, data_source_id).join(SOURCE_DB_FILE_NAME)
}

pub(crate) fn canonical_source_db_rel_path(data_source_id: &DataSourceId) -> String {
    format!(
        "{SOURCES_DIR_NAME}/{}/{SOURCE_DB_FILE_NAME}",
        data_source_id.0
    )
}

pub fn source_index_dir(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    source_dir(case_root, data_source_id).join(SOURCE_INDEX_DIR_NAME)
}

pub fn source_content_index_dir(file_index_dir: &Path) -> PathBuf {
    let mut directory_name = file_index_dir
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new(SOURCE_INDEX_DIR_NAME))
        .to_os_string();
    directory_name.push("-content");
    file_index_dir.with_file_name(directory_name)
}

pub fn source_staging_dir(case_root: &Path, data_source_id: &DataSourceId) -> DbResult<PathBuf> {
    if !is_safe_data_source_id(&data_source_id.0) {
        return Err(DbError::System(format!(
            "Data source '{}' cannot own a staging directory",
            data_source_id.0
        )));
    }
    Ok(case_root.join(STAGING_DIR_NAME).join(&data_source_id.0))
}

#[derive(Debug, Clone)]
pub struct SourceDbLocator {
    pub(super) case_root: PathBuf,
}

impl SourceDbLocator {
    pub fn new(case_root: impl Into<PathBuf>) -> Self {
        Self {
            case_root: case_root.into(),
        }
    }

    pub fn source_dir(&self, data_source_id: &DataSourceId) -> PathBuf {
        source_dir(&self.case_root, data_source_id)
    }

    pub fn source_db_path(&self, data_source_id: &DataSourceId) -> PathBuf {
        source_db_path(&self.case_root, data_source_id)
    }

    pub fn source_index_dir(&self, data_source_id: &DataSourceId) -> PathBuf {
        source_index_dir(&self.case_root, data_source_id)
    }

    pub fn source_staging_dir(&self, data_source_id: &DataSourceId) -> DbResult<PathBuf> {
        source_staging_dir(&self.case_root, data_source_id)
    }
}
