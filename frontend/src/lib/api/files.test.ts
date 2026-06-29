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
  importDataSource,
  openFileHandle,
  readFileRange,
  readMediaRange,
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

  it('importDataSource sends sourcePath in request', async () => {
    requestMock.mockResolvedValueOnce('job-1' as never);
    const result = await importDataSource('/evidence/disk.E01');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.IMPORT_DATA_SOURCE, {
      request: { sourcePath: '/evidence/disk.E01' },
    });
    expect(result).toBe('job-1');
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

  it('readFileRange sends the request object', async () => {
    const req = { handleId: 'h-1', offset: 0, length: 1024 };
    requestMock.mockResolvedValueOnce({ data: [] } as never);
    await readFileRange(req);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.files.READ_FILE_RANGE, { request: req });
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
});
