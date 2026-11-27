use std::{
    collections::BTreeMap,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::DataSourceKind;

use super::{MdRaidArrayPlan, MdRaidMemberOrigin, MdRaidPlanError};
use crate::datasource_service;
use crate::source_runtime::content::{ContentError, ContentVerifiedReader, CONTENT_CHUNK_BYTES};

pub fn mdraid_reader_content_bytes(plan: &MdRaidArrayPlan) -> u64 {
    let manifest = plan
        .source_content
        .iter()
        .map(|content| {
            (content.chunk_sha256.len() * std::mem::size_of::<String>()) as u64
                + content
                    .chunk_sha256
                    .iter()
                    .map(|digest| digest.len() as u64)
                    .sum::<u64>()
        })
        .sum::<u64>();
    plan.members
        .iter()
        .map(|member| {
            let count = match &member.origin {
                MdRaidMemberOrigin::Direct { .. } => 1,
                MdRaidMemberOrigin::Lvm { identity, .. } => identity.pv_sources.len(),
            };
            (CONTENT_CHUNK_BYTES as u64 + manifest).saturating_mul(count as u64)
        })
        .sum()
}

pub(super) fn input_sources(
    plan: &MdRaidArrayPlan,
) -> Result<BTreeMap<String, DataSourceKind>, MdRaidPlanError> {
    let mut sources = BTreeMap::new();
    for member in &plan.members {
        let inputs = match &member.origin {
            MdRaidMemberOrigin::Direct {
                source_path,
                source_kind,
                ..
            } => {
                vec![(source_path.as_str(), source_kind)]
            }
            MdRaidMemberOrigin::Lvm { identity, .. } => identity
                .pv_sources
                .iter()
                .map(|source| {
                    Ok((
                        source.source_path.as_str(),
                        source
                            .source_kind
                            .as_ref()
                            .ok_or(MdRaidPlanError::InvalidContentIdentity)?,
                    ))
                })
                .collect::<Result<Vec<_>, MdRaidPlanError>>()?,
        };
        for (path, kind) in inputs {
            let path = std::fs::canonicalize(path)?.to_string_lossy().into_owned();
            if sources
                .insert(path, kind.clone())
                .is_some_and(|known| known != *kind)
            {
                return Err(MdRaidPlanError::InvalidContentIdentity);
            }
        }
    }
    Ok(sources)
}

pub fn verify_mdraid_source_content(
    plan: &MdRaidArrayPlan,
    cancel: &AtomicBool,
) -> Result<(), MdRaidPlanError> {
    validate_content_bindings(plan)?;
    plan.source_snapshot.validate()?;
    for expected in &plan.source_content {
        expected.verify(cancel).map_err(map_content_error)?;
    }
    plan.source_snapshot.validate()?;
    Ok(())
}

pub(super) fn validate_content_bindings(plan: &MdRaidArrayPlan) -> Result<(), MdRaidPlanError> {
    let inputs = input_sources(plan)?;
    if inputs.len() != plan.source_content.len() || inputs.is_empty() {
        return Err(MdRaidPlanError::InvalidContentIdentity);
    }
    for ((path, kind), content) in inputs.iter().zip(&plan.source_content) {
        if path != &content.source_path || kind != &content.source_kind {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        content.validate().map_err(map_content_error)?;
    }
    Ok(())
}

pub(super) fn open_verified_source(
    plan: &MdRaidArrayPlan,
    path: &Path,
    kind: &DataSourceKind,
) -> Result<Box<dyn evidence_core::EvidenceReader>, MdRaidPlanError> {
    let canonical = path.canonicalize()?.to_string_lossy().into_owned();
    let identity = plan
        .source_content
        .iter()
        .find(|content| content.source_path == canonical && content.source_kind == *kind)
        .ok_or(MdRaidPlanError::InvalidContentIdentity)?;
    let source = datasource_service::open_evidence_reader(path, kind)?;
    Ok(Box::new(ContentVerifiedReader::new(
        source,
        Arc::new(identity.clone()),
    )?))
}

fn map_content_error(error: ContentError) -> MdRaidPlanError {
    match error {
        ContentError::Cancelled => MdRaidPlanError::Cancelled,
        ContentError::Changed => MdRaidPlanError::SourceContentChanged,
        ContentError::InvalidIdentity => MdRaidPlanError::InvalidContentIdentity,
        ContentError::Io(error) => MdRaidPlanError::Io(error),
        error => MdRaidPlanError::Io(std::io::Error::other(error)),
    }
}
