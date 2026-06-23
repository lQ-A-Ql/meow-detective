import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TopBar } from './TopBar';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  jobs: vi.fn(),
  warnings: vi.fn(),
  uiState: {
    currentPage: 'home',
    setCurrentPage: vi.fn(),
    globalSearchQuery: '',
    setGlobalSearchQuery: vi.fn(),
    toggleDrawer: vi.fn(),
  },
  importSignals: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useDataSources: mocks.dataSources,
}));

vi.mock('@/features/jobs/hooks', () => ({
  useJobsSnapshot: mocks.jobs,
  useWarnings: mocks.warnings,
}));

vi.mock('@/stores/ui-store', () => ({
  useUiStore: (selector: (state: typeof mocks.uiState) => unknown) => selector(mocks.uiState),
}));

vi.mock('@/features/jobs/import-event-state', () => ({
  useImportEventState: () => mocks.importSignals(),
  getImportPhaseLabel: (phase: string) => phase,
  getImportPhaseStateLabel: (state: string) => state,
  getFreshnessLabel: (freshness: string) => freshness,
  getCacheStateLabel: (state: string) => state,
  getPartialKindLabel: (kind: string) => kind,
  getEvidenceHashStatusLabel: (status: string) => status,
  getEvidenceHashCaveatText: (status: string) => `hash caveat ${status}`,
  deriveEvidenceHashStatus: (partials: Array<{ kind: string; freshness: string }>, sources: Array<{ hashStatus?: string }>) => {
    if (sources.some((source) => source.hashStatus === 'failed')) return 'failed';
    if (partials.some((partial) => partial.kind === 'evidenceHash' && partial.freshness === 'partial')) return 'pending';
    if (sources.some((source) => source.hashStatus === 'unavailable')) return 'unavailable';
    if (partials.some((partial) => partial.kind === 'evidenceHash' && partial.freshness === 'ready')) return 'ready';
    return undefined;
  },
}));

function queryState(data: unknown) {
  return {
    data,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
  };
}

describe('TopBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryState({
      number: '2026-FX-091',
      name: 'WannaCry 爆发溯源',
      examiner: 'Qin Ao',
      updatedAt: '2026-05-16T11:20:00Z',
    }));
    mocks.jobs.mockReturnValue(queryState([]));
    mocks.warnings.mockReturnValue(queryState([]));
    mocks.dataSources.mockReturnValue(queryState([]));
    mocks.importSignals.mockReturnValue({
      latestPhase: undefined,
      latestCancellation: undefined,
      partialResults: [],
      cacheStatuses: [],
      latestReport: undefined,
    });
    mocks.uiState.currentPage = 'home';
    mocks.uiState.globalSearchQuery = '';
  });

  it('renders typed import status chips when event signals are present', () => {
    mocks.importSignals.mockReturnValue({
      latestPhase: { phase: 'analyze', percent: 64, state: 'running', detail: 'workers=2', metrics: {}, partialResults: [] },
      latestCancellation: { state: 'draining', safeToClose: false, detail: 'Waiting for workers', jobId: 'job-1' },
      partialResults: [{ kind: 'searchIndex', freshness: 'partial', readyCount: 120, totalEstimate: 400, scopeId: 'ds-1' }],
      cacheStatuses: [{ cacheKey: 'search:index:ds-1', state: 'warming', indexedCount: 10, updatedAt: '2026-06-05T10:00:00Z' }],
      latestReport: {
        summary: {
          reportId: 'perf-1',
          generatedAt: '2026-06-05T10:04:00Z',
          elapsedMs: 842,
          summary: 'Timeline query stayed within bounded metrics.',
        },
        metrics: [{ key: 'timeline.query.elapsedMs', value: 842, unit: 'ms' }],
      },
    });

    render(
      <MemoryRouter>
        <TopBar />
      </MemoryRouter>,
    );

    expect(screen.getByText('Import')).toBeDefined();
    expect(screen.getByText('analyze 64%')).toBeDefined();
    expect(screen.getByText('Cancel')).toBeDefined();
    expect(screen.getByText('draining')).toBeDefined();
    expect(screen.getByText('Cache')).toBeDefined();
    expect(screen.getByText('warming')).toBeDefined();
    expect(screen.getByText('Perf')).toBeDefined();
    expect(screen.getByText('842ms')).toBeDefined();
  });

  it('shows evidence hash status without exposing source paths', () => {
    mocks.dataSources.mockReturnValue(queryState([
      {
        id: 'ds-1',
        name: 'Evidence Source',
        kind: 'raw',
        sourcePath: 'D:/private/sample.raw',
        importedAt: '2026-06-05T10:00:00Z',
        hashStatus: 'unavailable',
        partitions: [],
      },
    ]));

    render(
      <MemoryRouter>
        <TopBar />
      </MemoryRouter>,
    );

    expect(screen.getByText('Hash')).toBeDefined();
    expect(screen.getByText('unavailable')).toBeDefined();
    expect(screen.queryByText('D:/private/sample.raw')).toBeNull();
  });
});
