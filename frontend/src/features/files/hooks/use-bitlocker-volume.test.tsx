import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { type PropsWithChildren } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useBitLockerVolumeModel } from './use-bitlocker-volume';
import type { BitLockerCatalogImport, BitLockerVolumeStatus } from '@/types/models';

const api = vi.hoisted(() => ({
  forgetPersistedBitLockerKey: vi.fn(),
  importUnlockedBitLockerCatalog: vi.fn(),
  inspectBitLockerVolume: vi.fn(),
  lockBitLockerVolume: vi.fn(),
  restorePersistedBitLockerKey: vi.fn(),
  unlockBitLockerWithPassword: vi.fn(),
  unlockBitLockerWithRecoveryPassword: vi.fn(),
  unlockBitLockerWithMemoryImage: vi.fn(),
}));

const platform = vi.hoisted(() => ({
  openDialog: vi.fn(),
  singleDialogPath: vi.fn((path: string | string[] | null) => Array.isArray(path) ? path[0] ?? null : path),
}));

vi.mock('@/lib/api/files', () => api);
vi.mock('@/lib/platform/dialog', () => platform);

const target = { dataSourceId: 'source-1', partitionIndex: 2 };

const volume = {
  dataSourceId: target.dataSourceId,
  partitionIndex: target.partitionIndex,
  unlocked: true,
  encryptionMethod: 'AES-128-CBC',
  encryptionMethodCode: 0x8002,
  decryptable: true,
  bytesPerSector: 512,
  metadataFingerprint: 'fingerprint',
  metadataCopyCount: 2,
  protectors: [],
  supportsPassword: true,
  supportsRecoveryPassword: true,
  storedKeyAvailable: true,
} satisfies BitLockerVolumeStatus;

function createWrapper(queryClient: QueryClient) {
  return function QueryWrapper({ children }: PropsWithChildren) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe('useBitLockerVolumeModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.inspectBitLockerVolume.mockResolvedValue(volume);
    platform.openDialog.mockResolvedValue(null);
  });

  it('selects a real memory image path through the platform adapter', async () => {
    const lockedVolume = { ...volume, unlocked: false, storedKeyAvailable: false };
    platform.openDialog.mockResolvedValue('D:\\evidence\\memory.raw');
    api.unlockBitLockerWithMemoryImage.mockResolvedValue(lockedVolume);
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const { result } = renderHook(() => useBitLockerVolumeModel(target), {
      wrapper: createWrapper(queryClient),
    });

    await waitFor(() => expect(api.inspectBitLockerVolume).toHaveBeenCalled());
    await act(async () => {
      expect(await result.current.unlockFromMemoryImage()).toBe(true);
    });

    expect(api.unlockBitLockerWithMemoryImage).toHaveBeenCalledWith(
      'source-1',
      2,
      'D:\\evidence\\memory.raw',
    );
    expect(result.current.status).toEqual(lockedVolume);
    expect(result.current.memoryUnlocking).toBe(false);
  });

  it('keeps the file browser locked until catalog queries have refreshed', async () => {
    let resolveImport!: (value: BitLockerCatalogImport) => void;
    const importResult = new Promise<BitLockerCatalogImport>((resolve) => {
      resolveImport = resolve;
    });
    let resolveRefresh!: () => void;
    const refreshResult = new Promise<void>((resolve) => {
      resolveRefresh = resolve;
    });
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateQueries = vi
      .spyOn(queryClient, 'invalidateQueries')
      .mockImplementation(() => refreshResult);
    api.importUnlockedBitLockerCatalog.mockReturnValue(importResult);

    const { result } = renderHook(() => useBitLockerVolumeModel(target), {
      wrapper: createWrapper(queryClient),
    });

    await waitFor(() => expect(api.inspectBitLockerVolume).toHaveBeenCalledWith('source-1', 2));

    act(() => {
      void result.current.importCatalog();
    });

    expect(result.current.importing).toBe(true);
    expect(result.current.catalogImport?.phase).toBe('catalog');

    await act(async () => {
      resolveImport({
        volume,
        imported: true,
        fileCount: 10,
        directoryCount: 2,
        warnings: [],
      });
      await Promise.resolve();
    });

    await waitFor(() => expect(result.current.catalogImport?.phase).toBe('refreshing'));
    expect(invalidateQueries).toHaveBeenCalledTimes(5);
    expect(result.current.importing).toBe(true);

    await act(async () => {
      resolveRefresh();
      await Promise.resolve();
    });

    await waitFor(() => expect(result.current.importing).toBe(false));
    expect(result.current.catalogImport).toBeUndefined();
    expect(result.current.catalog).toMatchObject({ fileCount: 10, directoryCount: 2 });
  });
});
