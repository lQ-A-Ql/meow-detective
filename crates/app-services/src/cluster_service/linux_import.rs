use std::path::{Path, PathBuf};

use domain::{DataSourceKind, DataSourcePlatform};
use serde::{Deserialize, Serialize};

use crate::datasource_service;
use crate::import_precheck::{ImportSetMemberConfig, ImportSourceConfig, ImportSourceMode};

use super::{ClusterServiceError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxEvidenceSetImportPlan {
    pub import_set_id: String,
    pub import_set_name: String,
    pub root_path: PathBuf,
    pub manifest_rel_path: String,
    pub members: Vec<LinuxEvidenceSetMemberPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceSetMemberPlan {
    pub member_index: u32,
    pub source_path: PathBuf,
    pub source_name: String,
    pub source_kind: DataSourceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinuxEvidenceSetManifest {
    schema_version: u32,
    import_set_id: String,
    import_set_name: String,
    root_path: PathBuf,
    member_count: u32,
    members: Vec<LinuxEvidenceSetMemberPlan>,
}

impl LinuxEvidenceSetImportPlan {
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
                            "local physical-disk sources cannot enter the Linux evidence-set import pipeline"
                        );
                        return None;
                    }
                    DataSourceKind::LogicalDirectory => ImportSourceMode::LogicalDirectory,
                    DataSourceKind::LogicalArchive => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "logical archive sources cannot enter the Linux evidence-set import pipeline"
                        );
                        return None;
                    }
                    DataSourceKind::AndroidSparse => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "Android sparse sources cannot enter the Linux evidence-set import pipeline"
                        );
                        return None;
                    }
                    DataSourceKind::CephRbd | DataSourceKind::CephFs => {
                        tracing::warn!(
                            source = %member.source_path.display(),
                            "Ceph RBD derived source cannot enter the host-path evidence-set import pipeline"
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
                    profile: None,
                    mode,
                    import_set: Some(ImportSetMemberConfig {
                        import_set_id: self.import_set_id.clone(),
                        member_index: member.member_index,
                        member_count,
                    }),
                })
            })
            .collect()
    }
}

pub fn plan_linux_evidence_set_import(
    root_path: impl Into<PathBuf>,
    import_set_name: Option<String>,
) -> Result<LinuxEvidenceSetImportPlan> {
    let root_path = root_path.into();
    let import_set_name = normalize_import_set_name(import_set_name);
    let metadata =
        std::fs::metadata(&root_path).map_err(|_| ClusterServiceError::InvalidClusterRoot)?;
    if !metadata.is_dir() {
        return Err(ClusterServiceError::InvalidClusterRoot);
    }

    let mut candidate_paths = collect_evidence_set_image_candidates(&root_path)?;
    candidate_paths.sort_by(|left, right| {
        normalized_candidate_sort_key(&root_path, left)
            .cmp(&normalized_candidate_sort_key(&root_path, right))
    });

    let mut members = Vec::new();
    for path in candidate_paths {
        let kind = datasource_service::classify_data_source_path(&path)?;
        if matches!(kind, DataSourceKind::E01 | DataSourceKind::Raw) {
            members.push(LinuxEvidenceSetMemberPlan {
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

    let import_set_id = uuid::Uuid::new_v4().to_string();
    let import_set_name = import_set_name.unwrap_or_else(|| derive_source_name(&root_path));
    let manifest_rel_path = format!("import-sets/{import_set_id}/import-set-manifest.json");

    Ok(LinuxEvidenceSetImportPlan {
        import_set_id,
        import_set_name,
        root_path,
        manifest_rel_path,
        members,
    })
}

pub fn register_linux_evidence_set_import(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<()> {
    let repo =
        persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo::new(conn);
    repo.insert(
        &persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRecord {
            id: plan.import_set_id.clone(),
            case_id: case_id.0.clone(),
            name: plan.import_set_name.clone(),
            root_path: plan.root_path.display().to_string(),
            import_state: "pending".to_string(),
            member_count: plan.members.len() as u32,
            ready_count: 0,
            failed_count: 0,
            last_error: None,
        },
    )?;
    for member in &plan.members {
        repo.insert_member(
            &persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetMemberRecord {
                import_set_id: plan.import_set_id.clone(),
                member_index: member.member_index,
                source_path: member.source_path.display().to_string(),
                source_kind: member.source_kind.to_string(),
                data_source_id: None,
                import_state: "pending".to_string(),
                last_error: None,
            },
        )?;
    }
    Ok(())
}

pub fn update_linux_evidence_set_import_state(
    conn: &rusqlite::Connection,
    import_set_id: &str,
    import_state: &str,
    ready_count: u32,
    failed_count: u32,
    last_error: Option<&str>,
) -> Result<()> {
    persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo::new(conn)
        .update_state(
            import_set_id,
            import_state,
            ready_count,
            failed_count,
            last_error,
        )?;
    Ok(())
}

pub fn write_linux_evidence_set_manifest(
    case_root: &Path,
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<PathBuf> {
    let manifest = LinuxEvidenceSetManifest {
        schema_version: 1,
        import_set_id: plan.import_set_id.clone(),
        import_set_name: plan.import_set_name.clone(),
        root_path: plan.root_path.clone(),
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

fn normalize_import_set_name(name: Option<String>) -> Option<String> {
    name.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_evidence_set_image_candidate(path: &Path) -> bool {
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

fn collect_evidence_set_image_candidates(root_path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    collect_evidence_set_image_candidates_inner(root_path, &mut candidates)?;
    Ok(candidates)
}

fn collect_evidence_set_image_candidates_inner(
    directory: &Path,
    candidates: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_evidence_set_image_candidates_inner(&path, candidates)?;
        } else if file_type.is_file() && is_evidence_set_image_candidate(&path) {
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
        .unwrap_or_else(|| "linux-evidence-set".to_string())
}

fn normalized_candidate_sort_key(root_path: &Path, path: &Path) -> String {
    path.strip_prefix(root_path)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}
