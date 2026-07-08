import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BottomDrawer } from './BottomDrawer';

const mocks = vi.hoisted(() => ({
  jobs: vi.fn(),
  dataSources: vi.fn(),
  warnings: vi.fn(),
  trace: vi.fn(),
  uiStore: vi.fn(),
  importSignals: vi.fn(),
}));

vi.mock('@/features/jobs/hooks', () => ({
  useJobsSnapshot: mocks.jobs,
  useWarnings: mocks.warnings,
  useTraceItems: mocks.trace,
}));

vi.mock('@/features/case/hooks', () => ({
  useDataSources: mocks.dataSources,
}));

vi.mock('@/stores/ui-store', () => ({
  useUiStore: (
    selector: (state: {
      drawerOpen: boolean;
      setDrawerOpen: (open: boolean) => void;
      toggleDrawer: () => void;
    }) => unknown,
  ) => selector(mocks.uiStore()),
}));

vi.mock('@/features/jobs/import-event-state', () => ({
  useImportEventState: () => mocks.importSignals(),
  getImportPhaseLabel: (phase: string) => phase,
  getImportPhaseStateLabel: (state: string) => state,
  getFreshnessLabel: (freshness: string) => freshness,
  getPartialKindLabel: (kind: string) => kind,
  getCacheStateLabel: (state: string) => state,
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

describe('BottomDrawer jobs panel', () => {
  beforeEach(() => {
    mocks.jobs.mockReturnValue({
      data: [
        {
          id: 'job-running',
          name: 'Import data source',
          scope: 'Case ingest',
          progress: 41,
          status: 'running',
          detail: 'Enumerating',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
        {
          id: 'job-partial',
          name: 'Artifact extraction',
          scope: 'Post-import',
          progress: 100,
          status: 'completed',
          detail: 'Completed with warnings',
          warningCount: 2,
          skippedCount: 3,
          failedCount: 0,
          partial: true,
        },
      ],
    });
    mocks.warnings.mockReturnValue({ data: [] });
    mocks.dataSources.mockReturnValue({ data: [] });
    mocks.trace.mockReturnValue({ data: [] });
    mocks.uiStore.mockReturnValue({
      drawerOpen: true,
      setDrawerOpen: vi.fn(),
      toggleDrawer: vi.fn(),
    });
    mocks.importSignals.mockReturnValue({
      latestPhase: undefined,
      latestCancellation: undefined,
      partialResults: [],
      cacheStatuses: [],
      latestReport: undefined,
      lastUpdatedAt: undefined,
    });
  });

  it('renders partial badge and outcome counts', () => {
    render(<BottomDrawer />);

    expect(screen.getAllByText('部分完成').length).toBeGreaterThan(0);
    expect(screen.getByText('警告 2')).toBeDefined();
    expect(screen.getByText('跳过 3')).toBeDefined();
    expect(screen.getByText('失败 0')).toBeDefined();
    expect(screen.getByText(/1 运行 \/ 1 完成 \/ 1 部分 \/ 0 失败/)).toBeDefined();
  });

  it('keeps running jobs visible for cancel surfaces outside the drawer', () => {
    render(<BottomDrawer />);

    expect(screen.getByText('Import data source')).toBeDefined();
    expect(screen.getByText('41%')).toBeDefined();
  });

  it('shows typed import signals for partial freshness, cache state, cancellation, and report readiness', () => {
    mocks.importSignals.mockReturnValue({
      latestPhase: {
        phase: 'analyze',
        state: 'running',
        percent: 64,
        detail: 'scheduling=draining workerBudget=4',
        metrics: { rowsProcessed: 640, rowsTotal: 1000 },
      },
      latestCancellation: {
        jobId: 'job-running',
        state: 'draining',
        safeToClose: false,
        detail: 'Waiting for workers to settle',
      },
      partialResults: [
        {
          kind: 'searchIndex',
          freshness: 'partial',
          readyCount: 120,
          totalEstimate: 400,
          scopeId: 'ds-1',
          queryKey: 'search:index:ds-1',
        },
      ],
      cacheStatuses: [
        {
          cacheKey: 'search:index:ds-1',
          state: 'warming',
          indexedCount: 300,
          totalCount: 1000,
          updatedAt: '2026-06-05T10:03:00Z',
          message: 'Index warming',
        },
      ],
      latestReport: {
        summary: {
          reportId: 'perf-1',
          jobId: 'job-running',
          generatedAt: '2026-06-05T10:04:00Z',
          elapsedMs: 842,
          summary: 'Timeline query stayed within bounded metrics.',
        },
        metrics: [{ key: 'timeline.query.elapsedMs', value: 842, unit: 'ms' }],
      },
      lastUpdatedAt: '2026-06-05T10:04:00Z',
    });

    render(<BottomDrawer />);

    expect(screen.getByText('Import Signals')).toBeDefined();
    expect(screen.getByText('analyze · running')).toBeDefined();
    expect(screen.getByText('draining')).toBeDefined();
    expect(screen.getByText('searchIndex partial')).toBeDefined();
    expect(screen.getByText('Search Cache')).toBeDefined();
    expect(screen.getByText('842ms')).toBeDefined();
  });

  it('shows live warning/failed chips from import phase metrics', () => {
    mocks.importSignals.mockReturnValue({
      latestPhase: {
        phase: 'analyze',
        state: 'running',
        percent: 64,
        detail: 'scheduling=draining workerBudget=4',
        metrics: { rowsProcessed: 640, rowsTotal: 1000, warnings: 3, failed: 1 },
      },
      latestCancellation: undefined,
      partialResults: [],
      cacheStatuses: [],
      latestReport: undefined,
      lastUpdatedAt: undefined,
    });

    render(<BottomDrawer />);

    const warningChip = screen.getAllByText('警告').find((el) => el.textContent === '警告3');
    const failedChip = screen.getAllByText('失败').find((el) => el.textContent === '失败1');
    expect(warningChip).toBeDefined();
    expect(failedChip).toBeDefined();
  });

  it('surfaces failed jobs and query errors with detailed issue metadata', () => {
    mocks.jobs.mockReturnValue({
      data: [
        {
          id: 'job-failed',
          name: 'Linux artifact extraction',
          scope: 'Data source ds-linux',
          progress: 73,
          status: 'failed',
          detail: 'XFS directory read failed',
          warningCount: 2,
          skippedCount: 1,
          failedCount: 4,
          partial: false,
          currentPartition: 'P3 root LV',
        },
      ],
    });
    mocks.warnings.mockReturnValue({
      data: [],
      error: {
        code: 'COMMAND_GET_WARNINGS_FAILED',
        message: 'SQLite warning query failed',
        category: 'io',
        recoverable: true,
        suggestion: '重新打开案件后重试。',
        details: { table: 'job_warnings', reason: 'database is locked' },
      },
    });

    render(<BottomDrawer />);

    expect(screen.getByText('ISSUES')).toBeDefined();
    expect(screen.getByText('告警加载失败')).toBeDefined();
    expect(screen.getByText('SQLite warning query failed')).toBeDefined();
    expect(screen.getByText(/COMMAND_GET_WARNINGS_FAILED/)).toBeDefined();
    expect(screen.getByText(/database is locked/)).toBeDefined();
    expect(screen.getAllByText('Linux artifact extraction').length).toBeGreaterThan(0);
    expect(screen.getAllByText('XFS directory read failed').length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Data source ds-linux/).length).toBeGreaterThan(0);
    expect(screen.getByText(/P3 root LV/)).toBeDefined();
  });

  it('surfaces evidence hash caveats in the compact import signal panel', () => {
    mocks.importSignals.mockReturnValue({
      latestPhase: undefined,
      latestCancellation: undefined,
      partialResults: [
        {
          kind: 'evidenceHash',
          freshness: 'partial',
          readyCount: 0,
          totalEstimate: 1,
          scopeId: 'ds-1',
          queryKey: 'evidence:hash:ds-1',
        },
      ],
      cacheStatuses: [],
      latestReport: undefined,
      lastUpdatedAt: '2026-06-05T10:04:00Z',
    });

    render(<BottomDrawer />);

    expect(screen.getByText('evidenceHash partial')).toBeDefined();
    expect(screen.getByText('Evidence Hash pending')).toBeDefined();
    expect(screen.getByText('hash caveat pending')).toBeDefined();
  });
});

describe('BottomDrawer job status buckets', () => {
  beforeEach(() => {
    mocks.warnings.mockReturnValue({ data: [] });
    mocks.dataSources.mockReturnValue({ data: [] });
    mocks.trace.mockReturnValue({ data: [] });
    mocks.uiStore.mockReturnValue({
      drawerOpen: true,
      setDrawerOpen: vi.fn(),
      toggleDrawer: vi.fn(),
    });
    mocks.importSignals.mockReturnValue({
      latestPhase: undefined,
      latestCancellation: undefined,
      partialResults: [],
      cacheStatuses: [],
      latestReport: undefined,
      lastUpdatedAt: undefined,
    });
  });

  it('renders warning, cancelling, and cancelled jobs instead of dropping them', () => {
    mocks.jobs.mockReturnValue({
      data: [
        {
          id: 'job-warning',
          name: 'Registry extraction',
          scope: 'Post-import',
          progress: 100,
          status: 'warning',
          detail: 'Completed with a caveat',
          warningCount: 1,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
        {
          id: 'job-cancelling',
          name: 'Import data source',
          scope: 'Case ingest',
          progress: 70,
          status: 'cancelling',
          detail: 'Draining workers',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
        {
          id: 'job-cancelled',
          name: 'Import data source',
          scope: 'Case ingest',
          progress: 70,
          status: 'cancelled',
          detail: 'Cancelled by user',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
      ],
    });

    render(<BottomDrawer />);

    expect(screen.getAllByText('Registry extraction').length).toBeGreaterThan(1);
    expect(screen.getAllByText('Completed with a caveat').length).toBeGreaterThan(1);
    expect(screen.getByText('Draining workers')).toBeDefined();
    expect(screen.getByText('Cancelled by user')).toBeDefined();
  });
});

describe('BottomDrawer manual toggle', () => {
  beforeEach(() => {
    mocks.warnings.mockReturnValue({ data: [] });
    mocks.dataSources.mockReturnValue({ data: [] });
    mocks.trace.mockReturnValue({ data: [] });
    mocks.importSignals.mockReturnValue({
      latestPhase: undefined,
      latestCancellation: undefined,
      partialResults: [],
      cacheStatuses: [],
      latestReport: undefined,
      lastUpdatedAt: undefined,
    });
  });

  it('does not auto-close when a running job completes', () => {
    const setDrawerOpen = vi.fn();
    const toggleDrawer = vi.fn();
    mocks.uiStore.mockReturnValue({
      drawerOpen: true,
      setDrawerOpen,
      toggleDrawer,
    });
    mocks.jobs.mockReturnValue({
      data: [
        {
          id: 'job-running',
          name: 'Import',
          scope: 'Test',
          progress: 50,
          status: 'running',
          detail: 'running',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
      ],
    });

    const { rerender } = render(<BottomDrawer />);
    mocks.jobs.mockReturnValue({
      data: [
        {
          id: 'job-running',
          name: 'Import',
          scope: 'Test',
          progress: 100,
          status: 'completed',
          detail: 'done',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
      ],
    });
    rerender(<BottomDrawer />);
    expect(setDrawerOpen).not.toHaveBeenCalled();
    expect(toggleDrawer).not.toHaveBeenCalled();
  });

  it('calls toggleDrawer when the collapse/expand button is clicked', () => {
    const toggleDrawer = vi.fn();
    mocks.uiStore.mockReturnValue({
      drawerOpen: true,
      setDrawerOpen: vi.fn(),
      toggleDrawer,
    });
    mocks.jobs.mockReturnValue({ data: [] });

    render(<BottomDrawer />);
    const button = screen.getByRole('button', { name: /collapse|展开|收起/i });
    button.click();
    expect(toggleDrawer).toHaveBeenCalled();
  });
});
