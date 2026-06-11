use domain::FileEntry;

use crate::projection::{ExtensionProjection, PathPrefixProjection};

/// In-memory catalog index with materialized projections.
#[derive(Debug, Clone)]
pub struct CatalogIndex {
    extension_projection: ExtensionProjection,
    total_entries: usize,
}

impl CatalogIndex {
    /// Build a catalog index from a list of file entries.
    pub fn build(entries: &[FileEntry]) -> Self {
        Self {
            extension_projection: ExtensionProjection::build(entries),
            total_entries: entries.len(),
        }
    }

    /// Build a catalog index with an additional path prefix projection.
    pub fn build_with_prefixes(
        entries: &[FileEntry],
        prefixes: &[&str],
    ) -> (Self, PathPrefixProjection) {
        let index = Self::build(entries);
        let prefix_proj = PathPrefixProjection::build(entries, prefixes);
        (index, prefix_proj)
    }

    /// Query files by extension.
    pub fn by_extension(&self, ext: &str) -> &[domain::FileEntryId] {
        self.extension_projection.query(ext)
    }

    /// Get all extensions present in the catalog.
    pub fn extensions(&self) -> Vec<&str> {
        self.extension_projection.extensions()
    }

    /// Get the extension projection.
    pub fn extension_projection(&self) -> &ExtensionProjection {
        &self.extension_projection
    }

    /// Total number of indexed entries.
    pub fn len(&self) -> usize {
        self.total_entries
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.total_entries == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};

    fn make_entry(id: &str, path: &str, ext: Option<&str>) -> FileEntry {
        FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            entry_type: EntryType::File,
            size: Some(100),
            ext: ext.map(|s| s.to_string()),
            deleted: false,
            hidden: false,
            system: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }
    }

    #[test]
    fn extension_projection_groups_by_extension() {
        let entries = vec![
            make_entry("1", "a.txt", Some("txt")),
            make_entry("2", "b.txt", Some("txt")),
            make_entry("3", "c.exe", Some("exe")),
            make_entry("4", "d", None),
        ];
        let proj = ExtensionProjection::build(&entries);
        assert_eq!(proj.query("txt").len(), 2);
        assert_eq!(proj.query("exe").len(), 1);
        assert_eq!(proj.query("").len(), 1);
        assert_eq!(proj.query("missing").len(), 0);
    }

    #[test]
    fn path_prefix_projection_groups_by_prefix() {
        let entries = vec![
            make_entry("1", "C:/Windows/System32/cmd.exe", Some("exe")),
            make_entry("2", "C:/Windows/notepad.exe", Some("exe")),
            make_entry("3", "D:/Data/file.txt", Some("txt")),
        ];
        let proj = PathPrefixProjection::build(&entries, &["C:/Windows", "D:/Data"]);
        assert_eq!(proj.query("C:/Windows").len(), 2);
        assert_eq!(proj.query("D:/Data").len(), 1);
        assert_eq!(proj.query("E:/Other").len(), 0);
    }

    #[test]
    fn catalog_index_builds_and_queries() {
        let entries = vec![
            make_entry("1", "a.log", Some("log")),
            make_entry("2", "b.log", Some("log")),
            make_entry("3", "c.txt", Some("txt")),
        ];
        let index = CatalogIndex::build(&entries);
        assert_eq!(index.len(), 3);
        assert_eq!(index.by_extension("log").len(), 2);
        assert_eq!(index.by_extension("txt").len(), 1);
        assert!(index.extensions().contains(&"log"));
    }
}
