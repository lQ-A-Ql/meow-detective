mod cephfs;
mod cephfs_presence;
mod cephfs_presence_storage;
mod cephfs_presence_validation;
mod derived_finalizer;
mod derived_reader;
mod derived_runtime;
mod derived_source;
mod rados_provider;
mod rados_reader;
mod rbd_catalog;
mod rbd_image;
mod rbd_provider;
mod rbd_reader;
mod rbd_service;
mod source_bound_lvm;

pub const STRICT_RBD_REPLICA_COUNT: usize = 3;

pub use cephfs::{
    bind_cephfs_descriptors, discover_cephfs_journal_ranks, inventory_cephfs_metadata_pool,
    merge_cephfs_metadata_inventories, persist_cephfs_journal_replay, replay_cephfs_journal,
    CephFsBindingError, CephFsDescriptor, CephFsDescriptorState, CephFsInventoryError,
    CephFsJournalDiscoveryError, CephFsJournalFramingStatus, CephFsJournalNamespaceStopReason,
    CephFsJournalPersistenceError, CephFsJournalPersistenceOutcome, CephFsJournalRankCandidate,
    CephFsJournalReplay, CephFsJournalReplayError, CephFsJournalReplayEvent,
    CephFsJournalReplayLimits, CephFsJournalSourceSpan, CephFsJournalStopReason, CephFsMapEvidence,
    CephFsMapProvenance, CephFsMergedMetadataInventory, CephFsMergedMetadataObject,
    CephFsObjectLocator, CephFsObjectMetadata, CephFsObjectProvenance, CephFsObjectRange,
    CephFsObjectRangeReader, CephFsObjectReadError, CephFsObjectReadProvenance, CephFsObjectSource,
    CephFsPoolBinding, CephFsPoolEvidence, CephFsPoolProvenance, CephFsPoolRole, CephFsRankBinding,
    SourceDbCephFsObjectReader, CEPHFS_HEAD_SNAP_HEX, MAX_CEPHFS_OBJECT_RANGE_LENGTH,
};
pub use cephfs_presence::{
    assess_cephfs_presence, assess_cephfs_presence_for_cluster, CephFsFilesystemPresenceRecord,
    CephFsMapPresenceSnapshot, CephFsMdsFilesystemPresenceRecord, CephFsMdsMapPresenceSnapshot,
    CephFsPresenceAssessment, CephFsPresenceDiagnostic, CephFsPresenceError,
    CephFsPresenceEvidence, CephFsPresenceMapKind, CephFsPresenceState, FSMAP_PRESENCE_KEY,
    MDSMAP_PRESENCE_KEY,
};
pub use derived_reader::{open_derived_rbd_reader, DerivedRbdReaderError};
pub use derived_runtime::{build_derived_rbd_runtime, load_lineage_fingerprint, DerivedRbdRuntime};
pub(crate) use derived_source::CATALOG_MATERIALIZER_VERSION;
pub(crate) use derived_source::{derived_catalog_fingerprint, load_derived_catalog_fingerprint};
pub use derived_source::{
    finalize_rbd_source_processing, finalize_rbd_source_processing_with_cancel,
    materialize_rbd_sources_for_cluster, materialize_rbd_sources_for_cluster_with_cancel,
    verify_derived_source_catalog, DerivedSourceError, MaterializedRbdSource,
};
pub(super) use rados_provider::SharedRadosObjectProvider;
pub use rados_provider::{
    BluestoreDeviceOpener, FilesystemBluestoreDeviceOpener, RadosProviderError,
    RadosProviderReadMetrics, RadosReplicaSource, SourceDbRadosObjectProvider,
};
pub use rados_reader::{RadosObjectReader, RadosReadError};
pub use rbd_catalog::{discover_rbd_images, RbdCatalogError, RbdImageDescriptor};
pub use rbd_image::{
    build_rbd_head_layout, detect_rbd_image_filesystem, open_rbd_head_image, RbdImageError,
};
pub use rbd_provider::{
    RbdObjectProvider, RbdObjectProviderError, RbdObjectReadOutcome, RbdObjectReadRequest,
};
pub use rbd_reader::{
    RbdEvidenceReader, RbdReadContext, RbdReadError, RBD_FEATURES_ALL, RBD_FEATURE_DATA_POOL,
    RBD_FEATURE_DEEP_FLATTEN, RBD_FEATURE_DIRTY_CACHE, RBD_FEATURE_EXCLUSIVE_LOCK,
    RBD_FEATURE_FAST_DIFF, RBD_FEATURE_JOURNALING, RBD_FEATURE_LAYERING, RBD_FEATURE_MIGRATING,
    RBD_FEATURE_NON_PRIMARY, RBD_FEATURE_OBJECT_MAP, RBD_FEATURE_OPERATIONS,
    RBD_FEATURE_STRIPINGV2, RBD_OPERATION_FEATURE_CLONE_CHILD, RBD_OPERATION_FEATURE_CLONE_PARENT,
    RBD_OPERATION_FEATURE_GROUP, RBD_OPERATION_FEATURE_SNAP_TRASH,
};
pub use rbd_service::{
    detect_rbd_image_from_source_dbs, discover_rbd_images_from_source_dbs, RbdReconstructionError,
};
pub use source_bound_lvm::{
    open_source_bound_bluestore_lvm, BoundEvidenceOpenError, FilesystemEvidenceOpener,
    NeedReassociateReason, SourceBoundEvidenceOpener, SourceBoundLvmError,
};
