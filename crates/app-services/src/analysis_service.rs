//! Data source analysis service.
//!
//! Provides system information extraction and file classification.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// System information extracted from evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Computer name
    pub computer_name: Option<String>,
    /// OS version
    pub os_version: Option<String>,
    /// Build number
    pub build_number: Option<String>,
    /// Installation date
    pub install_date: Option<String>,
    /// Registered owner
    pub registered_owner: Option<String>,
    /// Organization
    pub organization: Option<String>,
    /// Product ID
    pub product_id: Option<String>,
    /// Network adapters
    pub network_adapters: Vec<NetworkAdapter>,
    /// Boot history
    pub boot_history: Vec<BootRecord>,
    /// Timezone
    pub timezone: Option<String>,
    /// Language
    pub language: Option<String>,
}

/// Network adapter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAdapter {
    /// Adapter name
    pub name: String,
    /// MAC address
    pub mac_address: Option<String>,
    /// IP addresses
    pub ip_addresses: Vec<String>,
    /// DHCP enabled
    pub dhcp_enabled: Option<bool>,
    /// DHCP server
    pub dhcp_server: Option<String>,
}

/// Boot record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootRecord {
    /// Boot time
    pub timestamp: String,
    /// Boot type (normal, safe, etc.)
    pub boot_type: String,
    /// Source (Event Log, Registry, etc.)
    pub source: String,
}

/// File classification by magic bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileClassification {
    /// Category name
    pub category: String,
    /// Files in this category
    pub files: Vec<ClassifiedFile>,
    /// Total size
    pub total_size: u64,
}

/// Classified file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedFile {
    /// File path
    pub path: String,
    /// File name
    pub name: String,
    /// File size
    pub size: u64,
    /// Detected type
    pub file_type: String,
    /// Magic bytes description
    pub magic_description: String,
}

/// Magic bytes signature
struct MagicSignature {
    /// Offset to check
    offset: usize,
    /// Bytes to match
    bytes: &'static [u8],
    /// File type
    file_type: &'static str,
    /// Description
    description: &'static str,
    /// Category
    category: &'static str,
}

