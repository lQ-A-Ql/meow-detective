use super::namespace_assembly::CephFsNamespaceAssemblyInput;
use super::{CephFsDentryKind, CephFsInodeKind, CephFsInodeProjection, CephFsNamespaceRecord};
use sha2::{Digest, Sha256};

pub(super) fn assembly_digest(input: &CephFsNamespaceAssemblyInput) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-namespace-assembly/v1\0");
    digest_inode(&mut digest, &input.root_inode);
    let mut expected = input.expected_dirfrags.clone();
    expected.sort();
    digest.update((expected.len() as u64).to_le_bytes());
    for identity in expected {
        digest.update(identity.inode.to_le_bytes());
        digest.update(identity.fragment.to_le_bytes());
    }
    let mut batches = input.batches.iter().collect::<Vec<_>>();
    batches.sort_by_key(|batch| batch.identity.clone());
    digest.update((batches.len() as u64).to_le_bytes());
    for batch in batches {
        digest.update(batch.identity.inode.to_le_bytes());
        digest.update(batch.identity.fragment.to_le_bytes());
        digest.update([u8::from(batch.complete)]);
        digest.update([u8::from(batch.parent_proof.is_some())]);
        if let Some(proof) = &batch.parent_proof {
            digest.update(proof.parent_inode.to_le_bytes());
            digest.update(proof.parent_fragment.to_le_bytes());
            field(&mut digest, &proof.name);
            field(&mut digest, &proof.proof_sha256);
        }
        let mut records = batch.records.iter().collect::<Vec<_>>();
        records.sort_by_key(|record| {
            (
                record.dentry.key.name.clone(),
                record.dentry.key.snap_id,
                record.dentry.child_inode,
            )
        });
        digest.update((records.len() as u64).to_le_bytes());
        for record in records {
            digest_record(&mut digest, record);
        }
    }
    match &input.mutation_state {
        super::namespace_assembly::CephFsMetadataMutationState::Complete => digest.update([0]),
        super::namespace_assembly::CephFsMetadataMutationState::Unknown { digest: value } => {
            digest.update([1]);
            field(&mut digest, value);
        }
    }
    hex::encode(digest.finalize())
}

fn digest_record(digest: &mut Sha256, record: &CephFsNamespaceRecord) {
    digest.update(record.parent.inode.to_le_bytes());
    digest.update(record.parent.fragment.to_le_bytes());
    field(digest, &record.dentry.key.name);
    digest.update(record.dentry.key.snap_id.to_le_bytes());
    digest.update(record.dentry.first_snap.to_le_bytes());
    match record.dentry.kind {
        CephFsDentryKind::Primary => digest.update([0]),
        CephFsDentryKind::Remote { d_type } => digest.update([1, d_type]),
    }
    digest.update(record.dentry.child_inode.to_le_bytes());
    field(digest, &record.dentry.alternate_name);
    match &record.dentry.inode {
        None => digest.update([0]),
        Some(inode) => {
            digest.update([1]);
            digest_inode(digest, inode);
        }
    }
}

fn digest_inode(digest: &mut Sha256, inode: &CephFsInodeProjection) {
    digest.update(inode.ino.to_le_bytes());
    digest.update(inode.mode.to_le_bytes());
    digest.update(inode.uid.to_le_bytes());
    digest.update(inode.gid.to_le_bytes());
    digest.update(inode.nlink.to_le_bytes());
    digest.update(inode.size.to_le_bytes());
    digest.update([match inode.kind {
        CephFsInodeKind::File => 0,
        CephFsInodeKind::Directory => 1,
        CephFsInodeKind::Symlink => 2,
        CephFsInodeKind::Other => 3,
    }]);
    digest.update(inode.layout.stripe_unit.to_le_bytes());
    digest.update(inode.layout.stripe_count.to_le_bytes());
    digest.update(inode.layout.object_size.to_le_bytes());
    digest.update(inode.layout.pool_id.to_le_bytes());
    field(digest, &inode.layout.pool_namespace);
    digest.update([inode.encoded_version]);
    digest.update((inode.remaining_inode_bytes as u64).to_le_bytes());
}

fn field(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value.as_bytes());
}
