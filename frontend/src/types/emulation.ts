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
  osdataEmpty?: boolean;
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

export interface EmulationBypassAccount {
  rid: number;
  username: string;
  disabled: boolean;
  lockedOut: boolean;
  hasPassword: boolean;
}

export type EmulationBypassAction = 'clearPassword' | 'enableAndClearPassword';

export interface EmulationBypassApplyRequest {
  sessionId: string;
  partitionIndex: number;
  rid: number;
  action: EmulationBypassAction;
}

export interface EmulationBypassResult {
  sessionId: string;
  partitionIndex: number;
  rid: number;
  username: string;
  passwordCleared: boolean;
  accountEnabled: boolean;
  alreadyPasswordless: boolean;
}

export interface EmulationOsdataCleanupRequest {
  sessionId: string;
  partitionIndex: number;
}

export type EmulationOsdataCleanupState = 'removed' | 'absent' | 'refusedNonEmpty';

export interface EmulationOsdataCleanupResult {
  sessionId: string;
  dataSourceId: string;
  partitionIndex: number;
  state: EmulationOsdataCleanupState;
  editsApplied: number;
}
