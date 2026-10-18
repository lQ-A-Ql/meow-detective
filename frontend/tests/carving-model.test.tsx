import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePngCarvingModel } from '@/features/carving/hooks';
import type { CarvedArtifact, CarvedRange } from '@/types/carving';

const mocks = vi.hoisted(() => ({ start: vi.fn(), search: vi.fn(), read: vi.fn(), extract: vi.fn(), save: vi.fn() }));
vi.mock('@/features/jobs/hooks/use-evidence-task', () => ({ useEvidenceTask: () => ({ start: mocks.start, cancel: vi.fn(), loading: false, task: undefined, errorText: undefined }) }));
vi.mock('@/lib/api/carving', () => ({ searchCarvedArtifacts: mocks.search, readCarvedArtifactRange: mocks.read, exportCarvedArtifact: mocks.extract }));
vi.mock('@/lib/platform/dialog', () => ({ saveDialog: mocks.save }));
const artifact: CarvedArtifact = { artifactId: 'ds:source:carved-png:id', dataSourceId: 'source', sourceRevision: 'revision', view: 'logicalDisk', offset: '9007199254740993', length: '67', sha256: 'a'.repeat(64), validatorVersion: 'v1' };
function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}
beforeEach(() => { vi.clearAllMocks(); mocks.search.mockResolvedValue({ items: [] }); });
describe('PNG carving source scope', () => {
  it('never auto-scans and preserves addresses above the JavaScript safe integer boundary', async () => {
    const { result } = renderHook(() => usePngCarvingModel('case', 'source'), { wrapper });
    expect(mocks.start).not.toHaveBeenCalled();
    act(() => result.current.setOffset('9007199254740993'));
    act(() => result.current.start());
    expect(mocks.start).toHaveBeenCalledWith({ kind: 'carvePng', request: { dataSourceId: 'source', offset: '9007199254740993', length: undefined } });
    act(() => result.current.setLength('1073741825'));
    expect(result.current.valid).toBe(false);
    act(() => result.current.start());
    expect(mocks.start).toHaveBeenCalledTimes(1);
  });
  it('drops a late byte preview when its source changes', async () => {
    let resolve!: (value: CarvedRange) => void;
    mocks.read.mockImplementation(() => new Promise<CarvedRange>(r => { resolve = r; }));
    const { result, rerender } = renderHook(({ source }) => usePngCarvingModel('case', source), { initialProps: { source: 'source' }, wrapper });
    act(() => result.current.read(artifact));
    rerender({ source: 'other' });
    await act(async () => resolve({ artifactId: artifact.artifactId, offset: '0', base64: 'AQI=', eof: true }));
    expect(result.current.preview).toBeUndefined();
  });
  it('does not export after a save dialog resolves for a retired source', async () => {
    let resolve!: (value: string) => void;
    mocks.save.mockImplementation(() => new Promise<string>(r => { resolve = r; }));
    const { result, rerender } = renderHook(({ source }) => usePngCarvingModel('case', source), { initialProps: { source: 'source' }, wrapper });
    act(() => result.current.extract(artifact));
    await waitFor(() => expect(mocks.save).toHaveBeenCalled());
    rerender({ source: 'other' });
    await act(async () => resolve('D:/exports/candidate.png'));
    expect(mocks.extract).not.toHaveBeenCalled();
  });
});
