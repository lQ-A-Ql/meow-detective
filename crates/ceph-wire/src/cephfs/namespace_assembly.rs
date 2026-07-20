use std::collections::{BTreeMap, BTreeSet};

use super::namespace_assembly_checks::{
    append_backtrace_reasons, append_untracked_directory_reasons, collect_namespace_records,
};
use super::namespace_assembly_digest::assembly_digest;
use super::{
    build_cephfs_namespace, CephFsDentryKind, CephFsDirfragIdentity, CephFsFileLayout,
    CephFsInodeKind, CephFsInodeProjection, CephFsNamespaceGraph, CephFsNamespaceRecord, S_IFDIR,
    S_IFLNK, S_IFMT, S_IFREG,
};
use crate::{CephWireError, Result};
use sha2::{Digest, Sha256};

pub const CEPHFS_NAMESPACE_ASSEMBLY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CephFsMetadataMutationState {
    Complete,
    Unknown { digest: String },
}

impl CephFsMetadataMutationState {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Unknown { .. } => "unknown",
        }
    }

    pub fn digest(&self) -> Option<&str> {
        match self {
            Self::Complete => None,
            Self::Unknown { digest } => Some(digest),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDirfragParentProof {
    pub parent_inode: u64,
    pub parent_fragment: u32,
    pub name: String,
    pub proof_sha256: String,
}

impl CephFsDirfragParentProof {
    pub fn new(
        child: &CephFsDirfragIdentity,
        parent_inode: u64,
        parent_fragment: u32,
        name: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        validate_parent_name(&name)?;
        Ok(Self {
            parent_inode,
            parent_fragment,
            proof_sha256: cephfs_backtrace_proof_sha256(
                child,
                parent_inode,
                parent_fragment,
                &name,
            ),
            name,
        })
    }

    fn validate(&self, child: &CephFsDirfragIdentity) -> Result<()> {
        validate_parent_name(&self.name)?;
        if self.parent_inode == 0 || !is_sha256(&self.proof_sha256) {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "backtrace",
                reason: "parent identity or proof digest is invalid",
            });
        }
        if self.proof_sha256
            != cephfs_backtrace_proof_sha256(
                child,
                self.parent_inode,
                self.parent_fragment,
                &self.name,
            )
        {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "backtrace",
                reason: "parent proof does not bind to the dirfrag identity",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDirfragBatch {
    pub identity: CephFsDirfragIdentity,
    pub records: Vec<CephFsNamespaceRecord>,
    pub complete: bool,
    pub parent_proof: Option<CephFsDirfragParentProof>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceAssemblyInput {
    pub root_inode: CephFsInodeProjection,
    pub expected_dirfrags: Vec<CephFsDirfragIdentity>,
    pub batches: Vec<CephFsDirfragBatch>,
    pub mutation_state: CephFsMetadataMutationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CephFsNamespaceFreezeReason {
    MissingDirfrag(CephFsDirfragIdentity),
    UntrackedDirectory { inode: u64 },
    IncompleteDirfrag(CephFsDirfragIdentity),
    MissingBacktrace(CephFsDirfragIdentity),
    UnmatchedBacktrace(CephFsDirfragIdentity),
    UnknownMetadataMutation { digest: String },
    NamespaceGraphIncomplete,
}

impl CephFsNamespaceFreezeReason {
    pub fn code(&self) -> String {
        match self {
            Self::MissingDirfrag(identity) => {
                format!("missing_dirfrag:{}:{}", identity.inode, identity.fragment)
            }
            Self::UntrackedDirectory { inode } => format!("untracked_directory:{inode}"),
            Self::IncompleteDirfrag(identity) => {
                format!(
                    "incomplete_dirfrag:{}:{}",
                    identity.inode, identity.fragment
                )
            }
            Self::MissingBacktrace(identity) => {
                format!("missing_backtrace:{}:{}", identity.inode, identity.fragment)
            }
            Self::UnmatchedBacktrace(identity) => {
                format!(
                    "unmatched_backtrace:{}:{}",
                    identity.inode, identity.fragment
                )
            }
            Self::UnknownMetadataMutation { digest } => {
                format!("unknown_metadata_mutation:{digest}")
            }
            Self::NamespaceGraphIncomplete => "namespace_graph_incomplete".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceAssembly {
    graph: CephFsNamespaceGraph,
    complete: bool,
    frozen: bool,
    freeze_reasons: Vec<CephFsNamespaceFreezeReason>,
    mutation_state: CephFsMetadataMutationState,
    assembly_sha256: String,
}

impl CephFsNamespaceAssembly {
    pub fn graph(&self) -> &CephFsNamespaceGraph {
        &self.graph
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn freeze_reasons(&self) -> &[CephFsNamespaceFreezeReason] {
        &self.freeze_reasons
    }

    pub fn mutation_state(&self) -> &CephFsMetadataMutationState {
        &self.mutation_state
    }

    pub fn assembly_sha256(&self) -> &str {
        &self.assembly_sha256
    }
}

pub fn assemble_cephfs_namespace(
    input: CephFsNamespaceAssemblyInput,
) -> Result<CephFsNamespaceAssembly> {
    validate_input(&input)?;
    let expected = input
        .expected_dirfrags
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut reasons = collect_batch_reasons(&input, &expected)?;
    append_mutation_reason(&input, &mut reasons)?;
    let records = collect_namespace_records(&input);
    append_untracked_directory_reasons(&records, &expected, &mut reasons);
    append_backtrace_reasons(&input, &records, &mut reasons);
    let mut graph = build_cephfs_namespace(input.root_inode.clone(), &records)?;
    if !graph.complete {
        reasons.push(CephFsNamespaceFreezeReason::NamespaceGraphIncomplete);
    }
    reasons.sort_by_key(freeze_reason_key);
    reasons.dedup();
    let complete = reasons.is_empty();
    graph.complete = complete;
    let assembly_sha256 = assembly_digest(&input);
    Ok(CephFsNamespaceAssembly {
        graph,
        complete,
        frozen: !complete,
        freeze_reasons: reasons,
        mutation_state: input.mutation_state,
        assembly_sha256,
    })
}

fn collect_batch_reasons(
    input: &CephFsNamespaceAssemblyInput,
    expected: &BTreeSet<CephFsDirfragIdentity>,
) -> Result<Vec<CephFsNamespaceFreezeReason>> {
    let mut batches = BTreeMap::new();
    let mut reasons = Vec::new();
    for batch in &input.batches {
        if !expected.contains(&batch.identity) {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "dirfrag",
                reason: "dirfrag batch is not part of the expected inventory",
            });
        }
        if batches.insert(batch.identity.clone(), batch).is_some() {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "dirfrag",
                reason: "duplicate dirfrag batch identity",
            });
        }
        validate_batch(batch)?;
        if !batch.complete {
            reasons.push(CephFsNamespaceFreezeReason::IncompleteDirfrag(
                batch.identity.clone(),
            ));
        }
        if batch.identity.inode != input.root_inode.ino && batch.parent_proof.is_none() {
            reasons.push(CephFsNamespaceFreezeReason::MissingBacktrace(
                batch.identity.clone(),
            ));
        }
    }
    append_missing_dirfrag_reasons(expected, &batches, &mut reasons);
    Ok(reasons)
}

fn append_missing_dirfrag_reasons(
    expected: &BTreeSet<CephFsDirfragIdentity>,
    batches: &BTreeMap<CephFsDirfragIdentity, &CephFsDirfragBatch>,
    reasons: &mut Vec<CephFsNamespaceFreezeReason>,
) {
    for identity in expected {
        if !batches.contains_key(identity) {
            reasons.push(CephFsNamespaceFreezeReason::MissingDirfrag(
                identity.clone(),
            ));
        }
    }
}

fn append_mutation_reason(
    input: &CephFsNamespaceAssemblyInput,
    reasons: &mut Vec<CephFsNamespaceFreezeReason>,
) -> Result<()> {
    if let CephFsMetadataMutationState::Unknown { digest } = &input.mutation_state {
        if !is_sha256(digest) {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "metadata_mutation",
                reason: "unknown mutation digest is not canonical SHA-256",
            });
        }
        reasons.push(CephFsNamespaceFreezeReason::UnknownMetadataMutation {
            digest: digest.clone(),
        });
    }
    Ok(())
}

pub fn cephfs_backtrace_proof_sha256(
    child: &CephFsDirfragIdentity,
    parent_inode: u64,
    parent_fragment: u32,
    name: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-backtrace/v1\0");
    digest.update(child.inode.to_le_bytes());
    digest.update(child.fragment.to_le_bytes());
    digest.update(parent_inode.to_le_bytes());
    digest.update(parent_fragment.to_le_bytes());
    digest.update((name.len() as u64).to_le_bytes());
    digest.update(name.as_bytes());
    hex::encode(digest.finalize())
}

fn validate_input(input: &CephFsNamespaceAssemblyInput) -> Result<()> {
    validate_inode_projection(&input.root_inode)?;
    if !input.root_inode.is_directory() || input.root_inode.ino == 0 {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "root",
            reason: "namespace assembly root must be a directory inode",
        });
    }
    if input.expected_dirfrags.is_empty() {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "expected_dirfrags",
            reason: "at least the root dirfrag must be expected",
        });
    }
    let mut expected = BTreeSet::new();
    for identity in &input.expected_dirfrags {
        if !expected.insert(identity.clone()) {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "expected_dirfrags",
                reason: "expected dirfrag identities are duplicated",
            });
        }
    }
    if !expected.contains(&CephFsDirfragIdentity::new(input.root_inode.ino, 0)?) {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "expected_dirfrags",
            reason: "root dirfrag is not included",
        });
    }
    Ok(())
}

