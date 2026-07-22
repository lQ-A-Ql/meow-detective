use serde::{Deserialize, Serialize};

use super::validation::{default_case_root, validate_config_directory_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub case_root: String,
    pub image_search_paths: Vec<String>,
    pub dev_event_trace: bool,
    /// Maximum parallel workers for import. None = bounded automatic scheduling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_import_workers: Option<usize>,
    /// Maximum parallel workers for post-import analysis. None = bounded automatic scheduling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_analysis_workers: Option<usize>,
    /// Default analysis depth for import-time post processing.
    #[serde(default = "default_import_analysis_mode")]
    pub import_analysis_mode: String,
    /// Number of bytes requested in one hex viewer chunk.
    #[serde(default = "default_hex_chunk_bytes")]
    pub hex_chunk_bytes: u32,
    /// Maximum number of bytes returned by a single viewer range request.
    #[serde(default = "default_max_viewer_range_length")]
    pub max_viewer_range_length: u32,
    /// Maximum file size for inline image previews.
    #[serde(default = "default_max_inline_image_preview_bytes")]
    pub max_inline_image_preview_bytes: u64,
    /// Maximum file size for inline media previews.
    #[serde(default = "default_max_inline_media_preview_bytes")]
    pub max_inline_media_preview_bytes: u64,
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            case_root: default_case_root(),
            image_search_paths: Vec::new(),
            dev_event_trace: false,
            max_import_workers: None,
            max_analysis_workers: None,
            import_analysis_mode: default_import_analysis_mode(),
            hex_chunk_bytes: default_hex_chunk_bytes(),
            max_viewer_range_length: default_max_viewer_range_length(),
            max_inline_image_preview_bytes: default_max_inline_image_preview_bytes(),
            max_inline_media_preview_bytes: default_max_inline_media_preview_bytes(),
        }
    }
}

impl AppSettingsDto {
    pub fn validate(&self) -> Result<(), String> {
        validate_config_directory_path("caseRoot", &self.case_root, true)?;
        for path in &self.image_search_paths {
            validate_config_directory_path("imageSearchPaths", path, false)?;
        }
        if self.max_import_workers == Some(0) {
            return Err("maxImportWorkers must be greater than zero".to_string());
        }
        if self.max_analysis_workers == Some(0) {
            return Err("maxAnalysisWorkers must be greater than zero".to_string());
        }
        if !matches!(
            self.import_analysis_mode.as_str(),
            "metadataOnly" | "budgetedContent" | "fullContent"
        ) {
            return Err(
                "importAnalysisMode must be metadataOnly, budgetedContent, or fullContent"
                    .to_string(),
            );
        }
        if self.hex_chunk_bytes == 0 {
            return Err("hexChunkBytes must be greater than zero".to_string());
        }
        if self.hex_chunk_bytes < 1024 {
            return Err("hexChunkBytes must be at least 1024".to_string());
        }
        if self.max_viewer_range_length == 0 {
            return Err("maxViewerRangeLength must be greater than zero".to_string());
        }
        if self.max_viewer_range_length < 4096 {
            return Err("maxViewerRangeLength must be at least 4096".to_string());
        }
        if self.max_inline_image_preview_bytes == 0 {
            return Err("maxInlineImagePreviewBytes must be greater than zero".to_string());
        }
        if self.max_inline_media_preview_bytes == 0 {
            return Err("maxInlineMediaPreviewBytes must be greater than zero".to_string());
        }
        Ok(())
    }
}

fn default_import_analysis_mode() -> String {
    "metadataOnly".to_string()
}

fn default_hex_chunk_bytes() -> u32 {
    64 * 1024
}

fn default_max_viewer_range_length() -> u32 {
    1024 * 1024
}

fn default_max_inline_image_preview_bytes() -> u64 {
    5 * 1024 * 1024
}

fn default_max_inline_media_preview_bytes() -> u64 {
    20 * 1024 * 1024
}
