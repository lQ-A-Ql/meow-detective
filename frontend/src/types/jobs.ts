export interface JobSnapshot {
  id: string;
  name: string;
  scope: string;
  progress: number;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'warning' | 'cancelling' | 'cancelled';
  detail: string;
  warningCount: number;
  skippedCount: number;
  failedCount: number;
  partial: boolean;
  currentPartition?: string;
  completedPartitions?: number;
  totalPartitions?: number;
  partitionProgress?: number;
}
