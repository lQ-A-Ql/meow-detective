//! Source and generation scoped metadata conversion. Evidence content is never read here.
use domain::{DataSourceId, FileEntry};
use rusqlite::Row;
use search::SearchFileDocument;
use transport::CommandError;

pub(crate) struct MetadataProjection<'a> {
    source: &'a DataSourceId,
    revision: String,
}

impl<'a> MetadataProjection<'a> {
    pub(crate) fn new(source: &'a DataSourceId, revision: String) -> Self {
        Self { source, revision }
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

    /// The bounded catalog query selects source identity as its final column.
    pub(crate) fn row(
        &self,
        row: &Row<'_>,
        file_id: String,
    ) -> rusqlite::Result<SearchFileDocument> {
        if row.get::<_, String>(11)? != self.source.0 || self.revision.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(SearchFileDocument {
            file_id,
            path: row.get(1)?,
            name: row.get(2)?,
            entry_type: row.get::<_, String>(3)?.to_ascii_lowercase(),
            size: row.get(4)?,
            extension: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            modified_at: row
                .get::<_, Option<String>>(6)?
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
                .map(|value| value.timestamp_millis()),
            deleted: row.get::<_, i32>(7)? != 0,
            hidden: row.get::<_, i32>(8)? != 0,
            system: row.get::<_, i32>(9)? != 0,
            encrypted: row.get::<_, i32>(10)? != 0,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/search_metadata_projection.rs"]
mod tests;
