import type { AnalysisParseStatus } from './analysis';

export interface RegistryExtractionSummary {
  status: AnalysisParseStatus;
  total: number;
  values: RegistryValue[];
  generatedAt: string;
  warnings: string[];
}

export interface RegistryValue {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  hivePath: string;
  keyPath: string;
  valueName: string;
  valueType: string;
  data: string;
  parser: string;
  createdAt: string;
}

// SAM User Account (structured view)
export interface SamUserAccount {
  username: string;
  rid: number;
  ridHex: string;
  sid: string;
  groups: string[];
  loginCount: number;
  lastLogin?: string;
  accountCreated?: string;
  accountStatus: 'enabled' | 'disabled' | 'locked';
  profilePath?: string;
  passwordHash?: string;
  passwordHashType?: string; // 'NTLM' | 'LM' | 'Both'
  passwordHint?: string;
  dataSourceId: string;
  hivePath: string;
  keyPath: string;
  parser: string;
}

// Registry Hive Overview
export interface RegistryHiveOverview {
  hiveName: string;
  status: AnalysisParseStatus;
  keyValueCount: number;
  extractedAt: string;
  dataSourceId: string;
  sourcePath: string;
  txlogMerged: boolean;
  deletedKeysFound: number;
}

// UserAssist Entry (structured view)
export interface UserAssistEntry {
  programPath: string;
  execCount: number;
  lastExecTime?: string;
  isSuspicious?: boolean;
  suspiciousReason?: string;
}

// Network Profile from SOFTWARE\NetworkList (structured view)
export interface NetworkProfileEntry {
  profileGuid: string;
  profileName: string;
  description?: string;
  dateCreated?: string;
  dateLastConnected?: string;
  nameType?: number;
  managed: boolean;
  firstNetwork?: string;
  defaultGatewayMacHex?: string;
  dnsSuffix?: string;
  sourceKeyPath: string;
}

// Installed Software (structured view)
export interface InstalledSoftware {
  displayName: string;
  version: string;
  publisher?: string;
  installDate?: string;
  estimatedSize?: string;
  isSuspicious?: boolean;
}

// USB Device History (structured view)
export interface UsbDeviceHistory {
  deviceName: string;
  serialNumber: string;
  firstConnect?: string;
  lastConnect?: string;
  volumeLabel?: string;
  driveLetter?: string;
  fileSystem?: string;
  capacity?: string;
  isSuspicious?: boolean;
  suspiciousReason?: string;
}

// Mounted Device (structured view)
export interface MountedDevice {
  deviceName: string;
  driveLetter?: string;
  volumeGuid?: string;
  diskSignatureHex?: string;
  targetName?: string;
}

// SYSTEM Service / Driver (structured view)
export interface SystemService {
  serviceName: string;
  displayName?: string;
  imagePath?: string;
  serviceDll?: string;
  serviceType: string;
  startType: string;
  delayedAutoStart: boolean;
  errorControl?: number;
  group?: string;
  objectName?: string;
  dependOnService: string[];
  dependOnGroup: string[];
  failureCommand?: string;
  requiredPrivileges: string[];
  keyPath: string;
  keyLastWrite?: string;
}

// Shutdown Time entry (structured view)
export interface ShutdownTime {
  keyPath: string;
  shutdownTime: string;
}

// ShimCache / AppCompatCache entry (structured view)
export interface ShimCacheEntry {
  path: string;
  lastModified?: string;
  sourceKeyPath: string;
}

