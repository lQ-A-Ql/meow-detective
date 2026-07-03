use crate::analysis_service::classification::metadata_category_stats;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::provenance::unknown_provenance;
use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use transport::dto::analysis::AnalysisProvenanceDto;
use transport::dto::{
    AnalysisParseStatusDto, EvidenceCategoryDto, EvidenceClassificationSummaryDto,
    EvidenceClassificationTotalsDto, EvidenceSourceDto,
};

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
            EvidencePathPattern::Suffix("/windows/system32/config/default"),
            EvidencePathPattern::Suffix("/ntuser.dat"),
            EvidencePathPattern::Suffix("/usrclass.dat"),
            EvidencePathPattern::Suffix("/windows/appcompat/programs/amcache.hve"),
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
        artifact_families: &[
            "Prefetch",
            "RegistryAmcacheApplication",
            "RegistryAmcacheApplicationFile",
        ],
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
        category: "BrowserHistory",
        display_name: "浏览器历史",
        evidence_kind: "browser_history",
        parser: "browser.history",
        artifact_families: &[
            "BrowserHistory",
            "BrowserDownload",
            "BrowserCookie",
            "BrowserSessionTab",
            "BrowserPassword",
        ],
        patterns: &[
            EvidencePathPattern::Contains("/google/chrome/user data/"),
            EvidencePathPattern::Contains("/microsoft/edge/user data/"),
            EvidencePathPattern::Contains("/mozilla/firefox/profiles/"),
            EvidencePathPattern::Suffix("/history"),
            EvidencePathPattern::Suffix("/archived history"),
            EvidencePathPattern::Suffix("/cookies"),
            EvidencePathPattern::Suffix("/cookies.sqlite"),
            EvidencePathPattern::Suffix("/login data"),
            EvidencePathPattern::Suffix("/logins.json"),
            EvidencePathPattern::Suffix("/last session"),
            EvidencePathPattern::Suffix("/last tabs"),
            EvidencePathPattern::Suffix("/current session"),
            EvidencePathPattern::Suffix("/current tabs"),
            EvidencePathPattern::Suffix("/recovery.jsonlz4"),
            EvidencePathPattern::Suffix("/previous.jsonlz4"),
            EvidencePathPattern::Suffix("/sessionstore.jsonlz4"),
            EvidencePathPattern::Suffix("/places.sqlite"),
        ],
    },
    EvidenceCategoryDef {
        category: "Email",
        display_name: "电子邮件",
        evidence_kind: "email",
        parser: "email",
        artifact_families: &["EmailMessage"],
        patterns: &[
            EvidencePathPattern::Suffix(".eml"),
            EvidencePathPattern::Suffix(".emlx"),
            EvidencePathPattern::Suffix(".mbox"),
            EvidencePathPattern::Suffix(".pst"),
            EvidencePathPattern::Suffix(".ost"),
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
    EvidenceCategoryDef {
        category: "LinuxArtifacts",
        display_name: "Linux 痕迹",
        evidence_kind: "linux_artifact",
        parser: "linux.artifacts",
        artifact_families: &[
            "LinuxJournal",
            "LinuxWtmp",
            "LinuxBashCommand",
            "LinuxAptEvent",
            "LinuxCronJob",
            "LinuxSudoEvent",
        ],
        patterns: &[
            EvidencePathPattern::Suffix(".journal"),
            EvidencePathPattern::Suffix(".journal~"),
            EvidencePathPattern::Contains("/var/log/journal/"),
            EvidencePathPattern::Contains("/run/log/journal/"),
            EvidencePathPattern::Suffix("/var/log/wtmp"),
            EvidencePathPattern::Contains("/var/log/wtmp."),
            EvidencePathPattern::Suffix("/var/log/btmp"),
            EvidencePathPattern::Contains("/var/log/btmp."),
            EvidencePathPattern::Suffix("/var/log/lastlog"),
            EvidencePathPattern::Suffix("/var/log/faillog"),
            EvidencePathPattern::Suffix(".bash_history"),
            EvidencePathPattern::Suffix(".zsh_history"),
            EvidencePathPattern::Suffix(".fish_history"),
            EvidencePathPattern::Suffix(".python_history"),
            EvidencePathPattern::Contains("/.local/share/fish/fish_history"),
            EvidencePathPattern::Contains("/var/log/apt/history.log"),
            EvidencePathPattern::Contains("/var/log/apt/history.log."),
            EvidencePathPattern::Contains("/var/log/dpkg.log"),
            EvidencePathPattern::Contains("/var/log/dpkg.log."),
            EvidencePathPattern::Suffix("/etc/crontab"),
            EvidencePathPattern::Contains("/etc/cron.d/"),
            EvidencePathPattern::Contains("/etc/cron.daily/"),
            EvidencePathPattern::Contains("/etc/cron.hourly/"),
            EvidencePathPattern::Contains("/etc/cron.monthly/"),
            EvidencePathPattern::Contains("/etc/cron.weekly/"),
            EvidencePathPattern::Contains("/var/spool/cron/crontabs/"),
            EvidencePathPattern::Contains("/var/spool/cron/"),
            EvidencePathPattern::Suffix("/var/log/auth.log"),
            EvidencePathPattern::Contains("/var/log/auth.log."),
            EvidencePathPattern::Suffix("/var/log/secure"),
            EvidencePathPattern::Contains("/var/log/secure."),
            EvidencePathPattern::Suffix("/var/log/audit/audit.log"),
            EvidencePathPattern::Contains("/var/log/audit/audit.log."),
            EvidencePathPattern::Suffix("/var/log/syslog"),
            EvidencePathPattern::Contains("/var/log/syslog."),
            EvidencePathPattern::Suffix("/var/log/messages"),
            EvidencePathPattern::Contains("/var/log/messages."),
            EvidencePathPattern::Suffix("/var/log/kern.log"),
            EvidencePathPattern::Contains("/var/log/kern.log."),
            EvidencePathPattern::Suffix("/var/log/cloud-init.log"),
            EvidencePathPattern::Contains("/var/log/cloud-init.log."),
            EvidencePathPattern::Contains("/.ssh/authorized_keys"),
            EvidencePathPattern::Contains("/.ssh/known_hosts"),
            EvidencePathPattern::Contains("/etc/ssh/"),
            EvidencePathPattern::Contains("/etc/systemd/system/"),
            EvidencePathPattern::Contains("/lib/systemd/system/"),
            EvidencePathPattern::Contains("/usr/lib/systemd/system/"),
            EvidencePathPattern::Contains("/etc/init.d/"),
            EvidencePathPattern::Suffix("/etc/rc.local"),
            EvidencePathPattern::Contains("/etc/profile.d/"),
            EvidencePathPattern::Suffix("/etc/sudoers"),
            EvidencePathPattern::Contains("/etc/sudoers.d/"),
        ],
    },
    EvidenceCategoryDef {
        category: "MacArtifacts",
        display_name: "macOS 痕迹",
        evidence_kind: "macos_artifact",
        parser: "macos.artifacts",
        artifact_families: &[
            "MacFSEvent",
            "MacLaunchService",
            "MacQuarantineEvent",
            "MacRecentItem",
            "MacSpotlightEntry",
            "MacUnifiedLogEntry",
        ],
        patterns: &[
            EvidencePathPattern::Contains("/.fseventsd/"),
            EvidencePathPattern::Contains("com.apple.launchservices"),
            EvidencePathPattern::Contains("com.apple.recentitems"),
            EvidencePathPattern::Suffix(".sfl2"),
            EvidencePathPattern::Contains("/launchservices.quarantineevents"),
            EvidencePathPattern::Contains("/.spotlight-v100/"),
            EvidencePathPattern::Suffix(".store.db"),
            EvidencePathPattern::Contains("/var/db/diagnostics/"),
            EvidencePathPattern::Suffix(".tracev3"),
        ],
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

pub fn collect_file_entries(conn: &Connection) -> Result<Vec<FileEntry>, AnalysisServiceError> {
    let file_repo = FileRepo::new(conn);
    let roots = file_repo.find_root_entries()?;

    let mut all_files = Vec::new();
    let mut queue = roots;

    while let Some(entry) = queue.pop() {
        if entry.entry_type == EntryType::Directory {
            let children = file_repo.find_children(&entry.id)?;
            queue.extend(children);
        } else {
            all_files.push(entry);
        }
    }

    Ok(all_files)
}

pub(crate) fn find_candidate_by_path_suffix(
    conn: &Connection,
    suffix: &str,
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    Ok(conn
        .query_row(
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
        .optional()?)
}

pub(crate) fn row_to_file_entry_for_analysis(row: &rusqlite::Row) -> rusqlite::Result<FileEntry> {
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
        encrypted: false,
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

pub fn evidence_category_defs() -> &'static [EvidenceCategoryDef] {
    EVIDENCE_CATEGORY_DEFS
}

pub fn discover_evidence_candidates(
    conn: &Connection,
) -> Result<HashMap<String, Vec<EvidenceCandidate>>, AnalysisServiceError> {
    let mut map: HashMap<String, Vec<EvidenceCandidate>> = EVIDENCE_CATEGORY_DEFS
        .iter()
        .map(|def| (def.category.to_string(), Vec::new()))
        .collect();

    let mut stmt = conn.prepare(
        "SELECT id, data_source_id, path, COALESCE(size, 0)
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let file_id: String = row.get(0)?;
        let data_source_id: String = row.get(1)?;
        let path: String = row.get(2)?;
        let size: u64 = row.get(3)?;
        let normalized = normalize_evidence_path(&path);

        for def in EVIDENCE_CATEGORY_DEFS {
            if def.patterns.is_empty() || !evidence_path_matches(&normalized, def.patterns) {
                continue;
            }
            if def.category == "BrowserHistory" && !is_browser_history_path(&normalized) {
                continue;
            }
            let (evidence_kind, parser) = if def.category == "Email" {
                email_kind_and_parser(&normalized)
            } else {
                (def.evidence_kind.to_string(), def.parser.to_string())
            };
            map.entry(def.category.to_string())
                .or_default()
                .push(EvidenceCandidate {
                    file_id: FileEntryId(file_id.clone()),
                    data_source_id: data_source_id.clone(),
                    path: path.clone(),
                    size,
                    evidence_kind,
                    parser,
                    category: def.category.to_string(),
                });
        }
    }

    Ok(map)
}

fn email_kind_and_parser(normalized: &str) -> (String, String) {
    if normalized.ends_with(".eml") || normalized.ends_with(".emlx") {
        return ("email_eml_emlx".to_string(), "email.eml_emlx".to_string());
    }
    if normalized.ends_with(".mbox") {
        return ("email_mbox".to_string(), "email.mbox".to_string());
    }
    if normalized.ends_with(".pst") {
        return ("email_pst".to_string(), "email.pst".to_string());
    }
    if normalized.ends_with(".ost") {
        return ("email_ost".to_string(), "email.ost".to_string());
    }
    ("email".to_string(), "email".to_string())
}

pub fn evidence_candidates_for_categories(
    conn: &Connection,
    categories: &[&str],
) -> Result<Vec<EvidenceCandidate>, AnalysisServiceError> {
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
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
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
        warnings.push("未发现证据族候选文件；请确认数据源已导入且文件树可用。".to_string());
    }

    Ok(EvidenceClassificationSummaryDto {
        status,
        categories,
        totals,
        generated_at,
        warnings,
    })
}

pub(crate) fn is_browser_history_path(normalized: &str) -> bool {
    if normalized.contains("/google/chrome/user data/")
        || normalized.contains("/microsoft/edge/user data/")
    {
        return normalized.ends_with("/history")
            || normalized.ends_with("/archived history")
            || normalized.ends_with("/cookies")
            || normalized.ends_with("/login data")
            || normalized.ends_with("/last session")
            || normalized.ends_with("/last tabs")
            || normalized.ends_with("/current session")
            || normalized.ends_with("/current tabs");
    }
    if normalized.contains("/mozilla/firefox/profiles/") {
        return normalized.ends_with("/places.sqlite")
            || normalized.ends_with("/cookies.sqlite")
            || normalized.ends_with("/logins.json")
            || normalized.ends_with("/recovery.jsonlz4")
            || normalized.ends_with("/previous.jsonlz4")
            || normalized.ends_with("/sessionstore.jsonlz4");
    }
    false
}

fn artifact_counts_by_family(
    conn: &Connection,
) -> Result<HashMap<String, u64>, AnalysisServiceError> {
    let mut stmt =
        conn.prepare("SELECT artifact_type, COUNT(*) FROM artifacts GROUP BY artifact_type")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (family, count) = row?;
        counts.insert(family, count);
    }
    Ok(counts)
}

fn artifact_counts_by_source(
    conn: &Connection,
) -> Result<HashMap<String, u64>, AnalysisServiceError> {
    let mut stmt = conn.prepare(
        "SELECT source_object_id, COUNT(*)
             FROM artifacts
             WHERE source_object_id IS NOT NULL
             GROUP BY source_object_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (source_id, count) = row?;
        counts.insert(source_id, count);
    }
    Ok(counts)
}

pub(crate) fn normalize_evidence_path(path: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_kind_and_parser_splits_by_extension() {
        assert_eq!(
            email_kind_and_parser("mail.eml"),
            ("email_eml_emlx".to_string(), "email.eml_emlx".to_string())
        );
        assert_eq!(
            email_kind_and_parser("mail.emlx"),
            ("email_eml_emlx".to_string(), "email.eml_emlx".to_string())
        );
        assert_eq!(
            email_kind_and_parser("inbox.mbox"),
            ("email_mbox".to_string(), "email.mbox".to_string())
        );
        assert_eq!(
            email_kind_and_parser("archive.pst"),
            ("email_pst".to_string(), "email.pst".to_string())
        );
        assert_eq!(
            email_kind_and_parser("cache.ost"),
            ("email_ost".to_string(), "email.ost".to_string())
        );
        assert_eq!(
            email_kind_and_parser("unknown.bin"),
            ("email".to_string(), "email".to_string())
        );
    }
}
