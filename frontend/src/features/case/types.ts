import type { JobSnapshot } from '@/types/models';

export interface EvidenceHashJobView {
  dataSourceId: string;
  status: Extract<JobSnapshot['status'], 'pending' | 'running'>;
  progress: number;
}
