import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import { getJobsSnapshot, getTraceItems, getWarnings } from './jobs';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('jobs API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getJobsSnapshot calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce([
      { id: 'job-1', status: 'running', progress: 42 },
    ] as never);
    const result = await getJobsSnapshot();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.jobs.GET_JOBS_SNAPSHOT);
    expect(result).toEqual([{ id: 'job-1', status: 'running', progress: 42 }]);
  });

  it('getWarnings calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce([
      { code: 'W001', message: 'Low disk space' },
    ] as never);
    const result = await getWarnings();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.jobs.GET_WARNINGS);
    expect(result).toEqual([{ code: 'W001', message: 'Low disk space' }]);
  });

  it('getTraceItems calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce([
      { timestamp: '2026-01-01T00:00:00Z', message: 'Step started' },
    ] as never);
    const result = await getTraceItems();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.jobs.GET_TRACE_ITEMS);
    expect(result).toEqual([
      { timestamp: '2026-01-01T00:00:00Z', message: 'Step started' },
    ]);
  });
});
