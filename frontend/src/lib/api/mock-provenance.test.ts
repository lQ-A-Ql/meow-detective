import { describe, expect, it } from 'vitest';
import { getArtifactById, getArtifactRows } from '@/lib/api/artifacts';
import { getDataSources } from '@/lib/api/case';
import {
  classifyFiles,
  getCorrelationSnapshot,
  getEvidenceClassificationSummary,
  getSystemInfo,
  getV2GovernanceSnapshot,
} from '@/lib/api/analysis';
import { getReportHistory } from '@/lib/api/reports';
import { getTimelineEventById, getTimelineEvents } from '@/lib/api/timeline';
import { getFileJumpContext } from '@/lib/api/files';
import type {
  AnalysisFileClassification,
  AnalysisProvenance,
  ArtifactRow,
  DataSourceSummary,
  EvidenceClassificationSummary,
  ReportHistoryItem,
  TimelineEventDto,
} from '@/types/models';

const expectNoSnakeCaseKeys = (value: Record<string, unknown>, keys: string[]) => {
  for (const key of keys) {
    expect(value).not.toHaveProperty(key);
  }
};

const expectAnalysisProvenance = (provenance: AnalysisProvenance) => {
  expect(provenance.dataSourceId).toBeDefined();
  expect(provenance.artifactPath).toBeDefined();
  expect(provenance.parser).toBeDefined();
  expect(provenance.parsedAt).toBeDefined();
  expect(provenance.status).toBeDefined();
  expect(Array.isArray(provenance.warnings)).toBe(true);
  expectNoSnakeCaseKeys(provenance as unknown as Record<string, unknown>, [
    'data_source_id',
    'artifact_path',
    'parsed_at',
    'sourceHash',
    'parserVersion',
    'sourceAttribution',
  ]);
};

