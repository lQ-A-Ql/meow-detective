import { fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Reports } from '@/app/pages/Reports';

const mocks = vi.hoisted(() => ({
  templates: vi.fn(),
  history: vi.fn(),
  exportReport: vi.fn(),
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  importSignals: vi.fn(),
}));

vi.mock('@/features/reports/hooks', () => ({
  useReportTemplates: mocks.templates,
  useReportHistory: mocks.history,
  useExportReport: mocks.exportReport,
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useDataSources: mocks.dataSources,
}));

vi.mock('@/features/jobs/import-event-state', () => ({
  useImportEventState: () => mocks.importSignals(),
  getEvidenceHashStatusLabel: (status: string) => status,
  getEvidenceHashCaveatText: (status: string) => `hash caveat ${status}`,
  deriveEvidenceHashStatus: (_partials: unknown[], sources: Array<{ hashStatus?: string }>) => {
    if (sources.some((source) => source.hashStatus === 'pending')) return 'pending';
    if (sources.some((source) => source.hashStatus === 'unavailable')) return 'unavailable';
    if (sources.some((source) => source.hashStatus === 'hashed')) return 'ready';
    return undefined;
  },
}));

vi.mock('@/lib/api/reports', () => ({
  exportHtmlReport: vi.fn(),
  exportCsvReport: vi.fn(),
  exportJsonReport: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function queryState(data: unknown) {
  return { data };
}

function renderReports() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <Reports />
    </QueryClientProvider>,
  );
}

describe('Reports hash caveat visibility', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryState({ id: 'case-1' }));
    mocks.exportReport.mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
      error: null,
    });
  });

  it('shows pending hash caveat without displaying raw source paths', () => {
    mocks.templates.mockReturnValue(queryState([{ id: 'summary', name: 'Summary', description: 'Case summary' }]));
    mocks.history.mockReturnValue(queryState([]));
    mocks.importSignals.mockReturnValue({ partialResults: [] });
    mocks.dataSources.mockReturnValue(queryState([
      {
        id: 'ds-1',
        name: 'Evidence Source',
        kind: 'raw',
        sourcePath: 'D:/private/evidence.raw',
        importedAt: '2026-06-05T10:00:00Z',
        platform: 'windows',
        hashStatus: 'pending',
        partitions: [],
      },
    ]));

    renderReports();

    expect(screen.getByText('证据哈希: pending')).toBeDefined();
    expect(screen.getByText('hash caveat pending')).toBeDefined();
    expect(screen.queryByText('D:/private/evidence.raw')).toBeNull();
    expect(mocks.history).toHaveBeenCalledWith('case-1');
  });

  it('updates export summary when raw file extraction is enabled', () => {
    mocks.templates.mockReturnValue(queryState([{ id: 'summary', name: 'Summary', description: 'Case summary' }]));
    mocks.history.mockReturnValue(queryState([]));
    mocks.importSignals.mockReturnValue({ partialResults: [] });
    mocks.dataSources.mockReturnValue(queryState([]));

    renderReports();

    expect(screen.getByText('预计产物: 报告主体文件')).toBeDefined();
    fireEvent.click(screen.getByLabelText(/包含原始文件提取/));
    expect(screen.getByText('预计产物: 报告 + 原始文件批量导出清单 + SHA256SUMS')).toBeDefined();
  });
});
