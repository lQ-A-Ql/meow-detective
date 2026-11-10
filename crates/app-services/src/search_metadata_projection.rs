//! Source and generation scoped metadata conversion. Evidence content is never read here.
use domain::{DataSourceId, FileEntry};
use search::SearchFileDocument;
use serde::{Deserialize, Serialize};
use transport::CommandError;

pub(crate) struct MetadataProjection<'a> {
    source: &'a DataSourceId,
    revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SearchProjectionMarker {
    pub(crate) source_id: String,
    pub(crate) input_revision: String,
    pub(crate) generation: String,
    pub(crate) schema_version: u32,
    pub(crate) index_path: String,
}

impl<'a> MetadataProjection<'a> {
    pub(crate) fn new(source: &'a DataSourceId, revision: String) -> Self {
        Self { source, revision }
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) fn validate(
        &self,
        source: &DataSourceId,
        revision: &str,
    ) -> Result<(), CommandError> {
        if source != self.source || revision != self.revision || revision.is_empty() {
            return Err(CommandError::security("Search projection input changed"));
        }
        Ok(())
    }

    pub(crate) fn entry(&self, entry: &FileEntry) -> Result<SearchFileDocument, CommandError> {
        self.validate(&entry.data_source_id, &self.revision)?;
        Ok(SearchFileDocument {
            file_id: entry.id.0.clone(),
            path: entry.path.clone(),
            name: entry.name.clone(),
            extension: entry.ext.clone().unwrap_or_default(),
            entry_type: if entry.is_file() { "file" } else { "directory" }.into(),
            size: entry.size,
            modified_at: entry.modified_at.map(|time| time.timestamp_millis()),
            deleted: entry.deleted,
            hidden: entry.hidden,
            system: entry.system,
            encrypted: entry.encrypted,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/search_metadata_projection.rs"]
mod tests;
