use super::{evidence_category_defs, UNSUPPORTED_MACOS_CATEGORY};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

mod path_matching;

use path_matching::evidence_path_matches;
pub(crate) use path_matching::normalize_evidence_path;
pub(super) use path_matching::EvidencePathPattern;

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
    pub partition_index: Option<usize>,
    pub path: String,
    pub size: u64,
    pub content_identity: String,
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
    let cancel_token = AtomicBool::new(false);
    discover_evidence_candidates_with_definitions(conn, evidence_category_defs(), &cancel_token)
}

fn discover_evidence_candidates_with_definitions(
    conn: &Connection,
    definitions: &[EvidenceCategoryDef],
    cancel_token: &AtomicBool,
) -> Result<HashMap<String, Vec<EvidenceCandidate>>, AnalysisServiceError> {
    ensure_not_cancelled(cancel_token)?;
    let mut candidates = definitions
        .iter()
        .map(|definition| (definition.category.to_string(), Vec::new()))
        .collect::<HashMap<_, _>>();
    let partition_column = if file_entries_has_partition_index(conn)? {
        "partition_index"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT id, data_source_id, path, COALESCE(size, 0), {partition_column},
                created_at, modified_at, accessed_at, changed_at, hash_sha256
         FROM file_entries
         WHERE entry_type = 'file' COLLATE NOCASE"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        ensure_not_cancelled(cancel_token)?;
        let file_id: String = row.get(0)?;
        let data_source_id: String = row.get(1)?;
        let path: String = row.get(2)?;
        let size: u64 = row.get(3)?;
        let partition_index = parse_partition_index(row, &file_id)?;
        let content_identity = candidate_content_identity(
            &file_id,
            &data_source_id,
            partition_index,
            &path,
            size,
            [
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
            ],
        );
        add_matching_candidates(
            &mut candidates,
            definitions,
            CandidateRow {
                file_id: &file_id,
                data_source_id: &data_source_id,
                partition_index,
                path: &path,
                size,
                content_identity: &content_identity,
            },
            cancel_token,
        )?;
    }

    ensure_not_cancelled(cancel_token)?;
    Ok(candidates)
}

fn file_entries_has_partition_index(conn: &Connection) -> Result<bool, AnalysisServiceError> {
    let mut statement = conn.prepare("PRAGMA table_info(file_entries)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column?.eq_ignore_ascii_case("partition_index") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_partition_index(
    row: &rusqlite::Row<'_>,
    file_id: &str,
) -> Result<Option<usize>, AnalysisServiceError> {
    match row.get_ref(4)? {
        rusqlite::types::ValueRef::Null => Ok(None),
        rusqlite::types::ValueRef::Integer(value) => {
            let partition_index = u32::try_from(value).map_err(|_| {
                AnalysisServiceError::InvalidInput(format!(
                    "file entry '{file_id}' has invalid partition_index {value}"
                ))
            })?;
            Ok(Some(partition_index as usize))
        }
        _ => Err(AnalysisServiceError::InvalidInput(format!(
            "file entry '{file_id}' has non-integer partition_index"
        ))),
    }
}

struct CandidateRow<'a> {
    file_id: &'a str,
    data_source_id: &'a str,
    partition_index: Option<usize>,
    path: &'a str,
    size: u64,
    content_identity: &'a str,
}

fn add_matching_candidates(
    candidates: &mut HashMap<String, Vec<EvidenceCandidate>>,
    definitions: &[EvidenceCategoryDef],
    row: CandidateRow<'_>,
    cancel_token: &AtomicBool,
) -> Result<(), AnalysisServiceError> {
    let normalized = normalize_evidence_path(row.path);
    for definition in definitions {
        ensure_not_cancelled(cancel_token)?;
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
                file_id: FileEntryId(row.file_id.to_string()),
                data_source_id: row.data_source_id.to_string(),
                partition_index: row.partition_index,
                path: row.path.to_string(),
                size: row.size,
                content_identity: row.content_identity.to_string(),
                evidence_kind,
                parser,
                category: definition.category.to_string(),
            });
    }
    Ok(())
}

fn candidate_content_identity(
    file_id: &str,
    data_source_id: &str,
    partition_index: Option<usize>,
    path: &str,
    size: u64,
    metadata: [Option<String>; 5],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"analysis-candidate-content-v1");
    for value in [file_id, data_source_id, path] {
        update_identity_field(&mut hasher, value.as_bytes());
    }
    hasher.update(size.to_le_bytes());
    match partition_index {
        Some(partition_index) => {
            hasher.update([1]);
            hasher.update((partition_index as u64).to_le_bytes());
        }
        None => hasher.update([0]),
    }
    for value in metadata {
        match value {
            Some(value) => {
                hasher.update([1]);
                update_identity_field(&mut hasher, value.as_bytes());
            }
            None => hasher.update([0]),
        }
    }
    hex::encode(hasher.finalize())
}

fn update_identity_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
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
    let cancel_token = AtomicBool::new(false);
    evidence_candidates_for_categories_with_cancel(conn, categories, &cancel_token)
}

pub(crate) fn evidence_candidates_for_categories_with_cancel(
    conn: &Connection,
    categories: &[&str],
    cancel_token: &AtomicBool,
) -> Result<Vec<EvidenceCandidate>, AnalysisServiceError> {
    ensure_supported_analysis_categories(categories)?;
    ensure_not_cancelled(cancel_token)?;
    let definitions = selected_evidence_category_defs(categories);
    if definitions.is_empty() {
        return Ok(Vec::new());
    }
    let discovered =
        discover_evidence_candidates_with_definitions(conn, &definitions, cancel_token)?;
    ensure_not_cancelled(cancel_token)?;
    Ok(categories
        .iter()
        .filter_map(|category| discovered.get(*category))
        .flatten()
        .cloned()
        .collect())
}

fn selected_evidence_category_defs(categories: &[&str]) -> Vec<EvidenceCategoryDef> {
    evidence_category_defs()
        .iter()
        .filter(|definition| categories.contains(&definition.category))
        .copied()
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/candidates/common.rs"]
mod tests;
