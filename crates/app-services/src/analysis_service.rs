//! Data source analysis service.
//!
//! Provides system information status reporting and bounded file classification.

use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use transport::dto::analysis::AnalysisProvenanceDto;
use transport::dto::{
    AnalysisClassifiedFileDto, AnalysisFileClassificationDto, AnalysisParseStatusDto,
    AnalysisSystemInfoDto,
};

pub const DEFAULT_SAMPLE_SIZE: u32 = 1000;
pub const MAX_SAMPLE_SIZE: u32 = 5000;
pub const MAGIC_HEADER_LIMIT: usize = 8 * 1024;

/// Magic bytes signature.
struct MagicSignature {
    offset: usize,
    bytes: &'static [u8],
    file_type: &'static str,
    description: &'static str,
    category: &'static str,
}

/// Known magic signatures.
const MAGIC_SIGNATURES: &[MagicSignature] = &[
    MagicSignature {
        offset: 0,
        bytes: b"MZ",
        file_type: "PE",
        description: "Windows Executable",
        category: "Executables",
    },
    MagicSignature {
        offset: 0,
        bytes: b"\x7fELF",
        file_type: "ELF",
        description: "Linux Executable",
        category: "Executables",
    },
    MagicSignature {
        offset: 0,
        bytes: b"%PDF",
        file_type: "PDF",
        description: "PDF Document",
        category: "Documents",
    },
    MagicSignature {
        offset: 0,
        bytes: b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
        file_type: "OLE2",
        description: "Office Document",
        category: "Documents",
    },
    MagicSignature {
        offset: 0,
        bytes: b"\xff\xd8\xff",
        file_type: "JPEG",
        description: "JPEG Image",
        category: "Images",
    },
    MagicSignature {
        offset: 0,
        bytes: b"\x89PNG\r\n\x1a\n",
        file_type: "PNG",
        description: "PNG Image",
        category: "Images",
    },
    MagicSignature {
        offset: 0,
        bytes: b"GIF87a",
        file_type: "GIF",
        description: "GIF Image",
        category: "Images",
    },
    MagicSignature {
        offset: 0,
        bytes: b"GIF89a",
        file_type: "GIF",
        description: "GIF Image",
        category: "Images",
    },
    MagicSignature {
        offset: 0,
        bytes: b"BM",
        file_type: "BMP",
        description: "Bitmap Image",
        category: "Images",
    },
    MagicSignature {
        offset: 0,
        bytes: b"RIFF",
        file_type: "WEBP",
        description: "WebP Image",
        category: "Images",
    },
    MagicSignature {
        offset: 0,
        bytes: b"PK\x03\x04",
        file_type: "ZIP",
        description: "ZIP Archive",
        category: "Archives",
    },
    MagicSignature {
        offset: 0,
        bytes: b"Rar!\x1a\x07",
        file_type: "RAR",
        description: "RAR Archive",
        category: "Archives",
    },
    MagicSignature {
        offset: 0,
        bytes: b"\x37\x7a\xbc\xaf\x27\x1c",
        file_type: "7Z",
        description: "7-Zip Archive",
        category: "Archives",
    },
    MagicSignature {
        offset: 0,
        bytes: b"SQLite format 3",
        file_type: "SQLite",
        description: "SQLite Database",
        category: "Databases",
    },
    MagicSignature {
        offset: 0,
        bytes: b"EVF\x09\x0d\x0a\xff\x00",
        file_type: "E01",
        description: "EnCase Image",
        category: "Forensics",
    },
    MagicSignature {
        offset: 0,
        bytes: b"LF\x09\x0d\x0a\xff\x00",
        file_type: "AFF",
        description: "AFF Image",
        category: "Forensics",
    },
    MagicSignature {
        offset: 0,
        bytes: b"regf",
        file_type: "REG",
        description: "Registry Hive",
        category: "System",
    },
];

