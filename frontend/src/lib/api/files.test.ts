import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  cancelImport,
  getFileChildrenPage,
  getFileJumpContext,
  getFileRowsPage,
  getFileTree,
  getImagePreview,
  getMediaUrl,
  getTextPreview,
  exportDeletedRecovery,
  importDataSource,
  listDeletedRecoveries,
  openFileHandle,
  closeFileHandle,
  readDeletedRecoveryRange,
  readFileRange,
  readMediaRange,
  runDeletedRecovery,
  importUnlockedBitLockerCatalog,
  inspectBitLockerVolume,
  forgetPersistedBitLockerKey,
  lockBitLockerVolume,
  restorePersistedBitLockerKey,
  unlockBitLockerWithPassword,
  unlockBitLockerWithRecoveryPassword,
  unlockBitLockerWithMemoryImage,
} from './files';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

const requestMock = vi.mocked(apiClient.request);

describe('files API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getFileTree sends showHidden in request', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getFileTree(true);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_TREE_REQUEST, {
      request: { showHidden: true },
    });
  });

  it('getFileTree defaults showHidden to false', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getFileTree();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_TREE_REQUEST, {
      request: { showHidden: false },
    });
  });

  it('getFileRowsPage sends all paging parameters', async () => {
    requestMock.mockResolvedValueOnce({ rows: [], total: 0 } as never);
    await getFileRowsPage('parent-1', 10, 20, true, 'size', 'desc');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_ROWS_REQUEST, {
      request: {
        parentId: 'parent-1',
        offset: 10,
        limit: 20,
        showHidden: true,
        sortKey: 'size',
        sortDirection: 'desc',
      },
    });
  });

  it('getFileRowsPage defaults parentId to null when undefined', async () => {
    requestMock.mockResolvedValueOnce({ rows: [], total: 0 } as never);
    await getFileRowsPage();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_ROWS_REQUEST, {
      request: {
        parentId: null,
        offset: 0,
        limit: 500,
        showHidden: false,
        sortKey: 'name',
        sortDirection: 'asc',
      },
    });
  });

  it('importDataSource sends the required Windows platform in the request', async () => {
    requestMock.mockResolvedValueOnce('job-1' as never);
    const result = await importDataSource({
      sourcePath: '/evidence/disk.E01',
      platform: 'windows',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.IMPORT_DATA_SOURCE, {
      request: {
        sourcePath: '/evidence/disk.E01',
        platform: 'windows',
      },
    });
    expect(result).toBe('job-1');
  });

  it('importDataSource sends the required Linux platform and optional profile', async () => {
    requestMock.mockResolvedValueOnce('job-2' as never);
    const result = await importDataSource({
      sourcePath: '/evidence/linux.raw',
      platform: 'linux',
      profile: 'ubuntu-server',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.IMPORT_DATA_SOURCE, {
      request: {
        sourcePath: '/evidence/linux.raw',
        platform: 'linux',
        profile: 'ubuntu-server',
      },
    });
    expect(result).toBe('job-2');
  });

  it('importDataSource sends linux cluster source kind when requested', async () => {
    requestMock.mockResolvedValueOnce('job-cluster' as never);
    const result = await importDataSource({
      sourcePath: '/evidence/pve-cluster',
      sourceKind: 'linuxCluster',
      platform: 'linux',
      profile: 'pve',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.IMPORT_DATA_SOURCE, {
      request: {
        sourcePath: '/evidence/pve-cluster',
        sourceKind: 'linuxCluster',
        platform: 'linux',
        profile: 'pve',
      },
    });
    expect(result).toBe('job-cluster');
  });

  it('getFileChildrenPage sends paging parameters', async () => {
    requestMock.mockResolvedValueOnce({ children: [], total: 0 } as never);
    await getFileChildrenPage('parent-2', 5, 50, false);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_CHILDREN_REQUEST, {
      request: { parentId: 'parent-2', offset: 5, limit: 50, showHidden: false },
    });
  });

  it('openFileHandle sends fileId in request', async () => {
    requestMock.mockResolvedValueOnce({ handle: 'h-1' } as never);
    await openFileHandle('file-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.OPEN_FILE_HANDLE_REQUEST, {
      request: { fileId: 'file-1' },
    });
  });

  it('closeFileHandle sends the opaque handle ID', async () => {
    requestMock.mockResolvedValue(true as never);
    await closeFileHandle('preview-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.CLOSE_FILE_HANDLE, {
      handleId: 'preview-1',
    });
  });

  it('readFileRange sends the request object', async () => {
    const req = { handleId: 'h-1', offset: 0, length: 1024 };
    requestMock.mockResolvedValueOnce({ data: [] } as never);
    await readFileRange(req);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.READ_FILE_RANGE, { request: req });
  });

  it('routes BitLocker inspection and catalog import by source and partition', async () => {
    requestMock.mockResolvedValue({} as never);
    await inspectBitLockerVolume('source-1', 2);
    await importUnlockedBitLockerCatalog('source-1', 2);
    await lockBitLockerVolume('source-1', 2);
    await restorePersistedBitLockerKey('source-1', 2);
    await forgetPersistedBitLockerKey('source-1', 2);
    expect(requestMock).toHaveBeenNthCalledWith(1, COMMANDS.files.INSPECT_BITLOCKER_VOLUME, {
      dataSourceId: 'source-1',
      partitionIndex: 2,
    });
    expect(requestMock).toHaveBeenNthCalledWith(2, COMMANDS.files.IMPORT_UNLOCKED_BITLOCKER_CATALOG, {
      dataSourceId: 'source-1',
      partitionIndex: 2,
    });
    expect(requestMock).toHaveBeenNthCalledWith(3, COMMANDS.files.LOCK_BITLOCKER_VOLUME, {
      dataSourceId: 'source-1',
      partitionIndex: 2,
    });
    expect(requestMock).toHaveBeenNthCalledWith(
      4,
      COMMANDS.files.RESTORE_PERSISTED_BITLOCKER_KEY,
      { dataSourceId: 'source-1', partitionIndex: 2 },
    );
    expect(requestMock).toHaveBeenNthCalledWith(
      5,
      COMMANDS.files.FORGET_PERSISTED_BITLOCKER_KEY,
      { dataSourceId: 'source-1', partitionIndex: 2 },
    );
  });

  it('passes BitLocker credentials only to the selected unlock command', async () => {
    requestMock.mockResolvedValue({} as never);
    await unlockBitLockerWithPassword('source-1', 2, 'test-password');
    await unlockBitLockerWithRecoveryPassword('source-1', 2, 'test-recovery');
    expect(requestMock).toHaveBeenNthCalledWith(1, COMMANDS.files.UNLOCK_BITLOCKER_WITH_PASSWORD, {
      dataSourceId: 'source-1',
      partitionIndex: 2,
      credential: 'test-password',
    });
    expect(requestMock).toHaveBeenNthCalledWith(
      2,
      COMMANDS.files.UNLOCK_BITLOCKER_WITH_RECOVERY_PASSWORD,
      {
        dataSourceId: 'source-1',
        partitionIndex: 2,
        credential: 'test-recovery',
      },
    );
  });

  it('passes the memory image only to the dedicated BitLocker recovery command', async () => {
    requestMock.mockResolvedValue({} as never);
    await unlockBitLockerWithMemoryImage('source-1', 2, 'D:\\evidence\\memory.raw');

    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.files.UNLOCK_BITLOCKER_WITH_MEMORY_IMAGE,
      {
        dataSourceId: 'source-1',
        partitionIndex: 2,
        memoryImagePath: 'D:\\evidence\\memory.raw',
      },
    );
  });

  it('cancelImport sends jobId', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await cancelImport('job-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.CANCEL_IMPORT, { jobId: 'job-1' });
  });

  it('getTextPreview sends fileId and maxBytes', async () => {
    requestMock.mockResolvedValueOnce({ text: 'hello' } as never);
    await getTextPreview('file-1', 2048);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_TEXT_PREVIEW, {
      fileId: 'file-1',
      maxBytes: 2048,
    });
  });

  it('getTextPreview defaults maxBytes to 1MB', async () => {
    requestMock.mockResolvedValueOnce({ text: 'hello' } as never);
    await getTextPreview('file-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_TEXT_PREVIEW, {
      fileId: 'file-1',
      maxBytes: 1024 * 1024,
    });
  });

  it('getImagePreview sends fileId', async () => {
    requestMock.mockResolvedValueOnce({ base64: 'abc' } as never);
    await getImagePreview('file-2');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_IMAGE_PREVIEW, {
      fileId: 'file-2',
    });
  });

  it('getMediaUrl sends fileId', async () => {
    requestMock.mockResolvedValueOnce({ handle: 'media-1' } as never);
    await getMediaUrl('file-3');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_MEDIA_URL, {
      fileId: 'file-3',
    });
  });

  it('readMediaRange sends the request object', async () => {
    const req = { handleId: 'media-1', offset: 0, length: 4096 };
    requestMock.mockResolvedValueOnce({ data: [] } as never);
    await readMediaRange(req);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.READ_MEDIA_RANGE, { request: req });
  });

  it('getFileJumpContext sends fileId and options', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getFileJumpContext('file-4', {
      showHidden: true,
      pageLimit: 100,
      sortKey: 'size',
      sortDirection: 'desc',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_JUMP_CONTEXT, {
      request: {
        fileId: 'file-4',
        showHidden: true,
        pageLimit: 100,
        sortKey: 'size',
        sortDirection: 'desc',
      },
    });
  });

  it('getFileJumpContext defaults options', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getFileJumpContext('file-5');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.GET_FILE_JUMP_CONTEXT, {
      request: {
        fileId: 'file-5',
        showHidden: false,
        pageLimit: 500,
        sortKey: 'name',
        sortDirection: 'asc',
      },
    });
  });

  it('lists persisted deleted recoveries for one source partition', async () => {
    requestMock.mockResolvedValueOnce({ recoveries: [], total: 0 } as never);

    await listDeletedRecoveries('source-1', 2, 100, 50);

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.LIST_DELETED_RECOVERIES, {
      request: { dataSourceId: 'source-1', partitionIndex: 2, offset: 100, limit: 50 },
    });
  });

  it('runs deleted recovery for the selected partition', async () => {
    requestMock.mockResolvedValueOnce({ dataSourceId: 'source-1', scans: [], failures: [] } as never);

    await runDeletedRecovery('source-1', 2);

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.RUN_DELETED_RECOVERY, {
      request: { dataSourceId: 'source-1', partitionIndex: 2 },
    });
  });

  it('reads a verified deleted recovery range', async () => {
    requestMock.mockResolvedValueOnce({ bytesBase64: 'AA==' } as never);

    await readDeletedRecoveryRange('source-1', `recovery:${'a'.repeat(64)}`, 4096, 1024);

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.READ_DELETED_RECOVERY_RANGE, {
      request: {
        dataSourceId: 'source-1',
        recoveryId: `recovery:${'a'.repeat(64)}`,
        offset: 4096,
        length: 1024,
      },
    });
  });

  it('exports a complete deleted recovery without overwrite by default', async () => {
    requestMock.mockResolvedValueOnce({ bytesWritten: 12, sha256: 'abc' } as never);

    await exportDeletedRecovery('source-1', `recovery:${'b'.repeat(64)}`, 'D:/exports/file.bin');

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.EXPORT_DELETED_RECOVERY, {
      request: {
        dataSourceId: 'source-1',
        recoveryId: `recovery:${'b'.repeat(64)}`,
        destinationPath: 'D:/exports/file.bin',
        overwrite: false,
      },
    });
  });
});
