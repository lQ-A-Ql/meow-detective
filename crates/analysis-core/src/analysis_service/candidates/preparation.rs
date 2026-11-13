use super::common::{candidate_content_identity, normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::AnalysisServiceError;
use domain::{FileEncryptionStatus, FileEntryId};
use persistence_source::fjall::{CatalogEntryType, FileEntryRecord};

const MAX_PREPARATION_RECORDS: usize = 131_072;
const MAX_PREPARATION_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct PreparationCatalog {
    records: Vec<FileEntryRecord>,
    bytes: usize,
    scanned_records: u64,
}

impl PreparationCatalog {
    pub(crate) fn observe(&mut self, record: &FileEntryRecord) -> Result<(), AnalysisServiceError> {
        self.scanned_records = self.scanned_records.saturating_add(1);
        let path = normalize_evidence_path(&record.path);
        if !relevant(&path, &record.name) {
            return Ok(());
        }
        let bytes = record.path.len()
            + record.name.len()
            + record.id.len()
            + record.data_source_id.len()
            + std::mem::size_of::<FileEntryRecord>();
        if self.records.len() == MAX_PREPARATION_RECORDS
            || self.bytes.saturating_add(bytes) > MAX_PREPARATION_BYTES
        {
            return Err(AnalysisServiceError::Unsupported(
                "analysis preparation catalog exceeds its memory budget".into(),
            ));
        }
        self.bytes += bytes;
        self.records.push(record.clone());
        Ok(())
    }

    pub(crate) fn records(&self) -> impl Iterator<Item = &FileEntryRecord> {
        self.records.iter()
    }

    pub(crate) fn scanned_records(&self) -> u64 {
        self.scanned_records
    }

    pub(crate) fn suffix(&self, source: &str, suffix: &str) -> Option<EvidenceCandidate> {
        let suffix = normalize_evidence_path(suffix);
        self.files(source)
            .filter(|record| normalize_evidence_path(&record.path).ends_with(&suffix))
            .min_by(|left, right| left.id.cmp(&right.id))
            .map(locator)
    }

    pub(crate) fn shortest_suffix(&self, suffix: &str) -> Option<EvidenceCandidate> {
        let suffix = normalize_evidence_path(suffix);
        self.records
            .iter()
            .filter(|record| {
                record.entry_type == CatalogEntryType::File
                    && normalize_evidence_path(&record.path).ends_with(&suffix)
            })
            .min_by(|left, right| {
                left.path
                    .len()
                    .cmp(&right.path.len())
                    .then_with(|| left.path.cmp(&right.path))
            })
            .map(locator)
    }

    pub(crate) fn matching(&self, source: &str, marker: &str) -> Vec<EvidenceCandidate> {
        let marker = normalize_evidence_path(marker);
        let mut records = self
            .files(source)
            .filter(|record| normalize_evidence_path(&record.path).contains(&marker))
            .map(locator)
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.path.len());
        records.truncate(32);
        records
    }

    pub(crate) fn files<'a>(
        &'a self,
        source: &'a str,
    ) -> impl Iterator<Item = &'a FileEntryRecord> + 'a {
        self.records.iter().filter(move |record| {
            record.data_source_id == source
                && record.entry_type == CatalogEntryType::File
                && !record.deleted
        })
    }
}

fn relevant(path: &str, name: &str) -> bool {
    path.contains("/systemd/")
        || path.contains("/microsoft/protect/")
        || path.contains("/crypto/systemkeys/")
        || [
            "/etc/localtime",
            "/etc/timezone",
            "/etc/sysconfig/clock",
            "/local state",
            "/elevation_service.exe",
            "/windows/system32/config/system",
            "/windows/system32/config/sam",
            "/windows/system32/config/security",
        ]
        .iter()
        .any(|suffix| path.ends_with(suffix))
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "lib" | "usr" | "local" | "etc" | "run" | "systemd"
        )
}

pub(crate) fn locator(record: &FileEntryRecord) -> EvidenceCandidate {
    let encryption = match record.encrypted {
        None => FileEncryptionStatus::Unknown,
        Some(true) => FileEncryptionStatus::Encrypted,
        Some(false) => FileEncryptionStatus::Clear,
    };
    let partition = record.partition_index.map(|value| value as usize);
    EvidenceCandidate {
        file_id: FileEntryId(record.id.clone()),
        data_source_id: record.data_source_id.clone(),
        partition_index: partition,
        path: record.path.clone(),
        size: record.size.unwrap_or_default(),
        encrypted: encryption.blocks_content(),
        content_identity: candidate_content_identity(
            &record.id,
            &record.data_source_id,
            partition,
            &record.path,
            record.size.unwrap_or_default(),
            encryption,
            [
                record.created.map(|value| value.to_rfc3339()),
                record.modified.map(|value| value.to_rfc3339()),
                record.accessed.map(|value| value.to_rfc3339()),
                record.changed.map(|value| value.to_rfc3339()),
                record.hash_sha256.clone(),
            ],
        ),
        companions: Vec::new(),
        modified_at: record.modified,
        evidence_kind: "analysis_preparation".into(),
        parser: "analysis.preparation".into(),
        category: "BrowserPreload".into(),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/candidates/preparation.rs"]
mod tests;
