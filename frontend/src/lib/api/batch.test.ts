import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  cancelBatch,
  createBatchPlan,
  getBatchJob,
  listBatchJobs,
  pauseBatch,
  resumeBatch,
  startBatch,
} from './batch';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('batch API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('createBatchPlan transforms camelCase plan to snake_case payload', async () => {
    requestMock.mockResolvedValueOnce({ id: 'batch-1' } as never);
    const result = await createBatchPlan({
      name: 'Full Ingest',
      dataSourceIds: ['ds-1', 'ds-2'],
      phases: ['ExtractArtifacts', 'Index'],
      resourceLimits: { maxMemoryMb: 512, maxThreads: 4 },
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.CREATE_BATCH_PLAN, {
      name: 'Full Ingest',
      data_source_ids: ['ds-1', 'ds-2'],
      phases: ['ExtractArtifacts', 'Index'],
      resource_limits: {
        maxMemoryMb: 512,
        maxThreads: 4,
      },
    });
    expect(result).toEqual({ id: 'batch-1' });
  });

  it('startBatch sends batch_id', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await startBatch('batch-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.START_BATCH, {
      batch_id: 'batch-1',
    });
  });

  it('pauseBatch sends batch_id', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await pauseBatch('batch-2');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.PAUSE_BATCH, {
      batch_id: 'batch-2',
    });
  });

  it('resumeBatch sends batch_id', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await resumeBatch('batch-3');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.RESUME_BATCH, {
      batch_id: 'batch-3',
    });
  });

  it('cancelBatch sends batch_id', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);
    await cancelBatch('batch-4');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.CANCEL_BATCH, {
      batch_id: 'batch-4',
    });
  });

  it('getBatchJob sends batch_id and returns the result', async () => {
    requestMock.mockResolvedValueOnce({ id: 'batch-1', status: 'running' } as never);
    const result = await getBatchJob('batch-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.GET_BATCH_JOB, {
      batch_id: 'batch-1',
    });
    expect(result).toEqual({ id: 'batch-1', status: 'running' });
  });

  it('listBatchJobs calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    const result = await listBatchJobs();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.batch.LIST_BATCH_JOBS);
    expect(result).toEqual([]);
  });
});
