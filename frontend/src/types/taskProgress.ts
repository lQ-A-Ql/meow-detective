export type TaskStageKey =
  'validate' | 'register' | 'prepare' | 'probe' | 'enumerate' | 'analyze' | 'index' | 'publish';
export type TaskProgressState =
  'pending' | 'running' | 'completed' | 'skipped' | 'cancelling' | 'cancelled' | 'failed' | 'partial';

export interface TaskStageProgress {
  key: TaskStageKey;
  weight: number;
  state: TaskProgressState;
}

export interface TaskProgress {
  jobId: string;
  caseId: string;
  kind: 'import' | 'nestedImport' | 'analysis' | 'timeline';
  dataSourceId?: string;
  seq: number;
  state: TaskProgressState;
  progress: number;
  mode: 'determinate' | 'estimated' | 'indeterminate';
  stage: TaskStageKey;
  work?: 'timelineIdentity' | 'timelineWorkspace' | 'timelineEvents' | 'timelineGraphPrepare' | 'timelineGraph';
  stages: TaskStageProgress[];
  completedUnits: number;
  totalUnits?: number;
  unit: 'items' | 'bytes';
  elapsedMs: number;
  stageElapsedMs: number;
  heartbeatAt: string;
  heartbeat: boolean;
  rate?: number;
  etaSeconds?: number;
  cancellable: boolean;
}
