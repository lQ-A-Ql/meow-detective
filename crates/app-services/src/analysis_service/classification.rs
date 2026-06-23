use crate::analysis_service::candidates::row_to_file_entry_for_analysis;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::provenance::{
    file_classification_provenance, metadata_classification_provenance,
};
use domain::{FileEntry, FileEntryId};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use transport::dto::analysis::AnalysisProvenanceDto;
use transport::dto::{
    AnalysisClassifiedFileDto, AnalysisFileClassificationDto, AnalysisParseStatusDto,
};

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

pub fn classify_files_by_metadata(
    conn: &Connection,
    sample_size: u32,
) -> Result<Vec<AnalysisFileClassificationDto>, AnalysisServiceError> {
    let category_stats = metadata_category_stats(conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                    hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE
             ORDER BY size DESC, path ASC
             LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![sample_size as i64], row_to_file_entry_for_analysis)?;
    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
    }

    let mut classifications = classify_files_by_extension_path(&files, sample_size);
    apply_metadata_category_stats(&mut classifications, category_stats);
    Ok(classifications)
}

pub(crate) fn metadata_category_stats(
    conn: &Connection,
) -> Result<HashMap<String, (u64, u64)>, AnalysisServiceError> {
    let mut stmt = conn.prepare(
        "SELECT path, COALESCE(size, 0)
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE",
    )?;
    let mut rows = stmt.query([])?;
    let mut stats: HashMap<String, (u64, u64)> = HashMap::new();
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let size: u64 = row.get(1)?;
        let category = detect_file_type(&path, None)
            .map(|(_, category, _)| category)
            .unwrap_or("Other");
        let entry = stats.entry(category.to_string()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += size;
    }
    Ok(stats)
}

fn apply_metadata_category_stats(
    classifications: &mut Vec<AnalysisFileClassificationDto>,
    mut stats: HashMap<String, (u64, u64)>,
) {
    for classification in classifications.iter_mut() {
        if let Some((count, total_size)) = stats.remove(&classification.category) {
            classification.file_count = count;
            classification.total_size = total_size;
        }
    }

    for (category, (count, total_size)) in stats {
        classifications.push(AnalysisFileClassificationDto {
            category,
            files: Vec::new(),
            file_count: count,
            total_size,
            status: AnalysisParseStatusDto::Parsed,
            warnings: vec![
                "category totals are from metadata aggregate; sample list contains no rows for this category".to_string(),
            ],
            provenance: Vec::new(),
        });
    }

    classifications.sort_by_key(|classification| std::cmp::Reverse(classification.total_size));
}

fn classify_files_by_extension_path(
    files: &[FileEntry],
    sample_size: u32,
) -> Vec<AnalysisFileClassificationDto> {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut categories: HashMap<String, AnalysisFileClassificationDto> = HashMap::new();
    let mut other = AnalysisFileClassificationDto {
        category: "Other".to_string(),
        files: Vec::new(),
        file_count: 0,
        total_size: 0,
        status: AnalysisParseStatusDto::Parsed,
        warnings: Vec::new(),
        provenance: Vec::new(),
    };

    for entry in files.iter().take(sample_size as usize) {
        let size = entry.size.unwrap_or(0);
        let detected = detect_file_type(&entry.path, None);
        let provenance = metadata_classification_provenance(entry, &parsed_at);
        if let Some((file_type, category, description)) = detected {
            let file = classified_file(entry, size, file_type, description, provenance.clone());
            let cat = categories.entry(category.to_string()).or_insert_with(|| {
                AnalysisFileClassificationDto {
                    category: category.to_string(),
                    files: Vec::new(),
                    file_count: 0,
                    total_size: 0,
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                    provenance: Vec::new(),
                }
            });
            cat.files.push(file);
            cat.file_count += 1;
            cat.total_size += size;
            cat.provenance.push(provenance);
        } else {
            other.files.push(classified_file(
                entry,
                size,
                "Unknown",
                "Unknown file type from metadata",
                provenance.clone(),
            ));
            other.file_count += 1;
            other.total_size += size;
            other.provenance.push(provenance);
        }
    }

    if files.len() > sample_size as usize {
        other.warnings.push(format!(
            "仅按元数据抽样前 {} 个文件；查询候选包含 {} 个文件。",
            sample_size,
            files.len()
        ));
    }

    let mut result: Vec<_> = categories.into_values().collect();
    if !other.files.is_empty() || !other.warnings.is_empty() {
        result.push(other);
    }
    result.sort_by_key(|classification| std::cmp::Reverse(classification.total_size));
    result
}

/// Classify files by magic bytes. The reader receives a FileEntryId and must
/// return a bounded header buffer, not a whole-file read.
pub fn classify_files_by_magic<E: std::fmt::Display>(
    files: &[FileEntry],
    sample_size: u32,
    mut read_header_fn: impl FnMut(&FileEntryId) -> Result<Vec<u8>, E>,
) -> Vec<AnalysisFileClassificationDto> {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut categories: HashMap<String, AnalysisFileClassificationDto> = HashMap::new();
    let mut unclassified = AnalysisFileClassificationDto {
        category: "Other".to_string(),
        files: Vec::new(),
        file_count: 0,
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
                    file_count: 0,
                    total_size: 0,
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                    provenance: Vec::new(),
                }
            });
            cat.files.push(entry);
            cat.file_count += 1;
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
            unclassified.file_count += 1;
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
    result.sort_by_key(|classification| std::cmp::Reverse(classification.total_size));
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
