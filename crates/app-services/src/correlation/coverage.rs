//! Family coverage computation for correlation snapshots.
//!
//! This module derives per-family coverage status from correlation leads and
//! clusters. It is split out of `graph.rs` to keep the snapshot builder under
//! the crate's file-size guideline.

use super::{
    artifact_family, dedup_vec, has_family, CorrelationRuleGroup, CorrelationSourceGroup,
    CORRELATION_RULE_FAMILIES,
};
use transport::dto::{
    CorrelationClusterDto, CorrelationConfidenceDto, CorrelationCoverageStatusDto,
    CorrelationFamilyCoverageDto, CorrelationLeadDto,
};

pub(crate) fn build_family_coverage(
    leads: &[CorrelationLeadDto],
    clusters: &[CorrelationClusterDto],
) -> Vec<CorrelationFamilyCoverageDto> {
    CORRELATION_RULE_FAMILIES
        .iter()
        .map(|(family, display_name)| {
            let family_token = family.to_ascii_lowercase();
            let related_leads = family_leads_for(leads, family, &family_token);
            let cluster_count = family_clusters_for(clusters, family, &family_token);
            let lead_count = related_leads.len() as u32;
            let high_confidence_lead_count = family_high_confidence_lead_count(&related_leads);
            let review_lead_count = family_review_lead_count(&related_leads);
            let sample_signals = family_sample_signals(&related_leads, &family_token);
            let status = family_coverage_status(lead_count, high_confidence_lead_count);

            CorrelationFamilyCoverageDto {
                family: (*family).to_string(),
                display_name: (*display_name).to_string(),
                status,
                lead_count,
                high_confidence_lead_count,
                review_lead_count,
                cluster_count,
                sample_signals,
            }
        })
        .collect()
}

fn family_leads_for<'a>(
    leads: &'a [CorrelationLeadDto],
    family: &str,
    family_token: &str,
) -> Vec<&'a CorrelationLeadDto> {
    leads
        .iter()
        .filter(|lead| {
            has_family(&lead.families, family)
                || lead.provenance.iter().any(|item| {
                    item.source_label.eq_ignore_ascii_case(family)
                        || item.source_kind.eq_ignore_ascii_case(family)
                        || item
                            .producer
                            .as_deref()
                            .map(|producer| producer.to_ascii_lowercase().contains(family_token))
                            .unwrap_or(false)
                })
                || lead
                    .match_signals
                    .iter()
                    .any(|signal| signal.to_ascii_lowercase().contains(family_token))
        })
        .collect()
}

fn family_clusters_for(
    clusters: &[CorrelationClusterDto],
    family: &str,
    family_token: &str,
) -> u32 {
    clusters
        .iter()
        .filter(|cluster| {
            has_family(&cluster.families, family)
                || cluster.provenance.iter().any(|item| {
                    item.source_label.eq_ignore_ascii_case(family)
                        || item.source_kind.eq_ignore_ascii_case(family)
                        || item
                            .producer
                            .as_deref()
                            .map(|producer| producer.to_ascii_lowercase().contains(family_token))
                            .unwrap_or(false)
                })
                || cluster.summary.to_ascii_lowercase().contains(family_token)
        })
        .count() as u32
}

fn family_high_confidence_lead_count(related_leads: &[&CorrelationLeadDto]) -> u32 {
    related_leads
        .iter()
        .filter(|lead| {
            matches!(
                lead.confidence,
                CorrelationConfidenceDto::Direct | CorrelationConfidenceDto::Strong
            )
        })
        .count() as u32
}

fn family_review_lead_count(related_leads: &[&CorrelationLeadDto]) -> u32 {
    related_leads
        .iter()
        .filter(|lead| {
            !lead.caveats.is_empty()
                || matches!(
                    lead.confidence,
                    CorrelationConfidenceDto::Weak | CorrelationConfidenceDto::Heuristic
                )
        })
        .count() as u32
}

fn family_sample_signals(related_leads: &[&CorrelationLeadDto], family_token: &str) -> Vec<String> {
    let mut sample_signals = related_leads
        .iter()
        .flat_map(|lead| lead.match_signals.iter().cloned())
        .filter(|signal| signal.to_ascii_lowercase().contains(family_token))
        .take(3)
        .collect::<Vec<_>>();
    if sample_signals.is_empty() {
        sample_signals = related_leads
            .iter()
            .flat_map(|lead| lead.match_signals.iter().cloned())
            .take(3)
            .collect::<Vec<_>>();
    }
    sample_signals
}

fn family_coverage_status(
    lead_count: u32,
    high_confidence_lead_count: u32,
) -> CorrelationCoverageStatusDto {
    if lead_count == 0 {
        CorrelationCoverageStatusDto::Missing
    } else if high_confidence_lead_count > 0 {
        CorrelationCoverageStatusDto::Covered
    } else {
        CorrelationCoverageStatusDto::Review
    }
}

pub(crate) fn derive_source_group_families(group: &CorrelationSourceGroup) -> Vec<String> {
    let mut families = group
        .artifacts
        .iter()
        .filter_map(|artifact| artifact_family(&artifact.artifact_type))
        .collect::<Vec<_>>();
    dedup_vec(&mut families);
    families
}

pub(crate) fn derive_rule_group_families(group: &CorrelationRuleGroup) -> Vec<String> {
    let mut families = group
        .matches
        .iter()
        .filter_map(|item| artifact_family(&item.artifact.artifact_type))
        .collect::<Vec<_>>();
    dedup_vec(&mut families);
    families
}
