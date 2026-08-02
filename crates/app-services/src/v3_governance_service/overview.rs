use std::collections::BTreeMap;
use std::path::Path;

use chrono::Utc;
use domain::CaseId;
use rusqlite::Connection;
use transport::dto::{CaseOverviewSnapshotDto, CorrelationOverviewDto, FamilyCountDto};

use super::platform_coverage::assess_source_artifact_counts;
use super::{build_batch_status, build_rule_pack_status, V3GovernanceError};

pub fn get_case_overview_snapshot_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<CaseOverviewSnapshotDto, V3GovernanceError> {
    let case_id = CaseId(case_id.to_string());
    let data_sources = crate::file_service::get_data_sources_for_case(conn, case_root, &case_id)
        .map_err(|error| {
            V3GovernanceError::Other(format!("load overview data sources: {error}"))
        })?;
    let timeline_event_count =
        crate::timeline_service::count_timeline_events_for_case(conn, case_root, &case_id)
            .map_err(|error| {
                V3GovernanceError::Other(format!("count overview timeline events: {error}"))
            })?;
    let source_counts =
        crate::artifact_service::get_source_attributed_artifact_family_counts_for_case(
            conn, case_root, &case_id,
        )?;
    let artifact_family_counts = aggregate_artifact_family_counts(&source_counts);
    let platform_coverage = assess_source_artifact_counts(source_counts).coverage;
    let correlation = crate::correlation::get_correlation_snapshot_for_case(
        conn, case_root, &case_id,
    )
    .map_err(|error| {
        V3GovernanceError::Other(format!("load overview correlation statistics: {error}"))
    })?;

    Ok(CaseOverviewSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        data_sources,
        timeline_event_count,
        artifact_family_counts,
        correlation_statistics: CorrelationOverviewDto {
            node_count: correlation.node_count,
            edge_count: correlation.edge_count,
            cluster_count: correlation.cluster_count,
            lead_count: correlation.lead_count,
            family_coverage: correlation.family_coverage,
        },
        platform_coverage,
        rule_pack_coverage: build_rule_pack_status(),
        batch_status: build_batch_status(conn, &case_id.0)?,
    })
}

fn aggregate_artifact_family_counts(
    source_counts: &[crate::artifact_service::SourceArtifactFamilyCount],
) -> Vec<FamilyCountDto> {
    let mut counts = BTreeMap::<String, u64>::new();
    for source_count in source_counts {
        let count = counts.entry(source_count.family.clone()).or_default();
        *count = count.saturating_add(source_count.count);
    }
    counts
        .into_iter()
        .map(|(family, count)| FamilyCountDto { family, count })
        .collect()
}