fn validate_batch(batch: &CephFsDirfragBatch) -> Result<()> {
    if batch.identity.inode == 0 {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "dirfrag",
            reason: "dirfrag inode must be non-zero",
        });
    }
    let mut dentries = BTreeSet::new();
    for record in &batch.records {
        if record.parent != batch.identity {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "dirfrag.records",
                reason: "record parent identity does not match its batch",
            });
        }
        validate_dentry_name(&record.dentry.key.name)?;
        if record.dentry.child_inode == 0 {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "dirfrag.records",
                reason: "dentry child inode must be non-zero",
            });
        }
        if !dentries.insert((record.dentry.key.name.as_str(), record.dentry.key.snap_id)) {
            return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                field: "dirfrag.records",
                reason: "dentry identity is duplicated within a dirfrag batch",
            });
        }
        match (&record.dentry.kind, &record.dentry.inode) {
            (CephFsDentryKind::Primary, Some(inode)) if inode.ino == record.dentry.child_inode => {
                validate_inode_projection(inode)?;
            }
            (CephFsDentryKind::Remote { .. }, None) => {}
            _ => {
                return Err(CephWireError::InvalidCephFsNamespaceAssembly {
                    field: "dirfrag.records",
                    reason: "dentry kind, child inode, and inode payload are inconsistent",
                });
            }
        }
    }
    if let Some(proof) = &batch.parent_proof {
        proof.validate(&batch.identity)?;
    }
    Ok(())
}

