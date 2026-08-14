import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  EventEnvelope,
  FileEntryRow,
  FileExtractionProgress,
  FileExtractionResult,
} from '@/types/models';

const mocks = vi.hoisted(() => ({
  extractFileToPath: vi.fn(),
  saveDialog: vi.fn(),
  subscribeToEvent: vi.fn(),
}));

vi.mock('@/lib/api/files', () => ({
  extractFileToPath: mocks.extractFileToPath,
}));

vi.mock('@/lib/platform/dialog', () => ({
  saveDialog: mocks.saveDialog,
}));

vi.mock('@/lib/events/subscribers', () => ({
  subscribeToEvent: mocks.subscribeToEvent,
}));

import { useFileExtractionModel } from './use-file-extraction';

const FILE: FileEntryRow = {
  id: 'ds:source-1:file-1',
  name: 'evidence.bin',
  path: '/evidence.bin',
  entryType: 'file',
  size: 4096,
  deleted: false,
  hidden: false,
  system: false,
  readOnly: false,
  archive: false,
};

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe('useFileExtractionModel', () => {
  let progressListener: ((event: EventEnvelope<FileExtractionProgress>) => void) | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('crypto', { randomUUID: () => 'extract-operation-1' });
    mocks.subscribeToEvent.mockImplementation((topic, listener) => {
      expect(topic).toBe('file-extract-progress');
      progressListener = listener;
      return vi.fn();
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('rejects empty and relative destinations before calling the backend', async () => {
    const { result } = renderHook(() => useFileExtractionModel(), {
      wrapper: createWrapper(),
    });

    act(() => result.current.openForFile(FILE));
    await act(async () => result.current.submit());
    expect(result.current.validationError).toContain('目标路径');

    act(() => result.current.setDestinationPath('relative/evidence.bin'));
    await act(async () => result.current.submit());
    expect(result.current.validationError).toContain('绝对路径');
    expect(mocks.extractFileToPath).not.toHaveBeenCalled();
  });

  it('uses the save dialog, tracks matching byte progress, and opens the result dialog', async () => {
    const extraction = deferred<FileExtractionResult>();
    mocks.saveDialog.mockResolvedValue('D:/exports/evidence.bin');
    mocks.extractFileToPath.mockReturnValue(extraction.promise);
    const { result } = renderHook(() => useFileExtractionModel(), {
      wrapper: createWrapper(),
    });

    act(() => result.current.openForFile(FILE));
    await act(async () => result.current.browseDestination());
    expect(mocks.saveDialog).toHaveBeenCalledWith({ defaultPath: 'evidence.bin' });
    expect(result.current.destinationPath).toBe('D:/exports/evidence.bin');

    act(() => {
      void result.current.submit();
    });
    await waitFor(() => expect(mocks.extractFileToPath).toHaveBeenCalledWith({
      operationId: 'extract-operation-1',
      fileId: FILE.id,
      destinationPath: 'D:/exports/evidence.bin',
      overwrite: false,
    }));

    act(() => progressListener?.({
      eventId: 'event-ignored',
      topic: 'file-extract-progress',
      ts: '2026-07-28T00:00:00Z',
      payload: {
        operationId: 'another-operation',
        fileId: FILE.id,
        phase: 'copying',
        bytesWritten: 2048,
        totalBytes: 4096,
        percent: 50,
      },
    }));
    expect(result.current.progress).toBeUndefined();

    act(() => progressListener?.({
      eventId: 'event-progress',
      topic: 'file-extract-progress',
      ts: '2026-07-28T00:00:01Z',
      payload: {
        operationId: 'extract-operation-1',
        fileId: FILE.id,
        phase: 'copying',
        bytesWritten: 2048,
        totalBytes: 4096,
        percent: 50,
      },
    }));
    expect(result.current.progress?.bytesWritten).toBe(2048);
    expect(result.current.progress?.percent).toBe(50);

    act(() => progressListener?.({
      eventId: 'event-finalizing',
      topic: 'file-extract-progress',
      ts: '2026-07-28T00:00:02Z',
      payload: {
        operationId: 'extract-operation-1',
        fileId: FILE.id,
        phase: 'finalizing',
        bytesWritten: 4096,
        totalBytes: 4096,
        percent: 100,
      },
    }));
    expect(result.current.progress?.phase).toBe('finalizing');

    await act(async () => {
      extraction.resolve({
        fileId: FILE.id,
        bytesWritten: 4096,
        sourceSize: 4096,
        sha256: 'a'.repeat(64),
        destinationFileName: 'evidence.bin',
        sizeVerified: true,
        auditPersisted: true,
      });
      await extraction.promise;
    });

    await waitFor(() => expect(result.current.resultOpen).toBe(true));
    expect(result.current.formOpen).toBe(false);
    expect(result.current.result?.sizeVerified).toBe(true);
  });

  it('starts only one extraction when submit is invoked repeatedly before rerender', async () => {
    const extraction = deferred<FileExtractionResult>();
    mocks.extractFileToPath.mockReturnValue(extraction.promise);
    const { result } = renderHook(() => useFileExtractionModel(), {
      wrapper: createWrapper(),
    });

    act(() => {
      result.current.openForFile(FILE);
      result.current.setDestinationPath('D:/exports/evidence.bin');
    });
    await act(async () => {
      void result.current.submit();
      void result.current.submit();
    });

    await waitFor(() => expect(mocks.extractFileToPath).toHaveBeenCalledTimes(1));

    await act(async () => {
      extraction.resolve({
        fileId: FILE.id,
        bytesWritten: 4096,
        sourceSize: 4096,
        sha256: 'c'.repeat(64),
        destinationFileName: 'evidence.bin',
        sizeVerified: true,
        auditPersisted: true,
      });
      await extraction.promise;
    });
  });
});
