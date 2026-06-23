export interface CaseSummary {
  id: string;
  name: string;
  number?: string;
  examiner?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CaseMetrics {
  dataSourceCount: number;
  indexedFileCount: number;
  timelineEventCount: number;
  artifactCount: number;
}

export interface RecentObject {
  id: string;
  title: string;
  detail: string;
  time: string;
  kind: string;
}

export interface RecentCase {
  caseRoot: string;
  name: string;
  openedAt: string;
}
