import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  getSystemInfo: vi.fn(),
  classifyFiles: vi.fn(),
  generateAnalysisSummary: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/analysis', () => ({
  getSystemInfo: mocks.getSystemInfo,
  classifyFiles: mocks.classifyFiles,
  generateAnalysisSummary: mocks.generateAnalysisSummary,
}));

import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useGenerateAnalysisSummary,
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
});
