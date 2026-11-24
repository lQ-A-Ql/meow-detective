import type { DataSourceSummary, JobSnapshot, TaskProgress, TaskStageProgress } from '@/types/models';
import type { TFunction } from 'i18next';

export function taskDuration(seconds: number, language: string) {
  const unit = seconds >= 3600 ? 'hour' : seconds >= 60 ? 'minute' : 'second';
  const divisor = unit === 'hour' ? 3600 : unit === 'minute' ? 60 : 1;
  return new Intl.NumberFormat(language, { style: 'unit', unit, unitDisplay: 'long', maximumFractionDigits: 1 }).format(
    seconds / divisor,
  );
}

export function taskStageLabel(task: TaskProgress, stage: TaskStageProgress['key'], t: TFunction) {
  return task.kind === 'timeline' && stage === 'index'
    ? t('taskProgress.timelineWork')
    : t(`taskProgress.stage.${stage}`);
}

export function taskForJob(job: JobSnapshot, task?: TaskProgress): TaskProgress | undefined {
  if (!task || task.jobId !== job.id) return undefined;
  if (job.status === 'pending' || job.status === 'running' || job.status === 'cancelling') return task;
  const state = job.status === 'warning' ? 'partial' : job.status;
  return {
    ...task,
    state,
    progress: state === 'completed' ? 100 : Math.min(task.progress, 99),
    mode: 'determinate',
    cancellable: false,
    etaSeconds: undefined,
    rate: undefined,
    stages: task.stages.map((stage) => ({
      ...stage,
      state:
        state === 'completed' && stage.state !== 'skipped'
          ? 'completed'
          : stage.key === task.stage
            ? state
            : stage.state,
    })),
  };
}

export function mergeTaskJobs(
  jobs: JobSnapshot[],
  tasks: Record<string, TaskProgress>,
  sources: DataSourceSummary[],
  t: TFunction,
) {
  const ids = new Set(jobs.map((job) => job.id));
  const received = Object.values(tasks)
    .filter((task) => !ids.has(task.jobId))
    .map((task): JobSnapshot => ({
      id: task.jobId,
      name: t(`taskProgress.kind.${task.kind}`),
      scope: sources.find((source) => source.id === task.dataSourceId)?.name ?? '',
      progress: task.progress,
      status: task.state === 'partial' ? 'warning' : task.state === 'skipped' ? 'completed' : task.state,
      detail: t(`taskProgress.stage.${task.stage}`),
      warningCount: 0,
      skippedCount: 0,
      failedCount: 0,
      partial: task.state === 'partial',
    }));
  const observed = jobs.map((job): JobSnapshot => {
    const task = tasks[job.id];
    if (job.status !== 'pending' || !task || task.state === 'pending') return job;
    return {
      ...job,
      progress: task.progress,
      status: task.state === 'partial' ? 'warning' : task.state === 'skipped' ? 'completed' : task.state,
    };
  });
  return [...observed, ...received];
}

export function taskHeartbeatState(task: TaskProgress, now: number) {
  if (task.state !== 'running' && task.state !== 'cancelling') return 'current';
  const age = Math.max(0, now - Date.parse(task.heartbeatAt));
  if (age >= 30_000) return 'unresponsive';
  return age >= 10_000 ? 'waiting' : 'current';
}

export function taskStageProgress(task: TaskProgress, stage: TaskStageProgress): number | null {
  if (stage.state === 'completed' || stage.state === 'skipped') return 100;
  if (stage.key !== task.stage) return 0;
  if (task.mode === 'indeterminate' && task.state === 'running') return null;
  if (task.totalUnits === undefined || task.totalUnits <= 0) {
    return task.state === 'running' ? null : 0;
  }
  return Math.min(100, Math.round((task.completedUnits * 100) / task.totalUnits));
}
