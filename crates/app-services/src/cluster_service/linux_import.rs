use std::path::{Path, PathBuf};

use domain::{DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_cluster_repo::{
    DataSourceClusterRecord, DataSourceClusterRepo,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ceph_reconstruction::InventoryCoverageReport;
use crate::ceph_reconstruction::RbdReplicaPolicy;
use crate::datasource_service;
use crate::import_precheck::{ImportClusterMemberConfig, ImportSourceConfig, ImportSourceMode};

use super::{ClusterServiceError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxClusterImportPlan {
    pub cluster_id: String,
    pub cluster_name: String,
    pub root_path: PathBuf,
    pub profile: Option<String>,
    pub manifest_rel_path: String,
    pub members: Vec<LinuxClusterMemberPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxClusterMemberPlan {
    pub member_index: u32,
    pub source_path: PathBuf,
    pub source_name: String,
    pub source_kind: DataSourceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinuxClusterManifest {
    schema_version: u32,
    cluster_id: String,
    cluster_name: String,
    root_path: PathBuf,
    profile: Option<String>,
    member_count: u32,
    members: Vec<LinuxClusterMemberPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinuxClusterCoverageArtifact {
    schema_version: u32,
    cluster_id: String,
    evidence_kind: String,
    coverage_policy: String,
    #[serde(default = "legacy_policy_source")]
    policy_source: String,
    #[serde(default = "RbdReplicaPolicy::strict_legacy")]
    policy: RbdReplicaPolicy,
    report: InventoryCoverageReport,
    #[serde(default)]
    report_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsignedLinuxClusterCoverageArtifact<'a> {
    schema_version: u32,
    cluster_id: &'a str,
    evidence_kind: &'a str,
    coverage_policy: &'a str,
    policy_source: &'a str,
    policy: &'a RbdReplicaPolicy,
    report: &'a InventoryCoverageReport,
}

impl LinuxClusterImportPlan {
    pub fn member_import_configs(&self) -> Vec<ImportSourceConfig> {
        let member_count = self.members.len() as u32;
        self.members
            .iter()
            .filter_map(|member| {
                let mode = match member.source_kind {
                    DataSourceKind::E01 => ImportSourceMode::Image {
                        staging_kind: "E01",
                    },
                    DataSourceKind::Raw => ImportSourceMode::Image {
                        staging_kind: "Raw",
                    },
                    DataSourceKind::LocalDisk => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "local physical-disk sources cannot enter the Linux cluster import pipeline"
                        );
                        return None;
                    }
                    DataSourceKind::LogicalDirectory => ImportSourceMode::LogicalDirectory,
                    DataSourceKind::LogicalArchive => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "logical archive sources cannot enter the Linux cluster import pipeline"
                        );
                        return None;
                    }
                    DataSourceKind::AndroidSparse => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "Android sparse sources cannot enter the Linux cluster import pipeline"
                        );
                        return None;
                    }
                    DataSourceKind::CephRbd | DataSourceKind::CephFs => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "Ceph RBD derived source cannot enter the host-path cluster import pipeline"
                        );
                        return None;
                    }
                };
                Some(ImportSourceConfig {
                    source_path: member.source_path.clone(),
                    source_path_display: member.source_path.display().to_string(),
                    source_name: member.source_name.clone(),
                    kind: member.source_kind.clone(),
                    platform: DataSourcePlatform::Linux,
                    profile: self.profile.clone(),
                    mode,
                    cluster: Some(ImportClusterMemberConfig {
                        cluster_id: self.cluster_id.clone(),
                        member_index: member.member_index,
                        member_count,
                    }),
                })
            })
            .collect()
    }
}

