import { describe, expect, it } from 'vitest';
import { selectAnalysisCandidates } from '@/features/analysis/capability-filter';
import type { CapabilityMatrix, DataSourceSummary } from '@/types/models';

function source(id: string, overrides: Partial<DataSourceSummary> = {}): DataSourceSummary {
  return {
    id,
    name: id,
    kind: 'e01',
    sourcePath: `E:\\${id}.E01`,
    importedAt: '2026-11-19T00:00:00Z',
    platform: 'windows',
    importState: 'ready',
    ...overrides,
  };
}

function matrix(rows: Array<{ source: string; capability: 'catalog' | 'artifacts'; state: string }>): CapabilityMatrix {
  return {
    sources: rows.map((row, index) => ({
      dataSourceId: row.source,
      capability: row.capability,
      state: row.state as CapabilityMatrix['sources'][number]['state'],
      revision: index + 1,
      derived: false,
      updatedAt: '2026-11-19T00:00:00Z',
    })),
  };
}

describe('selectAnalysisCandidates', () => {
  it('admits a catalog-ready source while artifacts are still running (existing results stay viewable)', () => {
    const data = [source('a', { importState: 'ready' })];
    const m = matrix([
      { source: 'a', capability: 'catalog', state: 'ready' },
      { source: 'a', capability: 'artifacts', state: 'running' },
    ]);
    expect(selectAnalysisCandidates(data, m).map((s) => s.id)).toEqual(['a']);
  });

  it('also admits catalog-partial sources and never blocks on a partial artifacts row', () => {
    const data = [source('a', { importState: 'importing' })];
    const m = matrix([
      { source: 'a', capability: 'catalog', state: 'partial' },
      { source: 'a', capability: 'artifacts', state: 'partial' },
    ]);
    expect(selectAnalysisCandidates(data, m).map((s) => s.id)).toEqual(['a']);
  });

  it('excludes a catalog-pending source even when artifacts are already ready', () => {
    const data = [source('a', { importState: 'ready' })];
    const m = matrix([
      { source: 'a', capability: 'catalog', state: 'pending' },
      { source: 'a', capability: 'artifacts', state: 'ready' },
    ]);
    expect(selectAnalysisCandidates(data, m)).toEqual([]);
  });

  it('falls back to legacy importState ready when the matrix has no row', () => {
    const data = [source('ready-one'), source('importing-one', { importState: 'importing' })];
    expect(selectAnalysisCandidates(data, undefined).map((s) => s.id)).toEqual(['ready-one']);
    expect(selectAnalysisCandidates(data, matrix([])).map((s) => s.id)).toEqual(['ready-one']);
  });

  it('rejects unsupported platforms regardless of matrix state', () => {
    const data = [source('mac', { platform: 'macos' as never, importState: 'ready' })];
    const m = matrix([{ source: 'mac', capability: 'catalog', state: 'ready' }]);
    expect(selectAnalysisCandidates(data, m)).toEqual([]);
  });
});
