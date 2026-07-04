import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  closeCase,
  createCase,
  deleteCase,
  deleteDataSource,
  getCaseMetrics,
  getCurrentCase,
  getDataSources,
  getRecentCases,
  getRecentObjects,
  openCase,
  removeCaseFromList,
  renameDataSource,
} from './case';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('case API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getCurrentCase calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce({ id: 'case-1' } as never);
    const result = await getCurrentCase();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.GET_CURRENT_CASE);
    expect(result).toEqual({ id: 'case-1' });
  });

  it('getCaseMetrics calls the correct command', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    await getCaseMetrics();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.GET_CASE_METRICS);
  });

  it('getRecentObjects calls the correct command', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getRecentObjects();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.GET_RECENT_OBJECTS);
  });

  it('getRecentCases calls the correct command', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getRecentCases();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.GET_RECENT_CASES);
  });

  it('getDataSources calls the correct command', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getDataSources();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.GET_DATA_SOURCES);
  });

  it('createCase sends caseRoot, name, and examiner in request', async () => {
    requestMock.mockResolvedValueOnce({ id: 'case-2' } as never);
    const result = await createCase('/cases/test', 'My Case', 'Alice');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.CREATE_CASE, {
      request: { caseRoot: '/cases/test', name: 'My Case', examiner: 'Alice' },
    });
    expect(result).toEqual({ id: 'case-2' });
  });

  it('createCase defaults examiner to null when omitted', async () => {
    requestMock.mockResolvedValueOnce({ id: 'case-3' } as never);
    await createCase('/cases/test', 'My Case');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.CREATE_CASE, {
      request: { caseRoot: '/cases/test', name: 'My Case', examiner: null },
    });
  });

  it('openCase sends caseRoot in request', async () => {
    requestMock.mockResolvedValueOnce({ id: 'case-4' } as never);
    await openCase('/cases/old');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.OPEN_CASE, {
      request: { caseRoot: '/cases/old' },
    });
  });

  it('closeCase calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await closeCase();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.CLOSE_CASE);
  });

  it('renameDataSource sends dataSourceId and name in request', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await renameDataSource('ds-1', 'Renamed');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.RENAME_DATA_SOURCE, {
      request: { dataSourceId: 'ds-1', name: 'Renamed' },
    });
  });

  it('deleteCase sends caseRoot in request', async () => {
    requestMock.mockResolvedValueOnce('deleted' as never);
    const result = await deleteCase('/cases/old');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.DELETE_CASE, {
      request: { caseRoot: '/cases/old' },
    });
    expect(result).toBe('deleted');
  });

  it('removeCaseFromList sends caseRoot in request', async () => {
    requestMock.mockResolvedValueOnce('removed' as never);
    await removeCaseFromList('/cases/old');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.REMOVE_CASE_FROM_LIST, {
      request: { caseRoot: '/cases/old' },
    });
  });

  it('deleteDataSource sends dataSourceId in request', async () => {
    requestMock.mockResolvedValueOnce('deleted' as never);
    await deleteDataSource('ds-2');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.case.DELETE_DATA_SOURCE, {
      request: { dataSourceId: 'ds-2' },
    });
  });
});
