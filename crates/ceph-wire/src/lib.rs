//! Read-only primitives for Ceph's little-endian wire encoding.

pub mod bluefs;
pub mod bluefs_transaction;
pub mod bluestore;
pub mod bluestore_omap;
pub mod bluestore_semantic;
pub mod cephfs;
pub mod codec;
pub mod crc32c;
pub mod cursor;
pub mod error;
pub mod rbd;

pub use bluefs::{
    decode_bluefs_super_block, BluefsExtent, BluefsFnode, BluefsLayout, BluefsSuper,
    BLUEFS_MAX_EXTENTS, BLUEFS_SUPER_BLOCK_SIZE, BLUEFS_SUPER_OFFSET,
};
pub use bluefs_transaction::{
    decode_bluefs_transaction, inspect_bluefs_transaction, BluefsFnodeDelta, BluefsOperation,
    BluefsTransaction, BluefsTransactionPrefix, BLUEFS_MAX_OPERATIONS, BLUEFS_MAX_OPERATION_BYTES,
};
pub use bluestore::{
    decode_bdev_label_block, select_bdev_label, select_bdev_labels, BdevLabel, BdevLabelCandidate,
    BdevLabelSelection, BDEV_FIRST_LABEL_POSITION, BDEV_LABEL_BLOCK_SIZE, BDEV_LABEL_MAGIC,
    BDEV_LABEL_POSITIONS, BDEV_LABEL_PREFIX_LENGTH,
};
pub use bluestore_omap::{
    decode_bluestore_omap_key, decode_bluestore_omap_logical_key, decode_bluestore_raw_omap_key,
    BlueStoreOmapKey, BlueStoreOmapKeyFamily, BlueStoreOmapKeyKind, BlueStoreOmapPool,
};
pub use bluestore_semantic::{
    decode_bluestore_extent_payload, decode_bluestore_latest_value,
    decode_bluestore_latest_value_with_spanning_blobs, decode_bluestore_object_key,
    BlueStoreAllocationHints, BlueStoreAttributeSummary, BlueStoreBlob, BlueStoreBlobFlags,
    BlueStoreBlobIdentity, BlueStoreBlobUseRef, BlueStoreBlobUseTracker, BlueStoreChecksum,
    BlueStoreChecksumType, BlueStoreCnode, BlueStoreCollectionId, BlueStoreCollectionKind,
    BlueStoreCollectionRecord, BlueStoreDecodedRecord, BlueStoreDeferred, BlueStoreDeferredReason,
    BlueStoreExtentFlags, BlueStoreExtentPayload, BlueStoreExtentShardDescriptor,
    BlueStoreExtentStorage, BlueStoreKeySpace, BlueStoreLogicalExtent, BlueStoreObjectId,
    BlueStoreObjectKey, BlueStoreObjectRecord, BlueStoreOmapMode, BlueStoreOnodeFlags,
    BlueStoreOnodeHeader, BlueStoreOnodeTail, BlueStorePhysicalExtent, BlueStoreSemanticLimits,
    BlueStoreSharedBlobExtentRef, BlueStoreSharedBlobRecord, BlueStoreSuperRecord,
    BlueStoreZoneOffsetRef,
};
pub use cephfs::{
    assemble_cephfs_namespace, build_cephfs_namespace, cephfs_backtrace_proof_sha256,
    classify_cephfs_metadata_object_name, decode_ceph_fs_map, decode_ceph_mds_map,
    decode_cephfs_dentry_key, decode_cephfs_dentry_value, decode_cephfs_file_layout,
    decode_cephfs_inode_object, decode_cephfs_inode_store, decode_cephfs_inode_t_prefix,
    decode_cephfs_journal_frame, decode_cephfs_journal_frame_prefix, decode_cephfs_journal_header,
    decode_cephfs_journal_pointer, format_cephfs_data_object_name,
    format_cephfs_journal_data_object_name, format_cephfs_journal_pointer_object_name,
    plan_cephfs_journal_range, CephFsDentryKey, CephFsDentryKind, CephFsDentryProjection,
    CephFsDirfragBatch, CephFsDirfragIdentity, CephFsDirfragParentProof, CephFsFileLayout,
    CephFsFilesystem, CephFsInodeKind, CephFsInodeProjection, CephFsJournalEvent,
    CephFsJournalEventEncoding, CephFsJournalEventKind, CephFsJournalFrame,
    CephFsJournalFramePrefix, CephFsJournalHeader, CephFsJournalLayout, CephFsJournalObjectExtent,
    CephFsJournalPointer, CephFsJournalStreamFormat, CephFsLayoutSegment, CephFsMap,
    CephFsMetadataMutationState, CephFsMetadataObjectCandidates, CephFsMetadataObjectClass,
    CephFsNamespaceAssembly, CephFsNamespaceAssemblyInput, CephFsNamespaceDiagnostic,
    CephFsNamespaceEntry, CephFsNamespaceEntryKind, CephFsNamespaceFreezeReason,
    CephFsNamespaceGraph, CephFsNamespaceRecord, CephFsRankTableKind, CephMdsDaemon, CephMdsMap,
    CephMdsState, CEPHFS_JOURNAL_MAGIC, CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    CEPHFS_NAMESPACE_ASSEMBLY_VERSION, CEPH_FS_ONDISK_MAGIC, CEPH_NOSNAP, S_IFDIR, S_IFLNK, S_IFMT,
    S_IFREG,
};
pub use codec::{
    decode_lba_u64, decode_varint_lowz_u64, decode_varint_u64, CephDecode, CephEncode,
    CephStringMap, CephStructEnvelope, CephUtime,
};
pub use cursor::CephCursor;
pub use error::{CephWireError, Result};
pub use rbd::{
    decode_rbd_data_pool_id, decode_rbd_features, decode_rbd_id, decode_rbd_name,
    decode_rbd_object_prefix, decode_rbd_order, decode_rbd_size, decode_rbd_string,
    decode_rbd_stripe_count, decode_rbd_stripe_unit, format_rbd_data_object_name,
    RbdHeadImageLayout, RbdImageMetadata, RbdReadPlan, RBD_HEAD_SNAP_HEX, RBD_MAX_IMAGE_ID_LENGTH,
    RBD_MAX_IMAGE_NAME_LENGTH, RBD_MAX_OBJECT_PREFIX_LENGTH, RBD_MAX_ORDER, RBD_MIN_ORDER,
};
