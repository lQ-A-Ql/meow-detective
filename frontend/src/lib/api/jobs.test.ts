import { describe, it, expect } from 'vitest';
import { getJobsSnapshot, getWarnings, getTraceItems } from '@/lib/api/jobs';

describe('jobs API (mock mode)', () => {
  it('getJobsSnapshot returns job array', async () => {
    const result = await getJobsSnapshot();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getJobsSnapshot items have required fields', async () => {
    const result = await getJobsSnapshot();
    for (const job of result) {
      expect(job.id).toBeDefined();
      expect(job.name).toBeDefined();
      expect(job.scope).toBeDefined();
      expect(typeof job.progress).toBe('number');
      expect(['pending', 'running', 'completed', 'failed', 'warning']).toContain(job.status);
      expect(typeof job.detail).toBe('string');
      expect(typeof job.warningCount).toBe('number');
      expect(typeof job.skippedCount).toBe('number');
      expect(typeof job.failedCount).toBe('number');
      expect(typeof job.partial).toBe('boolean');
    }
  });

  it('getJobsSnapshot running jobs have partition info', async () => {
    const result = await getJobsSnapshot();
    const runningJobs = result.filter((j) => j.status === 'running');
    for (const job of runningJobs) {
      expect(job.currentPartition).toBeDefined();
      expect(typeof job.completedPartitions).toBe('number');
      expect(typeof job.totalPartitions).toBe('number');
    }
  });

  it('getWarnings returns warning array', async () => {
    const result = await getWarnings();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getWarnings items have required fields', async () => {
    const result = await getWarnings();
    for (const warning of result) {
      expect(warning.id).toBeDefined();
      expect(warning.title).toBeDefined();
      expect(warning.detail).toBeDefined();
    }
  });

  it('getTraceItems returns trace array', async () => {
    const result = await getTraceItems();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getTraceItems items have required fields', async () => {
    const result = await getTraceItems();
    for (const trace of result) {
      expect(trace.id).toBeDefined();
      expect(trace.ts).toBeDefined();
      expect(trace.message).toBeDefined();
    }
  });
});
