import { describe, it, expect } from 'vitest';
import {
  getCurrentCase,
  getCaseMetrics,
  getRecentObjects,
  getRecentCases,
  getDataSources,
} from '@/lib/api/case';

describe('case API (mock mode)', () => {
  it('getCurrentCase returns case summary', async () => {
    const result = await getCurrentCase();
    expect(result).not.toBeNull();
    expect(result!.id).toBe('case-2026-fx-091');
    expect(result!.name).toBe('WannaCry 爆发溯源');
  });

  it('getCaseMetrics returns metrics', async () => {
    const result = await getCaseMetrics();
    expect(result.dataSourceCount).toBe(4);
    expect(result.indexedFileCount).toBe(1492033);
    expect(result.artifactCount).toBe(45102);
  });

  it('getRecentObjects returns list', async () => {
    const result = await getRecentObjects();
    expect(result.length).toBeGreaterThan(0);
    expect(result[0].id).toBeDefined();
    expect(result[0].title).toBeDefined();
    expect(result[0].kind).toBeDefined();
  });

  it('getRecentCases returns list', async () => {
    const result = await getRecentCases();
    expect(result.length).toBeGreaterThan(0);
    expect(result[0].caseRoot).toBeDefined();
    expect(result[0].name).toBeDefined();
  });

  it('getDataSources returns sources with partitions', async () => {
    const result = await getDataSources();
    expect(result.length).toBeGreaterThan(0);
    expect(result[0].id).toBe('ds-001');
    expect(result[0].partitions.length).toBeGreaterThan(0);
    expect(result[0].sourceHash).toBe('mock-finche01-sha256-demo-value');
    expect(result[0].hashStatus).toBe('hashed');
    expect(result[0].canonicalPath).toContain('/mock/');
    expect(result[0].provenanceStatus).toBe('Recorded');
    expect(result[0].warnings?.[0]).toContain('MOCK DATA');
  });
});