const REGISTRY_HEADER_LIMIT: usize = 4096;
const REGISTRY_SYSTEM_PARSER: &str = "registry.system";
const REGISTRY_SOFTWARE_PARSER: &str = "registry.software";
const EVTX_BOOT_SHUTDOWN_PARSER: &str = "evtx.boot_shutdown";
const MAGIC_CLASSIFICATION_PARSER: &str = "analysis.magic";

/// Registry and EVTX value parsers are not fully wired yet. This analyzer only
/// verifies catalog presence/readability and preserves parser state as
/// provenance; it does not manufacture host facts from file presence.
pub fn extract_system_info_for_case(
    conn: &Connection,
    mut read_header_fn: impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
) -> AnalysisSystemInfoDto {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut warnings = Vec::new();
    let mut provenance = Vec::new();

    match collect_file_entries(conn) {
        Ok(files) => {
            let system_hive =
                find_windows_artifact(&files, &["windows", "system32", "config"], "system");
            let software_hive =
                find_windows_artifact(&files, &["windows", "system32", "config"], "software");
            let system_evtx = find_windows_artifact(
                &files,
                &["windows", "system32", "winevt", "logs"],
                "system.evtx",
            );

            inspect_registry_hive(
                system_hive,
                REGISTRY_SYSTEM_PARSER,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
            );
            inspect_registry_hive(
                software_hive,
                REGISTRY_SOFTWARE_PARSER,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
            );
            inspect_evtx_boot_source(
                system_evtx,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
            );
        }
        Err(err) => {
            let warning = format!("无法枚举文件目录以发现 Registry/EVTX: {}", err);
            warnings.push(warning.clone());
            provenance.push(unknown_provenance(
                REGISTRY_SYSTEM_PARSER,
                &parsed_at,
                AnalysisParseStatusDto::Unavailable,
                vec![warning],
            ));
        }
    }

    warnings.push(
        "Registry 值遍历与 EVTX 事件解析尚未接入；系统字段和开关机时间保持为空。".to_string(),
    );

    AnalysisSystemInfoDto {
        computer_name: None,
        os_version: None,
        build_number: None,
        install_date: None,
        registered_owner: None,
        organization: None,
        product_id: None,
        network_adapters: Vec::new(),
        boot_history: Vec::new(),
        timezone: None,
        language: None,
        status: AnalysisParseStatusDto::NotParsed,
        warnings,
        provenance,
    }
}

pub fn collect_file_entries(conn: &Connection) -> Result<Vec<FileEntry>, String> {
    let file_repo = FileRepo::new(conn);
    let roots = file_repo.find_root_entries().map_err(|e| e.to_string())?;

    let mut all_files = Vec::new();
    let mut queue = roots;

    while let Some(entry) = queue.pop() {
        if entry.entry_type == EntryType::Directory {
            let children = file_repo
                .find_children(&entry.id)
                .map_err(|e| e.to_string())?;
            queue.extend(children);
        } else {
            all_files.push(entry);
        }
    }

    Ok(all_files)
}

