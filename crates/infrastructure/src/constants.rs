/// Maximum number of files to process for artifact extraction during import.
pub const ARTIFACT_EXTRACTION_LIMIT: usize = 500;

/// Maximum number of files to index for full-text search during import.
pub const TEXT_INDEX_LIMIT: usize = 1000;

/// Maximum bytes read from one file for automatic import-time text indexing.
pub const IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES: u64 = 256 * 1024;

/// Maximum number of files indexed automatically during import.
pub const IMPORT_TEXT_INDEX_LIMIT: usize = 100;

/// Maximum number of bytes to read in a single file range request.
pub const MAX_RANGE_LENGTH: usize = 1024 * 1024;

/// Maximum number of rows returned by a single paginated command.
pub const MAX_PAGE_LIMIT: u32 = 500;

/// Default number of rows returned by paginated commands.
pub const DEFAULT_PAGE_LIMIT: u32 = 100;

/// Maximum inline image preview size.
pub const MAX_INLINE_IMAGE_PREVIEW_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum inline media preview size before scoped range streaming is required.
pub const MAX_INLINE_MEDIA_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

/// Maximum bytes artifact extractors may read from one file.
pub const ARTIFACT_FILE_LIMIT_BYTES: u64 = 50 * 1024 * 1024;

/// Maximum number of recent jobs to display.
pub const JOB_LIST_LIMIT: usize = 12;

/// Maximum number of recent cases to remember.
pub const MAX_RECENT_CASES: usize = 8;

/// Maximum number of timeline events per page (default).
pub const TIMELINE_PAGE_SIZE: usize = 100;

/// Maximum number of search results per page.
pub const SEARCH_PAGE_SIZE: usize = 50;

/// Batch size for inserting file entries during enumeration.
pub const FILE_INSERT_BATCH_SIZE: usize = 500;

/// Maximum length for file paths (Windows MAX_PATH).
pub const MAX_PATH_LENGTH: usize = 260;

/// Maximum length for search query strings.
pub const MAX_QUERY_LENGTH: usize = 1000;

/// Maximum length for case names.
pub const MAX_CASE_NAME_LENGTH: usize = 100;

/// Directory name for staging databases during parallel import.
pub const STAGING_DIR_NAME: &str = "staging";

/// Manifest file name for tracking import state.
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

/// Batch size for merging staging rows into main DB.
pub const MERGE_BATCH_SIZE: usize = 10_000;
