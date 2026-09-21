use super::{ClusterServiceError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxClusterKind {
    PveCeph,
    Kubernetes,
    Unknown,
}

impl LinuxClusterKind {
    pub fn from_profile(profile: Option<&str>) -> Self {
        match profile
            .map(str::trim)
            .map(|value| value.to_ascii_lowercase())
        {
            Some(value)
                if matches!(
                    value.as_str(),
                    "pve" | "pve-cluster" | "pve_cluster" | "pve-ceph" | "pve_ceph" | "ceph"
                ) =>
            {
                Self::PveCeph
            }
            Some(value) if value == "kubernetes" || value == "k8s" => Self::Kubernetes,
            _ => Self::Unknown,
        }
    }

    pub fn require_pve_ceph(self) -> Result<()> {
        if self == Self::PveCeph {
            Ok(())
        } else {
            Err(ClusterServiceError::Unsupported)
        }
    }

    pub fn require_kubernetes(self) -> Result<()> {
        if self == Self::Kubernetes {
            Ok(())
        } else {
            Err(ClusterServiceError::Unsupported)
        }
    }
}