/// Classify files by magic bytes. The reader receives a FileEntryId and must
/// return a bounded header buffer, not a whole-file read.
pub fn classify_files_by_magic(
    files: &[FileEntry],
    sample_size: u32,
    mut read_header_fn: impl FnMut(&FileEntryId) -> Result<Vec<u8>, String>,
) -> Vec<AnalysisFileClassificationDto> {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut categories: HashMap<String, AnalysisFileClassificationDto> = HashMap::new();
    let mut unclassified = AnalysisFileClassificationDto {
        category: "Other".to_string(),
        files: Vec::new(),
        total_size: 0,
        status: AnalysisParseStatusDto::Parsed,
        warnings: Vec::new(),
        provenance: Vec::new(),
    };

    for entry in files.iter().take(sample_size as usize) {
        let size = entry.size.unwrap_or(0);
        let read_result = read_header_fn(&entry.id);
        let file_type = detect_file_type(&entry.path, read_result.as_deref().ok());
        let provenance = file_classification_provenance(entry, &parsed_at, &read_result);

        if let Some((file_type, category, description)) = file_type {
            let entry = classified_file(entry, size, file_type, description, provenance.clone());
            let cat = categories.entry(category.to_string()).or_insert_with(|| {
                AnalysisFileClassificationDto {
                    category: category.to_string(),
                    files: Vec::new(),
                    total_size: 0,
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                    provenance: Vec::new(),
                }
            });
            cat.files.push(entry);
            cat.total_size += size;
            cat.provenance.push(provenance);
        } else {
            if let Err(err) = &read_result {
                unclassified
                    .warnings
                    .push(format!("{}: {}", entry.path, err));
            }
            unclassified.files.push(classified_file(
                entry,
                size,
                "Unknown",
                "Unknown file type",
                provenance.clone(),
            ));
            unclassified.total_size += size;
            unclassified.provenance.push(provenance);
        }
    }

    if sample_size as usize > files.len() {
        // no warning needed when the requested sample exceeds available files
    } else if files.len() > sample_size as usize {
        unclassified.warnings.push(format!(
            "仅分析前 {} 个文件；数据源包含 {} 个文件。",
            sample_size,
            files.len()
        ));
    }

    let mut result: Vec<_> = categories.into_values().collect();
    if !unclassified.files.is_empty() || !unclassified.warnings.is_empty() {
        result.push(unclassified);
    }
    result.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    result
}

fn classified_file(
    entry: &FileEntry,
    size: u64,
    file_type: &str,
    magic_description: &str,
    provenance: AnalysisProvenanceDto,
) -> AnalysisClassifiedFileDto {
    AnalysisClassifiedFileDto {
        file_id: entry.id.0.clone(),
        path: entry.path.clone(),
        name: if entry.name.is_empty() {
            Path::new(&entry.path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            entry.name.clone()
        },
        size,
        file_type: file_type.to_string(),
        magic_description: magic_description.to_string(),
        provenance,
    }
}

fn inspect_registry_hive(
    entry: Option<&FileEntry>,
    parser: &str,
    parsed_at: &str,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
) {
    match entry {
        Some(entry) => {
            let read_result = read_header_fn(&entry.id, REGISTRY_HEADER_LIMIT);
            let mut parser_warnings = Vec::new();
            match read_result {
                Ok(bytes) if bytes.starts_with(b"regf") => {
                    parser_warnings.push(format!(
                        "{} 已发现并验证 regf 头，但当前尚未实现 registry key/value 遍历。",
                        entry.path
                    ));
                }
                Ok(_) => {
                    parser_warnings.push(format!(
                        "{} 不含 regf 头，无法作为 Registry hive 解析。",
                        entry.path
                    ));
                }
                Err(err) => {
                    parser_warnings.push(format!("{} 读取失败: {}", entry.path, err));
                }
            }
            warnings.extend(parser_warnings.clone());
            provenance.push(entry_provenance(
                entry,
                parser,
                parsed_at,
                AnalysisParseStatusDto::NotParsed,
                parser_warnings,
            ));
        }
        None => {
            let artifact_path = match parser {
                REGISTRY_SYSTEM_PARSER => "Windows/System32/config/SYSTEM",
                REGISTRY_SOFTWARE_PARSER => "Windows/System32/config/SOFTWARE",
                _ => "Windows/System32/config",
            };
            let warning = format!("未在证据文件目录中发现 {}。", artifact_path);
            warnings.push(warning.clone());
            provenance.push(AnalysisProvenanceDto {
                data_source_id: String::new(),
                artifact_path: artifact_path.to_string(),
                parser: parser.to_string(),
                parsed_at: parsed_at.to_string(),
                status: AnalysisParseStatusDto::Unavailable,
                warnings: vec![warning],
            });
        }
    }
}

fn inspect_evtx_boot_source(
    entry: Option<&FileEntry>,
    parsed_at: &str,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
) {
    match entry {
        Some(entry) => {
            let mut parser_warnings = Vec::new();
            if let Err(err) = read_header_fn(&entry.id, 8) {
                parser_warnings.push(format!("{} 读取失败: {}", entry.path, err));
            }
            parser_warnings.push(
                "artifacts-windows 当前未提供 EVTX parser；不生成 boot/shutdown 时间戳。"
                    .to_string(),
            );
            warnings.extend(parser_warnings.clone());
            provenance.push(entry_provenance(
                entry,
                EVTX_BOOT_SHUTDOWN_PARSER,
                parsed_at,
                AnalysisParseStatusDto::NotParsed,
                parser_warnings,
            ));
        }
        None => {
            let artifact_path = "Windows/System32/winevt/Logs/System.evtx";
            let warning = format!(
                "未在证据文件目录中发现 {}；EVTX boot/shutdown 解析不可用。",
                artifact_path
            );
            warnings.push(warning.clone());
            provenance.push(AnalysisProvenanceDto {
                data_source_id: String::new(),
                artifact_path: artifact_path.to_string(),
                parser: EVTX_BOOT_SHUTDOWN_PARSER.to_string(),
                parsed_at: parsed_at.to_string(),
                status: AnalysisParseStatusDto::Unavailable,
                warnings: vec![warning],
            });
        }
    }
}

fn find_windows_artifact<'a>(
    files: &'a [FileEntry],
    required_components: &[&str],
    filename: &str,
) -> Option<&'a FileEntry> {
    let filename = filename.to_ascii_lowercase();
    files.iter().find(|entry| {
        if entry.entry_type != EntryType::File {
            return false;
        }
        let normalized = entry.path.replace('\\', "/").to_ascii_lowercase();
        let components = normalized
            .split('/')
            .filter(|component| !component.is_empty())
            .collect::<Vec<_>>();
        let Some(last) = components.last() else {
            return false;
        };
        *last == filename
            && required_components
                .iter()
                .all(|required| components.iter().any(|component| component == required))
    })
}

