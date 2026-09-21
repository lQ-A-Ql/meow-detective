use serde::{Deserialize, Serialize};

macro_rules! scope_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
        pub struct $name(pub String);
    };
}

scope_id!(TopologyScopeId);
scope_id!(PveScopeId);
scope_id!(CephScopeId);
scope_id!(KubernetesScopeId);
scope_id!(OsInstanceId);

scope_id!(StorageObjectId);
scope_id!(AnalysisObjectId);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentObjectKind {
    PhysicalHost,
    Pve,
    VirtualMachine,
    OsInstance,
    Kubernetes,
    KubernetesNode,
    Namespace,
    Pod,
    Container,
}

impl EnvironmentObjectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PhysicalHost => "physical_host",
            Self::Pve => "pve",
            Self::VirtualMachine => "virtual_machine",
            Self::OsInstance => "os_instance",
            Self::Kubernetes => "kubernetes",
            Self::KubernetesNode => "kubernetes_node",
            Self::Namespace => "namespace",
            Self::Pod => "pod",
            Self::Container => "container",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageObjectKind {
    Partition,
    LvmPhysicalVolume,
    VolumeGroup,
    LogicalVolume,
    VirtualDisk,
    CephCluster,
    CephOsd,
    CephRbd,
    CephFs,
    FileSystem,
}

impl StorageObjectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::LvmPhysicalVolume => "lvm_physical_volume",
            Self::VolumeGroup => "volume_group",
            Self::LogicalVolume => "logical_volume",
            Self::VirtualDisk => "virtual_disk",
            Self::CephCluster => "ceph_cluster",
            Self::CephOsd => "ceph_osd",
            Self::CephRbd => "ceph_rbd",
            Self::CephFs => "ceph_fs",
            Self::FileSystem => "file_system",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisObjectKind {
    FileEntry,
    Artifact,
    TimelineEvent,
    Finding,
    Report,
}

impl AnalysisObjectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileEntry => "file_entry",
            Self::Artifact => "artifact",
            Self::TimelineEvent => "timeline_event",
            Self::Finding => "finding",
            Self::Report => "report",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "domain", content = "kind")]
pub enum TopologyObjectKind {
    Environment(EnvironmentObjectKind),
    Storage(StorageObjectKind),
    Analysis(AnalysisObjectKind),
    EvidenceSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureRelationKind {
    Hosts,
    ProvidesStorage,
    Boots,
    Runs,
    Manages,
    ConsumesStorage,
    Contains,
    MaterializesAs,
    MountedFrom,
    DerivedFrom,
    Produces,
    SupportedBy,
}

impl InfrastructureRelationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hosts => "hosts",
            Self::ProvidesStorage => "provides_storage",
            Self::Boots => "boots",
            Self::Runs => "runs",
            Self::Manages => "manages",
            Self::ConsumesStorage => "consumes_storage",
            Self::Contains => "contains",
            Self::MaterializesAs => "materializes_as",
            Self::MountedFrom => "mounted_from",
            Self::DerivedFrom => "derived_from",
            Self::Produces => "produces",
            Self::SupportedBy => "supported_by",
        }
    }

    pub const fn allows(self, source: TopologyObjectKind, target: TopologyObjectKind) -> bool {
        use AnalysisObjectKind as A;
        use EnvironmentObjectKind as E;
        use InfrastructureRelationKind as R;
        use StorageObjectKind as S;
        use TopologyObjectKind::{Analysis, Environment, EvidenceSource, Storage};

        match self {
            R::Hosts => matches!(
                (source, target),
                (Environment(E::Pve), Environment(E::VirtualMachine))
                    | (Environment(E::PhysicalHost), Environment(E::OsInstance))
                    | (Environment(E::KubernetesNode), Environment(E::Pod))
            ),
            R::ProvidesStorage => matches!(
                (source, target),
                (
                    Storage(S::CephCluster),
                    Environment(E::Pve | E::VirtualMachine | E::Kubernetes)
                )
            ),
            R::Boots => matches!(
                (source, target),
                (Environment(E::VirtualMachine), Environment(E::OsInstance))
                    | (Storage(S::VirtualDisk), Environment(E::OsInstance))
            ),
            R::Runs => matches!(
                (source, target),
                (Environment(E::OsInstance), Environment(E::Kubernetes))
            ),
            R::Manages => matches!(
                (source, target),
                (Environment(E::Pve), Environment(E::PhysicalHost))
            ),
            R::ConsumesStorage => matches!(
                (source, target),
                (Environment(E::Kubernetes), Storage(S::CephRbd | S::CephFs))
            ),
            R::Contains => matches!(
                (source, target),
                (
                    Storage(S::CephCluster),
                    Storage(S::CephOsd | S::CephRbd | S::CephFs)
                ) | (Environment(E::Namespace), Environment(E::Pod))
                    | (Environment(E::Pod), Environment(E::Container))
                    | (Storage(S::FileSystem), Analysis(A::FileEntry))
            ),
            R::MaterializesAs => matches!(
                (source, target),
                (Storage(S::CephRbd), Storage(S::VirtualDisk))
            ),
            R::MountedFrom => matches!(
                (source, target),
                (
                    Storage(S::FileSystem),
                    Storage(S::LogicalVolume | S::VirtualDisk)
                )
            ),
            R::DerivedFrom => matches!(
                (source, target),
                (Analysis(_), EvidenceSource | Storage(_)) | (Storage(_), EvidenceSource)
            ),
            R::Produces => matches!((source, target), (Environment(_), Analysis(_))),
            R::SupportedBy => matches!(
                (source, target),
                (
                    Analysis(A::Finding | A::Report),
                    Analysis(A::Artifact | A::TimelineEvent)
                )
            ),
        }
    }
}

pub type TopologyEdgeKind = InfrastructureRelationKind;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopologyScopeKind {
    PhysicalHost,
    Pve,
    Ceph,
    VirtualMachine,
    OsInstance,
    Kubernetes,
}

impl TopologyScopeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PhysicalHost => "physical_host",
            Self::Pve => "pve",
            Self::Ceph => "ceph",
            Self::VirtualMachine => "virtual_machine",
            Self::OsInstance => "os_instance",
            Self::Kubernetes => "kubernetes",
        }
    }

    pub fn from_storage_key(value: &str) -> Option<Self> {
        match value {
            "physical_host" => Some(Self::PhysicalHost),
            "pve" => Some(Self::Pve),
            "ceph" => Some(Self::Ceph),
            "virtual_machine" => Some(Self::VirtualMachine),
            "os_instance" => Some(Self::OsInstance),
            "kubernetes" => Some(Self::Kubernetes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopologyMemberRole {
    Host,
    StorageNode,
    ControlPlane,
    Worker,
    VirtualMachine,
    OsRoot,
    Unknown,
}

impl TopologyMemberRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::StorageNode => "storage_node",
            Self::ControlPlane => "control_plane",
            Self::Worker => "worker",
            Self::VirtualMachine => "virtual_machine",
            Self::OsRoot => "os_root",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_storage_key(value: &str) -> Option<Self> {
        match value {
            "host" => Some(Self::Host),
            "storage_node" => Some(Self::StorageNode),
            "control_plane" => Some(Self::ControlPlane),
            "worker" => Some(Self::Worker),
            "virtual_machine" => Some(Self::VirtualMachine),
            "os_root" => Some(Self::OsRoot),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}
