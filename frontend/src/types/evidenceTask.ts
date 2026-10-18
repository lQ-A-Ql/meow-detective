import type { EvidenceDigest } from './digest';
import type { NestedEvidenceMaterialized } from './dataSource';
import type { CalculateEvidenceDigestRequest } from './digest';
import type { RegistrySearchPage } from './registryBrowser';
export interface ContentIndexSummary {
  indexed: number; binary: number; oversized: number; unreadable: number;
  unsupportedEncoding: number; notApplicable: number; expected: number; complete: boolean;
}
export type EvidenceTaskRequest =
  | { kind: 'carvePng'; request: import('./carving').PngCarvingRequest }
  | { kind: 'carveZip'; request: import('./carving').ZipCarvingRequest }
  | { kind: 'digest'; request: CalculateEvidenceDigestRequest }
  | { kind: 'nestedImport'; request: { parentDataSourceId: string; fileEntryId: string } }
  | { kind: 'contentIndex'; request: { dataSourceId: string } }
  | RegistrySearchTaskRequest;
export type RegistrySearchTaskRequest = { kind: 'registrySearch'; request: { fileId: string; sessionId: string; query: string } };
export type EvidenceTaskResult =
  | { kind: 'carvePng'; result: import('./carving').PngCarvingSummary }
  | { kind: 'carveZip'; result: import('./carving').ZipCarvingSummary }
  | { kind: 'digest'; result: EvidenceDigest }
  | { kind: 'nestedImport'; result: NestedEvidenceMaterialized }
  | { kind: 'contentIndex'; result: ContentIndexSummary }
  | { kind: 'registrySearch'; result: RegistrySearchPage };
export interface EvidenceTask {
  taskId: string; kind: string; state: 'queued' | 'running' | 'cancelling' | 'completed' | 'failed' | 'cancelled';
  sourceRevision: string; bytesProcessed: number; totalBytes: number;
  result?: EvidenceTaskResult; errorCode?: string;
}
