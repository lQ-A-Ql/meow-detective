use super::options::ImportAnalysisMode;

const DEFAULT_MEMORY_SOFT_LIMIT_MB: u64 = 4 * 1024;
const DEFAULT_MEMORY_HARD_LIMIT_MB: u64 = 6 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentBudget {
    pub max_files: u64,
    pub max_bytes_total: u64,
    pub max_bytes_per_file: u64,
    pub allowed_extensions: Vec<String>,
}

impl ContentBudget {
    pub fn disabled() -> Self {
        Self {
            max_files: 0,
            max_bytes_total: 0,
            max_bytes_per_file: 0,
            allowed_extensions: Vec::new(),
        }
    }

    pub fn conservative() -> Self {
        Self {
            max_files: infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT as u64,
            max_bytes_total: 64 * 1024 * 1024,
            max_bytes_per_file: infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES,
            allowed_extensions: vec![
                "txt".to_string(),
                "log".to_string(),
                "csv".to_string(),
                "json".to_string(),
                "xml".to_string(),
                "html".to_string(),
                "htm".to_string(),
                "md".to_string(),
                "pf".to_string(),
                "lnk".to_string(),
                "evtx".to_string(),
            ],
        }
    }

    pub fn full() -> Self {
        Self {
            max_files: 10_000,
            max_bytes_total: 512 * 1024 * 1024,
            max_bytes_per_file: infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES,
            allowed_extensions: Vec::new(),
        }
    }
}

pub fn default_memory_soft_limit_mb() -> u64 {
    DEFAULT_MEMORY_SOFT_LIMIT_MB
}

pub fn default_memory_hard_limit_mb() -> u64 {
    DEFAULT_MEMORY_HARD_LIMIT_MB
}

pub fn content_budget_for_mode(mode: ImportAnalysisMode) -> ContentBudget {
    match mode {
        ImportAnalysisMode::MetadataOnly => ContentBudget::disabled(),
        ImportAnalysisMode::BudgetedContent => ContentBudget::conservative(),
        ImportAnalysisMode::FullContent => ContentBudget::full(),
    }
}
