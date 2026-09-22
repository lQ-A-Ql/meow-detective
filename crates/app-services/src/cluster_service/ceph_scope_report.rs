use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ceph_reconstruction::{InventoryCoverageReport, RbdReplicaPolicy};

use super::{scope_storage, ClusterServiceError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CephScopeCoverageArtifact {
    schema_version: u32,
    ceph_scope_id: String,
    evidence_kind: String,
    coverage_policy: String,
    policy_source: String,
    policy: RbdReplicaPolicy,
    report: InventoryCoverageReport,
    report_digest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsignedCephScopeCoverageArtifact<'a> {
    schema_version: u32,
    ceph_scope_id: &'a str,
    evidence_kind: &'a str,
    coverage_policy: &'a str,
    policy_source: &'a str,
    policy: &'a RbdReplicaPolicy,
    report: &'a InventoryCoverageReport,
}

pub fn write_ceph_scope_coverage_report(
    case_root: &Path,
    ceph_scope_id: &str,
    report: &InventoryCoverageReport,
) -> Result<PathBuf> {
    validate_scope_id(ceph_scope_id)?;
    if report.policy.validate().is_err() || report.expected_count != report.policy.expected_count()
    {
        return Err(ClusterServiceError::InvalidCoverageReport);
    }
    let report_path = report_path(case_root, ceph_scope_id);
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut artifact = CephScopeCoverageArtifact {
        schema_version: 1,
        ceph_scope_id: ceph_scope_id.to_string(),
        evidence_kind: "rbd_osd_inventory".to_string(),
        coverage_policy: report.policy.storage_key().to_string(),
        policy_source: report.policy_source().to_string(),
        policy: report.policy.clone(),
        report: report.clone(),
        report_digest: String::new(),
    };
    artifact.report_digest = digest(&artifact)?;
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

pub fn read_ceph_scope_coverage_report(
    case_root: &Path,
    ceph_scope_id: &str,
) -> Result<Option<InventoryCoverageReport>> {
    validate_scope_id(ceph_scope_id)?;
    let report_path = report_path(case_root, ceph_scope_id);
    let payload = match std::fs::read_to_string(report_path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let artifact: CephScopeCoverageArtifact = serde_json::from_str(&payload)?;
    if artifact.schema_version != 1
        || artifact.ceph_scope_id != ceph_scope_id
        || artifact.evidence_kind != "rbd_osd_inventory"
        || artifact.coverage_policy != artifact.policy.storage_key()
        || artifact.policy_source != artifact.policy.source()
        || artifact.policy != artifact.report.policy
        || artifact.report.policy.validate().is_err()
        || artifact.report.observed_count > artifact.report.expected_count
        || artifact.report.expected_count != artifact.policy.expected_count()
        || digest(&artifact)? != artifact.report_digest
    {
        return Err(ClusterServiceError::InvalidCoverageReport);
    }
    Ok(Some(artifact.report))
}

fn digest(artifact: &CephScopeCoverageArtifact) -> Result<String> {
    let unsigned = UnsignedCephScopeCoverageArtifact {
        schema_version: artifact.schema_version,
        ceph_scope_id: &artifact.ceph_scope_id,
        evidence_kind: &artifact.evidence_kind,
        coverage_policy: &artifact.coverage_policy,
        policy_source: &artifact.policy_source,
        policy: &artifact.policy,
        report: &artifact.report,
    };
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&unsigned)?)))
}

fn validate_scope_id(scope_id: &str) -> Result<()> {
    if scope_id.is_empty()
        || scope_id.len() > 160
        || !scope_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
    {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    Ok(())
}

fn report_path(case_root: &Path, ceph_scope_id: &str) -> PathBuf {
    scope_storage::artifact_path(case_root, ceph_scope_id, "ceph-coverage", "coverage.json")
}
