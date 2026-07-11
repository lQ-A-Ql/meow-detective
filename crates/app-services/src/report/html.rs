use super::{correlation_confidence_str, ReportCorrelation, ReportError, ReportGovernance};
use domain::TimelineEvent;
use reports::HtmlCorrelationLeadSection;
use transport::commands::ExportScopeDto;
use transport::dto::{
    ArtifactRowDto, BenchmarkRequirementStatusDto, CorrelationCoverageStatusDto,
    CorrelationLeadDto, KnownLimitationStatusDto, ReleaseGateStatusDto, SupportMaturityDto,
    TimelineEventDto, VerificationGuaranteeLevelDto, VerificationResultDto,
};

// ---------------------------------------------------------------------------
// Governance rows
// ---------------------------------------------------------------------------

pub(crate) fn report_governance_rows(
    governance: &ReportGovernance,
    scope: &ExportScopeDto,
) -> Vec<String> {
    if !scope.file_system_metadata && !scope.full_timeline {
        return Vec::new();
    }

    let snapshot = &governance.snapshot;
    let mut rows = vec![
        format!(
            "governance summary generatedAt={} grade={} totalScore={} verification={} correlation={} performance={} security={}",
            snapshot.generated_at,
            snapshot.release_scorecard.grade,
            snapshot.release_scorecard.total_score,
            snapshot.release_scorecard.verification_score,
            snapshot.release_scorecard.correlation_score,
            snapshot.release_scorecard.performance_score,
            snapshot.release_scorecard.security_score
        ),
        format!(
            "governance supportMatrix ga={} beta={} experimental={} unsupported={} documentedLimit={}",
            snapshot.support_matrix.ga_count,
            snapshot.support_matrix.beta_count,
            snapshot.support_matrix.experimental_count,
            snapshot.support_matrix.unsupported_count,
            snapshot.support_matrix.documented_limit_count
        ),
        format!(
            "governance benchmarkSummary baselineVersion={} coveredRequired={} missingRequired={} exceededRequired={}",
            snapshot.benchmark.baseline_version,
            snapshot.benchmark.covered_required_count,
            snapshot.benchmark.missing_required_count,
            snapshot.benchmark.exceeded_required_count
        ),
    ];

    rows.extend(snapshot.release_gates.iter().map(|gate| {
        format!(
            "governance gate={} status={} evidence={} detail={}",
            gate.gate_id,
            release_gate_status_str(&gate.status),
            gate.evidence,
            gate.detail
        )
    }));
    rows.extend(snapshot.runtime_results.checks.iter().map(|check| {
        format!(
            "governance runtimeCheck={} status={} checkedAt={} evidence={} detail={}",
            check.check_id,
            release_gate_status_str(&check.status),
            check.checked_at,
            check.evidence,
            check.detail
        )
    }));
    rows.extend(snapshot.runtime_results.checks.iter().flat_map(|check| {
        check.sub_checks.iter().map(move |sub_check| {
            format!(
                "governance runtimeSubcheck={} parent={} status={} evidence={} detail={}",
                sub_check.check_id,
                check.check_id,
                release_gate_status_str(&sub_check.status),
                sub_check.evidence,
                sub_check.detail
            )
        })
    }));
    rows.extend(snapshot.verification_chains.iter().map(|chain| {
        format!(
            "governance chain={} displayName={} result={} maturity={} guarantee={} fixtureTier={} expectedJson={} sampleCount={}",
            chain.chain,
            chain.display_name,
            verification_result_str(&chain.result),
            support_maturity_str(&chain.maturity),
            guarantee_level_str(&chain.guarantee_level),
            chain.fixture_tier,
            chain.expected_json_version,
            chain.verified_sample_count
        )
    }));
    rows.extend(snapshot.benchmark.required_checks.iter().map(|check| {
        format!(
            "governance benchmarkCheck datasetLevel={} scenario={} status={} thresholdP95Ms={} measuredP95Ms={}",
            check.dataset_level,
            check.scenario,
            benchmark_requirement_status_str(&check.status),
            check.threshold_p95_ms,
            check
                .measured_p95_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    }));
    rows.extend(snapshot.fact_sources.iter().map(|source| {
        format!(
            "governance factSource area={} factFile={} factKind={} lastVerifiedAt={} outputs={}",
            source.area,
            source.fact_file,
            source.fact_kind,
            source.last_verified_at,
            source.derived_outputs.join(" | ")
        )
    }));
    rows.extend(snapshot.known_limitations.iter().map(|item| {
        format!(
            "governance knownLimitation category={} item={} status={} affectedChains={} sourceDoc={} summary={}",
            item.category,
            item.item,
            known_limitation_status_str(&item.status),
            item.affected_chains.join(" | "),
            item.source_doc,
            item.summary
        )
    }));
    rows.extend(snapshot.runtime_signals.correlation_family_coverage.iter().map(|family| {
        format!(
            "governance correlationFamily family={} displayName={} status={} leads={} highConfidenceLeads={} reviewLeads={} clusters={} signals={}",
            family.family,
            family.display_name,
            correlation_coverage_status_str(&family.status),
            family.lead_count,
            family.high_confidence_lead_count,
            family.review_lead_count,
            family.cluster_count,
            family.sample_signals.join(" | ")
        )
    }));
    rows
}

// ---------------------------------------------------------------------------
// Correlation rows
// ---------------------------------------------------------------------------

pub(crate) fn report_correlation_rows(
    correlation: &ReportCorrelation,
    scope: &ExportScopeDto,
) -> Vec<String> {
    if !scope.file_system_metadata && !scope.full_timeline {
        return Vec::new();
    }

    let mut rows = Vec::new();
    rows.push(format!(
        "correlation summary leads={} clusters={} nodes={} edges={} generatedAt={}",
        correlation.snapshot.lead_count,
        correlation.snapshot.cluster_count,
        correlation.snapshot.node_count,
        correlation.snapshot.edge_count,
        correlation.snapshot.generated_at
    ));
    rows.extend(
        correlation
            .snapshot
            .leads
            .iter()
            .map(format_correlation_lead_row),
    );
    rows
}

pub(crate) fn report_correlation_lead_sections(
    correlation: &ReportCorrelation,
    scope: &ExportScopeDto,
) -> Vec<HtmlCorrelationLeadSection> {
    if !scope.file_system_metadata && !scope.full_timeline {
        return Vec::new();
    }

    correlation
        .snapshot
        .leads
        .iter()
        .map(|lead| HtmlCorrelationLeadSection {
            title: lead.title.clone(),
            confidence: correlation_confidence_str(&lead.confidence).to_string(),
            families: lead.families.clone(),
            primary_file_id: lead.primary_file_id.clone(),
            summary: lead.summary.clone(),
            supporting_node_ids: lead.supporting_node_ids.clone(),
            match_signals: lead.match_signals.clone(),
            provenance: lead
                .provenance
                .iter()
                .map(|item| {
                    format!(
                        "{}:{}:{}:{}",
                        item.source_kind,
                        item.source_record_id,
                        item.source_label,
                        guarantee_level_str(&item.guarantee_level)
                    )
                })
                .collect(),
            caveats: lead.caveats.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Row formatting helpers
// ---------------------------------------------------------------------------

pub(crate) fn format_artifact_report_row(artifact: &domain::Artifact) -> String {
    format!(
        "artifact id={} sourceObjectId={} type={} title={} summary={} extractor={} extractorVersion={} confidence={} sourceAttribution={}",
        artifact.id.0,
        artifact
            .source_object_id
            .as_ref()
            .map(|id| id.0.as_str())
            .unwrap_or("unknown"),
        artifact.family,
        artifact.title,
        artifact.summary,
        optional_str(&artifact.extractor_id),
        optional_str(&artifact.extractor_version),
        optional_f32(artifact.confidence),
        optional_str(&artifact.source_attribution)
    )
}

pub(crate) fn format_artifact_dto_report_row(
    artifact: &ArtifactRowDto,
) -> Result<String, ReportError> {
    let data_source_id = super::source_identity::artifact_data_source_id(artifact)?;
    Ok(format!(
        "artifact id={} dataSourceId={} sourceObjectId={} type={} title={} summary={} extractor={} extractorVersion={} confidence={} sourceAttribution={}",
        artifact.id,
        data_source_id.0,
        optional_str(&artifact.source_object_id),
        artifact.artifact_type,
        artifact.title,
        artifact.summary,
        optional_str(&artifact.extractor_id),
        optional_str(&artifact.extractor_version),
        optional_f32(artifact.confidence),
        optional_str(&artifact.source_attribution),
    ))
}

pub(crate) fn format_timeline_report_row(event: &TimelineEvent) -> String {
    format!(
        "timeline id={} sourceObjectId={} eventType={} timestamp={} title={} parser={} parserVersion={} confidence={} sourceAttribution={}",
        event.id.0,
        event.source_object_id,
        event.event_type,
        event.timestamp.to_rfc3339(),
        event.title,
        optional_str(&event.parser_id),
        optional_str(&event.parser_version),
        optional_f32(event.confidence),
        optional_str(&event.source_attribution)
    )
}

pub(crate) fn format_timeline_dto_report_row(
    event: &TimelineEventDto,
) -> Result<String, ReportError> {
    let data_source_id = super::source_identity::timeline_data_source_id(event)?;
    Ok(format!(
        "timeline id={} dataSourceId={} sourceObjectId={} eventType={} timestamp={} title={} parser={} parserVersion={} confidence={} sourceAttribution={}",
        event.id,
        data_source_id.0,
        event.source_object_id,
        event.event_type,
        event.ts,
        event.title,
        optional_str(&event.parser_id),
        optional_str(&event.parser_version),
        optional_f32(event.confidence),
        optional_str(&event.source_attribution),
    ))
}

fn format_correlation_lead_row(lead: &CorrelationLeadDto) -> String {
    format!(
        "correlation lead={} confidence={} families={} primaryFileId={} supportNodes={} matchSignals={} summary={} caveats={} provenance={}",
        lead.title,
        correlation_confidence_str(&lead.confidence),
        lead.families.join(" | "),
        lead.primary_file_id,
        lead.supporting_node_ids.join(" | "),
        lead.match_signals.join(" | "),
        lead.summary,
        lead.caveats.join(" | "),
        lead.provenance
            .iter()
            .map(|item| format!(
                "{}:{}:{}:{}",
                item.source_kind,
                item.source_record_id,
                item.source_label,
                guarantee_level_str(&item.guarantee_level)
            ))
            .collect::<Vec<_>>()
            .join(" ; ")
    )
}

// ---------------------------------------------------------------------------
// String conversion helpers
// ---------------------------------------------------------------------------

fn release_gate_status_str(value: &ReleaseGateStatusDto) -> &'static str {
    match value {
        ReleaseGateStatusDto::Passed => "passed",
        ReleaseGateStatusDto::Warning => "warning",
        ReleaseGateStatusDto::Blocked => "blocked",
    }
}

fn verification_result_str(value: &VerificationResultDto) -> &'static str {
    match value {
        VerificationResultDto::Passed => "passed",
        VerificationResultDto::Partial => "partial",
        VerificationResultDto::Pending => "pending",
        VerificationResultDto::Failed => "failed",
    }
}

fn support_maturity_str(value: &SupportMaturityDto) -> &'static str {
    match value {
        SupportMaturityDto::Ga => "ga",
        SupportMaturityDto::Beta => "beta",
        SupportMaturityDto::Experimental => "experimental",
        SupportMaturityDto::Unsupported => "unsupported",
    }
}

fn known_limitation_status_str(value: &KnownLimitationStatusDto) -> &'static str {
    match value {
        KnownLimitationStatusDto::Partial => "partial",
        KnownLimitationStatusDto::Unsupported => "unsupported",
        KnownLimitationStatusDto::NotGuaranteed => "notGuaranteed",
    }
}

fn guarantee_level_str(value: &VerificationGuaranteeLevelDto) -> &'static str {
    match value {
        VerificationGuaranteeLevelDto::Guaranteed => "guaranteed",
        VerificationGuaranteeLevelDto::BestEffort => "bestEffort",
        VerificationGuaranteeLevelDto::Experimental => "experimental",
        VerificationGuaranteeLevelDto::NotGuaranteed => "notGuaranteed",
    }
}

fn correlation_coverage_status_str(value: &CorrelationCoverageStatusDto) -> &'static str {
    match value {
        CorrelationCoverageStatusDto::Covered => "covered",
        CorrelationCoverageStatusDto::Review => "review",
        CorrelationCoverageStatusDto::Missing => "missing",
    }
}

fn benchmark_requirement_status_str(value: &BenchmarkRequirementStatusDto) -> &'static str {
    match value {
        BenchmarkRequirementStatusDto::Covered => "covered",
        BenchmarkRequirementStatusDto::Missing => "missing",
        BenchmarkRequirementStatusDto::Exceeded => "exceeded",
    }
}

fn optional_str(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("unknown")
}

fn optional_f32(value: Option<f32>) -> String {
    value
        .map(|confidence| confidence.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