fn validate_inode_projection(inode: &CephFsInodeProjection) -> Result<()> {
    if inode.ino == 0 || inode.nlink <= 0 || inode.encoded_version == 0 {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "inode",
            reason: "inode identity, link count, or encoded version is invalid",
        });
    }
    let expected_kind = match inode.mode & S_IFMT {
        S_IFREG => CephFsInodeKind::File,
        S_IFDIR => CephFsInodeKind::Directory,
        S_IFLNK => CephFsInodeKind::Symlink,
        _ => CephFsInodeKind::Other,
    };
    if inode.kind != expected_kind {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "inode.mode",
            reason: "inode kind does not match the mode type bits",
        });
    }
    CephFsFileLayout::new(
        inode.layout.stripe_unit,
        inode.layout.stripe_count,
        inode.layout.object_size,
        inode.layout.pool_id,
        inode.layout.pool_namespace.clone(),
    )
    .map_err(|_| CephWireError::InvalidCephFsNamespaceAssembly {
        field: "inode.layout",
        reason: "inode layout is invalid",
    })?;
    Ok(())
}

fn freeze_reason_key(reason: &CephFsNamespaceFreezeReason) -> String {
    format!("{reason:?}")
}

fn validate_parent_name(name: &str) -> Result<()> {
    validate_dentry_name(name).map_err(|_| CephWireError::InvalidCephFsNamespaceAssembly {
        field: "backtrace.name",
        reason: "parent name is empty, unsafe, or exceeds the CephFS limit",
    })
}

fn validate_dentry_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 255
        || name.contains('\0')
        || name.contains('/')
        || matches!(name, "." | "..")
    {
        return Err(CephWireError::InvalidCephFsNamespaceAssembly {
            field: "dirfrag.records",
            reason: "dentry name is empty, unsafe, or exceeds the CephFS limit",
        });
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
