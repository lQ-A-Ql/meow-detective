import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DataAnalysis } from './DataAnalysis';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  systemInfo: vi.fn(),
  classifications: vi.fn(),
  summaryMutation: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useAnalysisSystemInfo: mocks.systemInfo,
  useAnalysisClassifications: mocks.classifications,
  useGenerateAnalysisSummary: mocks.summaryMutation,
}));

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(DataAnalysis),
    ),
  );
}

function queryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
    ...overrides,
  };
}

describe('DataAnalysis page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryState({
      data: { id: 'case-1', name: 'Case 1' },
    }));
    mocks.systemInfo.mockReturnValue(queryState({
      data: {
        networkAdapters: [],
        bootHistory: [],
        status: 'notParsed',
        warnings: ['系统信息解析器尚未接入 Registry/EVTX；当前不会输出未验证主机事实。'],
        provenance: [
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'notParsed',
            warnings: ['Registry parser not implemented'],
          },
        ],
      },
    }));
    mocks.classifications.mockReturnValue(queryState({
      data: [
        {
          category: 'Documents',
          totalSize: 4,
          status: 'parsed',
          warnings: [],
          files: [
            {
              fileId: 'file-1',
              path: 'doc.pdf',
              name: 'doc.pdf',
              size: 4,
              fileType: 'PDF',
              magicDescription: 'PDF Document',
              provenance: {
                dataSourceId: 'ds-1',
                artifactPath: 'doc.pdf',
                parser: 'analysis.magic',
                parsedAt: '2026-06-01T10:00:00Z',
                status: 'parsed',
                warnings: [],
              },
            },
          ],
          provenance: [
            {
              dataSourceId: 'ds-1',
              artifactPath: 'doc.pdf',
              parser: 'analysis.magic',
              parsedAt: '2026-06-01T10:00:00Z',
              status: 'parsed',
              warnings: [],
            },
          ],
        },
      ],
    }));
    mocks.summaryMutation.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync: vi.fn().mockResolvedValue('# 数据源分析报告'),
    });

    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:analysis'),
      revokeObjectURL: vi.fn(),
    });
  });

  it('shows empty state and does not render errors when no case is active', () => {
    mocks.currentCase.mockReturnValue(queryState({ data: null }));

    renderPage();

    expect(screen.getByText('请先创建或打开案件')).toBeDefined();
    expect(screen.queryByText('正在分析数据源...')).toBeNull();
  });

  it('renders accessible tabs and notParsed system info without fake facts', () => {
    renderPage();

    expect(screen.getByRole('tab', { name: /系统信息/ })).toBeDefined();
    expect(screen.getByRole('tab', { name: /文件分类/ })).toBeDefined();
    expect(screen.getByRole('tab', { name: /分析报告/ })).toBeDefined();
    expect(screen.getAllByText('未解析').length).toBeGreaterThan(0);
    expect(screen.getByText('registry.system')).toBeDefined();
    expect(screen.getByText('Windows/System32/config/SYSTEM')).toBeDefined();
    expect(screen.getByText('Registry parser not implemented')).toBeDefined();
    expect(screen.queryByText('FORENSICS-PC')).toBeNull();
    expect(screen.queryByText('Windows 10')).toBeNull();
  });

  it('renders file classifications from hook data', () => {
    renderPage();
    fireEvent.click(screen.getByRole('tab', { name: /文件分类/ }));

    expect(screen.getByText('Documents')).toBeDefined();
    expect(screen.getAllByText('doc.pdf').length).toBeGreaterThan(0);
    expect(screen.getByText('PDF Document')).toBeDefined();
    expect(screen.getAllByText(/analysis.magic/).length).toBeGreaterThan(0);
  });

  it('downloads markdown report through summary mutation', async () => {
    const click = vi.fn();
    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tagName) => {
      const element = originalCreateElement(tagName);
      if (tagName === 'a') {
        element.click = click;
      }
      return element;
    });
    const mutateAsync = vi.fn().mockResolvedValue('# 数据源分析报告');
    mocks.summaryMutation.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
    });

    renderPage();
    fireEvent.click(screen.getByRole('tab', { name: /分析报告/ }));
    fireEvent.click(screen.getByRole('button', { name: /下载 Markdown 报告/ }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(URL.createObjectURL).toHaveBeenCalled();
    expect(click).toHaveBeenCalledTimes(1);
  });
});