pub fn plan_linux_cluster_import(
    root_path: impl Into<PathBuf>,
    profile: Option<String>,
) -> Result<LinuxClusterImportPlan> {
    let root_path = root_path.into();
    let profile = normalize_cluster_profile(profile);
    let metadata =
        std::fs::metadata(&root_path).map_err(|_| ClusterServiceError::InvalidClusterRoot)?;
    if !metadata.is_dir() {
        return Err(ClusterServiceError::InvalidClusterRoot);
    }

    let mut candidate_paths = collect_cluster_image_candidates(&root_path)?;
    candidate_paths.sort_by(|left, right| {
        normalized_candidate_sort_key(&root_path, left)
            .cmp(&normalized_candidate_sort_key(&root_path, right))
    });

    let mut members = Vec::new();
    for path in candidate_paths {
        let kind = datasource_service::classify_data_source_path(&path)?;
        if matches!(kind, DataSourceKind::E01 | DataSourceKind::Raw) {
            members.push(LinuxClusterMemberPlan {
                member_index: members.len() as u32,
                source_name: derive_source_name(&path),
                source_path: path,
                source_kind: kind,
            });
        }
    }

    if members.is_empty() {
        return Err(ClusterServiceError::NoSupportedImages);
    }
    if members.len() < 2 {
        return Err(ClusterServiceError::InsufficientSources);
    }

    let cluster_id = uuid::Uuid::new_v4().to_string();
    let cluster_name = profile
        .clone()
        .unwrap_or_else(|| derive_source_name(&root_path));
    let manifest_rel_path = format!("clusters/{cluster_id}/cluster-manifest.json");

    Ok(LinuxClusterImportPlan {
        cluster_id,
        cluster_name,
        root_path,
        profile,
        manifest_rel_path,
        members,
    })
}

pub fn register_linux_cluster_import(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    plan: &LinuxClusterImportPlan,
) -> Result<()> {
    DataSourceClusterRepo::new(conn).insert_pending(&DataSourceClusterRecord {
        id: plan.cluster_id.clone(),
        case_id: case_id.clone(),
        name: plan.cluster_name.clone(),
        root_path: plan.root_path.display().to_string(),
        platform: DataSourcePlatform::Linux.as_storage_str().to_string(),
        profile: plan.profile.clone(),
        manifest_rel_path: plan.manifest_rel_path.clone(),
        import_state: "pending".to_string(),
        member_count: plan.members.len() as u32,
        ready_count: 0,
        failed_count: 0,
        last_error: None,
    })?;
    Ok(())
}

pub fn update_linux_cluster_import_state(
    conn: &rusqlite::Connection,
    cluster_id: &str,
    import_state: &str,
    ready_count: u32,
    failed_count: u32,
    last_error: Option<&str>,
) -> Result<()> {
    DataSourceClusterRepo::new(conn).update_state(
        cluster_id,
        import_state,
        ready_count,
        failed_count,
        last_error,
    )?;
    Ok(())
}

pub fn assess_linux_cluster_cephfs_presence(
    conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    cluster_id: &str,
) -> Result<crate::ceph_reconstruction::CephFsPresenceAssessment> {
    crate::ceph_reconstruction::assess_cephfs_presence_for_cluster(
        conn, case_root, case_id, cluster_id,
    )
    .map_err(Into::into)
}

pub fn write_linux_cluster_manifest(
    case_root: &Path,
    plan: &LinuxClusterImportPlan,
) -> Result<PathBuf> {
    let manifest = LinuxClusterManifest {
        schema_version: 1,
        cluster_id: plan.cluster_id.clone(),
        cluster_name: plan.cluster_name.clone(),
        root_path: plan.root_path.clone(),
        profile: plan.profile.clone(),
        member_count: plan.members.len() as u32,
        members: plan.members.clone(),
    };
    let manifest_path = case_root.join(&plan.manifest_rel_path);
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = manifest_path.with_extension("json.tmp");
    let payload = serde_json::to_vec_pretty(&manifest)?;
    if let Err(error) = std::fs::write(&temp_path, payload) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&temp_path, &manifest_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.into());
    }
    Ok(manifest_path)
}

