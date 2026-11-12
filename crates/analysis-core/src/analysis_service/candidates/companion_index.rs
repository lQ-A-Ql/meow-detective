use super::common::{normalize_evidence_path, EvidenceCandidate, EvidenceCompanion};
use crate::analysis_service::AnalysisServiceError;
use sha2::{Digest, Sha256};
use std::collections::{hash_map::Entry, HashMap};

#[derive(Default)]
pub(super) struct CompanionIndex(
    HashMap<(String, Option<usize>, String), Option<EvidenceCompanion>>,
);

impl CompanionIndex {
    pub(super) fn matches(path: &str) -> bool {
        let path = normalize_evidence_path(path);
        [".log1", ".log2", "-wal", "-shm"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
    }

    pub(super) fn insert(
        &mut self,
        source: &str,
        partition: Option<usize>,
        companion: EvidenceCompanion,
    ) {
        let key = (
            source.to_string(),
            partition,
            normalize_evidence_path(&companion.path),
        );
        match self.0.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(Some(companion));
            }
            Entry::Occupied(mut entry) => {
                entry.insert(None);
            }
        }
    }

    pub(super) fn attach(
        &self,
        candidate: &mut EvidenceCandidate,
    ) -> Result<(), AnalysisServiceError> {
        let (domain, suffixes): (&[u8], &[&str]) = match candidate.category.as_str() {
            "Registry" => (b"registry-page-log-input-v1", &[".log1", ".log2"]),
            "BrowserHistory" => (b"browser-sqlite-input-v1", &["-wal", "-shm"]),
            _ => return Ok(()),
        };
        let mut identity = Sha256::new();
        identity.update(domain);
        identity.update(candidate.content_identity.as_bytes());
        for suffix in suffixes {
            let key = (
                candidate.data_source_id.clone(),
                candidate.partition_index,
                format!("{}{suffix}", normalize_evidence_path(&candidate.path)),
            );
            let Some(companion) = self.0.get(&key) else {
                continue;
            };
            let companion = companion.as_ref().ok_or_else(|| {
                AnalysisServiceError::InvalidInput("analysis companion path is ambiguous".into())
            })?;
            identity.update(suffix.as_bytes());
            identity.update(companion.content_identity.as_bytes());
            candidate.companions.push(companion.clone());
        }
        candidate.content_identity = format!("sha256:{:x}", identity.finalize());
        Ok(())
    }
}
