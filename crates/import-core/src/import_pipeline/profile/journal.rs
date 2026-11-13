use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("import profile output failed: {message}")]
pub struct ImportProfileOutputError {
    pub kind: io::ErrorKind,
    pub message: String,
}

impl From<io::Error> for ImportProfileOutputError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl transport::ServiceErrorCategory for ImportProfileOutputError {
    fn category(&self) -> transport::ErrorCategory {
        transport::ErrorCategory::Io
    }
    fn code(&self) -> Option<&'static str> {
        Some("IMPORT_PROFILE_OUTPUT")
    }
    fn user_message(&self) -> Option<&'static str> {
        Some("Import performance measurements could not be saved")
    }
}

#[derive(Debug)]
pub(super) struct ProfileJournal {
    directory: PathBuf,
    sequence: u64,
    error: Option<ImportProfileOutputError>,
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/profile_journal.rs"]
mod tests;

impl ProfileJournal {
    pub(super) fn new(directory: &Path) -> io::Result<Self> {
        let directory = directory.join(format!("import-profile-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory)?;
        Ok(Self {
            directory,
            sequence: 0,
            error: None,
        })
    }

    pub(super) fn error(&self) -> Option<ImportProfileOutputError> {
        self.error.clone()
    }

    pub(super) fn append(
        &mut self,
        kind: &str,
        value: &impl Serialize,
    ) -> Result<(), ImportProfileOutputError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if let Err(error) = self.write_entry(kind, value) {
            let error = ImportProfileOutputError::from(error);
            self.error = Some(error.clone());
            return Err(error);
        }
        Ok(())
    }

    fn write_entry(&mut self, kind: &str, value: &impl Serialize) -> io::Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("profile sequence overflow"))?;
        let name = format!("{:020}-{kind}", self.sequence);
        let staging = self.directory.join(format!("{name}.writing"));
        let committed = self.directory.join(format!("{name}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)?;
        serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        fs::rename(staging, committed)
    }
}
