import type { ImportPhaseProgress, JobSnapshot, TaskProgress } from '@/types/models';
import { jobHasPartialOutcome } from '@/features/jobs/model/task-progress';

export function importPhaseProgress(phase: ImportPhaseProgress): number | null {
  return phase.phase === 'finalize' && phase.state === 'running' ? null : phase.percent;
}

export function activeJobProgress(job: JobSnapshot, phase?: ImportPhaseProgress, task?: TaskProgress): number | null {
  if (job.status === 'completed') return jobHasPartialOutcome(job) ? Math.min(task?.progress ?? job.progress, 99) : 100;
  if (task?.jobId === job.id && (job.status === 'running' || job.status === 'cancelling')) {
    return task.mode === 'indeterminate' ? null : task.progress;
  }
  if (task?.jobId === job.id && job.status !== 'pending') return task.progress;
  if (job.status === 'pending') return 0;
  if (job.status === 'running' && phase?.jobId === job.id && importPhaseProgress(phase) === null) {
    return null;
  }
  const progress =
    job.status === 'running' && phase?.jobId === job.id ? Math.max(job.progress, phase.percent) : job.progress;
  return Math.max(0, Math.min(progress, 99));
}
