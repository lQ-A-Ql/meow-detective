use super::common::{normalize_evidence_path, EvidenceCandidate, EvidenceCompanion};
use crate::analysis_service::error::AnalysisServiceError;
use sha2::{Digest, Sha256};
use std::collections::{hash_map::Entry, HashMap};

type CompanionKey = (String, Option<usize>, String);

#[derive(Default)]
pub(super) struct RegistryCompanions(HashMap<CompanionKey, Option<EvidenceCompanion>>);

impl RegistryCompanions {
    pub(super) fn matches(path: &str) -> bool {
        let path = normalize_evidence_path(path);
        path.ends_with(".log1") || path.ends_with(".log2")
    }

    pub(super) fn insert(
        &mut self,
        source: &str,
        partition: Option<usize>,
        companion: EvidenceCompanion,
    ) {
        let key = (
            source.into(),
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
        let mut identity = Sha256::new();
        identity.update(b"registry-page-log-input-v1");
        identity.update(candidate.content_identity.as_bytes());
        for suffix in [".log1", ".log2"] {
            let key = (
                candidate.data_source_id.clone(),
                candidate.partition_index,
                format!("{}{suffix}", normalize_evidence_path(&candidate.path)),
            );
            let Some(companion) = self.0.get(&key) else {
                continue;
            };
            let companion = companion.as_ref().ok_or_else(|| {
                AnalysisServiceError::InvalidInput("registry companion path is ambiguous".into())
            })?;
            identity.update(suffix.as_bytes());
            identity.update(companion.content_identity.as_bytes());
            candidate.companions.push(companion.clone());
        }
        candidate.content_identity = format!("sha256:{:x}", identity.finalize());
        Ok(())
    }
}
