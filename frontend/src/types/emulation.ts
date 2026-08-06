export type EmulationState =
  | 'descriptorReady'
  | 'running'
  | 'quiescing'
  | 'released'
  | 'failedCleanupPending';

export type EmulationControlMode = 'interactiveOnly';

export interface EmulationOptions {
  network: boolean;
  clipboard: boolean;
  timeSync: boolean;
}

export interface PrepareEmulationRequest {
  dataSourceId: string;
  recoveryIsoPath?: string | null;
  allowDirectBoot?: boolean;
  options?: EmulationOptions;
}

export interface EmulationInstall {
  partitionIndex: number;
  osdataPresent: boolean;
  samPresent: boolean;
  utilmanBypassAvailable: boolean;
}

export interface EmulationPreflight {
  dataSourceId: string;
  installs: EmulationInstall[];
  recommendedBootRoute: 'recoveryMedia' | 'directSystem';
  maintenanceToolAvailable?: boolean;
}

export interface EmulationSessionStatus {
  sessionId: string;
  dataSourceId: string;
  state: EmulationState;
  logicalLength: number;
  controlMode: EmulationControlMode;
  maintenanceMedia?: boolean;
  error?: string;
}
