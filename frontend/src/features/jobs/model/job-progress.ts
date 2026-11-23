import type { JobSnapshot } from '@/types/models';

export function activeJobProgress(job: JobSnapshot): number | null {
  return job.status === 'cancelling' || job.progress >= 99 ? null : job.progress;
}
