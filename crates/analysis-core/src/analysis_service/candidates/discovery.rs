//! Magic-based candidate discovery: definition matching and bounded probe state.

use domain::FileEntryId;
use persistence_source::fjall::FileEntryRecord;

use super::common::{
    evidence_path_matches, EvidenceCandidate, EvidenceCategoryDef, EvidenceDiscovery,
};
use super::definitions::IMAGE_METADATA_PROBE_CATEGORY;

pub(super) type MagicProbe<'p> =
    &'p mut dyn FnMut(&FileEntryRecord) -> Result<Option<&'static str>, String>;

pub(super) struct MagicDiscovery<'p> {
    types: Vec<&'static str>,
    probe: MagicProbe<'p>,
    probed: u64,
    matched: u64,
}

impl<'p> MagicDiscovery<'p> {
    pub(super) fn new(definitions: &[EvidenceCategoryDef], probe: MagicProbe<'p>) -> Self {
        let mut types = Vec::new();
        for definition in definitions {
            if let EvidenceDiscovery::Magic(known) = definition.discovery {
                for file_type in known {
                    if !types.contains(file_type) {
                        types.push(*file_type);
                    }
                }
            }
        }
        Self {
            types,
            probe,
            probed: 0,
            matched: 0,
        }
    }

    pub(super) fn probe(&mut self, entry: &FileEntryRecord) -> Option<&'static str> {
        if self.types.is_empty()
            || entry.size.unwrap_or_default() == 0
            || entry.encrypted == Some(true)
        {
            return None;
        }
        self.probed += 1;
        match (self.probe)(entry) {
            Ok(file_type) => {
                let matched =
                    file_type.filter(|file_type| self.types.iter().any(|known| known == file_type));
                if matched.is_some() {
                    self.matched += 1;
                }
                matched
            }
            Err(error) => {
                tracing::debug!(
                    path = %entry.path,
                    error = %error,
                    "image magic probe failed"
                );
                None
            }
        }
    }

    pub(super) fn log_summary(&self) {
        if self.probed > 0 {
            tracing::info!(
                probed = self.probed,
                matched = self.matched,
                "image magic discovery complete"
            );
        }
    }
}

pub(super) fn definition_matches(
    definition: &EvidenceCategoryDef,
    normalized: &str,
    magic_file_type: Option<&'static str>,
) -> bool {
    match definition.discovery {
        EvidenceDiscovery::PathPatterns => path_discovery_matches(definition, normalized),
        EvidenceDiscovery::Magic(types) => {
            magic_file_type.is_some_and(|file_type| types.contains(&file_type))
        }
    }
}

pub(super) fn path_discovery_matches(definition: &EvidenceCategoryDef, normalized: &str) -> bool {
    !definition.patterns.is_empty()
        && evidence_path_matches(normalized, definition.patterns)
        && definition.matcher.is_none_or(|matcher| matcher(normalized))
}

pub(crate) fn magic_probe_candidate(entry: &FileEntryRecord) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(entry.id.clone()),
        data_source_id: entry.data_source_id.clone(),
        partition_index: entry.partition_index.map(|value| value as usize),
        path: entry.path.clone(),
        size: entry.size.unwrap_or_default(),
        encrypted: entry.encrypted == Some(true),
        content_identity: String::new(),
        companions: Vec::new(),
        evidence_kind: "image_magic_probe".to_string(),
        parser: "image.magic".to_string(),
        category: IMAGE_METADATA_PROBE_CATEGORY.to_string(),
        modified_at: entry.modified,
    }
}
