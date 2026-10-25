//! Project Docker image references from the shared read-only Bolt worker.
use super::ExtractionOutcome;
use crate::{
    analysis_service::{
        artifact_builders::{base_attrs, make_artifact},
        candidates::EvidenceCandidate,
        error::AnalysisServiceError,
    },
    bolt_reader_service::{decode, BoltReadMode},
};
use std::{collections::BTreeMap, path::Path, sync::atomic::AtomicBool};
use transport::dto::bolt_reader::BoltReadReportDto;

pub(super) fn is_metadata_path(path: &str) -> bool {
    path.ends_with("/io.containerd.metadata.v1.bolt/meta.db")
}

pub(super) fn extract(
    conn: &rusqlite::Connection,
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    cancel: &AtomicBool,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    if bytes.len() as u64 != candidate.size {
        return Err(AnalysisServiceError::Extraction(
            "containerd metadata was truncated".into(),
        ));
    }
    let database = conn
        .path()
        .and_then(|path| Path::new(path).parent())
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            AnalysisServiceError::InvalidInput(
                "containerd decoding needs a source workspace".into(),
            )
        })?;
    let decoded = decode(
        &database.join("parser-inputs/bolt"),
        bytes,
        BoltReadMode::ContainerdImages,
        cancel,
    )?;
    let report = decoded.report();
    if report.missing_bucket == Some(0) {
        return Err(AnalysisServiceError::Unsupported(
            "containerd metadata schema is unavailable".into(),
        ));
    }
    if report.missing_bucket.is_some() {
        return Ok(ExtractionOutcome::default());
    }
    let images = images(report)?;
    let mut outcome = ExtractionOutcome::default();
    for (name, digest) in images {
        let name = std::str::from_utf8(&name).map_err(decode_error)?;
        let digest = digest.ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image target has no digest".into())
        })?;
        let digest = std::str::from_utf8(digest).map_err(decode_error)?;
        let mut attrs = base_attrs(candidate);
        attrs.insert("configKind".into(), "dockerImage".into());
        attrs.insert("name".into(), name.into());
        attrs.insert("line".into(), name.into());
        attrs.insert("value".into(), digest.into());
        attrs.insert("lineNumber".into(), 1.into());
        attrs.insert("namespace".into(), "moby".into());
        attrs.insert(
            "metadataChecksumVerified".into(),
            report.metadata_checksum_verified.into(),
        );
        attrs.insert(
            "metadataTransactionId".into(),
            report.txid.to_string().into(),
        );
        outcome.artifacts.push(make_artifact(
            "LinuxSystemConfig",
            format!("Docker image: {name}"),
            name.into(),
            candidate,
            "linux.docker_image",
            attrs,
        ));
    }
    Ok(outcome)
}

fn images(
    report: &BoltReadReportDto,
) -> Result<BTreeMap<Vec<u8>, Option<&[u8]>>, AnalysisServiceError> {
    let mut images = BTreeMap::new();
    for entry in report.entries.iter().filter(|entry| entry.is_bucket) {
        if images.len() >= 10000 {
            return Err(AnalysisServiceError::Unsupported(
                "containerd image inventory exceeds 10000 references".into(),
            ));
        }
        if images.insert(entry.key.clone(), None).is_some() {
            return Err(AnalysisServiceError::Extraction(
                "containerd image reference is duplicated".into(),
            ));
        }
    }
    for entry in report.entries.iter().filter(|entry| !entry.is_bucket) {
        let name = entry.bucket_path.get(3).ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image path is invalid".into())
        })?;
        let image = images.get_mut(name).ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image reference is missing".into())
        })?;
        let digest = entry.value.as_deref().ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image digest is missing".into())
        })?;
        if image.replace(digest).is_some() {
            return Err(AnalysisServiceError::Extraction(
                "containerd image target is duplicated".into(),
            ));
        }
    }
    Ok(images)
}

fn decode_error(error: impl std::fmt::Display) -> AnalysisServiceError {
    AnalysisServiceError::Extraction(format!("containerd metadata decode failed: {error}"))
}