fn file_classification_provenance(
    entry: &FileEntry,
    parsed_at: &str,
    read_result: &Result<Vec<u8>, String>,
) -> AnalysisProvenanceDto {
    let (status, warnings) = match read_result {
        Ok(_) => (AnalysisParseStatusDto::Parsed, Vec::new()),
        Err(err) => (
            AnalysisParseStatusDto::Unavailable,
            vec![format!("header read failed: {}", err)],
        ),
    };

    entry_provenance(
        entry,
        MAGIC_CLASSIFICATION_PARSER,
        parsed_at,
        status,
        warnings,
    )
}

fn entry_provenance(
    entry: &FileEntry,
    parser: &str,
    parsed_at: &str,
    status: AnalysisParseStatusDto,
    warnings: Vec<String>,
) -> AnalysisProvenanceDto {
    AnalysisProvenanceDto {
        data_source_id: entry.data_source_id.0.clone(),
        artifact_path: entry.path.clone(),
        parser: parser.to_string(),
        parsed_at: parsed_at.to_string(),
        status,
        warnings,
    }
}

fn unknown_provenance(
    parser: &str,
    parsed_at: &str,
    status: AnalysisParseStatusDto,
    warnings: Vec<String>,
) -> AnalysisProvenanceDto {
    AnalysisProvenanceDto {
        data_source_id: String::new(),
        artifact_path: String::new(),
        parser: parser.to_string(),
        parsed_at: parsed_at.to_string(),
        status,
        warnings,
    }
}

