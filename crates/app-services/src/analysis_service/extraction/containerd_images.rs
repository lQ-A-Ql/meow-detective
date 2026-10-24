//! Project Docker's containerd image references from its read-only Bolt store.
use super::ExtractionOutcome;
use crate::analysis_service::{
    artifact_builders::{base_attrs, make_artifact},
    candidates::EvidenceCandidate,
    error::AnalysisServiceError,
};
use bolt_lite::Bolt;
use std::{io::Write, path::Path};

pub(super) fn is_metadata_path(path: &str) -> bool {
    path.ends_with("/io.containerd.metadata.v1.bolt/meta.db")
}

pub(super) fn extract(
    conn: &rusqlite::Connection,
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    if bytes.len() as u64 != candidate.size {
        return Err(AnalysisServiceError::Extraction(
            "containerd metadata was truncated".into(),
        ));
    }
    let database = conn
        .path()
        .and_then(|path| Path::new(path).parent())
        .ok_or_else(|| {
            AnalysisServiceError::InvalidInput(
                "containerd decoding needs a source workspace".into(),
            )
        })?;
    let cache = database.join("parser-inputs");
    std::fs::create_dir_all(&cache)?;
    let mut copy = tempfile::NamedTempFile::new_in(cache)?;
    copy.write_all(bytes)?;
    copy.flush()?;
    let db = Bolt::open_ro(copy.path()).map_err(decode_error)?;
    let tx = db.begin().map_err(decode_error)?;
    let schema = tx.bucket(b"v1").ok_or_else(|| {
        AnalysisServiceError::Unsupported("containerd metadata schema is unavailable".into())
    })?;
    let mut outcome = ExtractionOutcome::default();
    let Some(namespace) = schema.bucket(b"moby") else {
        return Ok(outcome);
    };
    let Some(images) = namespace.bucket(b"images") else {
        return Ok(outcome);
    };
    for entry in images.cursor().map_err(decode_error)? {
        if entry.flags & bolt_lite::BUCKET_VALUE_FLAG == 0 {
            continue;
        }
        let image = images.bucket(&entry.key).ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image bucket is invalid".into())
        })?;
        if outcome.artifacts.len() >= 10000 {
            return Err(AnalysisServiceError::Unsupported(
                "containerd image inventory exceeds 10000 references".into(),
            ));
        }
        let name = std::str::from_utf8(&entry.key).map_err(decode_error)?;
        let target = image.bucket(b"target").ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image has no target".into())
        })?;
        let digest = target.get(b"digest").ok_or_else(|| {
            AnalysisServiceError::Extraction("containerd image target has no digest".into())
        })?;
        let digest = std::str::from_utf8(&digest).map_err(decode_error)?;
        let mut attrs = base_attrs(candidate);
        attrs.insert("configKind".into(), "dockerImage".into());
        attrs.insert("name".into(), name.into());
        attrs.insert("line".into(), name.into());
        attrs.insert("value".into(), digest.into());
        attrs.insert("lineNumber".into(), 1.into());
        attrs.insert("namespace".into(), "moby".into());
        attrs.insert("metadataChecksumVerified".into(), false.into());
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

fn decode_error(error: impl std::fmt::Display) -> AnalysisServiceError {
    AnalysisServiceError::Extraction(format!("containerd metadata decode failed: {error}"))
}
