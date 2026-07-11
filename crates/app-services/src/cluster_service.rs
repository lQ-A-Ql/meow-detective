use domain::{DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_cluster_repo::{
    DataSourceClusterRecord, DataSourceClusterRepo,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::datasource_service;
use crate::import_precheck::{ImportClusterMemberConfig, ImportSourceConfig, ImportSourceMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterParseRequest {
    pub sources: Vec<ClusterEvidenceSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterEvidenceSource {
    pub source_path: PathBuf,
    pub source_kind: DataSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterParsePlan {
    pub source_count: usize,
    pub supported_now: bool,
    pub boundary: ClusterParseBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterParseBoundary {
    PlannedPveLvmThinCluster,
}

#[derive(Debug, Error)]
pub enum ClusterServiceError {
    #[error("cluster parsing is planned but not implemented in this milestone")]
    Unsupported,
    #[error("at least two evidence sources are required for cluster parsing")]
    InsufficientSources,
    #[error("cluster root must point to a readable directory")]
    InvalidClusterRoot,
    #[error("linux cluster import did not find supported E01/RAW images in the selected folder")]
    NoSupportedImages,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("data source classification error: {0}")]
    Classification(#[from] datasource_service::DataSourceError),
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl transport::ServiceErrorCategory for ClusterServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Unsupported => transport::ErrorCategory::Unsupported,
            Self::InsufficientSources | Self::InvalidClusterRoot | Self::NoSupportedImages => {
                transport::ErrorCategory::Validation
            }
            Self::Io(_) | Self::Db(_) => transport::ErrorCategory::Io,
            Self::Classification(e) => e.category(),
            Self::Json(_) => transport::ErrorCategory::Internal,
        }
    }
}

pub type Result<T> = std::result::Result<T, ClusterServiceError>;

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

impl LinuxClusterImportPlan {
    pub fn member_import_configs(&self) -> Vec<ImportSourceConfig> {
        let member_count = self.members.len() as u32;
        self.members
            .iter()
            .map(|member| ImportSourceConfig {
                source_path: member.source_path.clone(),
                source_path_display: member.source_path.display().to_string(),
                source_name: member.source_name.clone(),
                kind: member.source_kind.clone(),
                platform: DataSourcePlatform::Linux,
                profile: self.profile.clone(),
                mode: match member.source_kind {
                    DataSourceKind::E01 => ImportSourceMode::Image {
                        staging_kind: "E01",
                    },
                    DataSourceKind::Raw => ImportSourceMode::Image {
                        staging_kind: "Raw",
                    },
                    DataSourceKind::LogicalDirectory => ImportSourceMode::LogicalDirectory,
                },
                cluster: Some(ImportClusterMemberConfig {
                    cluster_id: self.cluster_id.clone(),
                    member_index: member.member_index,
                    member_count,
                }),
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

/// Reserved entry point for future PVE / multi-source cluster parsing.
///
/// This milestone intentionally remains focused on single-disk XFS-on-LVM
/// parsing. Keeping a typed service boundary prevents later cluster work from
/// being mixed into datasource probing or the single-disk import path.
pub fn plan_cluster_parse(request: ClusterParseRequest) -> Result<ClusterParsePlan> {
    if request.sources.len() < 2 {
        return Err(ClusterServiceError::InsufficientSources);
    }

    Ok(ClusterParsePlan {
        source_count: request.sources.len(),
        supported_now: false,
        boundary: ClusterParseBoundary::PlannedPveLvmThinCluster,
    })
}

pub fn parse_cluster(_request: ClusterParseRequest) -> Result<()> {
    Err(ClusterServiceError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_parse_plan_is_explicitly_non_executing() {
        let plan = plan_cluster_parse(ClusterParseRequest {
            sources: vec![
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-a.E01"),
                    source_kind: DataSourceKind::E01,
                },
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-b.E01"),
                    source_kind: DataSourceKind::E01,
                },
            ],
        })
        .unwrap();

        assert_eq!(plan.source_count, 2);
        assert!(!plan.supported_now);
        assert_eq!(
            plan.boundary,
            ClusterParseBoundary::PlannedPveLvmThinCluster
        );
    }

    #[test]
    fn cluster_parse_execution_is_unsupported_in_single_disk_milestone() {
        let err = parse_cluster(ClusterParseRequest {
            sources: vec![
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-a.E01"),
                    source_kind: DataSourceKind::E01,
                },
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-b.E01"),
                    source_kind: DataSourceKind::E01,
                },
            ],
        })
        .unwrap_err();

        assert!(matches!(err, ClusterServiceError::Unsupported));
    }

    #[test]
    fn cluster_parse_plan_requires_multiple_sources() {
        let err = plan_cluster_parse(ClusterParseRequest {
            sources: vec![ClusterEvidenceSource {
                source_path: PathBuf::from("single.E01"),
                source_kind: DataSourceKind::E01,
            }],
        })
        .unwrap_err();

        assert!(matches!(err, ClusterServiceError::InsufficientSources));
    }

    #[test]
    fn linux_cluster_import_plan_discovers_supported_images() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("node-b.raw"), b"raw").unwrap();
        std::fs::write(tmp.path().join("node-a.E01"), b"short").unwrap();
        std::fs::write(tmp.path().join("node-a.E02"), b"segment").unwrap();
        std::fs::write(tmp.path().join("notes.txt"), b"ignore").unwrap();

        let plan = plan_linux_cluster_import(tmp.path(), Some("pve-cluster".to_string())).unwrap();

        assert_eq!(plan.cluster_name, "pve-cluster");
        assert_eq!(plan.members.len(), 2);
        assert_eq!(plan.members[0].source_name, "node-a.E01");
        assert_eq!(plan.members[0].member_index, 0);
        assert_eq!(plan.members[1].source_name, "node-b.raw");
        assert!(plan.manifest_rel_path.starts_with("clusters/"));
    }

    #[test]
    fn linux_cluster_import_plan_discovers_nested_node_images() {
        let tmp = tempfile::TempDir::new().unwrap();
        let server01 = tmp.path().join("server01");
        let server02 = tmp.path().join("server02");
        std::fs::create_dir_all(&server01).unwrap();
        std::fs::create_dir_all(&server02).unwrap();
        std::fs::write(server01.join("server01-disk01.E01"), b"e01").unwrap();
        std::fs::write(server01.join("server01-disk02.E01"), b"e01").unwrap();
        std::fs::write(server01.join("server01-disk02.E02"), b"segment").unwrap();
        std::fs::write(server02.join("server02-disk01.raw"), b"raw").unwrap();

        let plan = plan_linux_cluster_import(tmp.path(), Some("pve-cluster".to_string())).unwrap();

        let member_paths = plan
            .members
            .iter()
            .map(|member| {
                member
                    .source_path
                    .strip_prefix(tmp.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            member_paths,
            vec![
                "server01/server01-disk01.E01",
                "server01/server01-disk02.E01",
                "server02/server02-disk01.raw"
            ]
        );
        assert_eq!(plan.members[0].member_index, 0);
        assert_eq!(plan.members[1].member_index, 1);
        assert_eq!(plan.members[2].member_index, 2);
    }

    #[test]
    fn linux_cluster_import_plan_requires_multiple_images() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("single.raw"), b"raw").unwrap();

        let err = plan_linux_cluster_import(tmp.path(), None).unwrap_err();

        assert!(matches!(err, ClusterServiceError::InsufficientSources));
    }

    #[test]
    fn linux_cluster_member_configs_are_linux_scoped() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
        std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();
        let plan = plan_linux_cluster_import(tmp.path(), None).unwrap();

        let configs = plan.member_import_configs();

        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].platform, DataSourcePlatform::Linux);
        assert_eq!(
            configs[0]
                .cluster
                .as_ref()
                .map(|value| value.cluster_id.as_str()),
            Some(plan.cluster_id.as_str())
        );
        assert_eq!(configs[0].cluster.as_ref().unwrap().member_count, 2);
    }

    #[test]
    fn linux_cluster_import_plan_normalizes_profile_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
        std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();

        let plan =
            plan_linux_cluster_import(tmp.path(), Some("  pve-audit  ".to_string())).expect("plan");

        assert_eq!(plan.cluster_name, "pve-audit");
        assert_eq!(plan.profile.as_deref(), Some("pve-audit"));
    }

    #[test]
    fn linux_cluster_manifest_write_is_atomic_and_readable() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
        std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();
        let plan = plan_linux_cluster_import(tmp.path(), None).expect("plan");
        let case_root = tempfile::TempDir::new().unwrap();

        let manifest_path = write_linux_cluster_manifest(case_root.path(), &plan).expect("write");

        assert!(manifest_path.exists());
        assert!(!manifest_path.with_extension("json.tmp").exists());
        let manifest = std::fs::read_to_string(manifest_path).expect("manifest");
        assert!(manifest.contains(&plan.cluster_id));
        assert!(manifest.contains("\"memberCount\": 2"));
    }
}
