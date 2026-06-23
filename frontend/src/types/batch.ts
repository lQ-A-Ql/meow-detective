export type BatchPhaseName = 'Mount' | 'Catalog' | 'ExtractArtifacts' | 'Index' | 'Correlate' | 'Export';

export type BatchPhaseState = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

export type BatchJobStatus = 'pending' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';

export interface ResourceLimits {
  memoryMb: number;
  threadCount: number;
}

export interface BatchPlan {
  name: string;
  dataSourceIds: string[];
  phases: BatchPhaseName[];
  resourceLimits: ResourceLimits;
}

export interface BatchPlanSummary {
  name: string;
  dataSourceIds: string[];
  dataSourceCount: number;
  phases: BatchPhaseName[];
  phaseCount: number;
  resourceLimits: ResourceLimits;
}

export interface BatchPhaseProgress {
  phase: BatchPhaseName;
  state: BatchPhaseState;
  progress: number;
  detail: string;
}

export interface BatchJobLogLine {
  ts: string;
  level: 'info' | 'warn' | 'error';
  message: string;
}

export interface BatchJob {
  id: string;
  name: string;
  status: BatchJobStatus;
  progress: number;
  phases: BatchPhaseProgress[];
  plan: BatchPlanSummary;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
  elapsedMs?: number;
  etaMs?: number;
  fileCount: number;
  artifactCount: number;
  logTail: BatchJobLogLine[];
}

export interface BatchStatus {
  activeJobs: number;
  completedJobs: number;
  failedJobs: number;
  queuedJobs: number;
  totalJobs: number;
}
