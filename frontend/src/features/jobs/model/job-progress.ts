import type { ImportPhaseProgress, JobSnapshot } from '@/types/models';

export function importPhaseProgress(phase: ImportPhaseProgress): number | null {
  return phase.phase === 'finalize' && phase.state === 'running' ? null : phase.percent;
}

export function activeJobProgress(job: JobSnapshot, phase?: ImportPhaseProgress): number | null {
  if (job.status === 'running' && phase?.jobId === job.id && importPhaseProgress(phase) === null) {
    return null;
  }
  return Math.min(job.progress, 99);
}
