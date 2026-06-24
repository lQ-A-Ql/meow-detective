export type BatchPhaseName = 'Mount' | 'Catalog' | 'ExtractArtifacts' | 'Index' | 'Correlate' | 'Export';

export type BatchPhaseState = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

export type BatchJobStatus = 'pending' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';

export interface BatchResourceLimits {
  maxMemoryMb?: number;
  maxThreads?: number;
}

export interface BatchPlan {
  dataSourceRefs: string[];
  phases: BatchPhaseName[];
  resourceLimits: BatchResourceLimits;
}

export interface BatchPlanInput {
  name: string;
  dataSourceIds: string[];
  phases: BatchPhaseName[];
  resourceLimits: BatchResourceLimits;
}

export interface BatchPhaseProgress {
  kind: BatchPhaseName;
  state: BatchPhaseState;
  progress: number;
  startedAt?: string;
  completedAt?: string;
  errorCount: number;
  warnings: string[];
}

export interface BatchJob {
  id: string;
  caseId: string;
  label: string;
  status: BatchJobStatus;
  phases: BatchPhaseProgress[];
  plan: BatchPlan;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
}

export interface BatchStatus {
  activeJobs: number;
  completedJobs: number;
  failedJobs: number;
  queuedJobs: number;
  totalJobs: number;
}
