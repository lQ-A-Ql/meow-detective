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
pub enum TopologyEdgeKind {
    Hosts,
    ProvidesStorage,
    Boots,
    Runs,
    Manages,
    ConsumesStorage,
    DerivedFrom,
}

impl TopologyEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hosts => "hosts",
            Self::ProvidesStorage => "provides_storage",
            Self::Boots => "boots",
            Self::Runs => "runs",
            Self::Manages => "manages",
            Self::ConsumesStorage => "consumes_storage",
            Self::DerivedFrom => "derived_from",
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
