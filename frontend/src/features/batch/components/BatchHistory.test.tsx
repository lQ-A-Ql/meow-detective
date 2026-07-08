import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { BatchHistory } from './BatchHistory';
import type { BatchJob } from '@/types/models';

function makeJob(overrides: Partial<BatchJob> = {}): BatchJob {
  return {
    id: 'job-1',
    caseId: 'case-1',
    label: 'Import Case A',
    status: 'completed',
    phases: [
      { kind: 'Mount', state: 'completed', progress: 100, errorCount: 0, warnings: [] },
    ],
    plan: {
      dataSourceRefs: ['ds-1'],
      phases: ['Mount'],
      resourceLimits: { maxMemoryMb: 256, maxThreads: 2 },
    },
    createdAt: '2026-06-01T10:00:00Z',
    ...overrides,
  };
}

describe('BatchHistory', () => {
  it('renders header and empty state when no jobs', () => {
    render(createElement(BatchHistory, { jobs: [] }));
    expect(screen.getByText('Batch Job History')).toBeDefined();
    expect(screen.getByText('No batch jobs')).toBeDefined();
  });

  it('renders job rows with label and status', () => {
    const jobs = [
      makeJob({ id: 'j1', label: 'Import Case A', status: 'completed' }),
      makeJob({ id: 'j2', label: 'Import Case B', status: 'failed' }),
    ];
    render(createElement(BatchHistory, { jobs }));
    expect(screen.getByText('Import Case A')).toBeDefined();
    expect(screen.getByText('Import Case B')).toBeDefined();
  });

  it('calls onSelectJob when a row is clicked', () => {
    const onSelectJob = vi.fn();
    const job = makeJob();
    render(createElement(BatchHistory, { jobs: [job], onSelectJob }));
    fireEvent.click(screen.getByText('Import Case A'));
    expect(onSelectJob).toHaveBeenCalledWith(job);
  });
});
