//! Data source analysis service.
//!
//! Provides system information status reporting and bounded file classification.

use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use transport::dto::analysis::{
    AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisProvenanceDto,
};
use transport::dto::{
    AnalysisClassifiedFileDto, AnalysisFileClassificationDto, AnalysisParseStatusDto,
    AnalysisSystemInfoDto, EvidenceCategoryDto, EvidenceClassificationSummaryDto,
    EvidenceClassificationTotalsDto, EvidenceSourceDto,
};

pub const DEFAULT_SAMPLE_SIZE: u32 = 1000;
pub const MAX_SAMPLE_SIZE: u32 = 5000;
pub const MAGIC_HEADER_LIMIT: usize = 8 * 1024;
pub const MAX_REGISTRY_ANALYSIS_BYTES: usize = 256 * 1024 * 1024;

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

const REGISTRY_SYSTEM_PARSER: &str = "registry.system";
const REGISTRY_SOFTWARE_PARSER: &str = "registry.software";
const EVTX_BOOT_SHUTDOWN_PARSER: &str = "evtx.boot_shutdown";
const MAGIC_CLASSIFICATION_PARSER: &str = "analysis.magic";
const MAX_EVIDENCE_SOURCES_PER_CATEGORY: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct EvidenceCategoryDef {
    pub category: &'static str,
    pub display_name: &'static str,
    pub evidence_kind: &'static str,
    pub parser: &'static str,
    pub artifact_families: &'static [&'static str],
    patterns: &'static [EvidencePathPattern],
}

