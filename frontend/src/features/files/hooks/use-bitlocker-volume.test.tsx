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
}));

vi.mock('@/lib/api/files', () => api);

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
