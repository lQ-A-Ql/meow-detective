//! Two-level file classification board: magic-byte family detection first,
//! then scenario-based refinement (office flavors, logs, thumbnails, icons).
//!
//! Rows classified from actual header bytes are marked `magic`; the remainder
//! falls back to extension/path inference and is marked `metadata`.

use super::file_classification_taxonomy::{
    detect_magic, family_of, file_type_from_ext, is_office_ext,
};
use crate::analysis_service::candidates::row_to_file_entry_for_analysis;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{FileEntry, FileEntryId};
use rusqlite::Connection;
use std::collections::HashMap;
use transport::dto::{
    AnalysisParseStatusDto, ClassificationGroupDto, ClassificationSubcategoryDto,
    ClassifiedFileRowDto, FileClassificationBoardDto,
};

/// Header bytes read per file for magic classification.
pub(crate) const MAGIC_HEADER_BYTES: usize = 16;
const MAX_FILES_PER_SUBCATEGORY: usize = 30;

/// Magic-family groups in display order: `(category, display name)`.
const GROUPS: &[(&str, &str)] = &[
    ("documents", "文档"),
    ("images", "图片"),
    ("media", "媒体"),
    ("databases", "数据库"),
    ("executables", "可执行文件"),
    ("archives", "压缩包"),
    ("system", "系统文件"),
    ("forensics", "磁盘镜像"),
    ("other", "其他"),
];

/// Result of classifying one file entry.
struct Classification {
    file_type: Option<&'static str>,
    family: &'static str,
    subcategory: &'static str,
    via_magic: bool,
}

/// Build the two-level classification board for one data source connection.
///
/// The first `magic_read_limit` files (by size, descending) are classified
/// from header bytes; everything else uses extension/path inference. Totals
/// always cover the full file set.
pub fn build_file_classification_board<E: std::fmt::Display>(
    conn: &Connection,
    magic_read_limit: u32,
    mut read_header_fn: impl FnMut(&FileEntryId) -> Result<Vec<u8>, E>,
) -> Result<FileClassificationBoardDto, AnalysisServiceError> {
    let (total_files, total_size) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(COALESCE(size, 0)), 0)
             FROM file_entries WHERE entry_type = 'file' COLLATE NOCASE",
            [],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
        )
        .map_err(AnalysisServiceError::from)?;

    let mut statement = conn.prepare(
        "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
         FROM file_entries
         WHERE entry_type = 'file' COLLATE NOCASE
         ORDER BY COALESCE(size, 0) DESC, path ASC",
    )?;
    let rows = statement.query_map([], row_to_file_entry_for_analysis)?;

    let mut buckets: HashMap<(&'static str, &'static str), Bucket> = HashMap::new();
    let mut magic_classified = 0u64;
    let mut metadata_classified = 0u64;
    let mut seen = 0u64;

    for row in rows {
        let entry = row?;
        seen += 1;
        let header = if seen <= u64::from(magic_read_limit) {
            read_header_fn(&entry.id).ok()
        } else {
            None
        };
        let classification = classify_entry(&entry, header.as_deref());
        if classification.via_magic {
            magic_classified += 1;
        } else {
            metadata_classified += 1;
        }
        buckets
            .entry((classification.family, classification.subcategory))
            .or_default()
            .add(&entry, classification);
    }

    let status = if total_files > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    };
    Ok(FileClassificationBoardDto {
        status,
        generated_at: chrono::Utc::now().to_rfc3339(),
        total_files,
        total_size,
        magic_classified_count: magic_classified,
        metadata_classified_count: metadata_classified,
        groups: assemble_groups(&buckets),
        warnings: Vec::new(),
    })
}

fn assemble_groups(
    buckets: &HashMap<(&'static str, &'static str), Bucket>,
) -> Vec<ClassificationGroupDto> {
    let mut groups = Vec::new();
    for (category, display_name) in GROUPS {
        let mut subcategories = Vec::new();
        let mut group_count = 0u64;
        let mut group_size = 0u64;
        for ((family, name), bucket) in buckets {
            if family != category {
                continue;
            }
            group_count += bucket.count;
            group_size += bucket.size;
            subcategories.push(ClassificationSubcategoryDto {
                name: name.to_string(),
                file_count: bucket.count,
                total_size: bucket.size,
                truncated: bucket.count > bucket.samples.len() as u64,
                files: bucket.samples.clone(),
            });
        }
        if subcategories.is_empty() {
            continue;
        }
        subcategories.sort_by(|left, right| {
            right
                .total_size
                .cmp(&left.total_size)
                .then_with(|| left.name.cmp(&right.name))
        });
        groups.push(ClassificationGroupDto {
            category: category.to_string(),
            display_name: display_name.to_string(),
            file_count: group_count,
            total_size: group_size,
            subcategories,
        });
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.total_size));
    groups
}

#[derive(Default)]
struct Bucket {
    count: u64,
    size: u64,
    samples: Vec<ClassifiedFileRowDto>,
}