#[derive(Debug, Clone, Copy)]
enum EvidencePathPattern {
    Suffix(&'static str),
    Contains(&'static str),
}

const EVIDENCE_CATEGORY_DEFS: &[EvidenceCategoryDef] = &[
    EvidenceCategoryDef {
        category: "SystemInformation",
        display_name: "系统信息",
        evidence_kind: "registry_hive",
        parser: "registry.system_info",
        artifact_families: &["Registry"],
        patterns: &[
            EvidencePathPattern::Suffix("/windows/system32/config/system"),
            EvidencePathPattern::Suffix("/windows/system32/config/software"),
            EvidencePathPattern::Suffix("/windows/system32/config/sam"),
            EvidencePathPattern::Suffix("/windows/system32/config/security"),
            EvidencePathPattern::Suffix("/ntuser.dat"),
            EvidencePathPattern::Suffix("/usrclass.dat"),
        ],
    },
    EvidenceCategoryDef {
        category: "EventLogs",
        display_name: "事件日志",
        evidence_kind: "event_log",
        parser: "evtx.boot_shutdown",
        artifact_families: &[],
        patterns: &[EvidencePathPattern::Suffix(".evtx")],
    },
    EvidenceCategoryDef {
        category: "ProgramExecution",
        display_name: "程序执行",
        evidence_kind: "execution_artifact",
        parser: "prefetch.amcache.shimcache",
        artifact_families: &["Prefetch"],
        patterns: &[
            EvidencePathPattern::Suffix(".pf"),
            EvidencePathPattern::Suffix("/windows/appcompat/programs/amcache.hve"),
            EvidencePathPattern::Contains("/windows/prefetch/"),
        ],
    },
    EvidenceCategoryDef {
        category: "UserActivity",
        display_name: "用户活动",
        evidence_kind: "user_activity",
        parser: "lnk.jumplist.shellbags",
        artifact_families: &["LNK", "JumpList"],
        patterns: &[
            EvidencePathPattern::Suffix(".lnk"),
            EvidencePathPattern::Suffix(".automaticdestinations-ms"),
            EvidencePathPattern::Suffix(".customdestinations-ms"),
            EvidencePathPattern::Contains("/recent/"),
            EvidencePathPattern::Contains("/shellbags"),
        ],
    },
    EvidenceCategoryDef {
        category: "RecycleBin",
        display_name: "回收站",
        evidence_kind: "recycle_bin",
        parser: "recycle_bin",
        artifact_families: &["RecycleBin"],
        patterns: &[
            EvidencePathPattern::Contains("/$recycle.bin/"),
            EvidencePathPattern::Contains("/recycler/"),
        ],
    },
    EvidenceCategoryDef {
        category: "Thumbnails",
        display_name: "缩略图缓存",
        evidence_kind: "thumbnail_cache",
        parser: "thumbcache",
        artifact_families: &["Thumbcache"],
        patterns: &[
            EvidencePathPattern::Contains("/thumbcache_"),
            EvidencePathPattern::Contains("/iconcache_"),
        ],
    },
    EvidenceCategoryDef {
        category: "ResourceUsage",
        display_name: "资源使用",
        evidence_kind: "resource_usage",
        parser: "sru",
        artifact_families: &["SRU"],
        patterns: &[EvidencePathPattern::Suffix(
            "/windows/system32/sru/srudb.dat",
        )],
    },
    EvidenceCategoryDef {
        category: "BrowserData",
        display_name: "浏览器数据",
        evidence_kind: "browser_sqlite",
        parser: "browser.sqlite",
        artifact_families: &[],
        patterns: &[
            EvidencePathPattern::Contains("/google/chrome/user data/"),
            EvidencePathPattern::Contains("/microsoft/edge/user data/"),
            EvidencePathPattern::Contains("/mozilla/firefox/profiles/"),
            EvidencePathPattern::Suffix("/history"),
            EvidencePathPattern::Suffix("/cookies"),
            EvidencePathPattern::Suffix("/places.sqlite"),
        ],
    },
    EvidenceCategoryDef {
        category: "FileTypeInventory",
        display_name: "文件类型清单",
        evidence_kind: "metadata_inventory",
        parser: "metadata.extension_path",
        artifact_families: &[],
        patterns: &[],
    },
];

#[derive(Debug, Clone)]
pub struct EvidenceCandidate {
    pub file_id: FileEntryId,
    pub data_source_id: String,
    pub path: String,
    pub size: u64,
    pub evidence_kind: String,
    pub parser: String,
    pub category: String,
}

#[derive(Default)]
struct SystemInfoExtraction {
    computer_name: Option<String>,
    os_version: Option<String>,
    build_number: Option<String>,
    install_date: Option<String>,
    registered_owner: Option<String>,
    organization: Option<String>,
    product_id: Option<String>,
    timezone: Option<String>,
    boot_history: Vec<AnalysisBootRecordDto>,
    field_provenance: Vec<AnalysisFieldProvenanceDto>,
}

impl SystemInfoExtraction {
    fn has_registry_field(&self) -> bool {
        self.computer_name.is_some()
            || self.os_version.is_some()
            || self.build_number.is_some()
            || self.install_date.is_some()
            || self.registered_owner.is_some()
            || self.organization.is_some()
            || self.product_id.is_some()
            || self.timezone.is_some()
    }
}

/// Extracts bounded system analysis facts from evidence-backed Registry hives.
///
/// EVTX boot/shutdown records are reported as EventLog/User32 candidates, not as
/// direct boot assertions. This service never manufactures host facts from file
/// presence alone.
pub fn extract_system_info_for_case(
    conn: &Connection,
    mut read_header_fn: impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
) -> AnalysisSystemInfoDto {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut warnings = Vec::new();
    let mut provenance = Vec::new();
    let mut extraction = SystemInfoExtraction::default();

    match find_system_info_candidates(conn) {
        Ok(candidates) => {
            let system_hive = candidates.system_hive.as_ref();
            let software_hive = candidates.software_hive.as_ref();
            let system_evtx = candidates.system_evtx.as_ref();

            inspect_registry_hive(
                system_hive,
                REGISTRY_SYSTEM_PARSER,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_registry_hive(
                software_hive,
                REGISTRY_SOFTWARE_PARSER,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_evtx_boot_source(
                system_evtx,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction.boot_history,
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

    let status = if extraction.has_registry_field() {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotParsed
    };
    AnalysisSystemInfoDto {
        computer_name: extraction.computer_name,
        os_version: extraction.os_version,
        build_number: extraction.build_number,
        install_date: extraction.install_date,
        registered_owner: extraction.registered_owner,
        organization: extraction.organization,
        product_id: extraction.product_id,
        network_adapters: Vec::new(),
        boot_history: extraction.boot_history,
        timezone: extraction.timezone,
        language: None,
        status,
        warnings,
        provenance,
        field_provenance: extraction.field_provenance,
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

#[derive(Default)]
struct SystemInfoCandidates {
    system_hive: Option<FileEntry>,
    software_hive: Option<FileEntry>,
    system_evtx: Option<FileEntry>,
}

fn find_system_info_candidates(conn: &Connection) -> Result<SystemInfoCandidates, String> {
    Ok(SystemInfoCandidates {
        system_hive: find_candidate_by_path_suffix(conn, "windows/system32/config/system")?,
        software_hive: find_candidate_by_path_suffix(conn, "windows/system32/config/software")?,
        system_evtx: find_candidate_by_path_suffix(
            conn,
            "windows/system32/winevt/logs/system.evtx",
        )?,
    })
}

fn find_candidate_by_path_suffix(
    conn: &Connection,
    suffix: &str,
) -> Result<Option<FileEntry>, String> {
    conn.query_row(
        "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                created_at, modified_at, accessed_at, changed_at, hash_sha256
         FROM file_entries
         WHERE entry_type = 'file' COLLATE NOCASE
           AND REPLACE(LOWER(path), '\\', '/') LIKE ?1
         ORDER BY LENGTH(path) ASC
         LIMIT 1",
        params![format!("%{suffix}")],
        row_to_file_entry_for_analysis,
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn row_to_file_entry_for_analysis(row: &rusqlite::Row) -> rusqlite::Result<FileEntry> {
    let entry_type_str: String = row.get(5)?;
    Ok(FileEntry {
        id: FileEntryId(row.get::<_, String>(0)?),
        parent_id: row.get::<_, Option<String>>(1)?.map(FileEntryId),
        data_source_id: domain::DataSourceId(row.get::<_, String>(2)?),
        path: row.get(3)?,
        name: row.get(4)?,
        entry_type: if entry_type_str.eq_ignore_ascii_case("directory") {
            EntryType::Directory
        } else {
            EntryType::File
        },
        size: row.get(6)?,
        ext: row.get(7)?,
        deleted: row.get::<_, i32>(8)? != 0,
        created_at: row
            .get::<_, Option<String>>(9)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        modified_at: row
            .get::<_, Option<String>>(10)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        accessed_at: row
            .get::<_, Option<String>>(11)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        changed_at: row
            .get::<_, Option<String>>(12)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        hash_sha256: row.get(13)?,
    })
}

pub fn classify_files_by_metadata(
    conn: &Connection,
    sample_size: u32,
) -> Result<Vec<AnalysisFileClassificationDto>, String> {
    let category_stats = metadata_category_stats(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                    created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE
             ORDER BY size DESC, path ASC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![sample_size as i64], row_to_file_entry_for_analysis)
        .map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for row in rows {
        files.push(row.map_err(|e| e.to_string())?);
    }

    let mut classifications = classify_files_by_extension_path(&files, sample_size);
    apply_metadata_category_stats(&mut classifications, category_stats);
    Ok(classifications)
}

pub fn evidence_category_defs() -> &'static [EvidenceCategoryDef] {
    EVIDENCE_CATEGORY_DEFS
}

pub fn discover_evidence_candidates(
    conn: &Connection,
) -> Result<HashMap<String, Vec<EvidenceCandidate>>, String> {
    let mut map: HashMap<String, Vec<EvidenceCandidate>> = EVIDENCE_CATEGORY_DEFS
        .iter()
        .map(|def| (def.category.to_string(), Vec::new()))
        .collect();

    let mut stmt = conn
        .prepare(
            "SELECT id, data_source_id, path, COALESCE(size, 0)
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let file_id: String = row.get(0).map_err(|e| e.to_string())?;
        let data_source_id: String = row.get(1).map_err(|e| e.to_string())?;
        let path: String = row.get(2).map_err(|e| e.to_string())?;
        let size: u64 = row.get(3).map_err(|e| e.to_string())?;
        let normalized = normalize_evidence_path(&path);

        for def in EVIDENCE_CATEGORY_DEFS {
            if def.patterns.is_empty() || !evidence_path_matches(&normalized, def.patterns) {
                continue;
            }
            map.entry(def.category.to_string())
                .or_default()
                .push(EvidenceCandidate {
                    file_id: FileEntryId(file_id.clone()),
                    data_source_id: data_source_id.clone(),
                    path: path.clone(),
                    size,
                    evidence_kind: def.evidence_kind.to_string(),
                    parser: def.parser.to_string(),
                    category: def.category.to_string(),
                });
        }
    }

    Ok(map)
}

pub fn evidence_candidates_for_categories(
    conn: &Connection,
    categories: &[&str],
) -> Result<Vec<EvidenceCandidate>, String> {
    let discovered = discover_evidence_candidates(conn)?;
    let mut candidates = Vec::new();
    for category in categories {
        if let Some(items) = discovered.get(*category) {
            candidates.extend(items.iter().cloned());
        }
    }
    Ok(candidates)
}

pub fn get_evidence_classification_summary(
    conn: &Connection,
) -> Result<EvidenceClassificationSummaryDto, String> {
    let generated_at = chrono::Utc::now().to_rfc3339();
    let candidates = discover_evidence_candidates(conn)?;
    let artifact_counts = artifact_counts_by_family(conn)?;
    let source_artifact_counts = artifact_counts_by_source(conn)?;
    let mut warnings = Vec::new();
    let mut categories = Vec::new();

    let file_type_totals = metadata_category_stats(conn)?;
    let file_type_count = file_type_totals
        .values()
        .map(|(count, _)| *count)
        .sum::<u64>();
    let file_type_size = file_type_totals
        .values()
        .map(|(_, total_size)| *total_size)
        .sum::<u64>();

    for def in EVIDENCE_CATEGORY_DEFS {
        if def.category == "FileTypeInventory" {
            let status = if file_type_count > 0 {
                AnalysisParseStatusDto::Parsed
            } else {
                AnalysisParseStatusDto::NotFound
            };
            categories.push(EvidenceCategoryDto {
                category: def.category.to_string(),
                display_name: def.display_name.to_string(),
                status: status.clone(),
                file_count: file_type_count,
                total_size: file_type_size,
                artifact_count: 0,
                confidence: if file_type_count > 0 { 0.75 } else { 0.0 },
                sources: Vec::new(),
                warnings: if file_type_count > 0 {
                    vec!["文件类型清单来自 metadata-only 分类；未读取文件正文。".to_string()]
                } else {
                    Vec::new()
                },
                provenance: vec![unknown_provenance(
                    def.parser,
                    &generated_at,
                    status,
                    vec!["metadata aggregate from file_entries".to_string()],
                )],
            });
            continue;
        }

        let items = candidates.get(def.category).cloned().unwrap_or_default();
        let file_count = items.len() as u64;
        let total_size = items.iter().map(|item| item.size).sum::<u64>();
        let artifact_count = def
            .artifact_families
            .iter()
            .filter_map(|family| artifact_counts.get(*family))
            .sum::<u64>();
        let parsed_source_count = items
            .iter()
            .filter(|item| source_artifact_counts.contains_key(&item.file_id.0))
            .count() as u64;
        let status = evidence_category_status(file_count, artifact_count, parsed_source_count);
        let sources = items
            .iter()
            .take(MAX_EVIDENCE_SOURCES_PER_CATEGORY)
            .map(|item| {
                let source_artifacts = source_artifact_counts
                    .get(&item.file_id.0)
                    .copied()
                    .unwrap_or(0);
                EvidenceSourceDto {
                    file_id: item.file_id.0.clone(),
                    path: item.path.clone(),
                    size: item.size,
                    evidence_kind: item.evidence_kind.clone(),
                    parser: item.parser.clone(),
                    status: if source_artifacts > 0 {
                        AnalysisParseStatusDto::Parsed
                    } else {
                        AnalysisParseStatusDto::CandidateFound
                    },
                    artifact_count: source_artifacts,
                    warnings: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        let mut category_warnings = Vec::new();
        if file_count > sources.len() as u64 {
            category_warnings.push(format!(
                "仅展示前 {} 个候选来源；该证据族共 {} 个候选文件。",
                sources.len(),
                file_count
            ));
        }
        if file_count > 0 && artifact_count == 0 {
            category_warnings.push(
                "已发现候选文件；尚未运行证据分类解析或当前 parser 不支持该证据族。".to_string(),
            );
        }
        let provenance = sources
            .iter()
            .map(|source| AnalysisProvenanceDto {
                data_source_id: String::new(),
                artifact_path: source.path.clone(),
                parser: source.parser.clone(),
                parsed_at: generated_at.clone(),
                status: source.status.clone(),
                warnings: source.warnings.clone(),
            })
            .collect::<Vec<_>>();

        categories.push(EvidenceCategoryDto {
            category: def.category.to_string(),
            display_name: def.display_name.to_string(),
            status,
            file_count,
            total_size,
            artifact_count,
            confidence: evidence_confidence(file_count, artifact_count),
            sources,
            warnings: category_warnings,
            provenance,
        });
    }

    let totals = EvidenceClassificationTotalsDto {
        category_count: categories.len() as u64,
        candidate_file_count: categories
            .iter()
            .filter(|category| category.category != "FileTypeInventory")
            .map(|category| category.file_count)
            .sum(),
        total_size: categories
            .iter()
            .filter(|category| category.category != "FileTypeInventory")
            .map(|category| category.total_size)
            .sum(),
        artifact_count: artifact_counts.values().sum(),
    };
    let status = if categories
        .iter()
        .any(|category| category.status == AnalysisParseStatusDto::Failed)
    {
        AnalysisParseStatusDto::Partial
    } else if categories
        .iter()
        .any(|category| category.status == AnalysisParseStatusDto::Parsed)
    {
        AnalysisParseStatusDto::Parsed
    } else if categories
        .iter()
        .any(|category| category.status == AnalysisParseStatusDto::CandidateFound)
    {
        AnalysisParseStatusDto::CandidateFound
    } else {
        AnalysisParseStatusDto::NotFound
    };

    if totals.candidate_file_count == 0 {
        warnings
            .push("未发现 Windows 证据族候选文件；请确认数据源已导入且文件树可用。".to_string());
    }

    Ok(EvidenceClassificationSummaryDto {
        status,
        categories,
        totals,
        generated_at,
        warnings,
    })
}

fn metadata_category_stats(conn: &Connection) -> Result<HashMap<String, (u64, u64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT path, COALESCE(size, 0)
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    let mut stats: HashMap<String, (u64, u64)> = HashMap::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let path: String = row.get(0).map_err(|e| e.to_string())?;
        let size: u64 = row.get(1).map_err(|e| e.to_string())?;
        let category = detect_file_type(&path, None)
            .map(|(_, category, _)| category)
            .unwrap_or("Other");
        let entry = stats.entry(category.to_string()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += size;
    }
    Ok(stats)
}

fn artifact_counts_by_family(conn: &Connection) -> Result<HashMap<String, u64>, String> {
    let mut stmt = conn
        .prepare("SELECT artifact_type, COUNT(*) FROM artifacts GROUP BY artifact_type")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(|e| e.to_string())?;
    let mut counts = HashMap::new();
    for row in rows {
        let (family, count) = row.map_err(|e| e.to_string())?;
        counts.insert(family, count);
    }
    Ok(counts)
}

fn artifact_counts_by_source(conn: &Connection) -> Result<HashMap<String, u64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT source_object_id, COUNT(*)
             FROM artifacts
             WHERE source_object_id IS NOT NULL
             GROUP BY source_object_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(|e| e.to_string())?;
    let mut counts = HashMap::new();
    for row in rows {
        let (source_id, count) = row.map_err(|e| e.to_string())?;
        counts.insert(source_id, count);
    }
    Ok(counts)
}

fn normalize_evidence_path(path: &str) -> String {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

fn evidence_path_matches(path: &str, patterns: &[EvidencePathPattern]) -> bool {
    patterns.iter().any(|pattern| match pattern {
        EvidencePathPattern::Suffix(suffix) => path.ends_with(suffix),
        EvidencePathPattern::Contains(needle) => path.contains(needle),
    })
}

fn evidence_category_status(
    file_count: u64,
    artifact_count: u64,
    parsed_source_count: u64,
) -> AnalysisParseStatusDto {
    if file_count == 0 {
        AnalysisParseStatusDto::NotFound
    } else if artifact_count == 0 {
        AnalysisParseStatusDto::CandidateFound
    } else if parsed_source_count > 0 && parsed_source_count < file_count {
        AnalysisParseStatusDto::Partial
    } else {
        AnalysisParseStatusDto::Parsed
    }
}

fn evidence_confidence(file_count: u64, artifact_count: u64) -> f32 {
    if artifact_count > 0 {
        0.95
    } else if file_count > 0 {
        0.65
    } else {
        0.0
    }
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

    classifications.sort_by(|a, b| b.total_size.cmp(&a.total_size));
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
    result.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    result
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
    extraction: &mut SystemInfoExtraction,
) {
    match entry {
        Some(entry) => {
            let read_result = read_header_fn(&entry.id, MAX_REGISTRY_ANALYSIS_BYTES);
            let mut parser_warnings = Vec::new();
            let mut parsed_any = false;
            match read_result {
                Ok(bytes) if bytes.starts_with(b"regf") => match parser {
                    REGISTRY_SYSTEM_PARSER => {
                        match artifacts_windows::extract_system_hive_fields(&bytes, &entry.path) {
                            Ok(info) => {
                                parsed_any |= assign_registry_field(
                                    "computerName",
                                    info.computer_name,
                                    &mut extraction.computer_name,
                                    &mut extraction.field_provenance,
                                );
                                parsed_any |= assign_registry_field(
                                    "timezone",
                                    info.timezone,
                                    &mut extraction.timezone,
                                    &mut extraction.field_provenance,
                                );
                                parser_warnings.extend(info.warnings);
                            }
                            Err(err) => {
                                parser_warnings.push(format!("{} 解析失败: {}", entry.path, err))
                            }
                        }
                    }
                    REGISTRY_SOFTWARE_PARSER => {
                        match artifacts_windows::extract_software_hive_fields(&bytes, &entry.path) {
                            Ok(info) => {
                                parsed_any |= assign_registry_field(
                                    "osVersion",
                                    info.product_name,
                                    &mut extraction.os_version,
                                    &mut extraction.field_provenance,
                                );
                                parsed_any |= assign_registry_field(
                                    "buildNumber",
                                    info.current_build,
                                    &mut extraction.build_number,
                                    &mut extraction.field_provenance,
                                );
                                parsed_any |= assign_registry_field(
                                    "installDate",
                                    info.install_date,
                                    &mut extraction.install_date,
                                    &mut extraction.field_provenance,
                                );
                                parsed_any |= assign_registry_field(
                                    "registeredOwner",
                                    info.registered_owner,
                                    &mut extraction.registered_owner,
                                    &mut extraction.field_provenance,
                                );
                                parsed_any |= assign_registry_field(
                                    "organization",
                                    info.registered_organization,
                                    &mut extraction.organization,
                                    &mut extraction.field_provenance,
                                );
                                parsed_any |= assign_registry_field(
                                    "productId",
                                    info.product_id,
                                    &mut extraction.product_id,
                                    &mut extraction.field_provenance,
                                );
                                if let Some(display_version) = info.display_version {
                                    let value = display_version.value.clone();
                                    extraction.field_provenance.push(registry_field_provenance(
                                        "osDisplayVersion",
                                        display_version,
                                    ));
                                    match &mut extraction.os_version {
                                        Some(os) if !os.contains(&value) => {
                                            os.push(' ');
                                            os.push_str(&value);
                                        }
                                        None => extraction.os_version = Some(value),
                                        _ => {}
                                    }
                                    parsed_any = true;
                                }
                                if let Some(current_version) = info.current_version {
                                    extraction.field_provenance.push(registry_field_provenance(
                                        "osCurrentVersion",
                                        current_version,
                                    ));
                                    parsed_any = true;
                                }
                                parser_warnings.extend(info.warnings);
                            }
                            Err(err) => {
                                parser_warnings.push(format!("{} 解析失败: {}", entry.path, err))
                            }
                        }
                    }
                    _ => parser_warnings.push(format!("{} parser unsupported", parser)),
                },
                Ok(bytes) if bytes.len() >= MAX_REGISTRY_ANALYSIS_BYTES => {
                    parser_warnings.push(format!(
                        "{} 达到 Registry parser 读取上限 {} bytes，且未取得有效 regf 头。",
                        entry.path, MAX_REGISTRY_ANALYSIS_BYTES
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
                if parsed_any {
                    AnalysisParseStatusDto::Parsed
                } else {
                    AnalysisParseStatusDto::NotParsed
                },
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

fn assign_registry_field(
    field: &str,
    parsed: Option<artifacts_windows::ParsedRegistryField>,
    target: &mut Option<String>,
    field_provenance: &mut Vec<AnalysisFieldProvenanceDto>,
) -> bool {
    let Some(parsed) = parsed else {
        return false;
    };
    *target = Some(parsed.value.clone());
    field_provenance.push(registry_field_provenance(field, parsed));
    true
}

fn registry_field_provenance(
    field: &str,
    parsed: artifacts_windows::ParsedRegistryField,
) -> AnalysisFieldProvenanceDto {
    AnalysisFieldProvenanceDto {
        field: field.to_string(),
        value_name: parsed.value_name,
        key_path: parsed.key_path,
        hive_path: parsed.hive_path,
        parser: parsed.parser,
    }
}

fn inspect_evtx_boot_source(
    entry: Option<&FileEntry>,
    parsed_at: &str,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
    boot_history: &mut Vec<AnalysisBootRecordDto>,
) {
    match entry {
        Some(entry) => {
            let mut parser_warnings = Vec::new();
            let mut parsed_any = false;
            match read_header_fn(&entry.id, artifacts_windows::MAX_EVTX_ANALYSIS_BYTES) {
                Ok(bytes) => {
                    let extraction =
                        artifacts_windows::extract_boot_shutdown_events(&bytes, &entry.path);
                    parser_warnings.extend(extraction.warnings);
                    if !extraction.events.is_empty() {
                        parsed_any = true;
                    }
                    let event_provenance = entry_provenance(
                        entry,
                        EVTX_BOOT_SHUTDOWN_PARSER,
                        parsed_at,
                        AnalysisParseStatusDto::Parsed,
                        Vec::new(),
                    );
                    boot_history.extend(extraction.events.into_iter().map(|event| {
                        AnalysisBootRecordDto {
                            timestamp: event.timestamp,
                            boot_type: event.kind.as_str().to_string(),
                            source: event.source_path,
                            event_id: Some(event.event_id),
                            record_id: event.record_id,
                            note: Some(event.note),
                            provenance: event_provenance.clone(),
                        }
                    }));
                }
                Err(err) => parser_warnings.push(format!("{} 读取失败: {}", entry.path, err)),
            }
            provenance.push(entry_provenance(
                entry,
                EVTX_BOOT_SHUTDOWN_PARSER,
                parsed_at,
                if parsed_any {
                    AnalysisParseStatusDto::Parsed
                } else {
                    AnalysisParseStatusDto::NotParsed
                },
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

fn metadata_classification_provenance(entry: &FileEntry, parsed_at: &str) -> AnalysisProvenanceDto {
    AnalysisProvenanceDto {
        data_source_id: entry.data_source_id.0.clone(),
        artifact_path: entry.path.clone(),
        parser: "metadata.extension_path".to_string(),
        parsed_at: parsed_at.to_string(),
        status: AnalysisParseStatusDto::Parsed,
        warnings: vec![
            "metadata-only classification; file content/header was not read".to_string(),
        ],
    }
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
        AnalysisParseStatusDto::Partial => {
            summary.push_str("- **状态**: 部分解析\n");
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
        AnalysisParseStatusDto::CandidateFound => {
            summary.push_str("- **状态**: 已发现候选\n");
        }
        AnalysisParseStatusDto::NotFound => {
            summary.push_str("- **状态**: 未发现\n");
        }
        AnalysisParseStatusDto::Failed => {
            summary.push_str("- **状态**: 解析失败\n");
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
                cat.file_count,
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
        AnalysisParseStatusDto::Partial => "部分解析",
        AnalysisParseStatusDto::NotParsed => "未解析",
        AnalysisParseStatusDto::Unavailable => "不可用",
        AnalysisParseStatusDto::CandidateFound => "已发现候选",
        AnalysisParseStatusDto::NotFound => "未发现",
        AnalysisParseStatusDto::Failed => "解析失败",
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
    use testing::{builders::registry, fixtures};

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
            provenance: domain::DataSourceProvenance::unknown(),
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
    fn malformed_registry_hive_presence_keeps_system_fields_empty_with_provenance() {
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
                    .any(|warning| warning.contains("registry hive shorter than base block"))
        }));
    }

    #[test]
    fn registry_hive_fields_are_parsed_with_field_provenance() {
        let (conn, _tmp, ds_id) = setup_case_db();
        let system_hive = std::fs::read(fixtures::tiny_registry_system_hive())
            .expect("read tiny SYSTEM registry fixture");
        let software_hive = std::fs::read(fixtures::tiny_registry_software_hive())
            .expect("read tiny SOFTWARE registry fixture");
        FileRepo::new(&conn)
            .insert_batch(&[
                file_with_ds(
                    "system",
                    &ds_id,
                    "Windows/System32/config/SYSTEM",
                    system_hive.len() as u64,
                ),
                file_with_ds(
                    "software",
                    &ds_id,
                    "Windows/System32/config/SOFTWARE",
                    software_hive.len() as u64,
                ),
            ])
            .unwrap();

        let info =
            extract_system_info_for_case(&conn, |file_id, max_bytes| match file_id.0.as_str() {
                "system" => Ok(system_hive[..system_hive.len().min(max_bytes)].to_vec()),
                "software" => Ok(software_hive[..software_hive.len().min(max_bytes)].to_vec()),
                other => Err(format!("unexpected file id {other}")),
            });

        assert_eq!(info.status, AnalysisParseStatusDto::Parsed);
        assert_eq!(
            info.computer_name.as_deref(),
            Some(registry::SYSTEM_COMPUTER_NAME)
        );
        assert_eq!(
            info.os_version.as_deref(),
            Some("Forensics Fixture OS 24H2")
        );
        assert_eq!(
            info.build_number.as_deref(),
            Some(registry::SOFTWARE_CURRENT_BUILD)
        );
        assert_eq!(
            info.registered_owner.as_deref(),
            Some(registry::SOFTWARE_REGISTERED_OWNER)
        );
        assert_eq!(
            info.product_id.as_deref(),
            Some(registry::SOFTWARE_PRODUCT_ID)
        );
        assert_eq!(info.timezone.as_deref(), Some(registry::SYSTEM_TIMEZONE));
        assert!(info
            .install_date
            .as_deref()
            .is_some_and(|value| value.starts_with("2023-")));
        assert!(info.field_provenance.iter().any(|field| {
            field.field == "computerName"
                && field.value_name == "ComputerName"
                && field.key_path == "ControlSet001\\Control\\ComputerName\\ComputerName"
                && field.hive_path == "Windows/System32/config/SYSTEM"
                && field.parser == REGISTRY_SYSTEM_PARSER
        }));
        assert!(info.field_provenance.iter().any(|field| {
            field.field == "osVersion"
                && field.value_name == "ProductName"
                && field.key_path == "Microsoft\\Windows NT\\CurrentVersion"
                && field.hive_path == "Windows/System32/config/SOFTWARE"
                && field.parser == REGISTRY_SOFTWARE_PARSER
        }));
        assert!(info.provenance.iter().any(|item| {
            item.parser == REGISTRY_SYSTEM_PARSER
                && item.status == AnalysisParseStatusDto::Parsed
                && item.data_source_id == ds_id.0
        }));
        assert!(info.provenance.iter().any(|item| {
            item.parser == REGISTRY_SOFTWARE_PARSER
                && item.status == AnalysisParseStatusDto::Parsed
                && item.data_source_id == ds_id.0
        }));

        let summary = generate_analysis_summary(&info, &[]);
        assert!(summary.contains(registry::SYSTEM_COMPUTER_NAME));
        assert!(summary.contains("Forensics Fixture OS 24H2"));
        assert!(!summary.contains("FORENSICS-PC"));
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
    fn malformed_evtx_source_is_not_parsed_and_generates_no_boot_records() {
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
        assert!(info
            .warnings
            .iter()
            .all(|warning| !warning.contains("EVTX parser initialization failed")));
        assert!(info.provenance.iter().any(|item| {
            item.parser == EVTX_BOOT_SHUTDOWN_PARSER
                && item.status == AnalysisParseStatusDto::NotParsed
                && item
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("EVTX parser initialization failed"))
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

    #[test]
    fn evidence_discovery_maps_registry_evtx_prefetch_lnk_paths_to_categories() {
        let (conn, _tmp, ds_id) = setup_case_db();
        FileRepo::new(&conn)
            .insert_batch(&[
                file_with_ds("system", &ds_id, "Windows/System32/config/SYSTEM", 10),
                file_with_ds(
                    "evtx",
                    &ds_id,
                    "Windows/System32/winevt/Logs/System.evtx",
                    20,
                ),
                file_with_ds("pf", &ds_id, "Windows/Prefetch/CMD.EXE-12345678.pf", 30),
                file_with_ds(
                    "lnk",
                    &ds_id,
                    "Users/alice/AppData/Roaming/Microsoft/Windows/Recent/app.lnk",
                    40,
                ),
            ])
            .unwrap();

        let candidates = discover_evidence_candidates(&conn).unwrap();
        assert_eq!(candidates["SystemInformation"].len(), 1);
        assert_eq!(candidates["EventLogs"].len(), 1);
        assert_eq!(candidates["ProgramExecution"].len(), 1);
        assert_eq!(candidates["UserActivity"].len(), 1);
    }

    #[test]
    fn evidence_summary_reports_candidate_found_without_parser_run() {
        let (conn, _tmp, ds_id) = setup_case_db();
        FileRepo::new(&conn)
            .insert_batch(&[file_with_ds(
                "system",
                &ds_id,
                "Windows/System32/config/SYSTEM",
                10,
            )])
            .unwrap();

        let summary = get_evidence_classification_summary(&conn).unwrap();
        let system = summary
            .categories
            .iter()
            .find(|category| category.category == "SystemInformation")
            .unwrap();
        assert_eq!(system.status, AnalysisParseStatusDto::CandidateFound);
        assert_eq!(system.file_count, 1);
        assert_eq!(system.artifact_count, 0);
        assert_eq!(system.sources[0].path, "Windows/System32/config/SYSTEM");
    }

    #[test]
    fn evidence_summary_reports_parsed_when_artifacts_exist() {
        let (conn, _tmp, ds_id) = setup_case_db();
        FileRepo::new(&conn)
            .insert_batch(&[file_with_ds(
                "pf",
                &ds_id,
                "Windows/Prefetch/CMD.EXE-12345678.pf",
                10,
            )])
            .unwrap();
        conn.execute(
            "INSERT INTO artifacts
             (id, case_id, data_source_id, artifact_type, source_object_id, title, summary, attrs, created_at)
             VALUES ('artifact-1', 'case-analysis', ?1, 'Prefetch', 'pf', 'Prefetch: CMD.EXE', 'summary', '{}', '2026-01-01T00:00:00Z')",
            [&ds_id.0],
        )
        .unwrap();

        let summary = get_evidence_classification_summary(&conn).unwrap();
        let program = summary
            .categories
            .iter()
            .find(|category| category.category == "ProgramExecution")
            .unwrap();
        assert_eq!(program.status, AnalysisParseStatusDto::Parsed);
        assert_eq!(program.artifact_count, 1);
        assert_eq!(program.sources[0].status, AnalysisParseStatusDto::Parsed);
    }
}
