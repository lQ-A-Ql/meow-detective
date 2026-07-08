import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { BatchMonitor } from './BatchMonitor';
import type { BatchJob } from '@/types/models';

function makeJob(overrides: Partial<BatchJob> = {}): BatchJob {
  return {
    id: 'job-1',
    caseId: 'case-1',
    label: 'Test Batch Job',
    status: 'running',
    phases: [
      { kind: 'Mount', state: 'completed', progress: 100, errorCount: 0, warnings: [] },
      { kind: 'Catalog', state: 'running', progress: 50, errorCount: 0, warnings: [] },
      { kind: 'ExtractArtifacts', state: 'pending', progress: 0, errorCount: 0, warnings: [] },
    ],
    plan: {
      dataSourceRefs: ['ds-1', 'ds-2'],
      phases: ['Mount', 'Catalog', 'ExtractArtifacts'],
      resourceLimits: { maxMemoryMb: 512, maxThreads: 4 },
    },
    createdAt: '2026-06-01T10:00:00Z',
    ...overrides,
  };
}

describe('BatchMonitor', () => {
  it('renders job label and status badge', () => {
    render(createElement(BatchMonitor, { job: makeJob() }));
    expect(screen.getByText('Test Batch Job')).toBeDefined();
    // "Running" appears in both the job status badge and the Catalog phase badge
    expect(screen.getAllByText('Running').length).toBeGreaterThanOrEqual(1);
  });

  it('renders phase list and data source count', () => {
    render(createElement(BatchMonitor, { job: makeJob() }));
    expect(screen.getByText('Mount')).toBeDefined();
    expect(screen.getByText('Catalog')).toBeDefined();
    expect(screen.getByText('ExtractArtifacts')).toBeDefined();
    expect(screen.getByText('2')).toBeDefined(); // Sources count
  });

  it('shows pause button when running and calls onPause', () => {
    const onPause = vi.fn();
    render(createElement(BatchMonitor, { job: makeJob({ status: 'running' }), onPause }));
    const btn = screen.getByRole('button', { name: /Pause/ });
    fireEvent.click(btn);
    expect(onPause).toHaveBeenCalledOnce();
  });

  it('does not render action buttons for terminal status', () => {
    render(createElement(BatchMonitor, { job: makeJob({ status: 'completed' }) }));
    expect(screen.queryByRole('button', { name: /Pause/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /Cancel/ })).toBeNull();
  });
});
