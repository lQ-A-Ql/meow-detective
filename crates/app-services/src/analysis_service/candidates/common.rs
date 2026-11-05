use super::{evidence_category_defs, UNSUPPORTED_MACOS_CATEGORY};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{EntryType, FileEncryptionStatus, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::{file_encryption_status_from_row, FileRepo};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

mod path_matching;

pub(crate) use path_matching::evidence_path_matches;
pub(crate) use path_matching::evidence_relative_path;
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
    artifact_families: &["EmailMessage", "EmailAccount"],
    patterns: &[
        EvidencePathPattern::Suffix(".eml"),
        EvidencePathPattern::Suffix(".emlx"),
        EvidencePathPattern::Suffix(".mbox"),
        EvidencePathPattern::FoxmailBox,
        EvidencePathPattern::FoxmailIndex,
        EvidencePathPattern::FoxmailAccount,
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
pub struct EvidenceCompanion {
    pub file_id: FileEntryId,
    pub path: String,
    pub size: u64,
    pub encrypted: bool,
    pub content_identity: String,
}

#[derive(Debug, Clone)]
pub struct EvidenceCandidate {
    pub file_id: FileEntryId,
    pub data_source_id: String,
    pub partition_index: Option<usize>,
    pub path: String,
    pub size: u64,
    pub encrypted: bool,
    pub content_identity: String,
    pub companions: Vec<EvidenceCompanion>,
    pub evidence_kind: String,
    pub parser: String,
    pub category: String,
    /// File-entry mtime; anchors year-less log timestamps (classic syslog,
    /// sudo/auth lines) to the evidence timeframe.
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn collect_file_entries(conn: &Connection) -> Result<Vec<FileEntry>, AnalysisServiceError> {
    if let Some(entries) = collect_file_entries_from_fjall(conn)? {
        return Ok(entries);
    }
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

fn collect_file_entries_from_fjall(
    conn: &Connection,
) -> Result<Option<Vec<FileEntry>>, AnalysisServiceError> {
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .unwrap_or_default();
    let source_id = conn
        .query_row(
            "SELECT id FROM data_sources ORDER BY id LIMIT 2",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(source_id) = source_id else {
        return Ok(None);
    };
    let source_path = std::path::Path::new(&database_path);
    let Ok(fjall) = crate::source_db::open_fjall_source_read_only_from_source_db_path(
        source_path,
        &domain::DataSourceId(source_id.clone()),
    ) else {
        return Ok(None);
    };
    let snapshot = fjall.snapshot();
    let projection_key = persistence_fjall::encode_key(
        "projection",
        &[persistence_fjall::KeyPart::Text(source_id.clone())],
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let projection = fjall
        .read_envelope(
            &snapshot,
            persistence_fjall::KeyspaceName::Meta,
            &projection_key,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    if projection
        .as_ref()
        .is_some_and(|envelope| envelope.record_kind == "source_projection_building")
    {
        return Ok(None);
    }
    let projection_complete = fjall
        .source_projection_complete(&snapshot, &source_id)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let mut cursor = None;
    let mut entries = Vec::new();
    loop {
        let page = fjall
            .catalog_list_all(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        entries.extend(page.records.into_iter().map(file_entry_from_catalog_record));
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok((projection_complete || !entries.is_empty()).then_some(entries));
        }
    }
}

fn file_entry_from_catalog_record(record: persistence_fjall::FileEntryRecord) -> FileEntry {
    FileEntry {
        id: FileEntryId(record.id),
        parent_id: record.parent_id.map(FileEntryId),
        data_source_id: domain::DataSourceId(record.data_source_id),
        path: record.path,
        name: record.name,
        entry_type: match record.entry_type {
            persistence_fjall::CatalogEntryType::Directory => EntryType::Directory,
            persistence_fjall::CatalogEntryType::File => EntryType::File,
        },
        size: record.size,
        ext: record.ext,
        deleted: record.deleted,
        hidden: record.hidden,
        system: record.system,
        encrypted: record.encrypted,
        read_only: record.read_only,
        archive: record.archive,
        unix_mode: record.unix_mode,
        created_at: record.created,
        modified_at: record.modified,
        accessed_at: record.accessed,
        changed_at: record.changed,
        hash_sha256: record.hash_sha256,
    }
}

pub(crate) fn find_candidate_by_path_suffix(
    conn: &Connection,
    suffix: &str,
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    Ok(conn
        .query_row(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                    hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode
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
    let encryption_status = file_encryption_status_from_row(row, 16)?;
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
        encrypted: encryption_status.blocks_content(),
        created_at: parse_timestamp(row.get(11)?),
        modified_at: parse_timestamp(row.get(12)?),
        accessed_at: parse_timestamp(row.get(13)?),
        changed_at: parse_timestamp(row.get(14)?),
        hash_sha256: row.get(15)?,
        read_only: row.get::<_, i32>(17)? != 0,
        archive: row.get::<_, i32>(18)? != 0,
        unix_mode: row.get(19)?,
    })
}

pub(crate) fn parse_timestamp(value: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
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
    let entries = collect_file_entries(conn)?;
    let mut companions = super::companion_index::CompanionIndex::default();

    for entry in entries {
        ensure_not_cancelled(cancel_token)?;
        if !entry.is_file() {
            continue;
        }
        let file_id = entry.id.0;
        let data_source_id = entry.data_source_id.0;
        let path = entry.path;
        let size = entry.size.unwrap_or_default();
        let partition_index = None;
        let encryption_status: FileEncryptionStatus = entry.encrypted.into();
        let encrypted = encryption_status.blocks_content();
        let content_identity = candidate_content_identity(
            &file_id,
            &data_source_id,
            partition_index,
            &path,
            size,
            encryption_status,
            [
                entry.created_at.map(|value| value.to_rfc3339()),
                entry.modified_at.map(|value| value.to_rfc3339()),
                entry.accessed_at.map(|value| value.to_rfc3339()),
                entry.changed_at.map(|value| value.to_rfc3339()),
                entry.hash_sha256,
            ],
        );
        let modified_at = entry.modified_at;
        if super::companion_index::CompanionIndex::matches(&path) {
            companions.insert(
                &data_source_id,
                partition_index,
                EvidenceCompanion {
                    file_id: FileEntryId(file_id.clone()),
                    path: path.clone(),
                    size,
                    encrypted,
                    content_identity: content_identity.clone(),
                },
            );
        }
        add_matching_candidates(
            &mut candidates,
            definitions,
            CandidateRow {
                file_id: &file_id,
                data_source_id: &data_source_id,
                partition_index,
                path: &path,
                size,
                encrypted,
                content_identity: &content_identity,
                modified_at,
            },
            cancel_token,
        )?;
    }

    for category in ["Registry", "BrowserHistory"] {
        if let Some(group) = candidates.get_mut(category) {
            for candidate in group {
                ensure_not_cancelled(cancel_token)?;
                companions.attach(candidate)?;
            }
        }
    }
    ensure_not_cancelled(cancel_token)?;
    Ok(candidates)
}

struct CandidateRow<'a> {
    file_id: &'a str,
    data_source_id: &'a str,
    partition_index: Option<usize>,
    path: &'a str,
    size: u64,
    encrypted: bool,
    content_identity: &'a str,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
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
                encrypted: row.encrypted,
                content_identity: row.content_identity.to_string(),
                companions: Vec::new(),
                evidence_kind,
                parser,
                category: definition.category.to_string(),
                modified_at: row.modified_at,
            });
    }
    Ok(())
}

pub(crate) fn candidate_content_identity(
    file_id: &str,
    data_source_id: &str,
    partition_index: Option<usize>,
    path: &str,
    size: u64,
    encryption_status: FileEncryptionStatus,
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
    match encryption_status {
        FileEncryptionStatus::Unknown => hasher.update(b"analysis-candidate-encryption-unknown"),
        FileEncryptionStatus::Clear => hasher.update(b"analysis-candidate-content-clear"),
        FileEncryptionStatus::Encrypted => hasher.update(b"analysis-candidate-efs-encrypted"),
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
    if normalized.ends_with(".box") {
        return (
            "email_foxmail_box".to_string(),
            "email.foxmail_box".to_string(),
        );
    }
    if normalized.ends_with("/mails/index") {
        return (
            "email_foxmail_index".to_string(),
            "email.foxmail_index".to_string(),
        );
    }
    if normalized.ends_with("/accounts/account.rec0") {
        return (
            "email_foxmail_account".to_string(),
            "email.foxmail_account".to_string(),
        );
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
