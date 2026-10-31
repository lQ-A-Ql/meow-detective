import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import { useEvidenceDigestModel } from '@/features/files/hooks/use-evidence-digest-model';
import { listEvidenceDigests } from '@/lib/api/digest';
import type { DataSourceSummary } from '@/types/models';
import type { FileEntryRow } from '@/types/files';

vi.mock('@/features/jobs/hooks/use-evidence-task', () => ({ useEvidenceTask: vi.fn() }));
vi.mock('@/lib/api/digest', () => ({ listEvidenceDigests: vi.fn() }));

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client }, children);
  };
}

describe('useEvidenceDigestModel', () => {
  const start = vi.fn();
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listEvidenceDigests).mockResolvedValue([]);
    vi.mocked(useEvidenceTask).mockReturnValue({
      request: undefined,
      task: undefined,
      loading: false,
      errorText: undefined,
      cancelPending: false,
      start,
      retry: vi.fn(),
      cancel: vi.fn(),
    });
  });

  it('passes explicit force recompute to the digest task', () => {
    const source = { id: 'source-1', name: 'Evidence' } as DataSourceSummary;
    const file = { id: 'file-1', entryType: 'file' } as FileEntryRow;
    const { result } = renderHook(
      () => useEvidenceDigestModel('case-1', file, source),
      { wrapper: wrapper() },
    );

    act(() => result.current.setForceRecompute(true));
    act(() => result.current.calculate('sha256'));

    expect(start).toHaveBeenCalledWith({
      kind: 'digest',
      request: {
        scope: 'file',
        algorithm: 'sha256',
        dataSourceId: 'source-1',
        fileId: 'file-1',
        forceRecompute: true,
      },
    });
  });
});
