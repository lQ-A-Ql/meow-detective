import type { ImportPhaseProgress, JobSnapshot } from '@/types/models';

export function importPhaseProgress(phase: ImportPhaseProgress): number | null {
  return phase.phase === 'finalize' && phase.state === 'running' ? null : phase.percent;
}

export function activeJobProgress(job: JobSnapshot, phase?: ImportPhaseProgress): number | null {
  if (job.status === 'running' && phase?.jobId === job.id && importPhaseProgress(phase) === null) {
    return null;
  }
  if (job.status === 'completed') return 100;
  const progress =
    job.status === 'running' && phase?.jobId === job.id ? Math.max(job.progress, phase.percent) : job.progress;
  return Math.max(0, Math.min(progress, 99));
}
