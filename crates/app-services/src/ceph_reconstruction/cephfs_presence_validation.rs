use std::collections::BTreeSet;

use chrono::DateTime;

use super::cephfs_presence::{
    CephFsMapPresenceSnapshot, CephFsMdsMapPresenceSnapshot, CephFsPresenceAssessment,
    CephFsPresenceDiagnostic, CephFsPresenceEvidence, CephFsPresenceMapKind, CephFsPresenceState,
};
use super::cephfs_presence_bindings::{
    canonical_filesystem_ids, canonical_filesystems, validate_filesystem_bindings,
    validate_unique_filesystems, validate_unique_mds_filesystems,
};

const PRESENCE_SCHEMA_VERSION: u32 = 1;

struct SnapshotHeader<'a> {
    schema_version: u32,
    cluster_identity: &'a str,
    source_identity: &'a str,
    inventory_identity: &'a str,
    epoch: u64,
    captured_at: &'a str,
}

pub(super) fn assess_presence(
    evidence: &[CephFsPresenceEvidence],
    expected_source_count: usize,
) -> CephFsPresenceAssessment {
    if evidence.is_empty() {
        return indeterminate(0, vec![CephFsPresenceDiagnostic::NoSourceEvidence]);
    }

    let mut diagnostics = Vec::new();
    let unique_source_count = evidence
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    if evidence.len() != expected_source_count || unique_source_count != evidence.len() {
        diagnostics.push(CephFsPresenceDiagnostic::SourceSetIncomplete {
            expected: expected_source_count,
            observed: unique_source_count,
        });
    }

    let mut valid_fsmap = Vec::new();
    let mut valid_mdsmap = Vec::new();
    for source in evidence {
        collect_snapshot(
            source,
            CephFsPresenceMapKind::Fsmap,
            source.fsmap.as_ref(),
            source.fsmap_error.as_deref(),
            &mut diagnostics,
            &mut valid_fsmap,
        );
        collect_snapshot(
            source,
            CephFsPresenceMapKind::Mdsmap,
            source.mdsmap.as_ref(),
            source.mdsmap_error.as_deref(),
            &mut diagnostics,
            &mut valid_mdsmap,
        );
    }

    if !diagnostics.is_empty() {
        return indeterminate(evidence.len(), diagnostics);
    }

    let fsmap_epoch = valid_fsmap[0].epoch;
    let mdsmap_epoch = valid_mdsmap[0].epoch;
    compare_snapshot_consistency(
        &valid_fsmap,
        CephFsPresenceMapKind::Fsmap,
        fsmap_epoch,
        &mut diagnostics,
    );
    compare_snapshot_consistency(
        &valid_mdsmap,
        CephFsPresenceMapKind::Mdsmap,
        mdsmap_epoch,
        &mut diagnostics,
    );

    for (source, fsmap, mdsmap) in evidence
        .iter()
        .zip(valid_fsmap.iter())
        .zip(valid_mdsmap.iter())
        .map(|((source, fsmap), mdsmap)| (source, fsmap, mdsmap))
    {
        if fsmap.cluster_identity != mdsmap.cluster_identity {
            diagnostics.push(CephFsPresenceDiagnostic::FsmapMdsmapMismatch {
                source_id: source.source_id.clone(),
                reason: "FSMap and MDSMap cluster identities differ".to_string(),
            });
        }
        if fsmap.epoch != mdsmap.fsmap_epoch {
            diagnostics.push(CephFsPresenceDiagnostic::FsmapMdsmapMismatch {
                source_id: source.source_id.clone(),
                reason: format!(
                    "MDSMap references FSMap epoch {}, observed {}",
                    mdsmap.fsmap_epoch, fsmap.epoch
                ),
            });
        }
        validate_filesystem_bindings(fsmap, mdsmap, &mut diagnostics);
    }

    let first_fsmap = valid_fsmap[0];
    if !diagnostics.is_empty() {
        return indeterminate(evidence.len(), diagnostics);
    }
    completed_assessment(evidence, first_fsmap, fsmap_epoch, mdsmap_epoch)
}

