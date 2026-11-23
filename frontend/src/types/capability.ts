/** Mirrors `CapabilityKindDto` (camelCase) from the transport contract. */
export type CapabilityKind = 'catalog' | 'search' | 'timeline' | 'graph' | 'platformFacts' | 'artifacts';

/** Mirrors `CapabilityStateDto` (camelCase) from the transport contract. */
export type CapabilityState =
  | 'pending'
  | 'queued'
  | 'running'
  | 'ready'
  | 'partial'
  | 'deferred'
  | 'failed'
  | 'stale'
  | 'cancelled'
  | 'notApplicable';

/** Mirrors `CapabilityProgressDto` (camelCase; optional fields skip when none). */
export interface CapabilityProgress {
  done: number;
  total?: number;
  phase?: string;
}

/** Mirrors `SourceCapabilityDto` (camelCase; optional fields skip when none). */
export interface SourceCapability {
  dataSourceId: string;
  capability: CapabilityKind;
  state: CapabilityState;
  revision: number;
  progress?: CapabilityProgress;
  detail?: string;
  errorCode?: string;
  /** True = derived from markers at read time, not task-confirmed. */
  derived: boolean;
  updatedAt: string;
}

/** Mirrors `CapabilityMatrixDto` (camelCase). */
export interface CapabilityMatrix {
  sources: SourceCapability[];
}

export interface CapabilityStatusChangedPayload {
  caseId?: string;
  dataSourceId?: string;
  capability?: CapabilityKind;
  state?: CapabilityState;
  revision?: number;
}
