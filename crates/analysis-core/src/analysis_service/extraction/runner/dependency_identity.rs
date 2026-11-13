use super::super::reader::CandidateSource;
use crate::analysis_service::{
    cancellation::ensure_not_cancelled,
    candidates::{
        normalize_evidence_path, preparation::locator, EvidenceCandidate, PreparationCatalog,
    },
    AnalysisServiceError, MAX_REGISTRY_ANALYSIS_BYTES,
};
use persistence_source::fjall::CatalogEntryType;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, sync::atomic::AtomicBool};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DependencyKind {
    RegistryBootKey,
    BrowserKeys,
    LinuxTimezone,
}

pub(super) fn annotate(
    candidates: &mut [EvidenceCandidate],
    catalog: &PreparationCatalog,
    password: Option<&str>,
    cancel: &AtomicBool,
    read: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<(), AnalysisServiceError> {
    let mut cache = BTreeMap::new();
    let run_identity = uuid::Uuid::new_v4().to_string();
    for candidate in candidates {
        ensure_not_cancelled(cancel)?;
        let Some(kind) = dependency_kind(candidate) else {
            continue;
        };
        let key = (
            candidate.data_source_id.clone(),
            candidate.partition_index,
            kind,
        );
        if !cache.contains_key(&key) {
            let dependencies = dependency_candidates(candidate, catalog, kind);
            let fingerprint = fingerprint_dependencies(&dependencies, &run_identity, cancel, read)?;
            cache.insert(key.clone(), fingerprint);
        }
        let mut identity = Sha256::new();
        identity.update(b"analysis-dependencies-v2");
        identity.update(candidate.content_identity.as_bytes());
        identity.update(
            cache
                .get(&key)
                .expect("dependency identity was inserted")
                .as_bytes(),
        );
        if kind == DependencyKind::BrowserKeys && password.is_some() {
            identity.update(b"run-scoped-user-secret");
            identity.update(run_identity.as_bytes());
        }
        candidate.content_identity = format!("sha256:{:x}", identity.finalize());
    }
    Ok(())
}

fn dependency_kind(candidate: &EvidenceCandidate) -> Option<DependencyKind> {
    let path = normalize_evidence_path(&candidate.path);
    match candidate.category.as_str() {
        "Registry" if path.ends_with("/sam") || path.ends_with("/security") => {
            Some(DependencyKind::RegistryBootKey)
        }
        "BrowserHistory" if path.ends_with("/cookies") || path.ends_with("/login data") => {
            Some(DependencyKind::BrowserKeys)
        }
        "LinuxArtifacts" => Some(DependencyKind::LinuxTimezone),
        _ => None,
    }
}

fn dependency_candidates(
    candidate: &EvidenceCandidate,
    catalog: &PreparationCatalog,
    kind: DependencyKind,
) -> Vec<EvidenceCandidate> {
    let mut dependencies = catalog
        .records()
        .filter(|record| {
            record.entry_type == CatalogEntryType::File
                && !record.deleted
                && record.data_source_id == candidate.data_source_id
                && record.partition_index.map(|value| value as usize) == candidate.partition_index
                && relevant(&normalize_evidence_path(&record.path), kind)
        })
        .map(locator)
        .collect::<Vec<_>>();
    dependencies.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.file_id.0.cmp(&right.file_id.0))
    });
    dependencies
}

fn relevant(path: &str, kind: DependencyKind) -> bool {
    match kind {
        DependencyKind::RegistryBootKey => ["/system", "/system.log1", "/system.log2"]
            .iter()
            .any(|suffix| path.ends_with(&format!("/windows/system32/config{suffix}"))),
        DependencyKind::BrowserKeys => {
            ["system", "sam", "security"].iter().any(|name| {
                let base = format!("/windows/system32/config/{name}");
                path.ends_with(&base)
                    || path.ends_with(&format!("{base}.log1"))
                    || path.ends_with(&format!("{base}.log2"))
            }) || path.contains("/microsoft/protect/")
                || path.contains("/crypto/systemkeys/")
                || path.ends_with("/local state")
                || path.ends_with("/elevation_service.exe")
        }
        DependencyKind::LinuxTimezone => {
            ["/etc/localtime", "/etc/timezone", "/etc/sysconfig/clock"]
                .iter()
                .any(|suffix| path.ends_with(suffix))
        }
    }
}

fn fingerprint_dependencies(
    dependencies: &[EvidenceCandidate],
    run_identity: &str,
    cancel: &AtomicBool,
    read: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<String, AnalysisServiceError> {
    let mut fingerprint = Sha256::new();
    fingerprint.update(b"analysis-dependency-input-set-v1");
    for dependency in dependencies {
        ensure_not_cancelled(cancel)?;
        fingerprint.update(dependency.content_identity.as_bytes());
        match fingerprint_input(dependency, cancel, read) {
            Ok(identity) => fingerprint.update(identity.as_bytes()),
            Err(AnalysisServiceError::Cancelled) => return Err(AnalysisServiceError::Cancelled),
            Err(_) => {
                fingerprint.update(b"dependency-unavailable");
                fingerprint.update(run_identity.as_bytes());
            }
        }
    }
    Ok(format!("{:x}", fingerprint.finalize()))
}

fn fingerprint_input(
    dependency: &EvidenceCandidate,
    cancel: &AtomicBool,
    read: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<String, AnalysisServiceError> {
    if dependency.encrypted || dependency.size > MAX_REGISTRY_ANALYSIS_BYTES as u64 {
        return Err(AnalysisServiceError::Unsupported(
            "dependency input exceeds its readable scope".into(),
        ));
    }
    let source =
        read(dependency, MAX_REGISTRY_ANALYSIS_BYTES).map_err(AnalysisServiceError::Read)?;
    let reader: Box<dyn Read> = match source {
        CandidateSource::Seekable(reader) => reader,
        CandidateSource::Reader(reader) => reader,
        CandidateSource::Bytes(bytes) => Box::new(std::io::Cursor::new(bytes)),
    };
    let mut limited = reader.take(MAX_REGISTRY_ANALYSIS_BYTES as u64 + 1);
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    let mut fingerprint = Sha256::new();
    loop {
        ensure_not_cancelled(cancel)?;
        let count = limited.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        fingerprint.update(&buffer[..count]);
    }
    if total != dependency.size || total > MAX_REGISTRY_ANALYSIS_BYTES as u64 {
        return Err(AnalysisServiceError::Read(
            "dependency input length changed".into(),
        ));
    }
    Ok(format!("{:x}", fingerprint.finalize()))
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/dependency_identity.rs"]
mod tests;
