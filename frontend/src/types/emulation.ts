export type EmulationState =
  | 'descriptorReady'
  | 'running'
  | 'quiescing'
  | 'released'
  | 'failedCleanupPending';

export type EmulationControlMode = 'interactiveOnly';

export type EmulationNetworkMode = 'off' | 'hostOnly' | 'nat' | 'bridged';

export interface EmulationOptions {
  networkMode: EmulationNetworkMode;
  clipboard: boolean;
  timeSync: boolean;
  processorCount: number;
  memoryMib: number;
}

export interface PrepareEmulationRequest {
  dataSourceId: string;
  recoveryIsoPath?: string | null;
  allowDirectBoot?: boolean;
  options?: EmulationOptions;
}

export interface EmulationInstall {
  partitionIndex: number;
  platform?: 'windows' | 'linux';
  osdataPresent: boolean;
  samPresent: boolean;
  utilmanBypassAvailable: boolean;
  osdataEmpty?: boolean;
  osReleasePrettyName?: string;
  kernelPresent?: boolean;
  fstabPresent?: boolean;
  bootRiskNotes?: string[];
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
  dataSourceId: string;
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

export interface EmulationLinuxAccount {
  username: string;
  hasPassword: boolean;
  locked: boolean;
}

export interface EmulationLinuxBypassRequest {
  sessionId: string;
  partitionIndex: number;
  username: string;
}

export type EmulationEfiFallbackStrategy = 'shim' | 'grub' | 'systemdBoot';

export interface EmulationEfiFallbackResult {
  sessionId: string;
  dataSourceId: string;
  espPartitionIndex: number;
  strategy?: EmulationEfiFallbackStrategy | null;
  filesWritten: string[];
  alreadyPresent: boolean;
}

export type EmulationFsVolumeState = 'clean' | 'dirty' | 'unsupported';

export interface EmulationFsRepairItem {
  partitionIndex: number;
  initialState: EmulationFsVolumeState;
  state: EmulationFsVolumeState;
  repaired: boolean;
  logBytes: number;
}

export interface EmulationFsRepairResult {
  sessionId: string;
  dataSourceId: string;
  items: EmulationFsRepairItem[];
}

export interface EmulationLinuxBypassResult {
  sessionId: string;
  dataSourceId: string;
  partitionIndex: number;
  username: string;
  passwordSet: boolean;
  alreadyConfigured: boolean;
}
