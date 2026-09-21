use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopologyScopeKindDto {
    PhysicalHost,
    Pve,
    Ceph,
    VirtualMachine,
    OsInstance,
    Kubernetes,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopologyMemberRoleDto {
    Host,
    StorageNode,
    ControlPlane,
    Worker,
    VirtualMachine,
    OsRoot,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopologyEdgeKindDto {
    Hosts,
    ProvidesStorage,
    Boots,
    Runs,
    Manages,
    ConsumesStorage,
    DerivedFrom,
}
