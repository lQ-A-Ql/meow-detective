import { describe, it, expect } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement } from 'react';
import { useCurrentCase, useCaseMetrics, useRecentObjects, useDataSources } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('case hooks (mock mode)', () => {
  it('useCurrentCase returns case data', async () => {
    const { result } = renderHook(() => useCurrentCase(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).not.toBeNull();
    expect(result.current.data!.name).toBe('WannaCry 爆发溯源');
  });

  it('useCaseMetrics returns metrics', async () => {
    const { result } = renderHook(() => useCaseMetrics(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data!.dataSourceCount).toBe(4);
    expect(result.current.data!.artifactCount).toBeGreaterThan(0);
  });

  it('useRecentObjects returns list', async () => {
    const { result } = renderHook(() => useRecentObjects(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data!.length).toBeGreaterThan(0);
  });

  it('useDataSources returns sources', async () => {
    const { result } = renderHook(() => useDataSources(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data!.length).toBeGreaterThan(0);
    expect(result.current.data![0].partitions.length).toBeGreaterThan(0);
  });
});