// OpenSavePidlMRU entry (structured view)
export interface OpenSaveMruEntry {
  extension: string;
  valueName: string;
  fileName: string;
  rawPidlHex: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

// LastVisitedPidlMRU entry (structured view)
export interface LastVisitedMruEntry {
  valueName: string;
  path: string;
  rawPidlHex: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

// RunMRU entry (Win+R run history, structured view)
export interface RunMruEntry {
  valueName: string;
  command: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

// Shellbag entry from UsrClass.dat (structured view)
export interface ShellbagEntry {
  path: string;
  rawPidlHex: string;
  nodeSlot?: number;
  sourceKeyPath: string;
  lastWrite?: string;
}

// MuiCache entry from UsrClass.dat (structured view)
export interface MuiCacheEntry {
  programPath: string;
  friendlyName: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

// Amcache application entry from Amcache.hve (structured view)
export interface AmcacheApplicationEntry {
  programId?: string;
  name?: string;
  version?: string;
  publisher?: string;
  installDate?: string;
  source?: string;
  osVersionAtInstallTime?: string;
  registryKeyPath: string;
}

// Amcache application-file entry from Amcache.hve (structured view)
export interface AmcacheApplicationFileEntry {
  programId?: string;
  lowerCaseLongPath?: string;
  longPathHash?: string;
  fileSize?: number;
  productName?: string;
  companyName?: string;
  fileVersion?: string;
  isPeFile?: boolean;
  linkDate?: string;
  registryKeyPath: string;
}

// AppCompatFlags Layers entry from SOFTWARE or NTUSER.DAT (structured view)
export interface AppCompatLayerEntry {
  executablePath: string;
  layerString: string;
  sourceHivePath: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

// Registry Structured Summary (aggregates all structured views)
export interface RegistryStructuredSummary {
  hiveOverviews: RegistryHiveOverview[];
  samUsers: SamUserAccount[];
  userAssistEntries: UserAssistEntry[];
  networkProfiles: NetworkProfileEntry[];
  installedSoftware: InstalledSoftware[];
  usbDevices: UsbDeviceHistory[];
  mountedDevices: MountedDevice[];
  systemServices: SystemService[];
  shutdownTimes: ShutdownTime[];
  shimcacheEntries: ShimCacheEntry[];
  runKeys: RegistryRunKey[];
  openSaveMru: OpenSaveMruEntry[];
  lastVisitedMru: LastVisitedMruEntry[];
  runMru: RunMruEntry[];
  shellbagEntries: ShellbagEntry[];
  muicacheEntries: MuiCacheEntry[];
  amcacheApplications: AmcacheApplicationEntry[];
  amcacheApplicationFiles: AmcacheApplicationFileEntry[];
  winlogonConfig?: WinlogonConfig;
  lsaPackages: LsaPackage[];
  appCompatLayers: AppCompatLayerEntry[];
  securityPolicies: SecurityPolicyEntry[];
  lsaSecrets: LsaSecretEntry[];
  cachedCredentials: CachedCredentialEntry[];
  status: AnalysisParseStatus;
  generatedAt: string;
  warnings: string[];
}

export interface SecurityPolicyEntry {
  domainName?: string;
  accountDomainName?: string;
  machineSid?: string;
  auditPolicyHex?: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

export interface LsaSecretEntry {
  secretName: string;
  version: string;
  encryptedBlobHex: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

export interface CachedCredentialEntry {
  entryName: string;
  encryptedBlobHex: string;
  sourceKeyPath: string;
  lastWrite?: string;
}

export interface RegistryRunKey {
  keyPath: string;
  valueName: string;
  command: string;
  timestamp?: string;
  scope: string;
}

export interface WinlogonConfig {
  shell?: string;
  userinit?: string;
  notify?: string;
  autoAdminLogon?: string;
  defaultDomainName?: string;
  defaultUserName?: string;
  keyPath: string;
}

export interface LsaPackage {
  controlSet: string;
  authenticationPackages: string[];
  notificationPackages: string[];
  securityPackages: string[];
}

export interface RecentDoc {
  fileName: string;
  extension: string;
  lastAccessed?: string;
  lnkTarget?: string;
}

export interface MountPoint {
  driveLetter?: string;
  volumeGuid?: string;
  lastMounted?: string;
}

export interface NtuserInfo {
  runKeys: RegistryRunKey[];
  recentDocs: RecentDoc[];
  userAssist: UserAssistEntry[];
  typedUrls: string[];
  wordWheelQuery: string[];
  mountPoints: MountPoint[];
  warnings: string[];
}
