export interface AppUpdateRelease {
  version: string;
  notes: string;
  schemaCompatible: boolean;
}
export interface AppUpdateInfo {
  currentVersion: string;
  configured: boolean;
  portable: boolean;
  available?: AppUpdateRelease;
}
export interface AppUpdateProgress {
  phase: string;
  downloadedBytes: number;
  totalBytes?: number;
}
export interface PortablePackage {
  format: number;
  version: string;
  caseSchema: string;
}
