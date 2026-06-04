import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DataAnalysis } from './DataAnalysis';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  demoCase: vi.fn(),
  systemInfo: vi.fn(),
  evidenceSummary: vi.fn(),
  evidenceScan: vi.fn(),
  classifications: vi.fn(),
  summaryMutation: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useCreateAnalysisDemoCase: mocks.demoCase,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useAnalysisSystemInfo: mocks.systemInfo,
  useEvidenceClassificationSummary: mocks.evidenceSummary,
  useRunEvidenceClassification: mocks.evidenceScan,
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
    mocks.demoCase.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync: vi.fn().mockResolvedValue({ id: 'demo-case', name: 'Analysis Demo' }),
    });
    mocks.systemInfo.mockReturnValue(queryState({
      data: {
        computerName: 'BETA-LAB',
        osVersion: 'Windows Evidence Edition 24H2',
        buildNumber: '26000',
        networkAdapters: [],
        bootHistory: [
          {
            timestamp: '2026-06-01T08:15:00Z',
            bootType: 'eventLogStarted',
            source: 'Windows/System32/winevt/Logs/System.evtx',
            eventId: 6005,
            recordId: 42,
            note: 'EventLog 6005 candidate; indicates the Event Log service started, not a direct boot assertion.',
            provenance: {
              dataSourceId: 'ds-1',
              artifactPath: 'Windows/System32/winevt/Logs/System.evtx',
              parser: 'evtx.boot_shutdown',
              parsedAt: '2026-06-01T10:00:00Z',
              status: 'parsed',
              warnings: [],
            },
          },
        ],
        status: 'parsed',
        warnings: ['开关机历史来自 EVTX EventLog/User32 candidate events。'],
        provenance: [
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'parsed',
            warnings: [],
          },
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SOFTWARE',
            parser: 'registry.software',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'parsed',
            warnings: [],
          },
        ],
        fieldProvenance: [
          {
            field: 'computerName',
            valueName: 'ComputerName',
            keyPath: 'ControlSet001\\Control\\ComputerName\\ComputerName',
            hivePath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
          },
          {
            field: 'osVersion',
            valueName: 'ProductName',
            keyPath: 'Microsoft\\Windows NT\\CurrentVersion',
            hivePath: 'Windows/System32/config/SOFTWARE',
            parser: 'registry.software',
          },
        ],
      },
    }));
    mocks.evidenceSummary.mockReturnValue(queryState({
      data: {
        status: 'candidateFound',
        generatedAt: '2026-06-01T10:00:00Z',
        warnings: [],
        totals: {
          categoryCount: 3,
          candidateFileCount: 4,
          totalSize: 8192,
          artifactCount: 1,
        },
        categories: [
          {
            category: 'SystemInformation',
            displayName: '系统信息',
            status: 'parsed',
            fileCount: 2,
            totalSize: 4096,
            artifactCount: 1,
            confidence: 0.95,
            warnings: [],
            sources: [
              {
                fileId: 'system',
                path: 'Windows/System32/config/SYSTEM',
                size: 2048,
                evidenceKind: 'registry_hive',
                parser: 'registry.system_info',
                status: 'parsed',
                artifactCount: 1,
                warnings: [],
              },
            ],
            provenance: [],
          },
          {
            category: 'EventLogs',
            displayName: '事件日志',
            status: 'candidateFound',
            fileCount: 1,
            totalSize: 4096,
            artifactCount: 0,
            confidence: 0.65,
            warnings: ['已发现候选文件；尚未运行证据分类解析。'],
            sources: [
              {
                fileId: 'evtx',
                path: 'Windows/System32/winevt/Logs/System.evtx',
                size: 4096,
                evidenceKind: 'event_log',
                parser: 'evtx.boot_shutdown',
                status: 'candidateFound',
                artifactCount: 0,
                warnings: [],
              },
            ],
            provenance: [],
          },
        ],
      },
    }));
    mocks.evidenceScan.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync: vi.fn().mockResolvedValue({}),
    });
    mocks.classifications.mockReturnValue(queryState({
      data: [
        {
          category: 'Documents',
          fileCount: 1,
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
                parser: 'metadata.extension_path',
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
              parser: 'metadata.extension_path',
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
    expect(screen.getAllByRole('button', { name: /加载演示案件/ }).length).toBeGreaterThan(0);
    expect(screen.queryByText('正在分析数据源...')).toBeNull();
  });

  it('loads the demo case from the empty state', async () => {
    const mutateAsync = vi.fn().mockResolvedValue({ id: 'demo-case', name: 'Analysis Demo' });
    mocks.currentCase.mockReturnValue(queryState({ data: null }));
    mocks.demoCase.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
    });

    renderPage();
    fireEvent.click(screen.getAllByRole('button', { name: /加载演示案件/ })[0]);

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
  });

  it('renders accessible tabs and parsed registry facts with provenance', () => {
    renderPage();

    expect(screen.getByRole('tab', { name: /系统信息/ })).toBeDefined();
    expect(screen.getByRole('tab', { name: /证据分类/ })).toBeDefined();
    expect(screen.getByRole('tab', { name: /文件分类/ })).toBeDefined();
    expect(screen.getByRole('tab', { name: /分析报告/ })).toBeDefined();
    expect(screen.getAllByText('已解析').length).toBeGreaterThan(0);
    expect(screen.getByText('BETA-LAB')).toBeDefined();
    expect(screen.getByText('Windows Evidence Edition 24H2')).toBeDefined();
    expect(screen.getAllByText('registry.system').length).toBeGreaterThan(0);
    expect(screen.getAllByText('registry.software').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Windows/System32/config/SYSTEM').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Windows/System32/config/SOFTWARE').length).toBeGreaterThan(0);
    expect(screen.getByText('字段级来源')).toBeDefined();
    expect(screen.getByText('computerName')).toBeDefined();
    expect(screen.getByText('ProductName')).toBeDefined();
    expect(screen.getByText('EventID 6005')).toBeDefined();
    expect(screen.getByText(/EventLog 6005 candidate/)).toBeDefined();
    expect(screen.getAllByText(/evtx\.boot_shutdown/).length).toBeGreaterThan(0);
    expect(screen.queryByText('FORENSICS-PC')).toBeNull();
    expect(screen.queryByText('Windows 10')).toBeNull();
  });

  it('renders evidence semantic classification and can start targeted scan', async () => {
    const mutateAsync = vi.fn().mockResolvedValue({});
    mocks.evidenceScan.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
    });

    renderPage();
    fireEvent.click(screen.getByRole('tab', { name: /证据分类/ }));

    expect(screen.getByText('证据语义分类')).toBeDefined();
    expect(screen.getAllByText('系统信息').length).toBeGreaterThan(0);
    expect(screen.getByText('事件日志')).toBeDefined();
    expect(screen.getAllByText('Windows/System32/winevt/Logs/System.evtx').length).toBeGreaterThan(0);
    expect(screen.getAllByText('已发现候选').length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole('button', { name: /开始证据分类/ }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledWith([]));
  });

  it('renders notParsed system info without fake facts', () => {
    mocks.systemInfo.mockReturnValue(queryState({
      data: {
        networkAdapters: [],
        bootHistory: [],
        status: 'notParsed',
        warnings: ['Registry hive 缺失或损坏，系统字段未解析。'],
        provenance: [
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'notParsed',
            warnings: ['registry hive shorter than base block'],
          },
        ],
        fieldProvenance: [],
      },
    }));

    renderPage();

    expect(screen.getAllByText('未解析').length).toBeGreaterThan(0);
    expect(screen.getByText('registry hive shorter than base block')).toBeDefined();
    expect(screen.getByText('字段级 Registry provenance 暂不可用。')).toBeDefined();
    expect(screen.queryByText('FORENSICS-PC')).toBeNull();
    expect(screen.queryByText('Windows 10')).toBeNull();
  });

  it('renders file classifications from hook data', () => {
    renderPage();
    fireEvent.click(screen.getByRole('tab', { name: /文件分类/ }));

    expect(screen.getByText('Documents')).toBeDefined();
    expect(screen.getAllByText('doc.pdf').length).toBeGreaterThan(0);
    expect(screen.getByText('PDF Document')).toBeDefined();
    expect(screen.getAllByText(/metadata\.extension_path/).length).toBeGreaterThan(0);
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
