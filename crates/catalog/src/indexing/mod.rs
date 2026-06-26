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
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }
    }

    /// Builds entries with a list of (id, path, ext) tuples.
    fn make_entries(entries: &[(&str, &str, Option<&str>)]) -> Vec<FileEntry> {
        entries
            .iter()
            .map(|(id, path, ext)| make_entry(id, path, *ext))
            .collect()
    }

    // ---------------------------------------------------------------------------
    // ExtensionProjection tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_extension_projection_empty() {
        let proj = ExtensionProjection::build(&[]);
        assert!(proj.is_empty());
        assert_eq!(proj.len(), 0);
        assert!(proj.extensions().is_empty());
        assert_eq!(proj.query("txt").len(), 0);
    }

    #[test]
    fn test_extension_projection_groups_by_extension() {
        let entries = make_entries(&[
            ("1", "a.txt", Some("txt")),
            ("2", "b.txt", Some("txt")),
            ("3", "c.exe", Some("exe")),
            ("4", "d", None),
        ]);
        let proj = ExtensionProjection::build(&entries);
        assert_eq!(proj.query("txt").len(), 2);
        assert_eq!(proj.query("exe").len(), 1);
        assert_eq!(proj.query("").len(), 1);
        assert_eq!(proj.query("missing").len(), 0);
    }

    #[test]
    fn test_extension_projection_extensions_list() {
        let entries = make_entries(&[
            ("1", "a.txt", Some("txt")),
            ("2", "b.exe", Some("exe")),
            ("3", "c.dll", Some("dll")),
        ]);
        let proj = ExtensionProjection::build(&entries);
        let mut exts = proj.extensions();
        exts.sort();
        assert_eq!(exts, vec!["dll", "exe", "txt"]);
        assert_eq!(proj.len(), 3);
    }

    #[test]
    fn test_extension_projection_no_duplicate_ext_keys() {
        let entries = make_entries(&[
            ("1", "a.txt", Some("txt")),
            ("2", "b.txt", Some("txt")),
            ("3", "c.txt", Some("txt")),
        ]);
        let proj = ExtensionProjection::build(&entries);
        // Single extension group, three entries inside
        assert_eq!(proj.len(), 1);
        assert_eq!(proj.query("txt").len(), 3);
    }

    // ---------------------------------------------------------------------------
    // PathPrefixProjection tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_path_prefix_projection_empty_entries() {
        // When prefixes are provided, buckets are created for each one even
        // if no entries exist — so prefixes() is non-empty, but each bucket
        // is empty.
        let proj = PathPrefixProjection::build(&[], &["C:/Windows"]);
        assert_eq!(proj.prefixes(), vec!["C:/Windows"]);
        assert_eq!(proj.query("C:/Windows").len(), 0);
        assert_eq!(proj.query("D:/Other").len(), 0);
    }

    #[test]
    fn test_path_prefix_projection_groups_by_prefix() {
        let entries = make_entries(&[
            ("1", "C:/Windows/System32/cmd.exe", Some("exe")),
            ("2", "C:/Windows/notepad.exe", Some("exe")),
            ("3", "D:/Data/file.txt", Some("txt")),
        ]);
        let proj = PathPrefixProjection::build(&entries, &["C:/Windows", "D:/Data"]);
        assert_eq!(proj.query("C:/Windows").len(), 2);
        assert_eq!(proj.query("D:/Data").len(), 1);
        assert_eq!(proj.query("E:/Other").len(), 0);
    }

    #[test]
    fn test_path_prefix_projection_sorted_prefixes() {
        let entries = make_entries(&[
            ("1", "Z:/alpha/file.txt", Some("txt")),
            ("2", "A:/beta/file.exe", Some("exe")),
        ]);
        let proj = PathPrefixProjection::build(&entries, &["Z:/alpha", "A:/beta"]);
        let prefixes = proj.prefixes();
        assert_eq!(prefixes, vec!["A:/beta", "Z:/alpha"]);
    }

    #[test]
    fn test_path_prefix_projection_subset_match() {
        let entries = make_entries(&[
            ("1", "C:/Windows/System32/cmd.exe", Some("exe")),
            ("2", "C:/Windows/notepad.exe", Some("exe")),
            ("3", "C:/Windows", None), // exact match, not a sub-path
            ("4", "D:/Data/file.txt", Some("txt")),
        ]);
        let proj = PathPrefixProjection::build(&entries, &["C:/Windows", "D:/Data"]);
        // All three C:/Windows entries should match (starts_with)
        assert_eq!(proj.query("C:/Windows").len(), 3);
        assert_eq!(proj.query("D:/Data").len(), 1);
    }

    // ---------------------------------------------------------------------------
    // CatalogIndex tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_empty_catalog() {
        let index = CatalogIndex::build(&[]);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.extensions().is_empty());
        assert_eq!(index.by_extension("txt").len(), 0);
    }

    #[test]
    fn test_add_single_file() {
        let entries = make_entries(&[("1", "readme.txt", Some("txt"))]);
        let index = CatalogIndex::build(&entries);
        assert!(!index.is_empty());
        assert_eq!(index.len(), 1);
        assert_eq!(index.by_extension("txt").len(), 1);
        assert_eq!(index.by_extension("txt")[0], FileEntryId("1".to_string()));
    }

    #[test]
    fn test_add_multiple_files() {
        let exts = [
            "txt", "exe", "dll", "log", "json", "xml", "png", "jpg", "pdf", "zip",
        ];
        let entries: Vec<FileEntry> = exts
            .iter()
            .enumerate()
            .map(|(i, ext)| {
                make_entry(
                    &(i + 1).to_string(),
                    &format!("file{}.{}", i + 1, ext),
                    Some(ext),
                )
            })
            .collect();
        let index = CatalogIndex::build(&entries);
        assert_eq!(index.len(), 10);
        let mut got_exts = index.extensions();
        got_exts.sort();
        let mut expected: Vec<&str> = exts.to_vec();
        expected.sort();
        assert_eq!(got_exts, expected);
        // Verify each extension group contains exactly one entry
        for ext in &expected {
            assert_eq!(index.by_extension(ext).len(), 1);
        }
    }

    #[test]
    fn test_extension_projection_catalog() {
        let entries = make_entries(&[
            ("1", "a.txt", Some("txt")),
            ("2", "b.txt", Some("txt")),
            ("3", "c.exe", Some("exe")),
            ("4", "d", None),
        ]);
        let index = CatalogIndex::build(&entries);
        assert_eq!(index.len(), 4);
        assert_eq!(index.by_extension("txt").len(), 2);
        assert_eq!(index.by_extension("exe").len(), 1);
        assert_eq!(index.by_extension("").len(), 1);
        assert_eq!(index.by_extension("nonexistent").len(), 0);
    }

    #[test]
    fn test_path_prefix_projection_catalog() {
        let entries = make_entries(&[
            ("1", "C:/Windows/System32/cmd.exe", Some("exe")),
            ("2", "C:/Windows/notepad.exe", Some("exe")),
            ("3", "D:/Data/file.txt", Some("txt")),
        ]);
        let (index, prefix_proj) =
            CatalogIndex::build_with_prefixes(&entries, &["C:/Windows", "D:/Data"]);
        assert_eq!(index.len(), 3);
        assert_eq!(prefix_proj.query("C:/Windows").len(), 2);
        assert_eq!(prefix_proj.query("D:/Data").len(), 1);
        assert_eq!(prefix_proj.query("E:/Other").len(), 0);
    }

    #[test]
    fn test_rebuild_without_entry() {
        let entries = make_entries(&[("1", "a.txt", Some("txt")), ("2", "b.exe", Some("exe"))]);
        let index = CatalogIndex::build(&entries);
        assert_eq!(index.len(), 2);

        // "Remove" entry "1" by rebuilding without it
        let remaining = make_entries(&[("2", "b.exe", Some("exe"))]);
        let index2 = CatalogIndex::build(&remaining);
        assert_eq!(index2.len(), 1);
        assert_eq!(index2.by_extension("txt").len(), 0);
        assert_eq!(index2.by_extension("exe").len(), 1);
    }

    #[test]
    fn test_large_catalog() {
        let n: usize = 5000;
        let entries: Vec<FileEntry> = (0..n)
            .map(|i| {
                make_entry(
                    &i.to_string(),
                    &format!("dir/file_{}.{}", i, "txt"),
                    Some("txt"),
                )
            })
            .collect();
        let index = CatalogIndex::build(&entries);
        assert_eq!(index.len(), n);
        assert_eq!(index.by_extension("txt").len(), n);
        assert_eq!(index.extensions().len(), 1);
        assert_eq!(index.extensions()[0], "txt");
    }

    #[test]
    fn test_duplicate_handling() {
        let entries = make_entries(&[
            ("1", "a.txt", Some("txt")),
            ("1", "a.txt", Some("txt")), // same id, same data
        ]);
        let index = CatalogIndex::build(&entries);
        // The index counts entries as provided; duplicates are not deduplicated.
        assert_eq!(index.len(), 2);
        // Two copies of the same ID land in the extension bucket.
        assert_eq!(index.by_extension("txt").len(), 2);
        assert_eq!(
            index.by_extension("txt"),
            &[FileEntryId("1".to_string()), FileEntryId("1".to_string())]
        );
    }

    #[test]
    fn test_clear_catalog() {
        let entries = make_entries(&[("1", "a.txt", Some("txt")), ("2", "b.exe", Some("exe"))]);
        let index = CatalogIndex::build(&entries);
        assert_eq!(index.len(), 2);

        // "Clear" by building from empty
        let cleared = CatalogIndex::build(&[]);
        assert!(cleared.is_empty());
        assert_eq!(cleared.len(), 0);
        assert!(cleared.extensions().is_empty());
    }

    #[test]
    fn test_case_isolation() {
        let entries_a = make_entries(&[("1", "a.txt", Some("txt")), ("2", "b.exe", Some("exe"))]);
        let entries_b = make_entries(&[
            ("10", "x.pdf", Some("pdf")),
            ("20", "y.png", Some("png")),
            ("30", "z.zip", Some("zip")),
        ]);

        let index_a = CatalogIndex::build(&entries_a);
        let index_b = CatalogIndex::build(&entries_b);

        // Catalog A
        assert_eq!(index_a.len(), 2);
        assert_eq!(index_a.by_extension("txt").len(), 1);
        assert_eq!(index_a.by_extension("exe").len(), 1);
        assert_eq!(index_a.by_extension("pdf").len(), 0);

        // Catalog B
        assert_eq!(index_b.len(), 3);
        assert_eq!(index_b.by_extension("pdf").len(), 1);
        assert_eq!(index_b.by_extension("png").len(), 1);
        assert_eq!(index_b.by_extension("zip").len(), 1);
        assert_eq!(index_b.by_extension("txt").len(), 0);

        // A remains unchanged
        assert_eq!(index_a.len(), 2);
        assert_eq!(index_a.by_extension("txt").len(), 1);
    }

    #[test]
    fn test_extension_projection_all_empty_extensions() {
        let entries = make_entries(&[
            ("1", "file_without_ext", None),
            ("2", "another_no_ext", None),
            ("3", "yet_another", None),
        ]);
        let proj = ExtensionProjection::build(&entries);
        // All three should group under the empty-string extension key.
        assert_eq!(proj.len(), 1);
        assert_eq!(proj.query("").len(), 3);
        assert_eq!(proj.extensions(), vec![""]);
    }

    #[test]
    fn test_build_with_prefixes_empty_prefixes() {
        let entries = make_entries(&[
            ("1", "C:/Windows/cmd.exe", Some("exe")),
            ("2", "D:/Data/file.txt", Some("txt")),
        ]);
        let (index, prefix_proj) = CatalogIndex::build_with_prefixes(&entries, &[]);
        assert_eq!(index.len(), 2);
        assert!(prefix_proj.prefixes().is_empty());
        // Any query against an empty prefix projection returns nothing.
        assert_eq!(prefix_proj.query("C:/Windows").len(), 0);
    }
}
