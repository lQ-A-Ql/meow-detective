import type { RegistryRecovery } from './registryRecovery';
export * from './registryRecovery';
/** Source-bound native registry browsing contract. */
export type RegistryHiveView = 'base' | 'recovered';
export type RegistryHiveReadState = 'candidate' | 'readable' | 'locked' | 'unsupported' | 'corrupt';
export type RegistryItemReadState = 'readable' | 'deferred' | 'unsupported' | 'corrupt';
export interface RegistryBrowserSource {
  caseId: string;
  dataSourceId: string;
  fileId: string;
  sourceRevision: string;
  view: RegistryHiveView;
}
export interface RegistryBrowserCoverage {
  declaredCount: number;
  returnedCount: number;
  skippedCount: number;
  status: 'complete' | 'partial';
}
export interface RegistryBrowserDiagnostic {
  code: string;
  recoverable: boolean;
  itemId?: string;
}
export interface RegistryHiveDescriptor {
  fileId: string;
  dataSourceId: string;
  name: string;
  evidencePath: string;
  byteLength: number;
  readState: RegistryHiveReadState;
  partitionIndex?: number;
  profileLabel?: string;
}
export interface RegistryHiveListRequest {
  dataSourceId: string;
  limit: number;
  cursor?: string;
}
export interface RegistryHivePage {
  sourceRevision: string;
  hives: RegistryHiveDescriptor[];
  coverage: RegistryBrowserCoverage;
  diagnostics: RegistryBrowserDiagnostic[];
  nextCursor?: string;
}
export interface RegistryKeyMetadata {
  keyId: string;
  path: string;
  name: string;
  cellOffset: number;
  subkeyCount: number;
  valueCount: number;
  readState: RegistryItemReadState;
  lastWriteFiletime?: string;
}
export interface RegistryHiveOpenRequest {
  fileId: string;
  view: RegistryHiveView;
}
export interface RegistryHiveSession {
  sessionId: string;
  source: RegistryBrowserSource;
  hive: RegistryHiveDescriptor;
  rootKey: RegistryKeyMetadata;
  expiresAt: string;
  diagnostics: RegistryBrowserDiagnostic[];
  recovery?: RegistryRecovery;
}
export interface RegistryHiveCloseRequest {
  sessionId: string;
}
export interface RegistryKeyResolveRequest {
  sessionId: string;
  keyPath: string;
}
export interface RegistrySubkeysRequest {
  sessionId: string;
  keyId: string;
  limit: number;
  cursor?: string;
}
export interface RegistrySubkeyPage {
  keys: RegistryKeyMetadata[];
  coverage: RegistryBrowserCoverage;
  diagnostics: RegistryBrowserDiagnostic[];
  nextCursor?: string;
}
export interface RegistryValueMetadata {
  valueId: string;
  name: string;
  typeCode: number;
  typeName: string;
  byteLength: number;
  preview: string;
  cellOffset: number;
  readState: RegistryItemReadState;
}
export interface RegistryValuesRequest {
  sessionId: string;
  keyId: string;
  limit: number;
  cursor?: string;
}
export interface RegistryValuePage {
  values: RegistryValueMetadata[];
  coverage: RegistryBrowserCoverage;
  diagnostics: RegistryBrowserDiagnostic[];
  nextCursor?: string;
}
export interface RegistryValueRangeRequest {
  sessionId: string;
  keyId: string;
  valueId: string;
  offset: number;
  length: number;
}
export interface RegistryValueRange {
  valueId: string;
  offset: number;
  totalLength: number;
  bytes: number[];
  eof: boolean;
  decodeStatus: 'decoded' | 'rawOnly' | 'partial';
  diagnostics: RegistryBrowserDiagnostic[];
  decodedText?: string;
}

export interface RegistrySearchHit { kind: string; path: string; name: string; cellOffset: number; matchedField: string; preview: string; }
export interface RegistrySearchPage { hits: RegistrySearchHit[]; scannedKeys: number; scannedValues: number; skippedCount: number; truncated: boolean; }