fn completed_assessment(
    evidence: &[CephFsPresenceEvidence],
    fsmap: &CephFsMapPresenceSnapshot,
    fsmap_epoch: u64,
    mdsmap_epoch: u64,
) -> CephFsPresenceAssessment {
    let filesystems = canonical_filesystems(&fsmap.filesystems);
    let mut source_ids = evidence
        .iter()
        .map(|source| source.source_id.clone())
        .collect::<Vec<_>>();
    source_ids.sort();
    CephFsPresenceAssessment {
        state: if filesystems.is_empty() {
            CephFsPresenceState::Absent
        } else {
            CephFsPresenceState::Present
        },
        source_count: evidence.len(),
        source_ids,
        cluster_identity: Some(fsmap.cluster_identity.clone()),
        filesystem_count: filesystems.len(),
        filesystems,
        fsmap_epoch: Some(fsmap_epoch),
        mdsmap_epoch: Some(mdsmap_epoch),
        diagnostics: Vec::new(),
    }
}

fn indeterminate(
    source_count: usize,
    diagnostics: Vec<CephFsPresenceDiagnostic>,
) -> CephFsPresenceAssessment {
    CephFsPresenceAssessment {
        state: CephFsPresenceState::Indeterminate,
        source_count,
        source_ids: Vec::new(),
        cluster_identity: None,
        filesystem_count: 0,
        filesystems: Vec::new(),
        fsmap_epoch: None,
        mdsmap_epoch: None,
        diagnostics,
    }
}

fn collect_snapshot<'a, T>(
    source: &CephFsPresenceEvidence,
    map: CephFsPresenceMapKind,
    snapshot: Option<&'a T>,
    decode_error: Option<&str>,
    diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
    valid: &mut Vec<&'a T>,
) where
    T: SnapshotValidation,
{
    if let Some(error) = decode_error {
        diagnostics.push(CephFsPresenceDiagnostic::MalformedSnapshot {
            source_id: source.source_id.clone(),
            map,
            reason: error.to_string(),
        });
        return;
    }
    let Some(snapshot) = snapshot else {
        diagnostics.push(CephFsPresenceDiagnostic::MissingSnapshot {
            source_id: source.source_id.clone(),
            map,
        });
        return;
    };
    snapshot.validate(&source.source_id, map, diagnostics);
    if !has_source_diagnostic(diagnostics, &source.source_id, map) {
        valid.push(snapshot);
    }
}

fn has_source_diagnostic(
    diagnostics: &[CephFsPresenceDiagnostic],
    source_id: &str,
    map: CephFsPresenceMapKind,
) -> bool {
    diagnostics.iter().any(|diagnostic| match diagnostic {
        CephFsPresenceDiagnostic::MalformedSnapshot {
            source_id: id,
            map: kind,
            ..
        }
        | CephFsPresenceDiagnostic::FreshnessUnproven {
            source_id: id,
            map: kind,
            ..
        }
        | CephFsPresenceDiagnostic::SnapshotIdentityMismatch {
            source_id: id,
            map: kind,
            ..
        } => id == source_id && *kind == map,
        _ => false,
    })
}

trait SnapshotValidation {
    fn validate(
        &self,
        source_id: &str,
        map: CephFsPresenceMapKind,
        diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
    );
}

impl SnapshotValidation for CephFsMapPresenceSnapshot {
    fn validate(
        &self,
        source_id: &str,
        map: CephFsPresenceMapKind,
        diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
    ) {
        validate_common_snapshot(
            SnapshotHeader {
                schema_version: self.schema_version,
                cluster_identity: &self.cluster_identity,
                source_identity: &self.source_identity,
                inventory_identity: &self.inventory_identity,
                epoch: self.epoch,
                captured_at: &self.captured_at,
            },
            source_id,
            map,
            diagnostics,
        );
        validate_unique_filesystems(&self.filesystems, diagnostics);
    }
}

impl SnapshotValidation for CephFsMdsMapPresenceSnapshot {
    fn validate(
        &self,
        source_id: &str,
        map: CephFsPresenceMapKind,
        diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
    ) {
        validate_common_snapshot(
            SnapshotHeader {
                schema_version: self.schema_version,
                cluster_identity: &self.cluster_identity,
                source_identity: &self.source_identity,
                inventory_identity: &self.inventory_identity,
                epoch: self.epoch,
                captured_at: &self.captured_at,
            },
            source_id,
            map,
            diagnostics,
        );
        if self.fsmap_epoch == 0 {
            diagnostics.push(CephFsPresenceDiagnostic::MalformedSnapshot {
                source_id: source_id.to_string(),
                map,
                reason: "MDSMap FSMap epoch must be greater than zero".to_string(),
            });
        }
        validate_unique_mds_filesystems(source_id, &self.filesystems, diagnostics);
    }
}