fn detect_file_type(
    path: &str,
    header: Option<&[u8]>,
) -> Option<(&'static str, &'static str, &'static str)> {
    if let Some(data) = header {
        for sig in MAGIC_SIGNATURES {
            if data.len() >= sig.offset + sig.bytes.len()
                && &data[sig.offset..sig.offset + sig.bytes.len()] == sig.bytes
            {
                return Some((sig.file_type, sig.category, sig.description));
            }
        }
    }

    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());

    if let Some(ext) = ext {
        match ext.as_str() {
            "exe" | "dll" | "sys" => Some(("PE", "Executables", "Windows Executable")),
            "pdf" => Some(("PDF", "Documents", "PDF Document")),
            "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => {
                Some(("Office", "Documents", "Office Document"))
            }
            "jpg" | "jpeg" => Some(("JPEG", "Images", "JPEG Image")),
            "png" => Some(("PNG", "Images", "PNG Image")),
            "gif" => Some(("GIF", "Images", "GIF Image")),
            "zip" => Some(("ZIP", "Archives", "ZIP Archive")),
            "rar" => Some(("RAR", "Archives", "RAR Archive")),
            "7z" => Some(("7Z", "Archives", "7-Zip Archive")),
            "db" | "sqlite" | "sqlite3" => Some(("SQLite", "Databases", "SQLite Database")),
            "evtx" => Some(("EVTX", "Logs", "Windows Event Log")),
            "pf" => Some(("PF", "Prefetch", "Prefetch File")),
            "lnk" => Some(("LNK", "Shortcuts", "Windows Shortcut")),
            "reg" | "dat" => Some(("REG", "Registry", "Registry File")),
            _ => None,
        }
    } else {
        None
    }
}

/// Generate analysis summary.
pub fn generate_analysis_summary(
    system_info: &AnalysisSystemInfoDto,
    classifications: &[AnalysisFileClassificationDto],
) -> String {
    let mut summary = String::new();

    summary.push_str("# 数据源分析报告\n\n");

    summary.push_str("## 系统信息\n\n");
    match system_info.status {
        AnalysisParseStatusDto::Parsed => {
            push_optional_line(&mut summary, "计算机名", &system_info.computer_name);
            push_optional_line(&mut summary, "操作系统", &system_info.os_version);
            push_optional_line(&mut summary, "Build 号", &system_info.build_number);
            push_optional_line(&mut summary, "注册用户", &system_info.registered_owner);
            push_optional_line(&mut summary, "时区", &system_info.timezone);
        }
        AnalysisParseStatusDto::NotParsed => {
            summary.push_str("- **状态**: 未解析\n");
        }
        AnalysisParseStatusDto::Unavailable => {
            summary.push_str("- **状态**: 不可用\n");
        }
    }

    if !system_info.warnings.is_empty() {
        summary.push_str("\n### 系统信息告警\n\n");
        for warning in &system_info.warnings {
            summary.push_str(&format!("- {}\n", warning));
        }
    }

    if !system_info.network_adapters.is_empty() {
        summary.push_str("\n## 网络适配器\n\n");
        for adapter in &system_info.network_adapters {
            summary.push_str(&format!("- **{}**", adapter.name));
            if let Some(mac) = &adapter.mac_address {
                summary.push_str(&format!(" (MAC: {})", mac));
            }
            summary.push('\n');
        }
    }

    if !system_info.boot_history.is_empty() {
        summary.push_str("\n## 开关机历史\n\n");
        for boot in &system_info.boot_history {
            summary.push_str(&format!("- {} ({})\n", boot.timestamp, boot.boot_type));
        }
    }

    if !classifications.is_empty() {
        summary.push_str("\n## 文件分类\n\n");
        summary.push_str("| 类别 | 文件数 | 总大小 | 状态 |\n");
        summary.push_str("|------|--------|--------|------|\n");
        for cat in classifications {
            summary.push_str(&format!(
                "| {} | {} | {:.1} MB | {} |\n",
                cat.category,
                cat.files.len(),
                cat.total_size as f64 / (1024.0 * 1024.0),
                status_label(&cat.status),
            ));
        }

        let warnings = classifications
            .iter()
            .flat_map(|cat| cat.warnings.iter())
            .collect::<Vec<_>>();
        if !warnings.is_empty() {
            summary.push_str("\n### 文件分类告警\n\n");
            for warning in warnings {
                summary.push_str(&format!("- {}\n", warning));
            }
        }
    } else {
        summary.push_str("\n## 文件分类\n\n- **状态**: 未发现可分类文件。\n");
    }

    summary
}

fn push_optional_line(summary: &mut String, label: &str, value: &Option<String>) {
    if let Some(value) = value {
        summary.push_str(&format!("- **{}**: {}\n", label, value));
    }
}

