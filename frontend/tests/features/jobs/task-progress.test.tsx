import { render, screen, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { createImportEventStateStore } from '@/features/jobs/import-event-store';
import { activeJobProgress } from '@/features/jobs/model/job-progress';
import { taskHeartbeatState } from '@/features/jobs/model/task-progress';
import { mergeTaskJobs } from '@/features/jobs/model/task-progress';
import { taskForJob } from '@/features/jobs/model/task-progress';
import { taskDuration } from '@/features/jobs/model/task-progress';
import i18n from '@/i18n';
import { JobStatusCard } from '@/features/jobs/components/JobStatusCard';
import type { TaskProgress, JobSnapshot } from '@/types/models';

afterEach(cleanup);

const task: TaskProgress = {
  jobId: 'job',
  caseId: 'case',
  kind: 'import',
  seq: 2,
  state: 'running',
  progress: 26,
  mode: 'estimated',
  stage: 'enumerate',
  stages: [
    { key: 'prepare', weight: 8, state: 'completed' },
    { key: 'enumerate', weight: 37, state: 'running' },
    { key: 'analyze', weight: 35, state: 'pending' },
    { key: 'index', weight: 15, state: 'pending' },
    { key: 'publish', weight: 5, state: 'pending' },
  ],
  completedUnits: 50,
  totalUnits: 100,
  unit: 'items',
  elapsedMs: 10000,
  stageElapsedMs: 6000,
  heartbeatAt: '2026-11-23T10:00:00Z',
  heartbeat: true,
  rate: 5,
  etaSeconds: 10,
  cancellable: true,
};

describe('weighted progress and independent heartbeat', () => {
  it('shows actual background capability tasks without duplicating registered import jobs', () => {
    const received = { ...task, kind: 'analysis' as const };
    const jobs = mergeTaskJobs([], { job: received }, [], i18n.t);
    expect(jobs[0].name).toBe('制品与痕迹提取');
    expect(jobs[0].status).toBe('running');
    expect(mergeTaskJobs(jobs, { job: received }, [], i18n.t)).toHaveLength(1);
    const partial = mergeTaskJobs([], { job: { ...received, state: 'partial' } }, [], i18n.t);
    expect(partial[0].status).toBe('warning');
    expect(partial[0].partial).toBe(true);
  });
  it('uses sequenced task snapshots and rejects older or foreign task progress', () => {
    const store = createImportEventStateStore('case');
    store.ingestTask(task);
    store.ingestTask({ ...task, seq: 1, progress: 80 });
    store.ingestTask({ ...task, caseId: 'other', seq: 3, progress: 90 });
    expect(store.getSnapshot().tasksByJob.job.progress).toBe(26);
    store.ingestTask({ ...task, seq: 3, progress: 30 });
    expect(store.getSnapshot().tasksByJob.job.progress).toBe(30);
  });

  it('uses the backend weighted progress ahead of the numeric import snapshot', () => {
    const job: JobSnapshot = {
      id: 'job',
      name: 'Image import',
      scope: '',
      progress: 70,
      status: 'running',
      detail: '',
      warningCount: 0,
      skippedCount: 0,
      failedCount: 0,
      partial: false,
    };
    expect(activeJobProgress(job, undefined, task)).toBe(26);
    render(<JobStatusCard job={job} tone="running" task={{ ...task, heartbeatAt: new Date().toISOString() }} />);
    expect(screen.getByRole('progressbar', { name: job.name })).toHaveAttribute('aria-valuenow', '26');
    expect(screen.getByText('当前步骤预计剩余约 10秒钟')).toBeInTheDocument();
    expect(screen.getByText('50 / 100')).toBeInTheDocument();
    expect(screen.getByRole('list', { name: '任务阶段' }).children).toHaveLength(5);
  });

  it('distinguishes heartbeat silence from work counters and suppresses waiting after completion', () => {
    const received = Date.parse(task.heartbeatAt);
    expect(taskHeartbeatState(task, received + 9000)).toBe('current');
    expect(taskHeartbeatState(task, received + 10000)).toBe('waiting');
    expect(taskHeartbeatState(task, received + 30000)).toBe('unresponsive');
    expect(taskHeartbeatState({ ...task, state: 'completed' }, received + 90000)).toBe('current');
  });

  it('formats long running times using localized duration units', () => {
    expect(taskDuration(600, 'zh-CN')).toBe('10分钟');
    expect(taskDuration(7200, 'en')).toBe('2 hours');
  });

  it('uses the terminal job snapshot when a final progress event is missing', () => {
    const job: JobSnapshot = {
      id: task.jobId,
      name: 'Image import',
      scope: '',
      progress: 100,
      status: 'completed',
      detail: '',
      warningCount: 0,
      skippedCount: 0,
      failedCount: 0,
      partial: false,
    };
    const finished = taskForJob(job, task)!;
    expect(finished.state).toBe('completed');
    expect(finished.etaSeconds).toBeUndefined();
    expect(taskHeartbeatState(finished, Date.parse(task.heartbeatAt) + 90000)).toBe('current');
    expect(finished.stages.every((stage) => stage.state === 'completed')).toBe(true);
    expect(taskForJob({ ...job, status: 'failed' }, task)?.state).toBe('failed');
    const cancelled = { ...job, status: 'cancelled' as const, progress: 70 };
    expect(activeJobProgress(cancelled, undefined, taskForJob(cancelled, task))).toBe(26);
    const queued = { ...job, status: 'pending' as const, progress: 0 };
    expect(mergeTaskJobs([queued], { job: task }, [], i18n.t)[0].status).toBe('running');
  });
});
