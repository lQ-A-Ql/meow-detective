import { describe, expect, it } from 'vitest';
import { getArtifactRows } from '@/lib/api/artifacts';
import { getDataSources } from '@/lib/api/case';
import {
  classifyFiles,
  getEvidenceClassificationSummary,
  getSystemInfo,
} from '@/lib/api/analysis';
import { getReportHistory } from '@/lib/api/reports';
import { getTimelineEvents } from '@/lib/api/timeline';
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
});
