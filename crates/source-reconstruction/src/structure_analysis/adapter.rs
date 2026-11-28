use std::collections::{BTreeMap, BTreeSet};

use domain::structure::{AdapterOperation, AddressSpace, StructureAdapterDescriptor};

use super::request::StructureAnalysisError;

pub const MAX_ADAPTERS: usize = 64;
pub const MDRAID_ADAPTER_ID: &str = "mdraid";
pub const MDRAID_ADAPTER_VERSION: &str = "fs-lvm/mdraid-v3";
pub const FILESYSTEM_ADAPTER_ID: &str = "filesystem-window";
pub const FILESYSTEM_ADAPTER_VERSION: &str = "evidence-access/filesystem-window-v1";
pub const COLLECTION_ADAPTER_ID: &str = "collection-source";
pub const COLLECTION_ADAPTER_VERSION: &str = "collection-source-v2";
pub const LVM_ADAPTER_ID: &str = "lvm-volume";
pub const RBD_ADAPTER_ID: &str = "ceph-rbd-head";
pub const CEPHFS_ADAPTER_ID: &str = "cephfs-file";
pub const REGISTERED_SOURCE_ADAPTER_ID: &str = "registered-source";

pub struct AdapterRegistry {
    adapters: BTreeMap<String, StructureAdapterDescriptor>,
}

impl AdapterRegistry {
    pub fn default_registry() -> Result<Self, StructureAnalysisError> {
        let mut registry = Self {
            adapters: BTreeMap::new(),
        };
        registry.register(mdraid_descriptor())?;
        registry.register(filesystem_adapter())?;
        registry.register(collection_descriptor())?;
        for (id, version, formats, space) in [
            (
                LVM_ADAPTER_ID,
                evidence_access::lvm::LVM_ADAPTER_VERSION,
                vec!["lvm2"],
                AddressSpace::LogicalBlock,
            ),
            (
                RBD_ADAPTER_ID,
                "ceph-reconstruction/1",
                vec!["rbd-head"],
                AddressSpace::LogicalBlock,
            ),
            (
                CEPHFS_ADAPTER_ID,
                "ceph-reconstruction/1",
                vec!["cephfs"],
                AddressSpace::File,
            ),
            (
                REGISTERED_SOURCE_ADAPTER_ID,
                "registered-source/1",
                vec!["raw", "e01"],
                AddressSpace::PhysicalByte,
            ),
        ] {
            registry.register(StructureAdapterDescriptor {
                id: id.into(),
                version: version.into(),
                formats: formats.into_iter().map(str::to_owned).collect(),
                features: ["metadata", "range-read"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                input_spaces: vec![space],
                output_spaces: vec![space],
                operations: operations(),
                max_workers: 1,
                requires_independent_readers: true,
            })?;
        }
        Ok(registry)
    }

    pub fn register(
        &mut self,
        descriptor: StructureAdapterDescriptor,
    ) -> Result<(), StructureAnalysisError> {
        if descriptor.id.is_empty() || descriptor.version.is_empty() {
            return Err(StructureAnalysisError::AdapterRegistry(
                "adapter identity is empty".into(),
            ));
        }
        if self.adapters.len() >= MAX_ADAPTERS && !self.adapters.contains_key(&descriptor.id) {
            return Err(StructureAnalysisError::AdapterRegistry(
                "adapter bound exceeded".into(),
            ));
        }
        if self
            .adapters
            .insert(descriptor.id.clone(), descriptor)
            .is_some()
        {
            return Err(StructureAnalysisError::AdapterRegistry(
                "adapter id is already registered".into(),
            ));
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&StructureAdapterDescriptor> {
        self.adapters.get(id)
    }

    pub fn descriptors(&self) -> Vec<StructureAdapterDescriptor> {
        self.adapters.values().cloned().collect()
    }
}

fn operations() -> BTreeSet<AdapterOperation> {
    [
        AdapterOperation::Probe,
        AdapterOperation::Inspect,
        AdapterOperation::Bind,
        AdapterOperation::SelectVersion,
        AdapterOperation::Plan,
        AdapterOperation::Verify,
        AdapterOperation::Project,
    ]
    .into_iter()
    .collect()
}

fn mdraid_descriptor() -> StructureAdapterDescriptor {
    StructureAdapterDescriptor {
        id: MDRAID_ADAPTER_ID.into(),
        version: MDRAID_ADAPTER_VERSION.into(),
        formats: vec!["mdraid-v0.90".into(), "mdraid-v1".into()],
        features: [
            "linear".into(),
            "raid0".into(),
            "raid1".into(),
            "raid5".into(),
            "raid6".into(),
            "raid10".into(),
        ]
        .into_iter()
        .collect(),
        input_spaces: vec![AddressSpace::PhysicalByte, AddressSpace::LogicalBlock],
        output_spaces: vec![AddressSpace::LogicalBlock],
        operations: operations(),
        max_workers: 4,
        requires_independent_readers: true,
    }
}

fn filesystem_adapter() -> StructureAdapterDescriptor {
    StructureAdapterDescriptor {
        id: FILESYSTEM_ADAPTER_ID.into(),
        version: FILESYSTEM_ADAPTER_VERSION.into(),
        formats: vec!["ext4".into(), "xfs".into(), "btrfs".into()],
        features: ["metadata".into(), "file-range".into()]
            .into_iter()
            .collect(),
        input_spaces: vec![AddressSpace::LogicalBlock],
        output_spaces: vec![AddressSpace::File, AddressSpace::Namespace],
        operations: operations(),
        max_workers: 4,
        requires_independent_readers: true,
    }
}

fn collection_descriptor() -> StructureAdapterDescriptor {
    StructureAdapterDescriptor {
        id: COLLECTION_ADAPTER_ID.into(),
        version: COLLECTION_ADAPTER_VERSION.into(),
        formats: vec!["raw".into(), "e01".into()],
        features: ["verified-content".into(), "range-read".into()]
            .into_iter()
            .collect(),
        input_spaces: vec![AddressSpace::PhysicalByte],
        output_spaces: vec![AddressSpace::PhysicalByte],
        operations: operations(),
        max_workers: 16,
        requires_independent_readers: true,
    }
}