fn status_label(status: &AnalysisParseStatusDto) -> &'static str {
    match status {
        AnalysisParseStatusDto::Parsed => "已解析",
        AnalysisParseStatusDto::NotParsed => "未解析",
        AnalysisParseStatusDto::Unavailable => "不可用",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, EntryType, FileEntry,
    };
    use persistence_sqlite::repositories::{
        case_repo::CaseRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
    };
    use persistence_sqlite::{open_in_memory, runner};
    use tempfile::TempDir;

    fn file(id: &str, path: &str, size: u64) -> FileEntry {
        FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: path.to_string(),
            name: Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            entry_type: EntryType::File,
            size: Some(size),
            ext: Path::new(path)
                .extension()
                .map(|ext| ext.to_string_lossy().to_string()),
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }
    }

    fn file_with_ds(id: &str, data_source_id: &DataSourceId, path: &str, size: u64) -> FileEntry {
        let mut entry = file(id, path, size);
        entry.data_source_id = data_source_id.clone();
        entry
    }

    fn setup_case_db() -> (Connection, TempDir, DataSourceId) {
        let conn = open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();
        let case = CaseMeta {
            id: CaseId("case-analysis".to_string()),
            name: "Analysis Test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        CaseRepo::new(&conn).create(&case).unwrap();

        let tmp = TempDir::new().unwrap();
        let ds_id = DataSourceId("ds-analysis".to_string());
        let source = DataSource {
            id: ds_id.clone(),
            name: "logical".to_string(),
            kind: DataSourceKind::LogicalDirectory,
            source_path: tmp.path().to_path_buf(),
            imported_at: chrono::Utc::now(),
        };
        DataSourceRepo::new(&conn)
            .insert(&CaseId(case.id.0), &source)
            .unwrap();

        (conn, tmp, ds_id)
    }

    #[test]
    fn exact_length_magic_signatures_are_detected() {
        let files = vec![
            file("pdf", "doc.bin", 4),
            file("zip", "archive.bin", 4),
            file("reg", "NTUSER.DAT", 4),
        ];
        let classifications = classify_files_by_magic(&files, 100, |id| match id.0.as_str() {
            "pdf" => Ok(b"%PDF".to_vec()),
            "zip" => Ok(b"PK\x03\x04".to_vec()),
            "reg" => Ok(b"regf".to_vec()),
            _ => Ok(Vec::new()),
        });

        let detected = classifications
            .iter()
            .flat_map(|cat| cat.files.iter())
            .map(|file| file.file_type.as_str())
            .collect::<Vec<_>>();
        assert!(detected.contains(&"PDF"));
        assert!(detected.contains(&"ZIP"));
        assert!(detected.contains(&"REG"));
    }

    #[test]
    fn system_info_is_not_fabricated_without_parsers() {
        let (conn, _tmp, _ds_id) = setup_case_db();
        let info = extract_system_info_for_case(&conn, |_file_id, _max_bytes| {
            panic!("no files should be read when hives are missing")
        });
        assert_eq!(info.status, AnalysisParseStatusDto::NotParsed);
        assert!(info.computer_name.is_none());
        assert!(info.os_version.is_none());
        assert!(info.build_number.is_none());
        assert!(info.boot_history.is_empty());
        assert!(info.warnings.iter().any(
            |warning| warning.contains("未在证据文件目录中发现 Windows/System32/config/SYSTEM")
        ));
        assert!(info.provenance.iter().any(|item| {
            item.parser == REGISTRY_SYSTEM_PARSER
                && item.status == AnalysisParseStatusDto::Unavailable
        }));
    }

    #[test]
    fn summary_does_not_emit_fake_default_facts() {
        let (conn, _tmp, _ds_id) = setup_case_db();
        let info = extract_system_info_for_case(&conn, |_file_id, _max_bytes| {
            Err("unexpected read".to_string())
        });
        let summary = generate_analysis_summary(&info, &[]);

        assert!(summary.contains("未解析"));
        assert!(!summary.contains("FORENSICS-PC"));
        assert!(!summary.contains("Windows 10"));
        assert!(!summary.contains("19045"));
    }

    #[test]
    fn classification_uses_file_id_reader_and_limits_sample() {
        let files = vec![file("a", "a.exe", 2), file("b", "b.pdf", 4)];
        let mut requested = Vec::new();
        let classifications = classify_files_by_magic(&files, 1, |id| {
            requested.push(id.0.clone());
            Ok(b"MZ".to_vec())
        });

        assert_eq!(requested, vec!["a"]);
        let count: usize = classifications.iter().map(|cat| cat.files.len()).sum();
        assert_eq!(count, 1);
    }

    #[test]
    fn registry_hive_presence_keeps_system_fields_empty_with_provenance() {
        let (conn, _tmp, ds_id) = setup_case_db();
        FileRepo::new(&conn)
            .insert_batch(&[
                file_with_ds("system", &ds_id, "Windows/System32/config/SYSTEM", 4096),
                file_with_ds("software", &ds_id, "Windows/System32/config/SOFTWARE", 4096),
            ])
            .unwrap();

        let info =
            extract_system_info_for_case(&conn, |file_id, _max_bytes| match file_id.0.as_str() {
                "system" | "software" => Ok(b"regf".to_vec()),
                other => Err(format!("unexpected file id {other}")),
            });

        assert_eq!(info.status, AnalysisParseStatusDto::NotParsed);
        assert!(info.computer_name.is_none());
        assert!(info.os_version.is_none());
        assert!(info.registered_owner.is_none());
        assert!(info.provenance.iter().any(|item| {
            item.parser == REGISTRY_SYSTEM_PARSER
                && item.data_source_id == ds_id.0
                && item.artifact_path == "Windows/System32/config/SYSTEM"
                && item.status == AnalysisParseStatusDto::NotParsed
                && item
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("尚未实现 registry key/value 遍历"))
        }));
    }

    #[test]
    fn corrupted_registry_hive_records_warning_without_facts() {
        let (conn, _tmp, ds_id) = setup_case_db();
        FileRepo::new(&conn)
            .insert_batch(&[file_with_ds(
                "system",
                &ds_id,
                "Windows/System32/config/SYSTEM",
                4,
            )])
            .unwrap();

        let info = extract_system_info_for_case(&conn, |_file_id, _max_bytes| Ok(b"BAD!".to_vec()));

        assert!(info.computer_name.is_none());
        assert!(info.boot_history.is_empty());
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("不含 regf 头")));
    }

    #[test]
    fn evtx_source_is_not_parsed_and_generates_no_boot_records() {
        let (conn, _tmp, ds_id) = setup_case_db();
        FileRepo::new(&conn)
            .insert_batch(&[file_with_ds(
                "system-evtx",
                &ds_id,
                "Windows/System32/winevt/Logs/System.evtx",
                8192,
            )])
            .unwrap();

        let info = extract_system_info_for_case(&conn, |_file_id, _max_bytes| {
            Ok(vec![0x45, 0x6c, 0x66, 0x46])
        });

        assert!(info.boot_history.is_empty());
        assert!(info.provenance.iter().any(|item| {
            item.parser == EVTX_BOOT_SHUTDOWN_PARSER
                && item.status == AnalysisParseStatusDto::NotParsed
                && item
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("未提供 EVTX parser"))
        }));
    }

    #[test]
    fn classification_carries_read_error_provenance() {
        let files = vec![file("bad", "bad.bin", 10)];
        let classifications = classify_files_by_magic(&files, 10, |_id| {
            Err("unsupported data source kind".to_string())
        });

        let other = classifications
            .iter()
            .find(|cat| cat.category == "Other")
            .expect("unclassified bucket should be present");
        assert_eq!(
            other.files[0].provenance.status,
            AnalysisParseStatusDto::Unavailable
        );
        assert!(other.files[0]
            .provenance
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported data source kind")));
        assert!(other
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported data source kind")));
    }
}
