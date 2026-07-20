use std::{collections::BTreeMap, path::Path};

use domain::{CaseId, DataSource};

use crate::ceph_reconstruction::{CephFsDescriptor, CephFsPresenceAssessment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CephFsSourceCapability {
    MetadataOnly,
    MetadataBrowseable,
    BoundedPreview,
}

impl CephFsSourceCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata-only",
            Self::MetadataBrowseable => "metadata-browseable",
            Self::BoundedPreview => "bounded-preview",
        }
    }
}

pub struct CephFsSourceMaterializationRequest<'a> {
    pub case_conn: &'a rusqlite::Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub cluster_id: &'a str,
    pub presence: &'a CephFsPresenceAssessment,
    pub descriptor: &'a CephFsDescriptor,
    pub namespace_assembly_input: &'a ceph_wire::CephFsNamespaceAssemblyInput,
    pub namespace_input_sha256: &'a str,
    pub journal_boundary_sha256: Option<&'a str>,
    pub inline_data_by_inode: &'a BTreeMap<u64, Vec<u8>>,
    pub sparse_extents_by_inode:
        &'a BTreeMap<u64, Vec<crate::ceph_reconstruction::CephFsSparseExtentProof>>,
    pub expected_replica_count: usize,
}

#[derive(Debug, Clone)]
pub struct MaterializedCephFsSource {
    pub data_source: DataSource,
    pub file_count: u64,
    pub directory_count: u64,
    pub total_size: u64,
    pub catalog_digest: String,
    pub capability: CephFsSourceCapability,
    pub published: bool,
}
