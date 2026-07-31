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
