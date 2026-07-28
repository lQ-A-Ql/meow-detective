import { createElement, StrictMode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HexViewer } from '@/components/viewers/HexViewer';
import type { FileChildrenPage } from '@/types/models';

const mocks = vi.hoisted(() => ({
  getFileTree: vi.fn(),
  getFileChildrenPage: vi.fn(),
  getMediaUrl: vi.fn(),
  readMediaRange: vi.fn(),
  openFileHandle: vi.fn(),
  closeFileHandle: vi.fn(),
  readFileRange: vi.fn(),
  getTextPreview: vi.fn(),
  getImagePreview: vi.fn(),
  importDataSource: vi.fn(),
}));

vi.mock('@/lib/api/files', () => ({
  getFileTree: mocks.getFileTree,
  getMediaUrl: mocks.getMediaUrl,
  readMediaRange: mocks.readMediaRange,
  getFileRows: vi.fn(),
  getFileRowsPage: vi.fn(),
  getFileJumpContext: vi.fn(),
  getFileChildren: vi.fn(),
  getFileChildrenPage: mocks.getFileChildrenPage,
  importDataSource: mocks.importDataSource,
  openFileHandle: mocks.openFileHandle,
  closeFileHandle: mocks.closeFileHandle,
  readFileRange: mocks.readFileRange,
  extractFile: vi.fn(),
  getTextPreview: mocks.getTextPreview,
  getImagePreview: mocks.getImagePreview,
}));

vi.mock('@/features/jobs/hooks', () => ({
  expectJobsSnapshotActivity: vi.fn(),
}));

vi.mock('@/features/cache-invalidation', () => ({
  invalidateImportProjectionQueries: vi.fn(),
}));

