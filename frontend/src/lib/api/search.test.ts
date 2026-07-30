import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import { searchFiles } from './search';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('search API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('searchFiles sends query with default offset and limit', async () => {
    requestMock.mockResolvedValueOnce({ items: [], total: 0 } as never);
    const result = await searchFiles('password');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.search.SEARCH_FILES_REQUEST, {
      request: { query: 'password', offset: 0, limit: 50 },
    });
    expect(result).toEqual({ items: [], total: 0 });
  });

  it('searchFiles sends custom offset and limit', async () => {
    requestMock.mockResolvedValueOnce({ items: [], total: 100 } as never);
    await searchFiles('malware', 10, 25);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.search.SEARCH_FILES_REQUEST, {
      request: { query: 'malware', offset: 10, limit: 25 },
    });
  });

  it('searchFiles sends an opaque continuation cursor without changing the offset', async () => {
    requestMock.mockResolvedValueOnce({ items: [], total: 100 } as never);
    await searchFiles('malware', 0, 25, 'v1.payload.digest');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.search.SEARCH_FILES_REQUEST, {
      request: {
        query: 'malware',
        offset: 0,
        limit: 25,
        cursor: 'v1.payload.digest',
      },
    });
  });

  it('searchFiles forwards metadata filters and sort options', async () => {
    requestMock.mockResolvedValueOnce({ items: [], total: 0 } as never);
    await searchFiles('report', 0, 100, undefined, {
      matchPath: true,
      entryType: 'file',
      extensions: ['txt', 'log'],
      dataSourceIds: ['source-1'],
      sortKey: 'modifiedAt',
      sortDirection: 'desc',
    });

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.search.SEARCH_FILES_REQUEST, {
      request: {
        query: 'report',
        offset: 0,
        limit: 100,
        matchPath: true,
        entryType: 'file',
        extensions: ['txt', 'log'],
        dataSourceIds: ['source-1'],
        sortKey: 'modifiedAt',
        sortDirection: 'desc',
      },
    });
  });

  it('searchFiles sends empty string query', async () => {
    requestMock.mockResolvedValueOnce({ items: [], total: 0 } as never);
    await searchFiles('');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.search.SEARCH_FILES_REQUEST, {
      request: { query: '', offset: 0, limit: 50 },
    });
  });
});