describe('mock provenance API fixtures', () => {
  it('returns data source provenance fields using the Rust DTO camelCase contract', async () => {
    const sources: DataSourceSummary[] = await getDataSources();

    expect(sources.length).toBeGreaterThan(0);
    for (const source of sources) {
      expect(source.sourcePath).toBeDefined();
      expect(source.importedAt).toBeDefined();
      expect(source.hashStatus).toBeDefined();
      if (source.hashStatus === 'hashed') {
        expect(source.sourceHash).toBeDefined();
      } else {
        expect(source.sourceHash).toBeUndefined();
      }
      expect(source.canonicalPath).toBeDefined();
      expect(source.readerKind).toBeDefined();
      expect(source.provenanceStatus).toBe('Recorded');
      expect(Array.isArray(source.warnings)).toBe(true);
      expect(Array.isArray(source.partitions)).toBe(true);
      expectNoSnakeCaseKeys(source as unknown as Record<string, unknown>, [
        'source_path',
        'imported_at',
        'source_hash',
        'hash_status',
        'canonical_path',
        'reader_kind',
        'provenance_status',
      ]);
    }
  });

  it('returns artifact provenance fields and explicit mock source attribution', async () => {
    const rows: ArtifactRow[] = await getArtifactRows('LNK');

    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) {
      expect(row.extractorId).toBe('lnk');
      expect(row.extractorVersion).toBe('1.0.0-mock');
      expect(row.confidence).toBeGreaterThan(0.9);
      expect(row.sourceAttribution).toContain('MOCK SOURCE');
      expect(row.title).toContain('[MOCK]');
      expectNoSnakeCaseKeys(row as unknown as Record<string, unknown>, [
        'artifact_type',
        'source_object_id',
        'extractor_id',
        'extractor_version',
        'source_attribution',
        'created_at',
      ]);
    }
  });

  it('returns timeline provenance fields and explicit mock timeline labeling', async () => {
    const result = await getTimelineEvents();
    const items: TimelineEventDto[] = result.items;

    expect(items.length).toBeGreaterThan(0);
    for (const event of items) {
      expect(event.sourceObjectId).toBeDefined();
      expect(event.eventType).toBeDefined();
      expect(event.parserId).toBeDefined();
      expect(event.parserVersion).toBe('1.0.0-mock');
      expect(event.confidence).toBeGreaterThan(0.8);
      expect(event.sourceAttribution).toContain('MOCK SOURCE');
      expect(event.description).toContain('MOCK TIMELINE');
      expectNoSnakeCaseKeys(event as unknown as Record<string, unknown>, [
        'source_object_id',
        'event_type',
        'parser_id',
        'parser_version',
        'source_attribution',
      ]);
    }
  });

  it('returns bounded analysis provenance without future extension fields', async () => {
    const systemInfo = await getSystemInfo();
    const classifications: AnalysisFileClassification[] = await classifyFiles(1000);
    const evidenceSummary: EvidenceClassificationSummary = await getEvidenceClassificationSummary();

    expect(systemInfo.provenance.length).toBeGreaterThan(0);
    expectAnalysisProvenance(systemInfo.provenance[0]);
    expect(systemInfo.fieldProvenance.length).toBeGreaterThan(0);
    expectNoSnakeCaseKeys(systemInfo.fieldProvenance[0] as unknown as Record<string, unknown>, [
      'value_name',
      'key_path',
      'hive_path',
    ]);

    expect(classifications.length).toBeGreaterThan(0);
    expectAnalysisProvenance(classifications[0].provenance[0]);
    expectAnalysisProvenance(classifications[0].files[0].provenance);

    const categoryWithProvenance = evidenceSummary.categories.find(
      (category) => category.provenance.length > 0,
    );
    expect(categoryWithProvenance).toBeDefined();
    expectAnalysisProvenance(categoryWithProvenance!.provenance[0]);
    expectNoSnakeCaseKeys(categoryWithProvenance! as unknown as Record<string, unknown>, [
      'display_name',
      'file_count',
      'total_size',
      'artifact_count',
    ]);
    expectNoSnakeCaseKeys(categoryWithProvenance!.sources[0] as unknown as Record<string, unknown>, [
      'file_id',
      'evidence_kind',
      'artifact_count',
    ]);
  });

  it('returns only the Rust-backed report history shape', async () => {
    const reports: ReportHistoryItem[] = await getReportHistory();

    expect(reports.length).toBeGreaterThan(0);
    expect(reports[0].fileName).toBeDefined();
    expect(reports[0].createdBy).toBeDefined();
    expect(reports[0].createdAt).toBeDefined();
    expect(reports[0]).not.toHaveProperty('exportScope');
    expect(reports[0]).not.toHaveProperty('provenance');
    expectNoSnakeCaseKeys(reports[0] as unknown as Record<string, unknown>, [
      'file_name',
      'created_by',
      'created_at',
    ]);
  });

  it('returns V2 governance snapshot using camelCase DTO fields', async () => {
    const snapshot = await getV2GovernanceSnapshot();

    expect(snapshot.generatedAt).toBeDefined();
    expect(snapshot.verificationChains[0].displayName).toBeDefined();
    expect(snapshot.verificationChains[0].expectedJsonVersion).toBeDefined();
    expect(snapshot.supportMatrix.gaCount).toBeGreaterThan(0);
    expect(snapshot.supportMatrixEntries[0].verifiedSamples.length).toBeGreaterThan(0);
    expect(snapshot.knownLimitations[0].sourceDoc).toBeDefined();
    expect(snapshot.benchmark.lastVerifiedAt).toBeDefined();
    expect(snapshot.benchmark.requiredChecks[0].status).toBeDefined();
    expect(snapshot.security.exportPathGuardEnabled).toBe(true);
    expect(snapshot.security.auditEventCount).toBeGreaterThan(0);
    expect(snapshot.security.recentAuditEntries[0].action).toBeDefined();
    expect(snapshot.errorTaxonomyEntries[0].redactionRule).toBeDefined();
    expect(snapshot.releaseGates[0].gateId).toBeDefined();
    expect(snapshot.releaseScorecard.totalScore).toBeGreaterThan(0);
    expect(snapshot.runtimeResults.checkedAt).toBeDefined();
    expect(snapshot.runtimeResults.checks[0].subChecks[0].checkId).toBeDefined();
    expect(snapshot.runtimeSignals.dataSourceCount).toBeGreaterThan(0);
    expect(snapshot.runtimeSignals.correlationSnapshotAvailable).toBe(true);
    expect(snapshot.runtimeSignals.correlationLeadCount).toBeGreaterThan(0);
    expect(snapshot.runtimeSignals.correlationHighConfidenceLeadCount).toBeGreaterThan(0);
    expect(snapshot.runtimeSignals.correlationClusterCount).toBeGreaterThan(0);
    expect(snapshot.runtimeSignals.correlationRuleFamilyCount).toBeGreaterThan(0);
    expect(snapshot.runtimeSignals.correlationFamilyCoverage[0].family).toBeDefined();
    expect(snapshot.runtimeSignals.correlationFamilyCoverage[0].status).toBeDefined();
    expectNoSnakeCaseKeys(snapshot as unknown as Record<string, unknown>, ['generated_at']);
    expectNoSnakeCaseKeys(snapshot.verificationChains[0] as unknown as Record<string, unknown>, [
      'display_name',
      'guarantee_level',
      'fixture_tier',
      'expected_json_version',
      'verified_sample_count',
    ]);
    expectNoSnakeCaseKeys(snapshot.supportMatrixEntries[0] as unknown as Record<string, unknown>, [
      'verified_samples',
      'guarantee_summary',
    ]);
    expectNoSnakeCaseKeys(snapshot.knownLimitations[0] as unknown as Record<string, unknown>, [
      'affected_chains',
      'source_doc',
    ]);
    expectNoSnakeCaseKeys(snapshot.errorTaxonomyEntries[0] as unknown as Record<string, unknown>, [
      'redaction_rule',
    ]);
    expectNoSnakeCaseKeys(snapshot.security.recentAuditEntries[0] as unknown as Record<string, unknown>, [
      'resource_type',
      'resource_id',
      'created_at',
    ]);
    expectNoSnakeCaseKeys(snapshot.releaseGates[0] as unknown as Record<string, unknown>, [
      'gate_id',
    ]);
    expectNoSnakeCaseKeys(snapshot.runtimeResults as unknown as Record<string, unknown>, [
      'checked_at',
    ]);
    expectNoSnakeCaseKeys(snapshot.runtimeResults.checks[0] as unknown as Record<string, unknown>, [
      'check_id',
      'checked_at',
      'sub_checks',
    ]);
    expectNoSnakeCaseKeys(snapshot.runtimeResults.checks[0].subChecks[0] as unknown as Record<string, unknown>, [
      'check_id',
    ]);
    expectNoSnakeCaseKeys(snapshot.runtimeSignals as unknown as Record<string, unknown>, [
      'correlation_rule_family_count',
      'correlation_covered_family_count',
      'correlation_high_confidence_family_count',
      'correlation_family_coverage',
    ]);
  });

  it('returns correlation snapshot using camelCase DTO fields', async () => {
    const snapshot = await getCorrelationSnapshot();

    expect(snapshot.generatedAt).toBeDefined();
    expect(snapshot.familyCoverage[0].family).toBeDefined();
    expect(snapshot.familyCoverage[0].sampleSignals).toBeDefined();
    expect(snapshot.leads[0].primaryFileId).toBeDefined();
    expect(snapshot.leads[0].supportingNodeIds.length).toBeGreaterThan(0);
    expect(snapshot.nodes[0].relatedCount).toBeGreaterThanOrEqual(0);
    expect(snapshot.edges[0].fromNodeId).toBeDefined();
    expect(snapshot.edges.some((edge) => edge.kind === 'pathMatch')).toBe(true);
    expect(snapshot.clusters[0].provenance[0].sourceRecordId).toBeDefined();
    expectNoSnakeCaseKeys(snapshot as unknown as Record<string, unknown>, ['generated_at']);
    expectNoSnakeCaseKeys(snapshot.familyCoverage[0] as unknown as Record<string, unknown>, [
      'display_name',
      'lead_count',
      'high_confidence_lead_count',
      'review_lead_count',
      'cluster_count',
      'sample_signals',
    ]);
    expectNoSnakeCaseKeys(snapshot.leads[0] as unknown as Record<string, unknown>, [
      'primary_file_id',
      'supporting_node_ids',
    ]);
    expectNoSnakeCaseKeys(snapshot.clusters[0].provenance[0] as unknown as Record<string, unknown>, [
      'source_kind',
      'source_record_id',
      'source_label',
      'guarantee_level',
      'warning_summary',
    ]);
  });

  it('returns jump context and by-id helpers using camelCase DTO fields', async () => {
    const jump = await getFileJumpContext('file-cmd-exe', { pageLimit: 200 });
    const artifact = await getArtifactById('artifact-1');
    const timeline = await getTimelineEvents();
    const event = timeline.items[0]
      ? await getTimelineEventById(timeline.items[0].id)
      : null;

    expect(jump.target.id).toBeDefined();
    expect(jump.directory.id).toBeDefined();
    expect(Array.isArray(jump.ancestorDirectoryIds)).toBe(true);
    expectNoSnakeCaseKeys(jump as unknown as Record<string, unknown>, [
      'ancestor_directory_ids',
      'row_offset',
      'requires_show_hidden',
    ]);
    if (artifact) {
      expect(artifact.id).toBeDefined();
      expectNoSnakeCaseKeys(artifact as unknown as Record<string, unknown>, [
        'artifact_type',
        'source_object_id',
      ]);
    }
    if (event) {
      expect(event.id).toBeDefined();
      expectNoSnakeCaseKeys(event as unknown as Record<string, unknown>, [
        'source_object_id',
        'event_type',
      ]);
    }
  });
});
