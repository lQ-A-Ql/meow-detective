use std::collections::BTreeSet;
use std::path::Path;

use domain::CaseId;
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use transport::dto::{
    PlatformCoverageDto, ReleaseGateEntryDto, ReleaseGateStatusDto, V2GovernanceSnapshotDto,
};

use super::artifact_family_platform::{classify_artifact_family, ArtifactFamilyPlatform};
use super::V3GovernanceError;
use crate::artifact_service::SourceArtifactFamilyCount;

const PLATFORM_INTEGRITY_GATE_ID: &str = "source-platform-artifact-integrity";
const MAX_RENDERED_FINDINGS: usize = 20;

pub(super) struct PlatformCoverageAssessment {
    pub coverage: PlatformCoverageDto,
    mismatches: Vec<PlatformMismatch>,
    unclassified: Vec<SourceArtifactFamilyCount>,
}

struct PlatformMismatch {
    observed: SourceArtifactFamilyCount,
    expected_platform: ArtifactFamilyPlatform,
}

pub(super) fn build_platform_coverage(
    conn: &Connection,
) -> Result<PlatformCoverageDto, V3GovernanceError> {
    let family_counts = ArtifactRepo::new(conn)
        .count_by_family()
        .map_err(|error| V3GovernanceError::Other(error.to_string()))?;
    let mut windows = BTreeSet::new();
    let mut linux = BTreeSet::new();
    let mut observed = BTreeSet::new();

    for (family, _) in family_counts {
        observed.insert(family.clone());
        match classify_artifact_family(&family) {
            ArtifactFamilyPlatform::Windows => {
                windows.insert(family);
            }
            ArtifactFamilyPlatform::Linux => {
                linux.insert(family);
            }
            ArtifactFamilyPlatform::Unknown => {}
        }
    }

    Ok(coverage_dto(windows, linux, observed.len()))
}

pub(super) fn build_platform_coverage_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<PlatformCoverageAssessment, V3GovernanceError> {
    let source_counts =
        crate::artifact_service::get_source_attributed_artifact_family_counts_for_case(
            conn, case_root, case_id,
        )?;
    let mut windows = BTreeSet::new();
    let mut linux = BTreeSet::new();
    let mut observed = BTreeSet::new();
    let mut mismatches = Vec::new();
    let mut unclassified = Vec::new();

    for source_count in source_counts {
        observed.insert(source_count.family.clone());
        let family_platform = classify_artifact_family(&source_count.family);
        match family_platform {
            ArtifactFamilyPlatform::Unknown => unclassified.push(source_count),
            ArtifactFamilyPlatform::Windows if family_platform.matches(source_count.platform) => {
                windows.insert(source_count.family);
            }
            ArtifactFamilyPlatform::Linux if family_platform.matches(source_count.platform) => {
                linux.insert(source_count.family);
            }
            expected_platform => mismatches.push(PlatformMismatch {
                observed: source_count,
                expected_platform,
            }),
        }
    }

    Ok(PlatformCoverageAssessment {
        coverage: coverage_dto(windows, linux, observed.len()),
        mismatches,
        unclassified,
    })
}

pub(super) fn apply_platform_integrity_gate(
    snapshot: &mut V2GovernanceSnapshotDto,
    assessment: &PlatformCoverageAssessment,
) {
    log_integrity_findings(assessment);
    snapshot
        .release_gates
        .push(platform_integrity_gate(assessment));
    snapshot.release_scorecard = crate::governance::scoring::release_scorecard(
        &snapshot.release_gates,
        &snapshot.runtime_signals,
    );
}

fn coverage_dto(
    windows: BTreeSet<String>,
    linux: BTreeSet<String>,
    total_families: usize,
) -> PlatformCoverageDto {
    PlatformCoverageDto {
        windows_artifact_families: windows.len() as u32,
        linux_artifact_families: linux.len() as u32,
        cross_platform_artifact_families: 0,
        total_families: total_families as u32,
        windows_families: windows.into_iter().collect(),
        linux_families: linux.into_iter().collect(),
        cross_platform_families: Vec::new(),
    }
}

fn platform_integrity_gate(assessment: &PlatformCoverageAssessment) -> ReleaseGateEntryDto {
    let status = if !assessment.mismatches.is_empty() {
        ReleaseGateStatusDto::Blocked
    } else if !assessment.unclassified.is_empty() {
        ReleaseGateStatusDto::Warning
    } else {
        ReleaseGateStatusDto::Passed
    };
    let evidence = format!(
        "mismatches={}, unclassified={}{}{}",
        assessment.mismatches.len(),
        assessment.unclassified.len(),
        render_mismatches(&assessment.mismatches),
        render_unclassified(&assessment.unclassified),
    );
    let detail = match status {
        ReleaseGateStatusDto::Blocked => {
            "Artifact families conflict with their persisted data-source platforms; source isolation is not trustworthy until the contaminated records are removed and extraction is rerun."
        }
        ReleaseGateStatusDto::Warning => {
            "Artifact families without a production platform classification were found; platform isolation requires investigator review."
        }
        ReleaseGateStatusDto::Passed => {
            "Every classified artifact family matches the persisted platform of its ready data source."
        }
    };

    ReleaseGateEntryDto {
        gate_id: PLATFORM_INTEGRITY_GATE_ID.to_string(),
        title: "Data-source artifact platform integrity".to_string(),
        status,
        evidence,
        detail: detail.to_string(),
    }
}

fn render_mismatches(mismatches: &[PlatformMismatch]) -> String {
    let rendered = mismatches
        .iter()
        .take(MAX_RENDERED_FINDINGS)
        .map(|finding| {
            format!(
                "sourceId={},persistedPlatform={},family={},expectedPlatform={},count={}",
                finding.observed.data_source_id.0,
                finding.observed.platform,
                finding.observed.family,
                finding.expected_platform.as_str(),
                finding.observed.count,
            )
        })
        .collect::<Vec<_>>();
    render_finding_group("mismatch", &rendered, mismatches.len())
}

fn render_unclassified(unclassified: &[SourceArtifactFamilyCount]) -> String {
    let rendered = unclassified
        .iter()
        .take(MAX_RENDERED_FINDINGS)
        .map(|finding| {
            format!(
                "sourceId={},persistedPlatform={},family={},count={}",
                finding.data_source_id.0, finding.platform, finding.family, finding.count,
            )
        })
        .collect::<Vec<_>>();
    render_finding_group("unclassified", &rendered, unclassified.len())
}

fn render_finding_group(label: &str, rendered: &[String], total: usize) -> String {
    if rendered.is_empty() {
        return String::new();
    }
    let omitted = total.saturating_sub(rendered.len());
    if omitted == 0 {
        format!("; {label}=[{}]", rendered.join(" | "))
    } else {
        format!("; {label}=[{} | omitted={omitted}]", rendered.join(" | "))
    }
}

fn log_integrity_findings(assessment: &PlatformCoverageAssessment) {
    for finding in &assessment.mismatches {
        tracing::warn!(
            data_source_id = %finding.observed.data_source_id.0,
            persisted_platform = %finding.observed.platform,
            artifact_family = %finding.observed.family,
            expected_platform = finding.expected_platform.as_str(),
            artifact_count = finding.observed.count,
            "artifact family conflicts with persisted data-source platform"
        );
    }
    for finding in &assessment.unclassified {
        tracing::warn!(
            data_source_id = %finding.data_source_id.0,
            persisted_platform = %finding.platform,
            artifact_family = %finding.family,
            artifact_count = finding.count,
            "artifact family has no production platform classification"
        );
    }
}
