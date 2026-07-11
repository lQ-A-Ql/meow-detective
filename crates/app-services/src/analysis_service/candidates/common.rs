use super::{evidence_category_defs, UNSUPPORTED_MACOS_CATEGORY};
use crate::analysis_service::error::AnalysisServiceError;
use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct EvidenceCategoryDef {
    pub category: &'static str,
    pub display_name: &'static str,
    pub evidence_kind: &'static str,
    pub parser: &'static str,
    pub artifact_families: &'static [&'static str],
    pub(super) patterns: &'static [EvidencePathPattern],
    pub(super) matcher: Option<fn(&str) -> bool>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum EvidencePathPattern {
    Suffix(&'static str),
    Contains(&'static str),
    ContainsAndSuffix(&'static str, &'static str),
}

pub(super) const EMAIL_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
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
    matcher: None,
};

pub(super) const FILE_TYPE_INVENTORY_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "FileTypeInventory",
    display_name: "File type inventory",
    evidence_kind: "metadata_inventory",
    parser: "metadata.extension_path",
    artifact_families: &[],
    patterns: &[],
    matcher: None,
};

pub(crate) fn ensure_supported_analysis_categories(
    categories: &[&str],
) -> Result<(), AnalysisServiceError> {
    if categories.iter().any(|category| {
        category
            .trim()
            .eq_ignore_ascii_case(UNSUPPORTED_MACOS_CATEGORY)
    }) {
        return Err(AnalysisServiceError::Unsupported(
            UNSUPPORTED_MACOS_CATEGORY.to_string(),
        ));
    }
    Ok(())
}

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
            queue.extend(file_repo.find_children(&entry.id)?);
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
        created_at: parse_timestamp(row.get(11)?),
        modified_at: parse_timestamp(row.get(12)?),
        accessed_at: parse_timestamp(row.get(13)?),
        changed_at: parse_timestamp(row.get(14)?),
        hash_sha256: row.get(15)?,
    })
}

fn parse_timestamp(value: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    value
        .and_then(|timestamp| chrono::DateTime::parse_from_rfc3339(&timestamp).ok())
        .map(|timestamp| timestamp.with_timezone(&chrono::Utc))
}

pub fn discover_evidence_candidates(
    conn: &Connection,
) -> Result<HashMap<String, Vec<EvidenceCandidate>>, AnalysisServiceError> {
    let definitions = evidence_category_defs();
    let mut candidates = definitions
        .iter()
        .map(|definition| (definition.category.to_string(), Vec::new()))
        .collect::<HashMap<_, _>>();
    let mut statement = conn.prepare(
        "SELECT id, data_source_id, path, COALESCE(size, 0)
         FROM file_entries
         WHERE entry_type = 'file' COLLATE NOCASE",
    )?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let file_id: String = row.get(0)?;
        let data_source_id: String = row.get(1)?;
        let path: String = row.get(2)?;
        let size: u64 = row.get(3)?;
        add_matching_candidates(
            &mut candidates,
            definitions,
            &file_id,
            &data_source_id,
            &path,
            size,
        );
    }

    Ok(candidates)
}

fn add_matching_candidates(
    candidates: &mut HashMap<String, Vec<EvidenceCandidate>>,
    definitions: &[EvidenceCategoryDef],
    file_id: &str,
    data_source_id: &str,
    path: &str,
    size: u64,
) {
    let normalized = normalize_evidence_path(path);
    for definition in definitions {
        if definition.patterns.is_empty()
            || !evidence_path_matches(&normalized, definition.patterns)
            || definition
                .matcher
                .is_some_and(|matcher| !matcher(&normalized))
        {
            continue;
        }
        let (evidence_kind, parser) = candidate_kind_and_parser(definition, &normalized);
        candidates
            .entry(definition.category.to_string())
            .or_default()
            .push(EvidenceCandidate {
                file_id: FileEntryId(file_id.to_string()),
                data_source_id: data_source_id.to_string(),
                path: path.to_string(),
                size,
                evidence_kind,
                parser,
                category: definition.category.to_string(),
            });
    }
}

fn candidate_kind_and_parser(
    definition: &EvidenceCategoryDef,
    normalized: &str,
) -> (String, String) {
    if definition.category == "Email" {
        email_kind_and_parser(normalized)
    } else {
        (
            definition.evidence_kind.to_string(),
            definition.parser.to_string(),
        )
    }
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
    ensure_supported_analysis_categories(categories)?;
    let discovered = discover_evidence_candidates(conn)?;
    Ok(categories
        .iter()
        .filter_map(|category| discovered.get(*category))
        .flatten()
        .cloned()
        .collect())
}

pub(crate) fn normalize_evidence_path(path: &str) -> String {
    let normalized = strip_synthetic_root_prefix(&path.replace('\\', "/")).to_ascii_lowercase();
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

fn strip_synthetic_root_prefix(path: &str) -> String {
    let mut path = path.trim().trim_start_matches('/').to_string();
    let had_partition_marker = if let Some(stripped) = strip_partition_marker_prefix(&path) {
        path = stripped.to_string();
        true
    } else {
        false
    };
    let components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();

    if components
        .first()
        .is_some_and(|component| is_linux_root_component(component))
        || !looks_like_synthetic_linux_prefix(&components, had_partition_marker)
    {
        return path;
    }

    let Some(index) = linux_root_start_index(&components) else {
        return path;
    };
    if index == 0 {
        path
    } else {
        components[index..].join("/")
    }
}

fn strip_partition_marker_prefix(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("[P")?;
    let (partition, after_partition) = rest.split_once(']')?;
    if partition.is_empty() || !partition.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let stripped = after_partition.trim_start_matches('/');
    (!stripped.is_empty()).then_some(stripped)
}

fn looks_like_synthetic_linux_prefix(components: &[&str], had_partition_marker: bool) -> bool {
    had_partition_marker
        || components
            .first()
            .is_some_and(|component| looks_like_partition_or_volume_root(component))
        || (components.len() >= 3
            && components[1].eq_ignore_ascii_case("root")
            && !is_linux_root_component(components[0]))
}

fn looks_like_partition_or_volume_root(component: &str) -> bool {
    let lower = component.to_ascii_lowercase();
    lower.starts_with("partition ") || lower.starts_with("volume")
}

fn linux_root_start_index(components: &[&str]) -> Option<usize> {
    for (index, component) in components.iter().enumerate() {
        if !is_linux_root_component(component) {
            continue;
        }
        if component.eq_ignore_ascii_case("root")
            && index > 0
            && components
                .get(index + 1)
                .is_some_and(|next| is_linux_root_component(next))
        {
            continue;
        }
        return Some(index);
    }
    None
}

fn is_linux_root_component(component: &str) -> bool {
    matches!(
        component.to_ascii_lowercase().as_str(),
        "bin"
            | "boot"
            | "dev"
            | "etc"
            | "home"
            | "lib"
            | "lib64"
            | "opt"
            | "root"
            | "run"
            | "sbin"
            | "srv"
            | "tmp"
            | "usr"
            | "var"
    )
}

fn evidence_path_matches(path: &str, patterns: &[EvidencePathPattern]) -> bool {
    patterns.iter().any(|pattern| match pattern {
        EvidencePathPattern::Suffix(suffix) => path.ends_with(suffix),
        EvidencePathPattern::Contains(needle) => path.contains(needle),
        EvidencePathPattern::ContainsAndSuffix(needle, suffix) => {
            path.contains(needle) && path.ends_with(suffix)
        }
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/candidates/common.rs"]
mod tests;
