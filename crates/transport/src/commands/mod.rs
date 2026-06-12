use serde::{Deserialize, Serialize};

const MAX_PAGE_LIMIT: u32 = 500;
const DEFAULT_PAGE_LIMIT: u32 = 100;

pub use crate::dto::{
    ArtifactRowDto, CaseMetricsDto, CaseSummaryDto, DataSourceSummaryDto, FileChildrenDto,
    FileEntryRowDto, FileRowsPageDto, FileTreeNodeDto, JobSnapshotDto, RecentCaseDto,
    RecentObjectDto, ReportHistoryItemDto, ReportTemplateDto, SearchResultPageDto,
    TimelineEventDto, TraceItemDto, ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto,
    WarningItemDto,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCaseRequest {
    pub case_root: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examiner: Option<String>,
}

impl CreateCaseRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.case_root.trim().is_empty() {
            return Err("caseRoot is required".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCaseRequest {
    pub case_root: String,
}

impl OpenCaseRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.case_root.trim().is_empty() {
            return Err("caseRoot is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDataSourceRequest {
    pub source_path: String,
}

impl ImportDataSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_import_source_path(&self.source_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileHandleRequest {
    pub file_id: String,
}

impl OpenFileHandleRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractFileRequest {
    pub file_id: String,
    pub destination_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

impl ExtractFileRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        validate_export_destination_path(&self.destination_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub case_root: String,
    pub image_search_paths: Vec<String>,
    pub theme: String,
    pub dev_event_trace: bool,
    /// Maximum parallel workers for import. None = use all available cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_import_workers: Option<usize>,
    /// Maximum parallel workers for post-import analysis. None = use all available cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_analysis_workers: Option<usize>,
    /// Default analysis depth for import-time post processing.
    #[serde(default = "default_import_analysis_mode")]
    pub import_analysis_mode: String,
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            case_root: default_case_root(),
            image_search_paths: Vec::new(),
            theme: "light".to_string(),
            dev_event_trace: false,
            max_import_workers: None,
            max_analysis_workers: None,
            import_analysis_mode: default_import_analysis_mode(),
        }
    }
}

impl AppSettingsDto {
    pub fn validate(&self) -> Result<(), String> {
        validate_config_directory_path("caseRoot", &self.case_root, true)?;
        for path in &self.image_search_paths {
            validate_config_directory_path("imageSearchPaths", path, false)?;
        }
        if self.theme != "light" && self.theme != "dark" {
            return Err("theme must be light or dark".to_string());
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
        Ok(())
    }
}

fn default_import_analysis_mode() -> String {
    "metadataOnly".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_file_browser_limit")]
    pub limit: u32,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub sort_key: FileSortKeyDto,
    #[serde(default)]
    pub sort_direction: FileSortDirectionDto,
}

impl Default for GetFileRowsRequest {
    fn default() -> Self {
        Self {
            parent_id: None,
            offset: 0,
            limit: default_file_browser_limit(),
            show_hidden: false,
            sort_key: FileSortKeyDto::default(),
            sort_direction: FileSortDirectionDto::default(),
        }
    }
}

impl GetFileRowsRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = default_file_browser_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileChildrenRequest {
    pub parent_id: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_file_tree_limit")]
    pub limit: u32,
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetFileTreeRequest {
    #[serde(default)]
    pub show_hidden: bool,
}

impl GetFileChildrenRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.parent_id.trim().is_empty() {
            return Err("parentId is required".to_string());
        }
        if self.limit == 0 {
            self.limit = default_file_tree_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

fn default_file_browser_limit() -> u32 {
    500
}

fn default_file_tree_limit() -> u32 {
    500
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FileSortKeyDto {
    #[default]
    Name,
    Size,
    ModifiedAt,
    Ext,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FileSortDirectionDto {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilesRequest {
    pub query: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

impl SearchFilesRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.query.trim().is_empty() {
            return Err("query is required".to_string());
        }
        if self.limit == 0 {
            self.limit = default_search_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

fn default_search_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetArtifactRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyFilesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunEvidenceClassificationRequest {
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunAnalysisExtractionRequest {
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAnalysisExtractionRequest {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_analysis_extraction_limit")]
    pub limit: u32,
}

impl Default for GetAnalysisExtractionRequest {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: default_analysis_extraction_limit(),
        }
    }
}

impl GetAnalysisExtractionRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = default_analysis_extraction_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

fn default_analysis_extraction_limit() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameDataSourceRequest {
    pub data_source_id: String,
    pub name: String,
}

impl RenameDataSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.data_source_id.trim().is_empty() {
            return Err("dataSourceId is required".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCaseRequest {
    pub case_root: String,
}

impl DeleteCaseRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.case_root.trim().is_empty() {
            return Err("caseRoot is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDataSourceRequest {
    pub data_source_id: String,
}

impl DeleteDataSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.data_source_id.trim().is_empty() {
            return Err("dataSourceId is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimelineRequest {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_timeline_limit")]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
}

impl GetTimelineRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = DEFAULT_PAGE_LIMIT;
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        if let (Some(start), Some(end)) = (&self.time_start, &self.time_end) {
            if start > end {
                return Err("timeStart must be before or equal to timeEnd".to_string());
            }
        }
        Ok(())
    }
}

fn default_timeline_limit() -> u32 {
    100
}

fn validate_import_source_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("sourcePath is required".to_string());
    }
    if trimmed.contains('\0') {
        return Err("sourcePath contains a null byte".to_string());
    }

    let normalized = trimmed.replace('/', "\\");
    let upper = normalized.to_ascii_uppercase();
    if upper.starts_with("\\\\.\\") {
        return Err("Windows device paths are not supported".to_string());
    }
    if upper.starts_with("\\\\?\\") {
        return Err("Extended-length Windows paths are not supported".to_string());
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    for component in normalized
        .split('\\')
        .filter(|component| !component.is_empty())
    {
        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        if reserved.contains(&stem.as_str()) {
            return Err(format!("{} is a reserved Windows device name", stem));
        }
    }

    Ok(())
}

fn validate_export_destination_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("destinationPath is required".to_string());
    }
    if trimmed.contains('\0') {
        return Err("destinationPath contains a null byte".to_string());
    }
    let normalized = trimmed.replace('/', "\\");
    let upper = normalized.to_ascii_uppercase();
    if upper.starts_with("\\\\.\\") || upper.starts_with("\\\\?\\") {
        return Err("device destination paths are not supported".to_string());
    }
    Ok(())
}

fn validate_config_directory_path(field: &str, path: &str, must_exist: bool) -> Result<(), String> {
    validate_import_source_path(path)?;
    let metadata =
        std::fs::metadata(path).map_err(|_| format!("{field} must exist and be accessible"))?;
    if !metadata.is_dir() {
        return Err(format!("{field} must point to a directory"));
    }
    if must_exist {
        std::fs::read_dir(path).map_err(|_| format!("{field} must be a readable directory"))?;
    }
    Ok(())
}

fn default_case_root() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(|root| format!("{root}\\ForensicsWorkbench\\cases"))
            .unwrap_or_else(|_| "C:\\ForensicsWorkbench\\cases".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|root| format!("{root}/.forensics-workbench/cases"))
            .unwrap_or_else(|_| "/tmp/.forensics-workbench/cases".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportScopeDto {
    #[serde(default = "default_true")]
    pub file_system_metadata: bool,
    #[serde(default = "default_true")]
    pub registry: bool,
    #[serde(default = "default_true")]
    pub full_timeline: bool,
    #[serde(default)]
    pub raw_file_extraction: bool,
    #[serde(default)]
    pub overwrite: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ExportScopeDto {
    fn default() -> Self {
        Self {
            file_system_metadata: true,
            registry: true,
            full_timeline: true,
            raw_file_extraction: false,
            overwrite: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_files_request_deserializes_sample_size() {
        let request: ClassifyFilesRequest = serde_json::from_str(r#"{"sampleSize":1000}"#).unwrap();
        assert_eq!(request.sample_size, Some(1000));
    }

    #[test]
    fn import_source_rejects_reserved_device_names() {
        let request = ImportDataSourceRequest {
            source_path: "CON".to_string(),
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn extract_file_request_rejects_device_destination() {
        let request = ExtractFileRequest {
            file_id: "file-1".to_string(),
            destination_path: r"\\.\PhysicalDrive0".to_string(),
            overwrite: false,
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn import_source_rejects_windows_device_paths() {
        let request = ImportDataSourceRequest {
            source_path: r"\\.\PhysicalDrive0".to_string(),
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn import_source_rejects_extended_length_paths() {
        let request = ImportDataSourceRequest {
            source_path: r"\\?\C:\evidence.E01".to_string(),
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn timeline_request_clamps_limit() {
        let mut request = GetTimelineRequest {
            limit: u32::MAX,
            ..Default::default()
        };

        request.validate().unwrap();

        assert_eq!(request.limit, MAX_PAGE_LIMIT);
    }

    #[test]
    fn timeline_request_rejects_reversed_time_range() {
        let mut request = GetTimelineRequest {
            time_start: Some("2026-02-02T00:00:00Z".to_string()),
            time_end: Some("2026-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn app_settings_rejects_invalid_theme() {
        let settings = AppSettingsDto {
            case_root: std::env::temp_dir().display().to_string(),
            theme: "sepia".to_string(),
            ..Default::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn app_settings_rejects_missing_case_root() {
        let settings = AppSettingsDto {
            case_root: "Z:/definitely/missing/forensics/path".to_string(),
            ..Default::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn app_settings_rejects_zero_import_workers() {
        let settings = AppSettingsDto {
            case_root: std::env::temp_dir().display().to_string(),
            max_import_workers: Some(0),
            ..Default::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn app_settings_rejects_zero_analysis_workers() {
        let settings = AppSettingsDto {
            case_root: std::env::temp_dir().display().to_string(),
            max_analysis_workers: Some(0),
            ..Default::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn app_settings_defaults_to_metadata_only_import_analysis() {
        let settings: AppSettingsDto = serde_json::from_str(&format!(
            r#"{{"caseRoot":"{}","imageSearchPaths":[],"theme":"light","devEventTrace":false}}"#,
            std::env::temp_dir()
                .display()
                .to_string()
                .replace('\\', "\\\\")
        ))
        .unwrap();

        assert_eq!(settings.import_analysis_mode, "metadataOnly");
        settings.validate().unwrap();
    }

    #[test]
    fn app_settings_rejects_unknown_import_analysis_mode() {
        let settings = AppSettingsDto {
            case_root: std::env::temp_dir().display().to_string(),
            import_analysis_mode: "deepMagic".to_string(),
            ..Default::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn export_scope_defaults_enable_existing_sections_only() {
        let scope: ExportScopeDto = serde_json::from_str("{}").unwrap();

        assert!(scope.file_system_metadata);
        assert!(scope.registry);
        assert!(scope.full_timeline);
        assert!(!scope.raw_file_extraction);
        assert!(!scope.overwrite);
    }

    #[test]
    fn file_rows_request_deserializes_show_hidden_camel_case() {
        let request: GetFileRowsRequest =
            serde_json::from_str(r#"{"parentId":"root","offset":10,"limit":50,"showHidden":true}"#)
                .unwrap();

        assert_eq!(request.parent_id.as_deref(), Some("root"));
        assert_eq!(request.offset, 10);
        assert_eq!(request.limit, 50);
        assert!(request.show_hidden);
    }

    #[test]
    fn file_tree_request_defaults_show_hidden_to_false() {
        let request: GetFileTreeRequest = serde_json::from_str("{}").unwrap();
        assert!(!request.show_hidden);
    }

    #[test]
    fn file_children_request_deserializes_show_hidden_camel_case() {
        let request: GetFileChildrenRequest =
            serde_json::from_str(r#"{"parentId":"root","showHidden":true}"#).unwrap();

        assert_eq!(request.parent_id, "root");
        assert!(request.show_hidden);
    }
}
