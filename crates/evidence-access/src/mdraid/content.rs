use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::DataSourceKind;
use sha2::{Digest, Sha256};

use super::{MdRaidArrayPlan, MdRaidMemberOrigin, MdRaidPlanError};
use crate::datasource_service;

mod reader;

pub const CONTENT_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MdRaidSourceContent {
    pub source_path: String,
    pub source_kind: DataSourceKind,
    pub logical_size: u64,
    pub logical_sha256: String,
    pub chunk_bytes: u32,
    pub chunk_sha256: Vec<String>,
}

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

pub(super) fn bind_content(
    plans: &mut [MdRaidArrayPlan],
    cancel: &AtomicBool,
) -> Result<(), MdRaidPlanError> {
    let mut shared = BTreeMap::new();
    for plan in plans {
        plan.source_snapshot.validate()?;
        let mut contents = Vec::new();
        for (path, kind) in input_sources(plan)? {
            if !shared.contains_key(&path) {
                shared.insert(path.clone(), hash_source(&path, &kind, cancel)?);
            }
            let content = shared
                .get(&path)
                .ok_or(MdRaidPlanError::InvalidContentIdentity)?;
            if content.source_kind != kind {
                return Err(MdRaidPlanError::InvalidContentIdentity);
            }
            contents.push(content.clone());
        }
        plan.source_snapshot.validate()?;
        plan.source_content = contents;
    }
    Ok(())
}

fn hash_source(
    path: &str,
    kind: &DataSourceKind,
    cancel: &AtomicBool,
) -> Result<MdRaidSourceContent, MdRaidPlanError> {
    if cancel.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    let mut reader = datasource_service::open_evidence_reader(Path::new(path), kind)?;
    let size = reader.info().size;
    let manifest_bytes = size
        .div_ceil(CONTENT_CHUNK_BYTES as u64)
        .checked_mul((64 + std::mem::size_of::<String>()) as u64)
        .and_then(|bytes| bytes.checked_add(CONTENT_CHUNK_BYTES as u64))
        .ok_or(MdRaidPlanError::InvalidContentIdentity)?;
    let _permit = source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: manifest_bytes,
                readers: 1,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(|error| match error {
            source_kernel::resource_budget::ResourceError::Cancelled => MdRaidPlanError::Cancelled,
            error => std::io::Error::other(error.to_string()).into(),
        })?;
    let mut digest = Sha256::new();
    let mut chunks = Vec::new();
    let mut position = 0u64;
    let mut buffer = vec![0; CONTENT_CHUNK_BYTES];
    reader.seek(SeekFrom::Start(0))?;
    while position < size {
        if cancel.load(Ordering::Acquire) {
            return Err(MdRaidPlanError::Cancelled);
        }
        let count = (size - position).min(CONTENT_CHUNK_BYTES as u64) as usize;
        reader.read_exact(&mut buffer[..count])?;
        digest.update(&buffer[..count]);
        chunks.push(hex::encode(Sha256::digest(&buffer[..count])));
        position += count as u64;
    }
    if reader.read(&mut buffer[..1])? != 0 || reader.info().size != size {
        return Err(MdRaidPlanError::InvalidContentIdentity);
    }
    if cancel.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    Ok(MdRaidSourceContent {
        source_path: path.to_string(),
        source_kind: kind.clone(),
        logical_size: size,
        logical_sha256: hex::encode(digest.finalize()),
        chunk_bytes: CONTENT_CHUNK_BYTES as u32,
        chunk_sha256: chunks,
    })
}

pub fn verify_mdraid_source_content(
    plan: &MdRaidArrayPlan,
    cancel: &AtomicBool,
) -> Result<(), MdRaidPlanError> {
    if cancel.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    validate_content_bindings(plan)?;
    plan.source_snapshot.validate()?;
    for expected in &plan.source_content {
        let actual = hash_source(&expected.source_path, &expected.source_kind, cancel)?;
        if actual != *expected {
            return Err(MdRaidPlanError::SourceContentChanged);
        }
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
        if path != &content.source_path
            || kind != &content.source_kind
            || content.chunk_bytes != CONTENT_CHUNK_BYTES as u32
            || content.logical_size.div_ceil(CONTENT_CHUNK_BYTES as u64)
                != content.chunk_sha256.len() as u64
            || !valid_digest(&content.logical_sha256)
            || !content
                .chunk_sha256
                .iter()
                .all(|digest| valid_digest(digest))
        {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
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
    Ok(Box::new(reader::ContentVerifiedReader::new(
        source,
        Arc::new(identity.clone()),
    )?))
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "../../tests/unit/mdraid_content.rs"]
mod tests;
