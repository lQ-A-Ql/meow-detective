export interface TimelineEvent {
  id: string;
  dataSourceId?: string;
  sourceObjectId: string;
  eventType: string;
  ts: string;
  title: string;
  description: string;
  parserId?: string;
  parserVersion?: string;
  confidence?: number;
  sourceAttribution?: string;
  attrs: Record<string, unknown>;
}

export interface TimelineFacetsRequest {
  timeStart?: string;
  timeEnd?: string;
  eventType?: string;
  bucketCount?: number;
}

export interface TimelineFacetCount {
  value: string;
  count: number;
}

export interface TimelineHistogramBucket {
  startTs: string;
  endTs: string;
  count: number;
}

export interface TimelineFacets {
  totalEvents: number;
  startTs?: string;
  endTs?: string;
  eventTypes: TimelineFacetCount[];
  dataSources: TimelineFacetCount[];
  histogram: TimelineHistogramBucket[];
}

/** Mirrors `TimelineProjectionStateDto` (camelCase) from the transport contract. */
export type TimelineProjectionState = 'complete' | 'building' | 'deferred';

/** Mirrors `TimelineProjectionSourceStatusDto` (camelCase; optional fields skip when none). */
export interface TimelineProjectionSourceStatus {
  dataSourceId: string;
  state: TimelineProjectionState;
  identity?: string;
  insertedEvents: number;
  requiredBytes?: number;
  availableBytes?: number;
  reason?: string;
  updatedAt?: string;
}

/** Mirrors `TimelineProjectionStatusDto` (camelCase). Worst-of-source aggregate state. */
export interface TimelineProjectionStatus {
  state: TimelineProjectionState;
  sources: TimelineProjectionSourceStatus[];
}