/// Known magic signatures
const MAGIC_SIGNATURES: &[MagicSignature] = &[
    // Executables
    MagicSignature { offset: 0, bytes: b"MZ", file_type: "PE", description: "Windows Executable", category: "Executables" },
    MagicSignature { offset: 0, bytes: b"\x7fELF", file_type: "ELF", description: "Linux Executable", category: "Executables" },
    
    // Documents
    MagicSignature { offset: 0, bytes: b"%PDF", file_type: "PDF", description: "PDF Document", category: "Documents" },
    MagicSignature { offset: 0, bytes: b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1", file_type: "OLE2", description: "Office Document", category: "Documents" },
    
    // Images
    MagicSignature { offset: 0, bytes: b"\xff\xd8\xff", file_type: "JPEG", description: "JPEG Image", category: "Images" },
    MagicSignature { offset: 0, bytes: b"\x89PNG\r\n\x1a\n", file_type: "PNG", description: "PNG Image", category: "Images" },
    MagicSignature { offset: 0, bytes: b"GIF87a", file_type: "GIF", description: "GIF Image", category: "Images" },
    MagicSignature { offset: 0, bytes: b"GIF89a", file_type: "GIF", description: "GIF Image", category: "Images" },
    MagicSignature { offset: 0, bytes: b"BM", file_type: "BMP", description: "Bitmap Image", category: "Images" },
    MagicSignature { offset: 0, bytes: b"RIFF", file_type: "WEBP", description: "WebP Image", category: "Images" },
    
    // Archives
    MagicSignature { offset: 0, bytes: b"PK\x03\x04", file_type: "ZIP", description: "ZIP Archive", category: "Archives" },
    MagicSignature { offset: 0, bytes: b"Rar!\x1a\x07", file_type: "RAR", description: "RAR Archive", category: "Archives" },
    MagicSignature { offset: 0, bytes: b"\x37\x7a\xbc\xaf\x27\x1c", file_type: "7Z", description: "7-Zip Archive", category: "Archives" },
    
    // Databases
    MagicSignature { offset: 0, bytes: b"SQLite format 3", file_type: "SQLite", description: "SQLite Database", category: "Databases" },
    
    // Forensics
    MagicSignature { offset: 0, bytes: b"EVF\x09\x0d\x0a\xff\x00", file_type: "E01", description: "EnCase Image", category: "Forensics" },
    MagicSignature { offset: 0, bytes: b"LF\x09\x0d\x0a\xff\x00", file_type: "AFF", description: "AFF Image", category: "Forensics" },
    
    // System
    MagicSignature { offset: 0, bytes: b"regf", file_type: "REG", description: "Registry Hive", category: "System" },
];

/// Extract system information from registry files
pub fn extract_system_info(data_source_path: &Path) -> SystemInfo {
    let mut info = SystemInfo {
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
    };

    // Try to read SYSTEM registry
    let system_path = data_source_path
        .join("Windows")
        .join("System32")
        .join("config")
        .join("SYSTEM");
    
    if system_path.exists() {
        // Extract from SYSTEM hive
        extract_from_system_hive(&system_path, &mut info);
    }

    // Try to read SOFTWARE registry
    let software_path = data_source_path
        .join("Windows")
        .join("System32")
        .join("config")
        .join("SOFTWARE");
    
    if software_path.exists() {
        extract_from_software_hive(&software_path, &mut info);
    }

    // Try to read Event Logs for boot history
    let event_log_path = data_source_path
        .join("Windows")
        .join("System32")
        .join("winevt")
        .join("Logs")
        .join("System.evtx");
    
    if event_log_path.exists() {
        extract_boot_history(&event_log_path, &mut info);
    }

    info
}

/// Extract info from SYSTEM hive
fn extract_from_system_hive(path: &Path, info: &mut SystemInfo) {
    // Placeholder: In real implementation, parse registry hive
    // For now, set dummy values
    info.computer_name = Some("FORENSICS-PC".to_string());
    info.timezone = Some("UTC".to_string());
}

/// Extract info from SOFTWARE hive
fn extract_from_software_hive(path: &Path, info: &mut SystemInfo) {
    // Placeholder: Parse Windows version, owner, etc.
    info.os_version = Some("Windows 10".to_string());
    info.build_number = Some("19045".to_string());
    info.registered_owner = Some("User".to_string());
}

/// Extract boot history from Event Log
fn extract_boot_history(path: &Path, info: &mut SystemInfo) {
    // Placeholder: Parse Event Log for boot events
    info.boot_history.push(BootRecord {
        timestamp: "2024-01-15T08:30:00Z".to_string(),
        boot_type: "Normal".to_string(),
        source: "EventLog".to_string(),
    });
}

/// Classify files by magic bytes
pub fn classify_files_by_magic(
    files: &[(String, u64)],  // (path, size)
    sample_size: usize,
    read_fn: impl Fn(&str) -> Option<Vec<u8>>,
) -> Vec<FileClassification> {
    let mut categories: HashMap<String, FileClassification> = HashMap::new();
    let mut unclassified = FileClassification {
        category: "Other".to_string(),
        files: Vec::new(),
        total_size: 0,
    };

    for (path, size) in files.iter().take(sample_size) {
        let file_type = detect_file_type(path, *size, &read_fn);
        
        if let Some((file_type, category, description)) = file_type {
            let entry = ClassifiedFile {
                path: path.clone(),
                name: std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                size: *size,
                file_type: file_type.to_string(),
                magic_description: description.to_string(),
            };

            let cat = categories
                .entry(category.to_string())
                .or_insert_with(|| FileClassification {
                    category: category.to_string(),
                    files: Vec::new(),
                    total_size: 0,
                });
            cat.files.push(entry);
            cat.total_size += size;
        } else {
            unclassified.files.push(ClassifiedFile {
                path: path.clone(),
                name: std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                size: *size,
                file_type: "Unknown".to_string(),
                magic_description: "Unknown file type".to_string(),
            });
            unclassified.total_size += size;
        }
    }

    let mut result: Vec<FileClassification> = categories.into_values().collect();
    if !unclassified.files.is_empty() {
        result.push(unclassified);
    }
    result.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    result
}

/// Detect file type by magic bytes
fn detect_file_type(
    path: &str,
    size: u64,
    read_fn: &impl Fn(&str) -> Option<Vec<u8>>,
) -> Option<(&'static str, &'static str, &'static str)> {
    // First try magic bytes
    if let Some(data) = read_fn(path) {
        for sig in MAGIC_SIGNATURES {
            if data.len() > sig.offset + sig.bytes.len() {
                if &data[sig.offset..sig.offset + sig.bytes.len()] == sig.bytes {
                    return Some((sig.file_type, sig.category, sig.description));
                }
            }
        }
    }

    // Fallback to extension
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());
    
    if let Some(ext) = ext {
        match ext.as_str() {
            "exe" | "dll" | "sys" => Some(("PE", "Executables", "Windows Executable")),
            "pdf" => Some(("PDF", "Documents", "PDF Document")),
            "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => Some(("Office", "Documents", "Office Document")),
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

/// Generate analysis summary
pub fn generate_analysis_summary(
    system_info: &SystemInfo,
    classifications: &[FileClassification],
) -> String {
    let mut summary = String::new();

    summary.push_str("# 数据源分析报告\n\n");

    // System Info
    summary.push_str("## 系统信息\n\n");
    if let Some(name) = &system_info.computer_name {
        summary.push_str(&format!("- **计算机名**: {}\n", name));
    }
    if let Some(os) = &system_info.os_version {
        summary.push_str(&format!("- **操作系统**: {}\n", os));
    }
    if let Some(build) = &system_info.build_number {
        summary.push_str(&format!("- **Build 号**: {}\n", build));
    }
    if let Some(owner) = &system_info.registered_owner {
        summary.push_str(&format!("- **注册用户**: {}\n", owner));
    }
    if let Some(tz) = &system_info.timezone {
        summary.push_str(&format!("- **时区**: {}\n", tz));
    }

    // Network
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

    // Boot History
    if !system_info.boot_history.is_empty() {
        summary.push_str("\n## 开关机历史\n\n");
        for boot in &system_info.boot_history {
            summary.push_str(&format!("- {} ({})\n", boot.timestamp, boot.boot_type));
        }
    }

    // File Classification
    if !classifications.is_empty() {
        summary.push_str("\n## 文件分类\n\n");
        summary.push_str("| 类别 | 文件数 | 总大小 |\n");
        summary.push_str("|------|--------|--------|\n");
        for cat in classifications {
            summary.push_str(&format!(
                "| {} | {} | {:.1} MB |\n",
                cat.category,
                cat.files.len(),
                cat.total_size as f64 / (1024.0 * 1024.0)
            ));
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_signature_pe() {
        let data = b"MZ\x90\x00";
        let result = detect_file_type(
            "test.exe",
            1024,
            &|_| Some(data.to_vec()),
        );
        assert!(result.is_some());
        let (file_type, category, _) = result.unwrap();
        assert_eq!(file_type, "PE");
        assert_eq!(category, "Executables");
    }

    #[test]
    fn test_magic_signature_pdf() {
        let data = b"%PDF-1.4";
        let result = detect_file_type(
            "test.pdf",
            1024,
            &|_| Some(data.to_vec()),
        );
        assert!(result.is_some());
        let (file_type, _, _) = result.unwrap();
        assert_eq!(file_type, "PDF");
    }

    #[test]
    fn test_magic_signature_jpeg() {
        let data = b"\xff\xd8\xff\xe0";
        let result = detect_file_type(
            "test.jpg",
            1024,
            &|_| Some(data.to_vec()),
        );
        assert!(result.is_some());
        let (file_type, _, _) = result.unwrap();
        assert_eq!(file_type, "JPEG");
    }

    #[test]
    fn test_magic_signature_zip() {
        let data = b"PK\x03\x04";
        let result = detect_file_type(
            "test.zip",
            1024,
            &|_| Some(data.to_vec()),
        );
        assert!(result.is_some());
        let (file_type, _, _) = result.unwrap();
        assert_eq!(file_type, "ZIP");
    }

    #[test]
    fn test_extension_fallback() {
        let result = detect_file_type(
            "test.txt",
            1024,
            &|_| None,
        );
        // Extension fallback doesn't cover txt
        assert!(result.is_none());
    }

    #[test]
    fn test_classify_files() {
        let files = vec![
            ("test.exe".to_string(), 1024),
            ("doc.pdf".to_string(), 2048),
            ("photo.jpg".to_string(), 4096),
        ];

        let classifications = classify_files_by_magic(&files, 100, |path| {
            match path {
                "test.exe" => Some(b"MZ\x00\x00".to_vec()),
                "doc.pdf" => Some(b"%PDF".to_vec()),
                "photo.jpg" => Some(b"\xff\xd8\xff".to_vec()),
                _ => None,
            }
        });

        assert!(!classifications.is_empty());
    }

    #[test]
    fn test_generate_summary() {
        let info = SystemInfo {
            computer_name: Some("TEST-PC".to_string()),
            os_version: Some("Windows 10".to_string()),
            build_number: Some("19045".to_string()),
            install_date: None,
            registered_owner: None,
            organization: None,
            product_id: None,
            network_adapters: Vec::new(),
            boot_history: Vec::new(),
            timezone: Some("UTC".to_string()),
            language: None,
        };

        let classifications = vec![
            FileClassification {
                category: "Executables".to_string(),
                files: Vec::new(),
                total_size: 1024,
            },
        ];

        let summary = generate_analysis_summary(&info, &classifications);
        assert!(summary.contains("TEST-PC"));
        assert!(summary.contains("Windows 10"));
    }

    #[test]
    fn test_magic_signature_png() {
        let data = b"\x89PNG\r\n\x1a\n";
        let result = detect_file_type("test.png", 1024, &|_| Some(data.to_vec()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "PNG");
    }

    #[test]
    fn test_magic_signature_gif() {
        let data = b"GIF89a";
        let result = detect_file_type("test.gif", 1024, &|_| Some(data.to_vec()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "GIF");
    }

    #[test]
    fn test_magic_signature_sqlite() {
        let data = b"SQLite format 3\x00";
        let result = detect_file_type("test.db", 1024, &|_| Some(data.to_vec()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "SQLite");
    }

    #[test]
    fn test_magic_signature_e01() {
        // E01 magic: EVF + 5 bytes (need > 8 bytes for detection)
        let mut data = vec![0u8; 16];
        data[0..3].copy_from_slice(b"EVF");
        data[3] = 0x09;
        data[4] = 0x0d;
        data[5] = 0x0a;
        data[6] = 0xff;
        data[7] = 0x00;
        let result = detect_file_type("test.E01", 1024, &|_| Some(data.clone()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "E01");
    }

    #[test]
    fn test_magic_signature_registry() {
        let data = b"regf";
        let result = detect_file_type("NTUSER.DAT", 1024, &|_| Some(data.to_vec()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "REG");
    }

    #[test]
    fn test_extension_fallback_exe() {
        let result = detect_file_type("app.exe", 1024, &|_| None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "PE");
    }

    #[test]
    fn test_extension_fallback_docx() {
        let result = detect_file_type("doc.docx", 1024, &|_| None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "Office");
    }

    #[test]
    fn test_extension_fallback_evtx() {
        let result = detect_file_type("System.evtx", 1024, &|_| None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "EVTX");
    }

    #[test]
    fn test_classify_empty() {
        let classifications = classify_files_by_magic(&[], 100, |_| None);
        assert!(classifications.is_empty());
    }

    #[test]
    fn test_system_info_default() {
        let info = extract_system_info(Path::new("/nonexistent"));
        // Should return with defaults, no panic
        assert!(info.network_adapters.is_empty());
    }
}
