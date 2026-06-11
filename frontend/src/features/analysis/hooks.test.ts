import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  getSystemInfo: vi.fn(),
  classifyFiles: vi.fn(),
  getEvidenceClassificationSummary: vi.fn(),
  runEvidenceClassification: vi.fn(),
  runAnalysisExtraction: vi.fn(),
  getRegistryExtractionSummary: vi.fn(),
  getBrowserHistorySummary: vi.fn(),
  getEmailExtractionSummary: vi.fn(),
  generateAnalysisSummary: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/analysis', () => ({
  getSystemInfo: mocks.getSystemInfo,
  classifyFiles: mocks.classifyFiles,
  getEvidenceClassificationSummary: mocks.getEvidenceClassificationSummary,
  runEvidenceClassification: mocks.runEvidenceClassification,
  runAnalysisExtraction: mocks.runAnalysisExtraction,
  getRegistryExtractionSummary: mocks.getRegistryExtractionSummary,
  getBrowserHistorySummary: mocks.getBrowserHistorySummary,
  getEmailExtractionSummary: mocks.getEmailExtractionSummary,
  generateAnalysisSummary: mocks.generateAnalysisSummary,
}));

import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useBrowserHistorySummary,
  useEmailExtractionSummary,
  useGenerateAnalysisSummary,
  useRegistryExtractionSummary,
  useRunAnalysisExtraction,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('analysis hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getSystemInfo.mockResolvedValue({
      networkAdapters: [],
      bootHistory: [],
      status: 'notParsed',
      warnings: [],
      provenance: [],
    });
    mocks.classifyFiles.mockResolvedValue([]);
    mocks.getEvidenceClassificationSummary.mockResolvedValue({
      status: 'notFound',
      categories: [],
      generatedAt: '2026-06-01T10:00:00Z',
      warnings: [],
      totals: {
        categoryCount: 0,
        candidateFileCount: 0,
        totalSize: 0,
        artifactCount: 0,
      },
    });
    mocks.runEvidenceClassification.mockResolvedValue({
      status: 'parsed',
      categories: [],
      generatedAt: '2026-06-01T10:00:00Z',
      warnings: [],
      totals: {
        categoryCount: 0,
        candidateFileCount: 0,
        totalSize: 0,
        artifactCount: 0,
      },
    });
    mocks.runAnalysisExtraction.mockResolvedValue({
      status: 'parsed',
      scannedCount: 3,
      artifactCount: 2,
      timelineEventCount: 1,
      generatedAt: '2026-06-01T10:15:00Z',
      warnings: [],
    });
    mocks.getRegistryExtractionSummary.mockResolvedValue({
      status: 'parsed',
      total: 1,
      values: [],
      generatedAt: '2026-06-01T10:10:00Z',
      warnings: [],
    });
    mocks.getBrowserHistorySummary.mockResolvedValue({
      status: 'parsed',
      visitTotal: 1,
      downloadTotal: 0,
      visits: [],
      downloads: [],
      generatedAt: '2026-06-01T10:12:00Z',
      warnings: [],
    });
    mocks.getEmailExtractionSummary.mockResolvedValue({
      status: 'parsed',
      total: 1,
      messages: [],
      generatedAt: '2026-06-01T10:14:00Z',
      warnings: [],
    });
    mocks.generateAnalysisSummary.mockResolvedValue('# 数据源分析报告');
  });

  it('does not call analysis APIs without an active case', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: null,
    });

    const { result } = renderHook(() => useAnalysisSystemInfo(), { wrapper: createWrapper() });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.getSystemInfo).not.toHaveBeenCalled();
  });

  it('loads system info when current case exists', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useAnalysisSystemInfo(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getSystemInfo).toHaveBeenCalledTimes(1);
  });

  it('passes sample size to classification API', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useAnalysisClassifications(250), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.classifyFiles).toHaveBeenCalledWith(250);
  });

  it('exposes summary download mutation', async () => {
    const { result } = renderHook(() => useGenerateAnalysisSummary(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync();
    expect(mocks.generateAnalysisSummary).toHaveBeenCalledTimes(1);
  });

  it('loads registry, browser and email extraction summaries with paging defaults', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const wrapper = createWrapper();
    const registry = renderHook(() => useRegistryExtractionSummary(), { wrapper });
    const browser = renderHook(() => useBrowserHistorySummary({ limit: 50 }), { wrapper });
    const email = renderHook(() => useEmailExtractionSummary({ offset: 10, limit: 25 }), { wrapper });

    await waitFor(() => expect(registry.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(browser.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(email.result.current.isSuccess).toBe(true));

    expect(mocks.getRegistryExtractionSummary).toHaveBeenCalledWith({ offset: 0, limit: 200 });
    expect(mocks.getBrowserHistorySummary).toHaveBeenCalledWith({ offset: 0, limit: 50 });
    expect(mocks.getEmailExtractionSummary).toHaveBeenCalledWith({ offset: 10, limit: 25 });
  });

  it('runs analysis extraction mutation with selected categories', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useRunAnalysisExtraction(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({ categories: ['Registry', 'BrowserHistory', 'Email'] });

    expect(mocks.runAnalysisExtraction).toHaveBeenCalledWith({
      categories: ['Registry', 'BrowserHistory', 'Email'],
    });
  });
});
