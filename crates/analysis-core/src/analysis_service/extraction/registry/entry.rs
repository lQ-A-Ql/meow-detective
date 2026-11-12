use super::super::ExtractionOutcome;
use super::context::RegistryExtractionContext;
use super::shared::hive_meta_artifact;
use super::warnings::govern_registry_warnings;
use super::{dispatch, recovery};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use artifacts_windows::registry::RegistrySnapshot;

pub fn extract_registry_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> ExtractionOutcome {
    if !bytes.starts_with(b"regf") {
        return ExtractionOutcome {
            warnings: govern_registry_warnings(
                &candidate.path,
                vec![format!("{} is not a regf registry hive", candidate.path)],
            ),
            ..ExtractionOutcome::default()
        };
    }
    match RegistrySnapshot::prepare(bytes, txlog1, txlog2) {
        Ok(snapshot) => extract_snapshot_candidate(candidate, &snapshot, boot_key),
        Err(error) => ExtractionOutcome {
            warnings: govern_registry_warnings(
                &candidate.path,
                vec![format!(
                    "{} registry recovery failed: {error}",
                    candidate.path
                )],
            ),
            ..ExtractionOutcome::default()
        },
    }
}

pub(in crate::analysis_service::extraction) fn extract_snapshot_candidate(
    candidate: &EvidenceCandidate,
    snapshot: &RegistrySnapshot<'_>,
    boot_key: Option<[u8; 16]>,
) -> ExtractionOutcome {
    let bytes = snapshot.bytes();
    let mut outcome = ExtractionOutcome::default();
    let mut raw_warnings = Vec::new();
    if !bytes.starts_with(b"regf") {
        raw_warnings.push(format!("{} is not a regf registry hive", candidate.path));
        outcome.warnings = govern_registry_warnings(&candidate.path, raw_warnings);
        return outcome;
    }

    let normalized = normalize_evidence_path(&candidate.path);
    let txlog_merged = snapshot.logs_applied();
    let deleted_keys_found =
        recovery::count_deleted_cells(&candidate.path, bytes, &mut raw_warnings);

    dispatch::extract(
        &normalized,
        RegistryExtractionContext {
            candidate,
            bytes: snapshot,
            boot_key,
            outcome: &mut outcome,
            warnings: &mut raw_warnings,
        },
    );

    if let Some(report) = snapshot.report() {
        raw_warnings.extend(
            report
                .diagnostics
                .iter()
                .map(|error| format!("{} registry recovery diagnostic: {error}", candidate.path)),
        );
    }

    let view = if snapshot.logs_applied() {
        "recovered"
    } else {
        "base"
    };
    for artifact in &mut outcome.artifacts {
        artifact.attrs.insert("registryView".into(), view.into());
    }
    for event in &mut outcome.timeline_events {
        event.attrs.insert("registryView".into(), view.into());
    }

    outcome.artifacts.push(hive_meta_artifact(
        candidate,
        txlog_merged,
        deleted_keys_found,
    ));
    if let (Some(report), Some(metadata)) = (snapshot.report(), outcome.artifacts.last_mut()) {
        metadata.attrs.insert(
            "recoveryEntriesApplied".into(),
            report.applied_entries.into(),
        );
        metadata
            .attrs
            .insert("recoveryPagesApplied".into(), report.applied_pages.into());
        metadata.attrs.insert("registryView".into(), view.into());
        metadata.attrs.insert(
            "recoveryPartial".into(),
            (report.state == artifacts_windows::registry::log_replay::RecoveryState::Partial)
                .into(),
        );
    }
    raw_warnings.append(&mut outcome.warnings);
    outcome.warnings = govern_registry_warnings(&candidate.path, raw_warnings);
    outcome
}