impl Bucket {
    fn add(&mut self, entry: &FileEntry, classification: Classification) {
        self.count += 1;
        self.size += entry.size.unwrap_or(0);
        if self.samples.len() < MAX_FILES_PER_SUBCATEGORY {
            self.samples.push(ClassifiedFileRowDto {
                file_id: entry.id.0.clone(),
                name: entry.name.clone(),
                path: entry.path.clone(),
                size: entry.size.unwrap_or(0),
                magic_type: classification.file_type.map(str::to_string),
                classification_source: if classification.via_magic {
                    "magic".to_string()
                } else {
                    "metadata".to_string()
                },
            });
        }
    }
}

/// Classify one entry: header bytes first, extension/path inference after.
fn classify_entry(entry: &FileEntry, header: Option<&[u8]>) -> Classification {
    let ext = entry.ext.as_deref().unwrap_or("").to_ascii_lowercase();
    let name = entry.name.to_ascii_lowercase();
    let path = entry.path.replace('\\', "/").to_ascii_lowercase();

    if let Some(data) = header {
        if let Some((file_type, family)) = detect_magic(data, &ext) {
            let family = if file_type == "ZIP" && is_office_ext(&ext) {
                "documents"
            } else {
                family
            };
            return Classification {
                file_type: Some(file_type),
                family,
                subcategory: subcategory_of(file_type, &ext, &name, &path, family),
                via_magic: true,
            };
        }
    }

    let file_type = file_type_from_ext(&ext);
    let family = family_of(file_type, &ext, &name, &path);
    Classification {
        file_type,
        family,
        subcategory: subcategory_of(file_type.unwrap_or(""), &ext, &name, &path, family),
        via_magic: false,
    }
}

fn subcategory_of(
    file_type: &str,
    ext: &str,
    name: &str,
    path: &str,
    family: &'static str,
) -> &'static str {
    match family {
        "documents" => document_subcategory(file_type, ext, name, path),
        "images" => image_subcategory(file_type, ext, name),
        "media" => media_subcategory(file_type, ext),
        "databases" => database_subcategory(file_type),
        "executables" => executable_subcategory(file_type, ext),
        "archives" => archive_subcategory(file_type),
        "system" => system_subcategory(file_type, ext),
        "forensics" => forensics_subcategory(file_type, ext),
        _ => "未识别",
    }
}

fn document_subcategory(file_type: &str, ext: &str, name: &str, path: &str) -> &'static str {
    if file_type == "EVTX"
        || ext == "log"
        || path.contains("winevt/logs/")
        || matches!(name, "wtmp" | "btmp" | "utmp" | "journal")
        || ext == "evtx"
    {
        return "日志文档";
    }
    if matches!(ext, "doc" | "docx") {
        return "Word 文档";
    }
    if matches!(ext, "xls" | "xlsx") {
        return "Excel 文档";
    }
    if matches!(ext, "ppt" | "pptx") {
        return "PPT 文档";
    }
    if file_type == "PDF" || ext == "pdf" {
        return "PDF 文档";
    }
    if matches!(ext, "pst" | "ost" | "mbox" | "eml" | "emlx") {
        return "邮件文档";
    }
    if file_type == "LNK" || ext == "lnk" {
        return "快捷方式";
    }
    if matches!(
        ext,
        "txt" | "md" | "csv" | "json" | "xml" | "html" | "htm" | "yaml" | "yml" | "toml" | "ini"
    ) {
        return "纯文本文档";
    }
    "其他文档"
}

fn image_subcategory(file_type: &str, ext: &str, name: &str) -> &'static str {
    if file_type == "THUMBCACHE" || name.starts_with("thumbcache_") || name == "thumbs.db" {
        return "缩略图缓存";
    }
    if file_type == "ICO" || ext == "ico" {
        return "图标";
    }
    "普通图片"
}

fn media_subcategory(file_type: &str, ext: &str) -> &'static str {
    if matches!(file_type, "MP4" | "MKV" | "AVI")
        || matches!(ext, "mp4" | "mkv" | "avi" | "webm" | "mov")
    {
        return "视频";
    }
    "音频"
}

fn database_subcategory(file_type: &str) -> &'static str {
    match file_type {
        "ESE" => "ESE 数据库",
        _ => "SQLite 数据库",
    }
}

fn executable_subcategory(file_type: &str, ext: &str) -> &'static str {
    match file_type {
        "ELF" => "Linux 可执行",
        _ if matches!(ext, "exe" | "dll" | "sys" | "com" | "scr") => "Windows 可执行",
        _ => "Windows 可执行",
    }
}

fn archive_subcategory(file_type: &str) -> &'static str {
    match file_type {
        "RAR" => "RAR 压缩包",
        "7Z" => "7Z 压缩包",
        "GZ" => "GZip 压缩包",
        _ => "ZIP 压缩包",
    }
}

fn system_subcategory(file_type: &str, ext: &str) -> &'static str {
    match file_type {
        "PF" => "预取文件",
        _ if ext == "pf" => "预取文件",
        _ => "注册表配置单元",
    }
}

fn forensics_subcategory(file_type: &str, ext: &str) -> &'static str {
    match file_type {
        "AFF" => "AFF 镜像",
        "VMDK" => "VMDK 磁盘",
        _ if matches!(ext, "raw" | "img" | "dd") => "原始镜像",
        _ => "E01 镜像",
    }
}

#[cfg(test)]
#[path = "../../tests/unit/analysis_service/file_classification.rs"]
mod tests;