import {
  useFileHandle,
  useFileChildrenPage,
  useImportDataSource,
  useFileTree,
  useFileViewer,
  useImagePreview,
  useMediaUrl,
  useTextPreview,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

function createStrictWrapper() {
  const Wrapper = createWrapper();
  return function StrictWrapper({ children }: { children: React.ReactNode }) {
    return createElement(StrictMode, null, createElement(Wrapper, null, children));
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe('files hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.closeFileHandle.mockResolvedValue(false);
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

    it('does not reuse another parent directory page while switching branches', async () => {
      const firstPage = deferred<FileChildrenPage>();
      const secondPage = deferred<FileChildrenPage>();
      mocks.getFileChildrenPage
        .mockReturnValueOnce(firstPage.promise)
        .mockReturnValueOnce(secondPage.promise);

      const { result, rerender } = renderHook(
        ({ parentId }: { parentId?: string }) => useFileChildrenPage(parentId),
        { initialProps: { parentId: 'parent-a' }, wrapper: createWrapper() },
      );

      await waitFor(() => expect(mocks.getFileChildrenPage).toHaveBeenCalledWith('parent-a', 0, 500, false));
      await act(async () => {
        firstPage.resolve({
          children: [{ id: 'child-a', name: 'A', depth: 1, hasChildren: false, deleted: false, hidden: false, system: false }],
          totalCount: 1,
          offset: 0,
          limit: 500,
          truncated: false,
        });
        await firstPage.promise;
      });
      await waitFor(() => expect(result.current.data?.children[0]?.id).toBe('child-a'));

      rerender({ parentId: 'parent-b' });

      expect(result.current.data).toBeUndefined();
    });
  });

  describe('useImportDataSource', () => {
    it('passes the typed request object unchanged to the real API boundary', async () => {
      mocks.importDataSource.mockResolvedValue('job-import-1');
      const request = {
        sourcePath: 'D:/evidence/linux.E01',
        platform: 'linux' as const,
      };
      const { result } = renderHook(() => useImportDataSource(), {
        wrapper: createWrapper(),
      });

      await act(async () => {
        await result.current.mutateAsync(request);
      });

      expect(mocks.importDataSource).toHaveBeenCalledWith(request);
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

      const { result, unmount } = renderHook(() => useMediaUrl('file-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data?.previewMode).toBe('protocol');
      expect(result.current.data?.url).toBe('evidence-media://handle/abc123');
      expect(mocks.readMediaRange).not.toHaveBeenCalled();
      unmount();
      await waitFor(() => expect(mocks.closeFileHandle).toHaveBeenCalledWith('abc123'));
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

    it('closes the media handle when fallback range loading fails', async () => {
      mocks.getMediaUrl.mockResolvedValue({
        url: null,
        handleId: 'h-failed',
        mimeType: 'video/mp4',
        canReadRanges: true,
      });
      mocks.readMediaRange.mockRejectedValue(new Error('range failed'));

      const { result } = renderHook(() => useMediaUrl('file-failed'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isError).toBe(true));
      expect(mocks.closeFileHandle).toHaveBeenCalledWith('h-failed');
    });
  });

  describe('preview hook gating', () => {
    it('keeps handle and hex range calls idle when disabled', () => {
      const { result: handleResult } = renderHook(
        () => useFileHandle('file-disabled', false),
        { wrapper: createWrapper() },
      );
      const { result: viewerResult } = renderHook(
        () => useFileViewer('file-disabled', false),
        { wrapper: createWrapper() },
      );

      expect(handleResult.current.fetchStatus).toBe('idle');
      expect(viewerResult.current.fetchStatus).toBe('idle');
      expect(mocks.openFileHandle).not.toHaveBeenCalled();
      expect(mocks.readFileRange).not.toHaveBeenCalled();
    });

    it('keeps text, image, and media preview calls idle when disabled', () => {
      const { result: textResult } = renderHook(
        () => useTextPreview('file-disabled', false),
        { wrapper: createWrapper() },
      );
      const { result: imageResult } = renderHook(
        () => useImagePreview('file-disabled', false),
        { wrapper: createWrapper() },
      );
      const { result: mediaResult } = renderHook(
        () => useMediaUrl('file-disabled', false),
        { wrapper: createWrapper() },
      );

      expect(textResult.current.fetchStatus).toBe('idle');
      expect(imageResult.current.fetchStatus).toBe('idle');
      expect(mediaResult.current.fetchStatus).toBe('idle');
      expect(mocks.getTextPreview).not.toHaveBeenCalled();
      expect(mocks.getImagePreview).not.toHaveBeenCalled();
      expect(mocks.getMediaUrl).not.toHaveBeenCalled();
      expect(mocks.readMediaRange).not.toHaveBeenCalled();
    });
  });

  describe('useFileViewer', () => {
    it('loads small files completely instead of fixed 96 bytes', async () => {
      mocks.openFileHandle.mockResolvedValue({
        handleId: 'file:small',
        size: 4096,
        mime: 'application/octet-stream',
      });
      mocks.readFileRange.mockResolvedValue({
        kind: 'hex',
        lines: ['00000000  41 42 43 44'],
      });

      const { result } = renderHook(() => useFileViewer('file-small'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(mocks.readFileRange).toHaveBeenCalledWith({
        handleId: 'file:small',
        offset: 0,
        length: 4096,
      });
      expect(result.current.data?.mode).toBe('full');
      expect(result.current.data?.isFullyLoaded).toBe(true);
    });

    it('loads large files in 64KB chunks and can jump to a later chunk', async () => {
      mocks.openFileHandle.mockResolvedValue({
        handleId: 'file:large',
        size: 3 * 1024 * 1024,
        mime: 'application/octet-stream',
      });
      mocks.readFileRange
        .mockResolvedValueOnce({
          kind: 'hex',
          lines: ['00000000  41 42 43 44'],
          rawBytes: [0x41, 0x42, 0x43, 0x44],
        })
        .mockResolvedValueOnce({
          kind: 'hex',
          lines: ['00010000  45 46 47 48'],
          rawBytes: [0x45, 0x46, 0x47, 0x48],
        });

      const { result } = renderHook(() => useFileViewer('file-large'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(mocks.readFileRange).toHaveBeenNthCalledWith(1, {
        handleId: 'file:large',
        offset: 0,
        length: 64 * 1024,
      });
      expect(result.current.data?.mode).toBe('chunked');
      expect(result.current.data?.chunkSize).toBe(64 * 1024);
      expect(result.current.data?.rawBytes).toEqual([0x41, 0x42, 0x43, 0x44]);
      expect(result.current.data?.baseOffset).toBe(0);

      await act(async () => {
        result.current.jumpToOffset('0x10000');
      });

      await waitFor(() => {
        expect(mocks.readFileRange).toHaveBeenNthCalledWith(2, {
          handleId: 'file:large',
          offset: 64 * 1024,
          length: 64 * 1024,
        });
      });
      await waitFor(() => {
        expect(result.current.data?.activeOffset).toBe(64 * 1024);
        expect(result.current.data?.baseOffset).toBe(64 * 1024);
        expect(result.current.data?.rawBytes).toEqual([0x45, 0x46, 0x47, 0x48]);
      });
    });

    it('renders hex from raw bytes without depending on backend lines', async () => {
      mocks.openFileHandle.mockResolvedValue({
        handleId: 'file:raw-only',
        size: 2 * 1024 * 1024,
        mime: 'application/octet-stream',
      });
      mocks.readFileRange.mockResolvedValue({
        kind: 'hex',
        lines: [],
        rawBytes: [0x4d, 0x5a, 0x00, 0xff],
      });

      const { result } = renderHook(() => useFileViewer('file-raw-only'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data?.lines.length).toBeGreaterThan(0);
      expect(result.current.data?.lines.rawBytes).toEqual([0x4d, 0x5a, 0x00, 0xff]);

      render(
        createElement(HexViewer, {
          lines: result.current.data!.lines,
          activeOffset: result.current.data!.activeOffset,
        }),
      );

      expect(screen.getByText('00000000')).toBeDefined();
      expect(screen.getByText('4D')).toBeDefined();
      expect(screen.getByText('5A')).toBeDefined();
      expect(screen.getByText('00')).toBeDefined();
      expect(screen.getByText('FF')).toBeDefined();
    });

    it('closes the handle when the initial hex range fails', async () => {
      mocks.openFileHandle.mockResolvedValue({
        handleId: 'file:range-failed',
        size: 4096,
        mime: 'application/octet-stream',
      });
      mocks.readFileRange.mockRejectedValue(new Error('range failed'));

      const { result } = renderHook(() => useFileViewer('file-range-failed'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isError).toBe(true));
      expect(mocks.closeFileHandle).toHaveBeenCalledWith('file:range-failed');
    });
  });

  describe('useFileHandle', () => {
    it('does not dispose the live handle during StrictMode effect replay', async () => {
      mocks.openFileHandle
        .mockResolvedValueOnce({
          handleId: 'file:strict-replayed',
          size: 4096,
          mime: 'application/octet-stream',
        })
        .mockResolvedValueOnce({
          handleId: 'file:strict-live',
          size: 4096,
          mime: 'application/octet-stream',
        });

      const { result, unmount } = renderHook(() => useFileHandle('file-strict'), {
        wrapper: createStrictWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data?.handleId).toBe('file:strict-live');
      expect(mocks.closeFileHandle).toHaveBeenCalledWith('file:strict-replayed');
      expect(mocks.closeFileHandle).not.toHaveBeenCalledWith('file:strict-live');
      unmount();
      await waitFor(() => expect(mocks.closeFileHandle).toHaveBeenCalledWith('file:strict-live'));
    });

    it('closes a handle that resolves after the hook was unmounted', async () => {
      const pending = deferred<{ handleId: string; size: number; mime: string }>();
      mocks.openFileHandle.mockReturnValue(pending.promise);

      const { unmount } = renderHook(() => useFileHandle('file-pending'), {
        wrapper: createWrapper(),
      });
      unmount();
      pending.resolve({
        handleId: 'file:late',
        size: 4096,
        mime: 'application/octet-stream',
      });

      await waitFor(() => expect(mocks.closeFileHandle).toHaveBeenCalledWith('file:late'));
    });
  });
});
