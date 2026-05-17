import {
  ArtifactRow,
  CaseMetrics,
  CaseSummary,
  FileEntryRow,
  FileTreeNode,
  JobSnapshot,
  RecentObject,
  ReportHistoryItem,
  ReportTemplate,
  SearchResultPage,
  TimelineEventDto,
  TraceItem,
  ViewerHandle,
  ViewerRangeRequest,
  ViewerRangeResponse,
  WarningItem,
} from '@/types/models';
import {
  artifactFamilies,
  artifactRows,
  caseMetrics,
  currentCase,
  fileRows,
  filesTree,
  jobs,
  recentObjects,
  reportHistory,
  reportTemplates,
  searchHits,
  timelineEvents,
  traces,
  warnings,
} from './mock-data';

export interface ApiProvider {
  getCurrentCase(): Promise<CaseSummary>;
  getCaseMetrics(): Promise<CaseMetrics>;
  getRecentObjects(): Promise<RecentObject[]>;
  getFileTree(): Promise<FileTreeNode[]>;
  getFileRows(): Promise<FileEntryRow[]>;
  openFileHandle(fileId: string): Promise<ViewerHandle>;
  readFileRange(request: ViewerRangeRequest): Promise<ViewerRangeResponse>;
  searchFiles(query: string): Promise<SearchResultPage>;
  getTimelineEvents(): Promise<TimelineEventDto[]>;
  getArtifactFamilies(): Promise<string[]>;
  getArtifactRows(family?: string): Promise<ArtifactRow[]>;
  getReportTemplates(): Promise<ReportTemplate[]>;
  getReportHistory(): Promise<ReportHistoryItem[]>;
  getJobsSnapshot(): Promise<JobSnapshot[]>;
  getWarnings(): Promise<WarningItem[]>;
  getTraceItems(): Promise<TraceItem[]>;
}

export const mockProvider: ApiProvider = {
  async getCurrentCase() {
    return currentCase;
  },
  async getCaseMetrics() {
    return caseMetrics;
  },
  async getRecentObjects() {
    return recentObjects;
  },
  async getFileTree() {
    return filesTree;
  },
  async getFileRows() {
    return fileRows;
  },
  async openFileHandle(fileId: string) {
    const file = fileRows.find((item) => item.id === fileId);
    return {
      handleId: `handle-${fileId}`,
      size: file?.size ?? 289000,
      mime: 'application/x-msdownload',
    };
  },
  async readFileRange(_request: ViewerRangeRequest) {
    return {
      kind: 'hex',
      lines: [
        '4D 5A 90 00 03 00 00 00 04 00 00 00 FF FF 00 00',
        'B8 00 00 00 00 00 00 00 40 00 00 00 00 00 00 00',
        '0E 1F BA 0E 00 B4 09 CD 21 B8 01 4C CD 21 54 68',
      ],
    };
  },
  async searchFiles(_query: string) {
    return {
      total: searchHits.length,
      tookMs: 45,
      items: searchHits,
    };
  },
  async getTimelineEvents() {
    return timelineEvents;
  },
  async getArtifactFamilies() {
    return artifactFamilies;
  },
  async getArtifactRows(family?: string) {
    return artifactRows.filter((item) => !family || item.artifactType === family);
  },
  async getReportTemplates() {
    return reportTemplates;
  },
  async getReportHistory() {
    return reportHistory;
  },
  async getJobsSnapshot() {
    return jobs;
  },
  async getWarnings() {
    return warnings;
  },
  async getTraceItems() {
    return traces;
  },
};
