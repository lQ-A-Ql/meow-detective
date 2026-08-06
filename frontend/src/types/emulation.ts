export type EmulationState =
  | 'descriptorReady'
  | 'running'
  | 'quiescing'
  | 'released'
  | 'failedCleanupPending';

export type EmulationControlMode = 'interactiveOnly';

export interface PrepareEmulationRequest {
  dataSourceId: string;
  recoveryIsoPath?: string | null;
  allowDirectBoot?: boolean;
}

export interface EmulationSessionStatus {
  sessionId: string;
  dataSourceId: string;
  state: EmulationState;
  logicalLength: number;
  controlMode: EmulationControlMode;
  error?: string;
}
