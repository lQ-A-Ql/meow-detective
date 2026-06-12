import { fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';
import { Reports } from './Reports';

const mocks = vi.hoisted(() => ({
  templates: vi.fn(),
  history: vi.fn(),
  dataSources: vi.fn(),
  importSignals: vi.fn(),
}));

vi.mock('@/features/reports/hooks', () => ({
  useReportTemplates: mocks.templates,
  useReportHistory: mocks.history,
}));

vi.mock('@/features/case/hooks', () => ({
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
        hashStatus: 'pending',
        partitions: [],
      },
    ]));

    renderReports();

    expect(screen.getByText('Evidence Hash: pending')).toBeDefined();
    expect(screen.getByText('hash caveat pending')).toBeDefined();
    expect(screen.queryByText('D:/private/evidence.raw')).toBeNull();
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
