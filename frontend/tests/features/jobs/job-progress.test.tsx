import { render, screen, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import i18n from '@/i18n';
import { createImportEventStateStore } from '@/features/jobs/import-event-store';
import { buildBottomDrawerModel } from '@/features/jobs/model/bottom-drawer-model';
import { activeJobProgress } from '@/features/jobs/model/job-progress';
import { JobStatusCard } from '@/features/jobs/components/JobStatusCard';
import { importProgressMetrics } from '@/features/jobs/model/import-progress-metrics';
import { Progress } from '@/app/components/ui/progress';
import type { ImportPhaseProgress, JobSnapshot } from '@/types/models';

afterEach(cleanup);

const job: JobSnapshot = {
  id: 'import-1',
  name: 'Import image',
  scope: 'Image',
  progress: 30,
  status: 'running',
  detail: '',
  warningCount: 0,
  skippedCount: 0,
  failedCount: 0,
  partial: false,
};
const phase: ImportPhaseProgress = {
  jobId: job.id,
  caseId: 'case-1',
  phase: 'enumerate',
  state: 'running',
  percent: 40,
  detail: 'phase=enumeration rows=100',
  metrics: {
    rowsProcessed: 100,
    rowsTotal: 200,
    rowsPerSec: 10,
    rssMb: 32,
    workers: 1,
    bytesProcessed: 0,
    warnings: 0,
    skipped: 0,
    failed: 0,
  },
  partialResults: [],
  cancellable: true,
  cancelRequested: false,
};

describe('task progress ownership', () => {
  it('keeps each job phase while other jobs emit progress', () => {
    const store = createImportEventStateStore('case-1');
    store.ingestPhase('2026-11-23T10:00:00Z', { ...phase, phase: 'finalize', percent: 99 });
    store.ingestPhase('2026-11-23T10:00:01Z', { ...phase, jobId: 'import-2' });
    const model = buildBottomDrawerModel({
      drawerOpen: false,
      toggleDrawer() {},
      jobs: [job],
      importSignals: store.getSnapshot(),
      t: i18n.t,
    });
    expect(model.activePhase?.phase).toBe('finalize');
    expect(activeJobProgress(job, model.activePhase)).toBeNull();
    expect(model.headline).not.toContain('rows=');
  });

  it('ignores older events and keeps progress monotonic across stages', () => {
    const store = createImportEventStateStore('case-1');
    store.ingestPhase('2026-11-23T10:00:02Z', phase);
    store.ingestPhase('2026-11-23T10:00:01Z', { ...phase, phase: 'attach', percent: 10 });
    expect(store.getSnapshot().latestPhase?.phase).toBe('enumerate');
    store.ingestPhase('2026-11-23T10:00:03Z', { ...phase, phase: 'analyze', percent: 20 });
    expect(store.getSnapshot().latestPhase?.percent).toBe(40);
  });

  it('clears the previous case and rejects its delayed progress and cancellation', () => {
    const store = createImportEventStateStore('case-1');
    store.ingestPhase('2026-11-23T10:00:00Z', phase);
    store.setCase('case-2');
    store.ingestPhase('2026-11-23T10:00:01Z', phase);
    store.ingestCancellation('2026-11-23T10:00:02Z', {
      jobId: job.id,
      state: 'cancelled',
      safeToClose: true,
      detail: '',
    });
    expect(store.getSnapshot().latestPhase).toBeUndefined();
    expect(store.getSnapshot().latestCancellation).toBeUndefined();
    expect(store.getSnapshot().phasesByJob).toEqual({});
    store.ingestPartial('2026-11-23T10:00:03Z', {
      scopeId: 'old-source',
      queryKey: 'search:index:old-source',
      kind: 'searchIndex',
      readyCount: 10,
      freshness: 'ready',
    });
    expect(store.getSnapshot().partialResults).toEqual([]);
    store.setCase(undefined);
    expect(store.getSnapshot().caseId).toBeUndefined();
  });

  it('uses live progress ahead of the snapshot and reserves 100 for completion', () => {
    expect(activeJobProgress(job, phase)).toBe(40);
    expect(activeJobProgress({ ...job, progress: 100 })).toBe(99);
    expect(activeJobProgress({ ...job, status: 'completed' }, phase)).toBe(100);
    render(<JobStatusCard tone="running" job={job} phase={phase} />);
    expect(screen.getByRole('progressbar', { name: job.name })).toHaveAttribute('aria-valuenow', '40');
  });

  it('renders stage metrics and keeps diagnostic counters behind disclosure', () => {
    render(
      <JobStatusCard tone="running" job={job} phase={{ ...phase, metrics: { ...phase.metrics, elapsedMs: 5000 } }} />,
    );
    const progress = screen.getByRole('progressbar', { name: job.name });
    expect(progress.getAttribute('aria-valuetext')).toContain('40%');
    expect(screen.getByRole('status').textContent).not.toContain('rows=');
    expect(screen.getByText('100 / 200')).toBeInTheDocument();
    expect(screen.getByText('10 项/秒')).toBeInTheDocument();
    expect(screen.getByText('5 秒')).toBeInTheDocument();
    const details = screen.getByText(phase.detail).closest('details');
    expect(details).not.toHaveAttribute('open');
    expect(progress.closest('[aria-busy]')).toHaveAttribute('aria-busy', 'true');
  });

  it('distinguishes zero totals from unknown totals and omits unavailable rates', () => {
    const empty = { ...phase.metrics, rowsProcessed: 0, rowsTotal: 0, rowsPerSec: undefined };
    expect(importProgressMetrics(empty, i18n.t, 'zh-CN')).toEqual([{ label: '已处理条目', value: '0 / 0' }]);
    expect(importProgressMetrics({ ...empty, rowsTotal: undefined }, i18n.t, 'zh-CN')).toEqual([]);
  });

  it('uses Radix semantics for a custom maximum and for unknown progress', () => {
    const { rerender } = render(<Progress aria-label="Entries" value={2} max={4} />);
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuemax', '4');
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuetext', '50%');
    rerender(<Progress aria-label="Entries" value={null} />);
    expect(screen.getByRole('progressbar')).toHaveAttribute('data-state', 'indeterminate');
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
  });

  it('keeps queued tasks visible with a static progress indicator', () => {
    const pending = { ...job, status: 'pending' as const, progress: 0 };
    const model = buildBottomDrawerModel({
      drawerOpen: false,
      toggleDrawer() {},
      jobs: [pending],
      importSignals: createImportEventStateStore('case-1').getSnapshot(),
      t: i18n.t,
    });
    expect(model.activeJob?.id).toBe(pending.id);
    expect(model.pendingJobs).toEqual([pending]);
    render(<JobStatusCard tone="pending" job={pending} />);
    expect(screen.getByRole('status')).toHaveTextContent('排队中');
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');
    expect(screen.getByRole('progressbar')).toHaveAttribute('data-state', 'loading');
  });
});