fn validate_common_snapshot(
    header: SnapshotHeader<'_>,
    source_id: &str,
    map: CephFsPresenceMapKind,
    diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
) {
    if header.schema_version != PRESENCE_SCHEMA_VERSION {
        diagnostics.push(CephFsPresenceDiagnostic::MalformedSnapshot {
            source_id: source_id.to_string(),
            map,
            reason: format!(
                "unsupported snapshot schema version {}",
                header.schema_version
            ),
        });
    }
    if header.cluster_identity.trim().is_empty()
        || header.source_identity.trim() != source_id
        || header.inventory_identity.trim().is_empty()
    {
        diagnostics.push(CephFsPresenceDiagnostic::SnapshotIdentityMismatch {
            source_id: source_id.to_string(),
            map,
            reason: "cluster, source, and inventory identities are incomplete or inconsistent"
                .to_string(),
        });
    }
    if header.epoch == 0 {
        diagnostics.push(CephFsPresenceDiagnostic::MalformedSnapshot {
            source_id: source_id.to_string(),
            map,
            reason: "map epoch must be greater than zero".to_string(),
        });
    }
    if DateTime::parse_from_rfc3339(header.captured_at).is_err() {
        diagnostics.push(CephFsPresenceDiagnostic::FreshnessUnproven {
            source_id: source_id.to_string(),
            map,
            reason: "capture timestamp is not RFC3339".to_string(),
        });
    }
}

fn compare_snapshot_consistency<T: SnapshotIdentity>(
    snapshots: &[T],
    map: CephFsPresenceMapKind,
    expected_epoch: u64,
    diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
) {
    let first = &snapshots[0];
    for snapshot in snapshots.iter().skip(1) {
        if snapshot.cluster_identity() != first.cluster_identity() {
            diagnostics.push(CephFsPresenceDiagnostic::ConflictingClusterIdentity {
                source_id: snapshot.source_id().to_string(),
                expected: first.cluster_identity().to_string(),
                observed: snapshot.cluster_identity().to_string(),
            });
        }
        if snapshot.epoch() != expected_epoch {
            diagnostics.push(CephFsPresenceDiagnostic::ConflictingMapEpoch {
                source_id: snapshot.source_id().to_string(),
                map,
                expected: expected_epoch,
                observed: snapshot.epoch(),
            });
        }
        let expected_filesystems = first.filesystem_ids();
        let observed_filesystems = snapshot.filesystem_ids();
        if observed_filesystems != expected_filesystems {
            diagnostics.push(CephFsPresenceDiagnostic::ConflictingFilesystemSet {
                source_id: snapshot.source_id().to_string(),
                map,
                expected: expected_filesystems,
                observed: observed_filesystems,
            });
        }
    }
}

trait SnapshotIdentity {
    fn cluster_identity(&self) -> &str;
    fn filesystem_ids(&self) -> Vec<u64>;
    fn source_id(&self) -> &str;
    fn epoch(&self) -> u64;
}

impl SnapshotIdentity for &CephFsMapPresenceSnapshot {
    fn cluster_identity(&self) -> &str {
        &self.cluster_identity
    }

    fn filesystem_ids(&self) -> Vec<u64> {
        canonical_filesystem_ids(
            self.filesystems
                .iter()
                .map(|filesystem| filesystem.filesystem_id),
        )
    }

    fn source_id(&self) -> &str {
        &self.source_identity
    }

    fn epoch(&self) -> u64 {
        self.epoch
    }
}

impl SnapshotIdentity for &CephFsMdsMapPresenceSnapshot {
    fn cluster_identity(&self) -> &str {
        &self.cluster_identity
    }

    fn filesystem_ids(&self) -> Vec<u64> {
        canonical_filesystem_ids(
            self.filesystems
                .iter()
                .map(|filesystem| filesystem.filesystem_id),
        )
    }

    fn source_id(&self) -> &str {
        &self.source_identity
    }

    fn epoch(&self) -> u64 {
        self.epoch
    }
}
