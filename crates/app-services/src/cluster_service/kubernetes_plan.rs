use std::path::PathBuf;

use super::{plan_linux_cluster_import, ClusterServiceError, LinuxClusterImportPlan, Result};

pub const KUBERNETES_CLUSTER_PROFILE: &str = "kubernetes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesClusterPlan {
    pub import: LinuxClusterImportPlan,
}

impl KubernetesClusterPlan {
    pub fn member_import_configs(&self) -> Vec<crate::import_precheck::ImportSourceConfig> {
        self.import.member_import_configs()
    }

    pub fn members(&self) -> &[super::LinuxClusterMemberPlan] {
        &self.import.members
    }
}

pub fn plan_kubernetes_cluster_import(
    root_path: impl Into<PathBuf>,
    cluster_name: Option<String>,
) -> Result<KubernetesClusterPlan> {
    let mut import = plan_linux_cluster_import(root_path, cluster_name)?;
    import.profile = Some(KUBERNETES_CLUSTER_PROFILE.to_string());
    if import.members.is_empty() {
        return Err(ClusterServiceError::NoSupportedImages);
    }
    Ok(KubernetesClusterPlan { import })
}
