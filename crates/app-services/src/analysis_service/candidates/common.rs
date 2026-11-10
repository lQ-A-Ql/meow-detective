use super::{evidence_category_defs, UNSUPPORTED_MACOS_CATEGORY};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{FileEncryptionStatus, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

mod catalog;
mod path_matching;
use catalog::collect_file_entries_from_fjall;

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
    collect_file_entries_from_fjall(conn)
}

pub(crate) fn find_candidate_by_path_suffix(
    conn: &Connection,
    suffix: &str,
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    let suffix = suffix.to_ascii_lowercase().replace('\\', "/");
    Ok(collect_file_entries(conn)?
        .into_iter()
        .filter(|entry| {
            entry.is_file()
                && entry
                    .path
                    .to_ascii_lowercase()
                    .replace('\\', "/")
                    .ends_with(&suffix)
        })
        .min_by_key(|entry| entry.path.len()))
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
    let repository = FileRepo::new(conn);

    for entry in entries {
        ensure_not_cancelled(cancel_token)?;
        if !entry.is_file() {
            continue;
        }
        let partition_index = repository.find_partition_index_by_id(&entry.id)?;
        let encryption_status = repository
            .find_encryption_status(&entry.id)?
            .ok_or_else(|| AnalysisServiceError::Other("analysis candidate disappeared".into()))?;
        let file_id = entry.id.0;
        let data_source_id = entry.data_source_id.0;
        let path = entry.path;
        let size = entry.size.unwrap_or_default();
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