pub fn write_linux_cluster_coverage_report(
    case_root: &Path,
    cluster_id: &str,
    report: &InventoryCoverageReport,
) -> Result<PathBuf> {
    validate_cluster_id(cluster_id)?;
    if report.policy.validate().is_err() || report.expected_count != report.policy.expected_count()
    {
        return Err(ClusterServiceError::InvalidCoverageReport);
    }
    let report_path = case_root.join(format!("clusters/{cluster_id}/coverage-report.json"));
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let artifact = LinuxClusterCoverageArtifact {
        schema_version: 3,
        cluster_id: cluster_id.to_string(),
        evidence_kind: "rbd_osd_inventory".to_string(),
        coverage_policy: report.policy.storage_key().to_string(),
        policy_source: report.policy_source().to_string(),
        policy: report.policy.clone(),
        report: report.clone(),
        report_digest: None,
    };
    let mut artifact = artifact;
    artifact.report_digest = Some(coverage_artifact_digest(&artifact)?);
    let temp_path = report_path.with_extension("json.tmp");
    let payload = serde_json::to_vec_pretty(&artifact)?;
    if let Err(error) = std::fs::write(&temp_path, payload) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&temp_path, &report_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.into());
    }
    Ok(report_path)
}

pub fn read_linux_cluster_coverage_report(
    case_root: &Path,
    cluster_id: &str,
) -> Result<Option<InventoryCoverageReport>> {
    validate_cluster_id(cluster_id)?;
    let report_path = case_root.join(format!("clusters/{cluster_id}/coverage-report.json"));
    let payload = match std::fs::read_to_string(&report_path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let artifact: LinuxClusterCoverageArtifact = serde_json::from_str(&payload)?;
    let digest_valid = if artifact.schema_version >= 3 {
        artifact
            .report_digest
            .as_deref()
            .and_then(|stored| {
                coverage_artifact_digest(&artifact)
                    .ok()
                    .map(|actual| actual == stored)
            })
            .unwrap_or(false)
    } else {
        true
    };
    if !(1..=3).contains(&artifact.schema_version)
        || artifact.cluster_id != cluster_id
        || artifact.evidence_kind != "rbd_osd_inventory"
        || artifact.coverage_policy != artifact.report.policy.storage_key()
        || artifact.policy_source != artifact.report.policy_source()
        || artifact.policy != artifact.report.policy
        || artifact.report.policy.validate().is_err()
        || artifact.report.observed_count > artifact.report.expected_count
        || artifact.report.expected_count != artifact.report.policy.expected_count()
        || !digest_valid
    {
        return Err(ClusterServiceError::InvalidCoverageReport);
    }
    Ok(Some(artifact.report))
}

fn coverage_artifact_digest(artifact: &LinuxClusterCoverageArtifact) -> Result<String> {
    let unsigned = UnsignedLinuxClusterCoverageArtifact {
        schema_version: artifact.schema_version,
        cluster_id: &artifact.cluster_id,
        evidence_kind: &artifact.evidence_kind,
        coverage_policy: &artifact.coverage_policy,
        policy_source: &artifact.policy_source,
        policy: &artifact.policy,
        report: &artifact.report,
    };
    let payload = serde_json::to_vec(&unsigned)?;
    Ok(hex::encode(Sha256::digest(payload)))
}

fn legacy_policy_source() -> String {
    RbdReplicaPolicy::strict_legacy().source().to_string()
}

fn validate_cluster_id(cluster_id: &str) -> Result<()> {
    if cluster_id.is_empty()
        || cluster_id == "."
        || cluster_id == ".."
        || cluster_id.contains(['/', '\\'])
    {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    Ok(())
}

fn normalize_cluster_profile(profile: Option<String>) -> Option<String> {
    profile
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_cluster_image_candidate(path: &Path) -> bool {
    if is_secondary_e01_segment(path) {
        return false;
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(extension.as_str(), "e01" | "ewf" | "raw" | "dd" | "img")
}

fn collect_cluster_image_candidates(root_path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    collect_cluster_image_candidates_inner(root_path, &mut candidates)?;
    Ok(candidates)
}

fn collect_cluster_image_candidates_inner(
    directory: &Path,
    candidates: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_cluster_image_candidates_inner(&path, candidates)?;
        } else if file_type.is_file() && is_cluster_image_candidate(&path) {
            candidates.push(path);
        }
    }
    Ok(())
}

fn is_secondary_e01_segment(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Some(digits) = extension.strip_prefix('e') else {
        return false;
    };
    digits.len() == 2 && digits.chars().all(|ch| ch.is_ascii_digit()) && digits != "01"
}

fn derive_source_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "linux-cluster".to_string())
}

fn normalized_candidate_sort_key(root_path: &Path, path: &Path) -> String {
    path.strip_prefix(root_path)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}
