/// Maximum number of files to process for artifact extraction during import.
pub const ARTIFACT_EXTRACTION_LIMIT: usize = 500;

/// Maximum number of files to index for full-text search during import.
pub const TEXT_INDEX_LIMIT: usize = 1000;

/// Maximum number of bytes to read in a single file range request.
pub const MAX_RANGE_LENGTH: usize = 1024 * 1024;

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
