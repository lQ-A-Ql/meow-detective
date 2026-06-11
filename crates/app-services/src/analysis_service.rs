//! Data source analysis service.
//!
//! Provides system information status reporting and bounded file classification.

use chrono::{DateTime, TimeZone, Utc};
use domain::{
    Artifact, ArtifactId, EntryType, FileEntry, FileEntryId, TimelineEvent, TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use transport::dto::analysis::{
    AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisProvenanceDto,
};
use transport::dto::{
    AnalysisClassifiedFileDto, AnalysisExtractionRunDto, AnalysisFileClassificationDto,
    AnalysisParseStatusDto, AnalysisSystemInfoDto, BrowserDownloadDto, BrowserHistorySummaryDto,
    BrowserVisitDto, EmailExtractionSummaryDto, EmailMessageDto, EvidenceCategoryDto,
    EvidenceClassificationSummaryDto, EvidenceClassificationTotalsDto, EvidenceSourceDto,
    RegistryExtractionSummaryDto, RegistryValueDto,
};
use uuid::Uuid;

pub const DEFAULT_SAMPLE_SIZE: u32 = 1000;
pub const MAX_SAMPLE_SIZE: u32 = 5000;
pub const MAGIC_HEADER_LIMIT: usize = 8 * 1024;
pub const MAX_REGISTRY_ANALYSIS_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_ANALYSIS_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const ANALYSIS_EXTRACTOR_VERSION: &str = "1.0.0";

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
        display_name: "System information",
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
        category: "Registry",
        display_name: "注册表",
        evidence_kind: "registry_hive",
        parser: "registry.hive",
        artifact_families: &["Registry"],
        patterns: &[
            EvidencePathPattern::Suffix("/windows/system32/config/system"),
            EvidencePathPattern::Suffix("/windows/system32/config/software"),
            EvidencePathPattern::Suffix("/windows/system32/config/sam"),
            EvidencePathPattern::Suffix("/windows/system32/config/security"),
            EvidencePathPattern::Suffix("/ntuser.dat"),
            EvidencePathPattern::Suffix("/usrclass.dat"),
            EvidencePathPattern::Contains("/registry/"),
            EvidencePathPattern::Suffix(".reg"),
            EvidencePathPattern::Suffix(".hive"),
        ],
    },
    EvidenceCategoryDef {
        category: "EventLogs",
        display_name: "Event logs",
        evidence_kind: "event_log",
        parser: "evtx.boot_shutdown",
        artifact_families: &[],
        patterns: &[EvidencePathPattern::Suffix(".evtx")],
    },
    EvidenceCategoryDef {
        category: "ProgramExecution",
        display_name: "Program execution",
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
        display_name: "User activity",
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
        display_name: "Recycle bin",
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
        display_name: "Thumbnail cache",
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
        display_name: "Resource usage",
        evidence_kind: "resource_usage",
        parser: "sru",
        artifact_families: &["SRU"],
        patterns: &[EvidencePathPattern::Suffix(
            "/windows/system32/sru/srudb.dat",
        )],
    },
    EvidenceCategoryDef {
        category: "BrowserData",
        display_name: "Browser data",
        evidence_kind: "browser_sqlite",
        parser: "browser.sqlite",
        artifact_families: &[],
        patterns: &[
            EvidencePathPattern::Contains("/google/chrome/user data/"),
            EvidencePathPattern::Contains("/microsoft/edge/user data/"),
            EvidencePathPattern::Contains("/mozilla/firefox/profiles/"),
            EvidencePathPattern::Suffix("/history"),
            EvidencePathPattern::Suffix("/archived history"),
            EvidencePathPattern::Suffix("/cookies"),
            EvidencePathPattern::Suffix("/places.sqlite"),
        ],
    },
    EvidenceCategoryDef {
        category: "BrowserHistory",
        display_name: "浏览器历史",
        evidence_kind: "browser_history",
        parser: "browser.history",
        artifact_families: &["BrowserHistory", "BrowserDownload"],
        patterns: &[
            EvidencePathPattern::Contains("/google/chrome/user data/default/history"),
            EvidencePathPattern::Contains("/google/chrome/user data/profile"),
            EvidencePathPattern::Contains("/microsoft/edge/user data/default/history"),
            EvidencePathPattern::Contains("/microsoft/edge/user data/profile"),
            EvidencePathPattern::Contains("/mozilla/firefox/profiles/"),
            EvidencePathPattern::Suffix("/history"),
            EvidencePathPattern::Suffix("/places.sqlite"),
        ],
    },
    EvidenceCategoryDef {
        category: "Email",
        display_name: "电子邮件",
        evidence_kind: "email_eml_emlx",
        parser: "email.eml_emlx",
        artifact_families: &["EmailMessage"],
        patterns: &[
            EvidencePathPattern::Suffix(".eml"),
            EvidencePathPattern::Suffix(".emlx"),
        ],
    },
    EvidenceCategoryDef {
        category: "FileTypeInventory",
        display_name: "File type inventory",
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
                hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
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
        hidden: row.get::<_, i32>(9)? != 0,
        system: row.get::<_, i32>(10)? != 0,
        created_at: row
            .get::<_, Option<String>>(11)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        modified_at: row
            .get::<_, Option<String>>(12)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        accessed_at: row
            .get::<_, Option<String>>(13)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        changed_at: row
            .get::<_, Option<String>>(14)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        hash_sha256: row.get(15)?,
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
                    hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
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
            if def.category == "BrowserHistory" && !is_browser_history_path(&normalized) {
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

pub fn run_analysis_extraction(
    conn: &Connection,
    case_id: &str,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, String>,
) -> Result<AnalysisExtractionRunDto, String> {
    let generated_at = Utc::now().to_rfc3339();
    let selected = if categories.is_empty() {
        vec!["Registry", "BrowserHistory", "Email"]
    } else {
        categories.to_vec()
    };
    let candidates = evidence_candidates_for_categories(conn, &selected)?;
    let mut artifacts = Vec::new();
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut scanned_count = 0u64;

    for candidate in candidates {
        if !matches!(
            candidate.category.as_str(),
            "Registry" | "BrowserHistory" | "Email"
        ) {
            continue;
        }
        if already_has_v1_artifacts(conn, &candidate)? {
            continue;
        }

        let mut reader = match file_reader(&candidate.file_id) {
            Ok(reader) => reader,
            Err(err) => {
                warnings.push(format!("{} read failed: {}", candidate.path, err));
                continue;
            }
        };
        let mut bytes = Vec::new();
        if let Err(err) = reader
            .by_ref()
            .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
            .read_to_end(&mut bytes)
        {
            warnings.push(format!("{} read failed: {}", candidate.path, err));
            continue;
        }

        scanned_count += 1;
        let outcome = match candidate.category.as_str() {
            "Registry" => extract_registry_candidate(&candidate, &bytes),
            "BrowserHistory" => extract_browser_candidate(&candidate, &bytes),
            "Email" => extract_email_candidate(&candidate, &bytes),
            _ => ExtractionOutcome::default(),
        };
        warnings.extend(outcome.warnings);
        artifacts.extend(outcome.artifacts);
        events.extend(outcome.timeline_events);
    }

    if !artifacts.is_empty() {
        let by_source = artifacts_by_data_source(artifacts);
        let repo = ArtifactRepo::new(conn);
        for (data_source_id, group) in by_source {
            repo.insert_batch(&group, case_id, &data_source_id)
                .map_err(|e| e.to_string())?;
        }
    }
    if !events.is_empty() {
        TimelineRepo::new(conn)
            .insert_batch_with_case(&events, case_id)
            .map_err(|e| e.to_string())?;
    }

    let artifact_count = count_analysis_artifacts(conn)?;
    Ok(AnalysisExtractionRunDto {
        status: if scanned_count == 0 {
            AnalysisParseStatusDto::NotFound
        } else if warnings.is_empty() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        },
        scanned_count,
        artifact_count,
        timeline_event_count: events.len() as u64,
        generated_at,
        warnings,
    })
}

pub fn get_registry_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, String> {
    let total = count_artifacts_by_type(conn, "RegistryValue")?;
    let rows = query_artifact_rows(conn, &["RegistryValue"], offset, limit)?;
    let values = rows
        .into_iter()
        .map(|row| RegistryValueDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            hive_path: string_attr(&row.attrs, "hivePath"),
            key_path: string_attr(&row.attrs, "keyPath"),
            value_name: string_attr(&row.attrs, "valueName"),
            value_type: string_attr(&row.attrs, "valueType"),
            data: string_attr(&row.attrs, "data"),
            parser: row
                .extractor_id
                .unwrap_or_else(|| "registry.v1".to_string()),
            created_at: row.created_at,
        })
        .collect::<Vec<_>>();
    Ok(RegistryExtractionSummaryDto {
        status: status_from_total(total),
        total,
        values,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_browser_history_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<BrowserHistorySummaryDto, String> {
    let visit_total = count_artifacts_by_type(conn, "BrowserHistory")?;
    let download_total = count_artifacts_by_type(conn, "BrowserDownload")?;
    let visit_rows = query_artifact_rows(conn, &["BrowserHistory"], offset, limit)?;
    let download_rows = query_artifact_rows(conn, &["BrowserDownload"], offset, limit)?;
    let visits = visit_rows
        .into_iter()
        .map(|row| BrowserVisitDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            title: string_attr(&row.attrs, "title"),
            visit_time: optional_string_attr(&row.attrs, "visitTime"),
            visit_count: u64_attr(&row.attrs, "visitCount"),
        })
        .collect::<Vec<_>>();
    let downloads = download_rows
        .into_iter()
        .map(|row| BrowserDownloadDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            target_path: string_attr(&row.attrs, "targetPath"),
            start_time: optional_string_attr(&row.attrs, "startTime"),
            total_bytes: u64_attr(&row.attrs, "totalBytes"),
        })
        .collect::<Vec<_>>();
    Ok(BrowserHistorySummaryDto {
        status: status_from_total(visit_total + download_total),
        visit_total,
        download_total,
        visits,
        downloads,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_email_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<EmailExtractionSummaryDto, String> {
    let total = count_artifacts_by_type(conn, "EmailMessage")?;
    let rows = query_artifact_rows(conn, &["EmailMessage"], offset, limit)?;
    let messages = rows
        .into_iter()
        .map(|row| EmailMessageDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            sent_at: optional_string_attr(&row.attrs, "sentAt"),
            from: string_attr(&row.attrs, "from"),
            to: string_vec_attr(&row.attrs, "to"),
            cc: string_vec_attr(&row.attrs, "cc"),
            bcc: string_vec_attr(&row.attrs, "bcc"),
            subject: string_attr(&row.attrs, "subject"),
            message_id: string_attr(&row.attrs, "messageId"),
            attachments: string_vec_attr(&row.attrs, "attachments"),
            body_preview: string_attr(&row.attrs, "bodyPreview"),
        })
        .collect::<Vec<_>>();
    Ok(EmailExtractionSummaryDto {
        status: status_from_total(total),
        total,
        messages,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

#[derive(Default)]
struct ExtractionOutcome {
    artifacts: Vec<Artifact>,
    timeline_events: Vec<TimelineEvent>,
    warnings: Vec<String>,
}

struct AnalysisArtifactRow {
    id: String,
    source_object_id: Option<String>,
    extractor_id: Option<String>,
    created_at: String,
    attrs: BTreeMap<String, Value>,
}

fn already_has_v1_artifacts(
    conn: &Connection,
    candidate: &EvidenceCandidate,
) -> Result<bool, String> {
    let families = match candidate.category.as_str() {
        "Registry" => &["RegistryValue"][..],
        "BrowserHistory" => &["BrowserHistory", "BrowserDownload"][..],
        "Email" => &["EmailMessage"][..],
        _ => &[][..],
    };
    if families.is_empty() {
        return Ok(false);
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{}", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT COUNT(*) FROM artifacts WHERE source_object_id = ?1 AND artifact_type IN ({})",
        placeholders
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(candidate.file_id.0.clone())];
    for family in families {
        params_values.push(Box::new((*family).to_string()));
    }
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    let count: i64 = conn
        .query_row(&sql, params_refs.as_slice(), |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

fn extract_registry_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    if !bytes.starts_with(b"regf") {
        outcome
            .warnings
            .push(format!("{} is not a regf registry hive", candidate.path));
        return outcome;
    }

    let normalized = normalize_evidence_path(&candidate.path);
    if normalized.ends_with("/windows/system32/config/system") {
        match artifacts_windows::extract_system_hive_fields(bytes, &candidate.path) {
            Ok(info) => {
                outcome.artifacts.extend(registry_field_artifacts(
                    candidate,
                    vec![
                        ("computerName", info.computer_name),
                        ("timezone", info.timezone),
                    ],
                ));
                outcome.warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }
    } else if normalized.ends_with("/windows/system32/config/software") {
        match artifacts_windows::extract_software_hive_fields(bytes, &candidate.path) {
            Ok(info) => {
                outcome.artifacts.extend(registry_field_artifacts(
                    candidate,
                    vec![
                        ("productName", info.product_name),
                        ("currentBuild", info.current_build),
                        ("currentVersion", info.current_version),
                        ("displayVersion", info.display_version),
                        ("installDate", info.install_date),
                        ("registeredOwner", info.registered_owner),
                        ("registeredOrganization", info.registered_organization),
                        ("productId", info.product_id),
                    ],
                ));
                outcome.warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }
    } else {
        outcome.warnings.push(format!(
            "{} found as registry hive; v1 extracts key values only from SYSTEM/SOFTWARE",
            candidate.path
        ));
    }
    outcome
}

fn registry_field_artifacts(
    candidate: &EvidenceCandidate,
    fields: Vec<(&str, Option<artifacts_windows::ParsedRegistryField>)>,
) -> Vec<Artifact> {
    fields
        .into_iter()
        .filter_map(|(field_name, parsed)| parsed.map(|parsed| (field_name, parsed)))
        .map(|(field_name, parsed)| {
            let mut attrs = base_attrs(candidate);
            attrs.insert("field".to_string(), Value::String(field_name.to_string()));
            attrs.insert(
                "hivePath".to_string(),
                Value::String(parsed.hive_path.clone()),
            );
            attrs.insert(
                "keyPath".to_string(),
                Value::String(parsed.key_path.clone()),
            );
            attrs.insert(
                "valueName".to_string(),
                Value::String(parsed.value_name.clone()),
            );
            attrs.insert("valueType".to_string(), Value::String("string".to_string()));
            attrs.insert("data".to_string(), Value::String(parsed.value.clone()));
            attrs.insert("parser".to_string(), Value::String(parsed.parser.clone()));
            make_artifact(
                "RegistryValue",
                format!("Registry {}: {}", field_name, parsed.value),
                format!(
                    "{}\\{} = {}",
                    parsed.key_path, parsed.value_name, parsed.value
                ),
                candidate,
                "registry.v1",
                attrs,
            )
        })
        .collect()
}

fn extract_browser_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let normalized = normalize_evidence_path(&candidate.path);
    if !is_browser_history_path(&normalized) {
        return ExtractionOutcome {
            warnings: vec![format!(
                "{} is not a browser history database",
                candidate.path
            )],
            ..ExtractionOutcome::default()
        };
    }
    let (browser, profile) = browser_profile_from_path(&normalized);
    let parse_result = with_temp_sqlite(bytes, "browser-history", |db| {
        if normalized.ends_with("/places.sqlite") {
            extract_firefox_history(db, candidate, &browser, &profile)
        } else {
            extract_chromium_history(db, candidate, &browser, &profile)
        }
    });
    match parse_result {
        Ok(outcome) => outcome,
        Err(err) => ExtractionOutcome {
            warnings: vec![format!("{} browser parse failed: {}", candidate.path, err)],
            ..ExtractionOutcome::default()
        },
    }
}

fn extract_chromium_history(
    db: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, String> {
    let mut outcome = ExtractionOutcome::default();
    if table_exists(db, "urls")? {
        let mut stmt = db
            .prepare(
                "SELECT urls.url, COALESCE(urls.title, ''), COALESCE(urls.visit_count, 0),
                        COALESCE(visits.visit_time, urls.last_visit_time)
                 FROM urls
                 LEFT JOIN visits ON visits.url = urls.id
                 ORDER BY COALESCE(visits.visit_time, urls.last_visit_time) DESC
                 LIMIT 500",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (url, title, visit_count, raw_time) = row.map_err(|e| e.to_string())?;
            if url.trim().is_empty() {
                continue;
            }
            let visited_at = raw_time.and_then(chromium_time_to_dt);
            let mut attrs = browser_attrs(candidate, browser, profile);
            attrs.insert("url".to_string(), Value::String(url.clone()));
            attrs.insert("title".to_string(), Value::String(title.clone()));
            attrs.insert(
                "visitCount".to_string(),
                Value::Number(serde_json::Number::from(visit_count.max(0) as u64)),
            );
            if let Some(dt) = visited_at {
                attrs.insert("visitTime".to_string(), Value::String(dt.to_rfc3339()));
                outcome.timeline_events.push(make_timeline_event(
                    &candidate.file_id,
                    "BROWSER_VISIT",
                    dt,
                    format!("{} visit: {}", browser, title_or_url(&title, &url)),
                    url.clone(),
                    attrs.clone(),
                    "browser.history",
                ));
            }
            outcome.artifacts.push(make_artifact(
                "BrowserHistory",
                format!("{} visit: {}", browser, title_or_url(&title, &url)),
                url,
                candidate,
                "browser.history",
                attrs,
            ));
        }
    }

    if table_exists(db, "downloads")? {
        outcome.artifacts.extend(extract_chromium_downloads(
            db,
            candidate,
            browser,
            profile,
            &mut outcome.timeline_events,
        )?);
    }
    Ok(outcome)
}

fn extract_chromium_downloads(
    db: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
    events: &mut Vec<TimelineEvent>,
) -> Result<Vec<Artifact>, String> {
    let columns = table_columns(db, "downloads")?;
    let url_expr = if columns.iter().any(|column| column == "tab_url") {
        "COALESCE(tab_url, '')"
    } else if columns.iter().any(|column| column == "url") {
        "COALESCE(url, '')"
    } else {
        "''"
    };
    let target_expr = if columns.iter().any(|column| column == "target_path") {
        "COALESCE(target_path, '')"
    } else if columns.iter().any(|column| column == "current_path") {
        "COALESCE(current_path, '')"
    } else {
        "''"
    };
    let start_expr = if columns.iter().any(|column| column == "start_time") {
        "COALESCE(start_time, 0)"
    } else {
        "0"
    };
    let bytes_expr = if columns.iter().any(|column| column == "total_bytes") {
        "COALESCE(total_bytes, 0)"
    } else if columns.iter().any(|column| column == "received_bytes") {
        "COALESCE(received_bytes, 0)"
    } else {
        "0"
    };
    let sql = format!(
        "SELECT {url_expr}, {target_expr}, {start_expr}, {bytes_expr} FROM downloads ORDER BY {start_expr} DESC LIMIT 500"
    );
    let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut artifacts = Vec::new();
    for row in rows {
        let (url, target_path, raw_start, total_bytes) = row.map_err(|e| e.to_string())?;
        if url.trim().is_empty() && target_path.trim().is_empty() {
            continue;
        }
        let started_at = raw_start.and_then(chromium_time_to_dt);
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("url".to_string(), Value::String(url.clone()));
        attrs.insert("targetPath".to_string(), Value::String(target_path.clone()));
        attrs.insert(
            "totalBytes".to_string(),
            Value::Number(serde_json::Number::from(
                total_bytes.unwrap_or(0).max(0) as u64
            )),
        );
        if let Some(dt) = started_at {
            attrs.insert("startTime".to_string(), Value::String(dt.to_rfc3339()));
            events.push(make_timeline_event(
                &candidate.file_id,
                "BROWSER_DOWNLOAD",
                dt,
                format!("{} download: {}", browser, target_path),
                url.clone(),
                attrs.clone(),
                "browser.history",
            ));
        }
        artifacts.push(make_artifact(
            "BrowserDownload",
            format!("{} download: {}", browser, target_path),
            url,
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(artifacts)
}

fn extract_firefox_history(
    db: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, String> {
    let mut outcome = ExtractionOutcome::default();
    if !table_exists(db, "moz_places")? {
        outcome
            .warnings
            .push(format!("{} has no moz_places table", candidate.path));
        return Ok(outcome);
    }
    let mut stmt = db
        .prepare(
            "SELECT p.url, COALESCE(p.title, ''), COALESCE(p.visit_count, 0),
                    COALESCE(v.visit_date, p.last_visit_date)
             FROM moz_places p
             LEFT JOIN moz_historyvisits v ON v.place_id = p.id
             ORDER BY COALESCE(v.visit_date, p.last_visit_date) DESC
             LIMIT 500",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (url, title, visit_count, raw_time) = row.map_err(|e| e.to_string())?;
        if url.trim().is_empty() {
            continue;
        }
        let visited_at = raw_time.and_then(unix_microseconds_to_dt);
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("url".to_string(), Value::String(url.clone()));
        attrs.insert("title".to_string(), Value::String(title.clone()));
        attrs.insert(
            "visitCount".to_string(),
            Value::Number(serde_json::Number::from(visit_count.max(0) as u64)),
        );
        if let Some(dt) = visited_at {
            attrs.insert("visitTime".to_string(), Value::String(dt.to_rfc3339()));
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "BROWSER_VISIT",
                dt,
                format!("{} visit: {}", browser, title_or_url(&title, &url)),
                url.clone(),
                attrs.clone(),
                "browser.history",
            ));
        }
        outcome.artifacts.push(make_artifact(
            "BrowserHistory",
            format!("{} visit: {}", browser, title_or_url(&title, &url)),
            url,
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(outcome)
}

fn extract_email_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let parsed = parse_email_message(bytes);
    let mut attrs = base_attrs(candidate);
    attrs.insert("from".to_string(), Value::String(parsed.from.clone()));
    attrs.insert("to".to_string(), string_array_value(&parsed.to));
    attrs.insert("cc".to_string(), string_array_value(&parsed.cc));
    attrs.insert("bcc".to_string(), string_array_value(&parsed.bcc));
    attrs.insert("subject".to_string(), Value::String(parsed.subject.clone()));
    attrs.insert(
        "messageId".to_string(),
        Value::String(parsed.message_id.clone()),
    );
    attrs.insert(
        "attachments".to_string(),
        string_array_value(&parsed.attachments),
    );
    attrs.insert(
        "bodyPreview".to_string(),
        Value::String(parsed.body_preview.clone()),
    );
    if let Some(sent_at) = parsed.sent_at {
        attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
    }
    let mut outcome = ExtractionOutcome::default();
    if let Some(sent_at) = parsed.sent_at {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "EMAIL_SENT",
            sent_at,
            format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
            parsed.from.clone(),
            attrs.clone(),
            "email.eml_emlx",
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
        parsed.from,
        candidate,
        "email.eml_emlx",
        attrs,
    ));
    outcome
}

fn artifacts_by_data_source(artifacts: Vec<Artifact>) -> HashMap<String, Vec<Artifact>> {
    let mut grouped: HashMap<String, Vec<Artifact>> = HashMap::new();
    for artifact in artifacts {
        let data_source_id = artifact
            .attrs
            .get("dataSourceId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        grouped.entry(data_source_id).or_default().push(artifact);
    }
    grouped
}

fn make_artifact(
    family: &str,
    title: String,
    summary: String,
    candidate: &EvidenceCandidate,
    extractor_id: &str,
    attrs: BTreeMap<String, Value>,
) -> Artifact {
    Artifact {
        id: ArtifactId(Uuid::new_v4().to_string()),
        family: family.to_string(),
        title,
        summary,
        source_object_id: Some(candidate.file_id.clone()),
        extractor_id: Some(extractor_id.to_string()),
        extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: Some(candidate.path.clone()),
        created_at: Utc::now(),
        attrs,
    }
}

fn make_timeline_event(
    source_id: &FileEntryId,
    event_type: &str,
    timestamp: DateTime<Utc>,
    title: String,
    description: String,
    attrs: BTreeMap<String, Value>,
    parser_id: &str,
) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(Uuid::new_v4().to_string()),
        source_object_id: source_id.0.clone(),
        event_type: event_type.to_string(),
        timestamp,
        title,
        description,
        parser_id: Some(parser_id.to_string()),
        parser_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: None,
        attrs,
    }
}

fn base_attrs(candidate: &EvidenceCandidate) -> BTreeMap<String, Value> {
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "dataSourceId".to_string(),
        Value::String(candidate.data_source_id.clone()),
    );
    attrs.insert(
        "sourcePath".to_string(),
        Value::String(candidate.path.clone()),
    );
    attrs
}

fn browser_attrs(
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    attrs.insert("browser".to_string(), Value::String(browser.to_string()));
    attrs.insert("profile".to_string(), Value::String(profile.to_string()));
    attrs
}

fn count_analysis_artifacts(conn: &Connection) -> Result<u64, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type IN ('RegistryValue', 'BrowserHistory', 'BrowserDownload', 'EmailMessage')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count as u64)
}

fn count_artifacts_by_type(conn: &Connection, artifact_type: &str) -> Result<u64, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = ?1",
            [artifact_type],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count as u64)
}

fn query_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, String> {
    if families.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{}", index))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type IN ({})
         ORDER BY created_at DESC, id ASC
         LIMIT ?{} OFFSET ?{}",
        placeholders,
        families.len() + 1,
        families.len() + 2
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> = families
        .iter()
        .map(|family| Box::new((*family).to_string()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    params_values.push(Box::new(limit as i64));
    params_values.push(Box::new(offset as i64));
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            let attrs_text: String = row.get(4)?;
            Ok(AnalysisArtifactRow {
                id: row.get(0)?,
                source_object_id: row.get(1)?,
                extractor_id: row.get(2)?,
                created_at: row.get(3)?,
                attrs: serde_json::from_str(&attrs_text).unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

fn status_from_total(total: u64) -> AnalysisParseStatusDto {
    if total > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    }
}

fn string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> String {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default()
}

fn optional_string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> u64 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn string_vec_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_array_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn with_temp_sqlite(
    bytes: &[u8],
    prefix: &str,
    parse: impl FnOnce(&Connection) -> Result<ExtractionOutcome, String>,
) -> Result<ExtractionOutcome, String> {
    let path = temp_sqlite_path(prefix);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let result = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())
    .and_then(|conn| parse(&conn));
    let _ = std::fs::remove_file(path);
    result
}

fn temp_sqlite_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("forensics-{prefix}-{}.sqlite", Uuid::new_v4()))
}

fn table_exists(db: &Connection, table: &str) -> Result<bool, String> {
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

fn table_columns(db: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut stmt = db
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row.map_err(|e| e.to_string())?);
    }
    Ok(columns)
}

fn is_browser_history_path(normalized: &str) -> bool {
    (normalized.ends_with("/history") || normalized.ends_with("/archived history"))
        && (normalized.contains("/google/chrome/user data/")
            || normalized.contains("/microsoft/edge/user data/"))
        || (normalized.ends_with("/places.sqlite")
            && normalized.contains("/mozilla/firefox/profiles/"))
}

fn browser_profile_from_path(normalized: &str) -> (String, String) {
    let browser = if normalized.contains("/microsoft/edge/user data/") {
        "Edge"
    } else if normalized.contains("/mozilla/firefox/profiles/") {
        "Firefox"
    } else {
        "Chrome"
    };
    let marker = if browser == "Firefox" {
        "/mozilla/firefox/profiles/"
    } else if browser == "Edge" {
        "/microsoft/edge/user data/"
    } else {
        "/google/chrome/user data/"
    };
    let profile = normalized
        .split_once(marker)
        .map(|(_, rest)| rest.split('/').next().unwrap_or("default"))
        .filter(|value| !value.is_empty())
        .unwrap_or("default");
    (browser.to_string(), profile.to_string())
}

fn chromium_time_to_dt(value: i64) -> Option<DateTime<Utc>> {
    if value <= 0 {
        return None;
    }
    let seconds = value / 1_000_000 - 11_644_473_600;
    let nanos = ((value % 1_000_000) * 1_000) as u32;
    Utc.timestamp_opt(seconds, nanos).single()
}

fn unix_microseconds_to_dt(value: i64) -> Option<DateTime<Utc>> {
    if value <= 0 {
        return None;
    }
    Utc.timestamp_opt(value / 1_000_000, ((value % 1_000_000) * 1_000) as u32)
        .single()
}

fn title_or_url(title: &str, url: &str) -> String {
    if title.trim().is_empty() {
        url.to_string()
    } else {
        title.to_string()
    }
}

struct ParsedEmail {
    sent_at: Option<DateTime<Utc>>,
    from: String,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    message_id: String,
    attachments: Vec<String>,
    body_preview: String,
}

fn parse_email_message(bytes: &[u8]) -> ParsedEmail {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    let text = strip_emlx_size_line(&text);
    let (header_text, body_text) = text.split_once("\n\n").unwrap_or((text.as_str(), ""));
    let headers = parse_headers(header_text);
    let date = header_value(&headers, "date");
    ParsedEmail {
        sent_at: date.and_then(parse_email_datetime),
        from: header_value(&headers, "from").unwrap_or_default(),
        to: split_address_list(header_value(&headers, "to").unwrap_or_default()),
        cc: split_address_list(header_value(&headers, "cc").unwrap_or_default()),
        bcc: split_address_list(header_value(&headers, "bcc").unwrap_or_default()),
        subject: header_value(&headers, "subject").unwrap_or_default(),
        message_id: header_value(&headers, "message-id").unwrap_or_default(),
        attachments: extract_attachment_names(&text),
        body_preview: body_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(8)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(500)
            .collect(),
    }
}

fn strip_emlx_size_line(text: &str) -> String {
    let Some((first, rest)) = text.split_once('\n') else {
        return text.to_string();
    };
    if first.chars().all(|ch| ch.is_ascii_digit()) {
        rest.to_string()
    } else {
        text.to_string()
    }
}

fn parse_headers(header_text: &str) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in header_text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    headers
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name == name)
        .map(|(_, value)| value.clone())
}

fn split_address_list(value: String) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_email_datetime(value: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(&value)
        .or_else(|_| DateTime::parse_from_rfc3339(&value))
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn extract_attachment_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for marker in ["filename=", "name="] {
        let mut rest = text;
        while let Some((_, tail)) = rest.split_once(marker) {
            let trimmed = tail.trim_start_matches([' ', '\t']);
            let (name, next) = if let Some(stripped) = trimmed.strip_prefix('"') {
                stripped.split_once('"').unwrap_or((stripped, ""))
            } else {
                let end = trimmed
                    .find(|ch: char| ch == ';' || ch == '\n' || ch.is_whitespace())
                    .unwrap_or(trimmed.len());
                (&trimmed[..end], &trimmed[end..])
            };
            if !name.trim().is_empty() && !names.iter().any(|existing| existing == name) {
                names.push(name.trim().to_string());
            }
            rest = next;
        }
    }
    names
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
            hidden: false,
            system: false,
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

    fn sqlite_db_bytes(build: impl FnOnce(&Connection)) -> Vec<u8> {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("source.sqlite");
        {
            let db = Connection::open(&db_path).unwrap();
            build(&db);
        }
        std::fs::read(db_path).unwrap()
    }

    fn chromium_time(value: &str) -> i64 {
        let dt = DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc);
        (dt.timestamp() + 11_644_473_600) * 1_000_000 + i64::from(dt.timestamp_subsec_micros())
    }

    fn unix_microseconds(value: &str) -> i64 {
        let dt = DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc);
        dt.timestamp() * 1_000_000 + i64::from(dt.timestamp_subsec_micros())
    }

    fn chromium_history_bytes(url: &str, title: &str, target_path: &str) -> Vec<u8> {
        sqlite_db_bytes(|db| {
            db.execute_batch(
                "CREATE TABLE urls (
                    id INTEGER PRIMARY KEY,
                    url TEXT NOT NULL,
                    title TEXT,
                    visit_count INTEGER,
                    last_visit_time INTEGER
                );
                CREATE TABLE visits (
                    id INTEGER PRIMARY KEY,
                    url INTEGER NOT NULL,
                    visit_time INTEGER
                );
                CREATE TABLE downloads (
                    id INTEGER PRIMARY KEY,
                    tab_url TEXT,
                    target_path TEXT,
                    start_time INTEGER,
                    total_bytes INTEGER
                );",
            )
            .unwrap();
            db.execute(
                "INSERT INTO urls (id, url, title, visit_count, last_visit_time)
                 VALUES (1, ?1, ?2, 3, ?3)",
                params![url, title, chromium_time("2024-01-02T03:04:05Z")],
            )
            .unwrap();
            db.execute(
                "INSERT INTO visits (id, url, visit_time) VALUES (1, 1, ?1)",
                params![chromium_time("2024-01-02T03:04:05Z")],
            )
            .unwrap();
            db.execute(
                "INSERT INTO downloads (id, tab_url, target_path, start_time, total_bytes)
                 VALUES (1, ?1, ?2, ?3, 4096)",
                params![url, target_path, chromium_time("2024-01-03T04:05:06Z")],
            )
            .unwrap();
        })
    }

    fn firefox_places_bytes() -> Vec<u8> {
        sqlite_db_bytes(|db| {
            db.execute_batch(
                "CREATE TABLE moz_places (
                    id INTEGER PRIMARY KEY,
                    url TEXT NOT NULL,
                    title TEXT,
                    visit_count INTEGER,
                    last_visit_date INTEGER
                );
                CREATE TABLE moz_historyvisits (
                    id INTEGER PRIMARY KEY,
                    place_id INTEGER NOT NULL,
                    visit_date INTEGER
                );",
            )
            .unwrap();
            db.execute(
                "INSERT INTO moz_places (id, url, title, visit_count, last_visit_date)
                 VALUES (1, 'https://mozilla.example/', 'Firefox Example', 2, ?1)",
                params![unix_microseconds("2024-01-04T05:06:07Z")],
            )
            .unwrap();
            db.execute(
                "INSERT INTO moz_historyvisits (id, place_id, visit_date) VALUES (1, 1, ?1)",
                params![unix_microseconds("2024-01-04T05:06:07Z")],
            )
            .unwrap();
        })
    }

    fn sample_email_bytes() -> Vec<u8> {
        b"Date: Tue, 02 Jan 2024 03:04:05 +0000\r\nFrom: alice@example.com\r\nTo: bob@example.com, carol@example.com\r\nSubject: Quarterly evidence note\r\nMessage-ID: <msg-1@example.com>\r\nContent-Disposition: attachment; filename=\"evidence.txt\"\r\n\r\nThis is the first line of the message body.\r\nThis is the second line.\r\n".to_vec()
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
                file_with_ds("reg", &ds_id, "Users/alice/NTUSER.DAT", 50),
                file_with_ds(
                    "history",
                    &ds_id,
                    "Users/alice/AppData/Local/Google/Chrome/User Data/Default/History",
                    60,
                ),
                file_with_ds("email-eml", &ds_id, "Users/alice/Inbox/message.eml", 70),
                file_with_ds("email-emlx", &ds_id, "Users/alice/Inbox/message.emlx", 80),
            ])
            .unwrap();

        let candidates = discover_evidence_candidates(&conn).unwrap();
        assert_eq!(candidates.get("SystemInformation").map(Vec::len), Some(2));
        assert_eq!(candidates.get("Registry").map(Vec::len), Some(2));
        assert_eq!(candidates.get("BrowserHistory").map(Vec::len), Some(1));
        assert_eq!(candidates.get("Email").map(Vec::len), Some(2));
        assert_eq!(candidates.get("EventLogs").map(Vec::len), Some(1));
        assert_eq!(candidates.get("ProgramExecution").map(Vec::len), Some(1));
        assert_eq!(candidates.get("UserActivity").map(Vec::len), Some(1));
    }

    #[test]
    fn run_analysis_extraction_extracts_registry_browser_email_and_persists() {
        let (conn, _tmp, ds_id) = setup_case_db();
        let system_hive = std::fs::read(fixtures::tiny_registry_system_hive()).unwrap();
        let software_hive = std::fs::read(fixtures::tiny_registry_software_hive()).unwrap();
        let chrome_history = chromium_history_bytes(
            "https://chrome.example/",
            "Chrome Example",
            "C:/Temp/chrome.bin",
        );
        let edge_history =
            chromium_history_bytes("https://edge.example/", "Edge Example", "C:/Temp/edge.bin");
        let firefox_places = firefox_places_bytes();
        let email = sample_email_bytes();

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
                file_with_ds(
                    "chrome-history",
                    &ds_id,
                    "Users/alice/AppData/Local/Google/Chrome/User Data/Default/History",
                    chrome_history.len() as u64,
                ),
                file_with_ds(
                    "edge-history",
                    &ds_id,
                    "Users/alice/AppData/Local/Microsoft/Edge/User Data/Profile 1/History",
                    edge_history.len() as u64,
                ),
                file_with_ds(
                    "firefox-places",
                    &ds_id,
                    "Users/alice/AppData/Roaming/Mozilla/Firefox/Profiles/abc.default/places.sqlite",
                    firefox_places.len() as u64,
                ),
                file_with_ds(
                    "email",
                    &ds_id,
                    "Users/alice/Mail/message.eml",
                    email.len() as u64,
                ),
            ])
            .unwrap();

        let mut contents = HashMap::new();
        contents.insert("system".to_string(), system_hive);
        contents.insert("software".to_string(), software_hive);
        contents.insert("chrome-history".to_string(), chrome_history);
        contents.insert("edge-history".to_string(), edge_history);
        contents.insert("firefox-places".to_string(), firefox_places);
        contents.insert("email".to_string(), email);

        let run = run_analysis_extraction(&conn, "case-analysis", &[], |file_id| {
            contents
                .get(&file_id.0)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .ok_or_else(|| format!("missing bytes for {}", file_id.0))
        })
        .unwrap();

        assert_eq!(run.status, AnalysisParseStatusDto::Partial);
        assert!(run
            .warnings
            .iter()
            .any(|warning| warning.contains("CurrentVersion")));
        assert_eq!(run.scanned_count, 6);
        assert_eq!(run.artifact_count, 14);
        assert_eq!(run.timeline_event_count, 6);

        let mut stmt = conn
            .prepare(
                "SELECT artifact_type, COUNT(*)
                 FROM artifacts
                 GROUP BY artifact_type
                 ORDER BY artifact_type",
            )
            .unwrap();
        let counts = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .unwrap()
            .collect::<Result<HashMap<_, _>, _>>()
            .unwrap();
        assert_eq!(counts.get("RegistryValue").copied(), Some(8));
        assert_eq!(counts.get("BrowserHistory").copied(), Some(3));
        assert_eq!(counts.get("BrowserDownload").copied(), Some(2));
        assert_eq!(counts.get("EmailMessage").copied(), Some(1));

        let timeline_case_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM timeline_events WHERE case_id = 'case-analysis'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(timeline_case_count, 6);

        let registry = get_registry_extraction_summary(&conn, 0, 20).unwrap();
        assert_eq!(registry.total, 8);
        assert!(registry.values.iter().any(|value| {
            value.source_path == "Windows/System32/config/SYSTEM"
                && value.key_path == "ControlSet001\\Control\\ComputerName\\ComputerName"
                && value.value_name == "ComputerName"
                && value.data == registry::SYSTEM_COMPUTER_NAME
        }));

        let browser = get_browser_history_summary(&conn, 0, 20).unwrap();
        assert_eq!(browser.visit_total, 3);
        assert_eq!(browser.download_total, 2);
        assert!(browser.visits.iter().any(|visit| {
            visit.browser == "Chrome"
                && visit.profile == "default"
                && visit.url == "https://chrome.example/"
                && visit.visit_count == 3
                && visit.visit_time.as_deref() == Some("2024-01-02T03:04:05+00:00")
        }));
        assert!(browser.visits.iter().any(|visit| {
            visit.browser == "Firefox"
                && visit.profile == "abc.default"
                && visit.url == "https://mozilla.example/"
        }));
        assert!(browser.downloads.iter().any(|download| {
            download.browser == "Edge"
                && download.profile == "profile 1"
                && download.target_path == "C:/Temp/edge.bin"
                && download.total_bytes == 4096
        }));

        let email_summary = get_email_extraction_summary(&conn, 0, 20).unwrap();
        assert_eq!(email_summary.total, 1);
        let message = email_summary.messages.first().unwrap();
        assert_eq!(message.from, "alice@example.com");
        assert_eq!(
            message.to,
            vec![
                "bob@example.com".to_string(),
                "carol@example.com".to_string()
            ]
        );
        assert_eq!(message.subject, "Quarterly evidence note");
        assert_eq!(message.message_id, "<msg-1@example.com>");
        assert_eq!(message.attachments, vec!["evidence.txt".to_string()]);
        assert!(message
            .body_preview
            .contains("first line of the message body"));

        let second_run = run_analysis_extraction(&conn, "case-analysis", &[], |file_id| {
            contents
                .get(&file_id.0)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .ok_or_else(|| format!("missing bytes for {}", file_id.0))
        })
        .unwrap();
        assert_eq!(second_run.scanned_count, 0);
        assert_eq!(second_run.artifact_count, 14);
        let artifact_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        let timeline_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(artifact_count, 14);
        assert_eq!(timeline_count, 6);
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
