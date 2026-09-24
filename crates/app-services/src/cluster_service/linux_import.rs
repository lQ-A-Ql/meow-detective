use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use domain::{DataSourceKind, DataSourcePlatform};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::datasource_service;
use crate::import_precheck::{ImportSetMemberConfig, ImportSourceConfig, ImportSourceMode};

use super::{ClusterServiceError, Result};

mod scan;
use scan::collect_evidence_set_image_candidates;

static MANIFEST_UPDATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxEvidenceSetImportPlan {
    pub import_set_id: String,
    pub import_set_name: String,
    pub root_path: PathBuf,
    pub manifest_rel_path: String,
    pub members: Vec<LinuxEvidenceSetMemberPlan>,
    pub skipped_entries: Vec<LinuxEvidenceSetSkippedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceSetMemberPlan {
    pub member_index: u32,
    pub source_path: PathBuf,
    pub source_name: String,
    pub source_kind: DataSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceSetSkippedEntry {
    pub relative_path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinuxEvidenceSetManifest {
    schema_version: u32,
    import_set_id: String,
    import_set_name: String,
    root_path: PathBuf,
    member_count: u32,
    collected_at: String,
    scan_policy: String,
    members: Vec<LinuxEvidenceSetManifestMember>,
    skipped_entries: Vec<LinuxEvidenceSetSkippedEntry>,
    manifest_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinuxEvidenceSetManifestMember {
    #[serde(flatten)]
    plan: LinuxEvidenceSetMemberPlan,
    source_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_modified_at: Option<String>,
    hash_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsignedLinuxEvidenceSetManifest<'a> {
    schema_version: u32,
    import_set_id: &'a str,
    import_set_name: &'a str,
    root_path: &'a Path,
    member_count: u32,
    collected_at: &'a str,
    scan_policy: &'a str,
    members: &'a [LinuxEvidenceSetManifestMember],
    skipped_entries: &'a [LinuxEvidenceSetSkippedEntry],
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

    let (mut candidate_paths, skipped_entries) = collect_evidence_set_image_candidates(&root_path)?;
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
        skipped_entries,
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
    let collected_at = Utc::now().to_rfc3339();
    let members = plan
        .members
        .iter()
        .map(manifest_member)
        .collect::<Result<Vec<_>>>()?;
    let unsigned = UnsignedLinuxEvidenceSetManifest {
        schema_version: 2,
        import_set_id: &plan.import_set_id,
        import_set_name: &plan.import_set_name,
        root_path: &plan.root_path,
        member_count: members.len() as u32,
        collected_at: &collected_at,
        scan_policy: "recursive-root-relative-v1",
        members: &members,
        skipped_entries: &plan.skipped_entries,
    };
    let manifest_digest = hex::encode(Sha256::digest(serde_json::to_vec(&unsigned)?));
    let manifest = LinuxEvidenceSetManifest {
        schema_version: 2,
        import_set_id: plan.import_set_id.clone(),
        import_set_name: plan.import_set_name.clone(),
        root_path: plan.root_path.clone(),
        member_count: members.len() as u32,
        collected_at,
        scan_policy: "recursive-root-relative-v1".to_string(),
        members,
        skipped_entries: plan.skipped_entries.clone(),
        manifest_digest,
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

pub fn update_linux_evidence_set_manifest_hash(
    case_root: &Path,
    connection: &rusqlite::Connection,
    data_source_id: &str,
    hash_status: &str,
    source_sha256: Option<&str>,
) -> Result<()> {
    let _guard = MANIFEST_UPDATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| {
            ClusterServiceError::Io(std::io::Error::other(
                "Linux evidence-set manifest lock is poisoned",
            ))
        })?;
    let repo = persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo::new(
        connection,
    );
    let Some(member) = repo.find_member_by_source(data_source_id)? else {
        return Ok(());
    };
    let manifest_path = case_root.join(format!(
        "import-sets/{}/import-set-manifest.json",
        member.import_set_id
    ));
    let payload = std::fs::read(&manifest_path)?;
    let mut manifest: LinuxEvidenceSetManifest = serde_json::from_slice(&payload)?;
    if manifest.schema_version != 2 {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    let Some(entry) = manifest
        .members
        .iter_mut()
        .find(|entry| entry.plan.source_path.display().to_string() == member.source_path)
    else {
        return Err(ClusterServiceError::InvalidClusterId);
    };
    entry.hash_status = hash_status.to_string();
    entry.source_sha256 = source_sha256.map(str::to_string);
    manifest.manifest_digest = manifest_digest(&manifest)?;
    write_manifest_atomically(&manifest_path, &manifest)
}

pub fn validate_linux_evidence_set_manifest(case_root: &Path, import_set_id: &str) -> Result<()> {
    let manifest_path = case_root.join(format!(
        "import-sets/{import_set_id}/import-set-manifest.json"
    ));
    let payload = std::fs::read(&manifest_path)?;
    let manifest: LinuxEvidenceSetManifest = serde_json::from_slice(&payload)?;
    if manifest.schema_version != 2
        || manifest.import_set_id != import_set_id
        || manifest.member_count != manifest.members.len() as u32
        || manifest.scan_policy != "recursive-root-relative-v1"
        || manifest_digest(&manifest)? != manifest.manifest_digest
    {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    Ok(())
}

fn manifest_digest(manifest: &LinuxEvidenceSetManifest) -> Result<String> {
    let unsigned = UnsignedLinuxEvidenceSetManifest {
        schema_version: manifest.schema_version,
        import_set_id: &manifest.import_set_id,
        import_set_name: &manifest.import_set_name,
        root_path: &manifest.root_path,
        member_count: manifest.member_count,
        collected_at: &manifest.collected_at,
        scan_policy: &manifest.scan_policy,
        members: &manifest.members,
        skipped_entries: &manifest.skipped_entries,
    };
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&unsigned)?)))
}

fn write_manifest_atomically(case_path: &Path, manifest: &LinuxEvidenceSetManifest) -> Result<()> {
    let temp_path = case_path.with_extension("json.tmp");
    let payload = serde_json::to_vec_pretty(manifest)?;
    if let Err(error) = std::fs::write(&temp_path, payload) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&temp_path, case_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.into());
    }
    Ok(())
}

fn manifest_member(member: &LinuxEvidenceSetMemberPlan) -> Result<LinuxEvidenceSetManifestMember> {
    let metadata = std::fs::metadata(&member.source_path)?;
    let source_modified_at = metadata
        .modified()
        .ok()
        .map(|value| chrono::DateTime::<Utc>::from(value).to_rfc3339());
    Ok(LinuxEvidenceSetManifestMember {
        plan: member.clone(),
        source_size_bytes: metadata.len(),
        source_modified_at,
        // Evidence hashing is deliberately asynchronous so import admission
        // cannot block on large E01 members. The data-source hash job updates
        // the authoritative provenance row after import.
        hash_status: "pending".to_string(),
        source_sha256: None,
    })
}

fn normalize_import_set_name(name: Option<String>) -> Option<String> {
    name.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
