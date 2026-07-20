use std::collections::BTreeSet;

use super::{
    CephFsDentryKind, CephFsDirfragBatch, CephFsDirfragIdentity, CephFsInodeProjection,
    CephFsNamespaceAssemblyInput, CephFsNamespaceFreezeReason, CephFsNamespaceRecord,
};

pub(super) fn collect_namespace_records(
    input: &CephFsNamespaceAssemblyInput,
) -> Vec<CephFsNamespaceRecord> {
    input
        .batches
        .iter()
        .flat_map(|batch| batch.records.iter().cloned())
        .collect()
}

pub(super) fn append_untracked_directory_reasons(
    records: &[CephFsNamespaceRecord],
    expected: &BTreeSet<CephFsDirfragIdentity>,
    reasons: &mut Vec<CephFsNamespaceFreezeReason>,
) {
    let expected_directory_inodes = expected
        .iter()
        .map(|identity| identity.inode)
        .collect::<BTreeSet<_>>();
    for inode in records.iter().filter_map(directory_inode) {
        if !expected_directory_inodes.contains(&inode) {
            reasons.push(CephFsNamespaceFreezeReason::UntrackedDirectory { inode });
        }
    }
}

fn directory_inode(record: &CephFsNamespaceRecord) -> Option<u64> {
    match (&record.dentry.kind, &record.dentry.inode) {
        (CephFsDentryKind::Primary, Some(inode))
            if record.dentry.key.is_head() && inode.is_directory() =>
        {
            Some(inode.ino)
        }
        _ => None,
    }
}

pub(super) fn append_backtrace_reasons(
    input: &CephFsNamespaceAssemblyInput,
    records: &[CephFsNamespaceRecord],
    reasons: &mut Vec<CephFsNamespaceFreezeReason>,
) {
    for batch in input
        .batches
        .iter()
        .filter(|batch| batch.identity.inode != input.root_inode.ino)
    {
        if let Some(proof) = &batch.parent_proof {
            if !has_matching_backtrace(batch, proof, records) {
                reasons.push(CephFsNamespaceFreezeReason::UnmatchedBacktrace(
                    batch.identity.clone(),
                ));
            }
        }
    }
}

fn has_matching_backtrace(
    batch: &CephFsDirfragBatch,
    proof: &super::CephFsDirfragParentProof,
    records: &[CephFsNamespaceRecord],
) -> bool {
    records.iter().any(|record| {
        record.dentry.key.is_head()
            && matches!(record.dentry.kind, CephFsDentryKind::Primary)
            && record.dentry.child_inode == batch.identity.inode
            && record
                .dentry
                .inode
                .as_ref()
                .is_some_and(CephFsInodeProjection::is_directory)
            && record.parent.inode == proof.parent_inode
            && record.parent.fragment == proof.parent_fragment
            && record.dentry.key.name == proof.name
    })
}
