import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getFileTree: vi.fn(),
  getMediaUrl: vi.fn(),
  readMediaRange: vi.fn(),
}));

vi.mock('@/lib/api/files', () => ({
  getFileTree: mocks.getFileTree,
  getMediaUrl: mocks.getMediaUrl,
  readMediaRange: mocks.readMediaRange,
  getFileRows: vi.fn(),
  getFileRowsPage: vi.fn(),
  getFileJumpContext: vi.fn(),
  getFileChildren: vi.fn(),
  getFileChildrenPage: vi.fn(),
  importDataSource: vi.fn(),
  openFileHandle: vi.fn(),
  readFileRange: vi.fn(),
  extractFile: vi.fn(),
  getTextPreview: vi.fn(),
  getImagePreview: vi.fn(),
}));

vi.mock('@/features/jobs/hooks', () => ({
  expectJobsSnapshotActivity: vi.fn(),
}));

vi.mock('@/features/cache-invalidation', () => ({
  invalidateImportProjectionQueries: vi.fn(),
}));

import { useFileTree, useMediaUrl } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('files hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useFileTree', () => {
    it('fetches file tree with showHidden=false by default', async () => {
      const fakeTree = [{ id: 'root', name: 'Partition 0', isDirectory: true, children: [] }];
      mocks.getFileTree.mockResolvedValue(fakeTree);

      const { result } = renderHook(() => useFileTree(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(fakeTree);
      expect(mocks.getFileTree).toHaveBeenCalledWith(false);
    });

    it('passes showHidden=true when set', async () => {
      mocks.getFileTree.mockResolvedValue([]);

      const { result } = renderHook(() => useFileTree(true), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(mocks.getFileTree).toHaveBeenCalledWith(true);
    });

    it('uses staleTime Infinity to avoid refetching', async () => {
      mocks.getFileTree.mockResolvedValue([]);

      const { result } = renderHook(() => useFileTree(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      // staleTime Infinity means the data never becomes stale
      expect(result.current.isStale).toBe(false);
    });
  });

  describe('useMediaUrl', () => {
    it('is disabled when no fileId is provided', () => {
      const { result } = renderHook(() => useMediaUrl(), {
        wrapper: createWrapper(),
      });

      expect(result.current.fetchStatus).toBe('idle');
      expect(mocks.getMediaUrl).not.toHaveBeenCalled();
    });

    it('returns protocol mode directly without reading range', async () => {
      mocks.getMediaUrl.mockResolvedValue({
        url: 'evidence-media://handle/abc123',
        mode: 'protocol',
        handleId: 'abc123',
        mimeType: 'video/mp4',
        canReadRanges: true,
      });

      const { result } = renderHook(() => useMediaUrl('file-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data?.previewMode).toBe('protocol');
      expect(result.current.data?.url).toBe('evidence-media://handle/abc123');
      expect(mocks.readMediaRange).not.toHaveBeenCalled();
    });

    it('returns inline URL when getMediaUrl provides a direct url', async () => {
      mocks.getMediaUrl.mockResolvedValue({
        url: 'data:audio/mp3;base64,abc',
        mode: 'inline',
        handleId: 'h1',
        mimeType: 'audio/mp3',
        canReadRanges: false,
      });

      const { result } = renderHook(() => useMediaUrl('file-2'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data?.previewMode).toBe('inline');
    });

    it('falls back to range reading when no url and handle has canReadRanges', async () => {
      mocks.getMediaUrl.mockResolvedValue({
        url: null,
        mode: 'rangeFallback',
        handleId: 'h2',
        mimeType: 'video/mp4',
        canReadRanges: true,
      });
      mocks.readMediaRange.mockResolvedValue({
        bytesRead: 4,
        bytesBase64: btoa('test'),
      });

      const { result } = renderHook(() => useMediaUrl('file-3'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(mocks.readMediaRange).toHaveBeenCalledWith({
        handleId: 'h2',
        offset: 0,
        length: 1024 * 1024,
      });
      expect(result.current.data?.previewMode).toBe('rangeFallback');
      expect(result.current.data?.previewBytes).toBe(4);
    });

    it('returns rangeFallback with 0 bytes when readMediaRange returns empty', async () => {
      mocks.getMediaUrl.mockResolvedValue({
        url: null,
        handleId: 'h3',
        mimeType: 'video/mp4',
        canReadRanges: true,
      });
      mocks.readMediaRange.mockResolvedValue({
        bytesRead: 0,
        bytesBase64: '',
      });

      const { result } = renderHook(() => useMediaUrl('file-4'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data?.previewMode).toBe('rangeFallback');
      expect(result.current.data?.previewBytes).toBe(0);
    });
  });
});
