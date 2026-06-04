import {
  ArtifactRow,
  AnalysisFileClassification,
  AnalysisSystemInfo,
  CaseMetrics,
  DataSourceSummary,
  CaseSummary,
  FileEntryRow,
  EvidenceClassificationSummary,
  FileTreeNode,
  JobSnapshot,
  RecentCase,
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
  analysisClassifications,
  evidenceClassificationSummary,
  analysisSummary,
  analysisSystemInfo,
  caseMetrics,
  currentCase,
  dataSources,
  fileRows,
  filesTree,
  jobs,
  recentCases,
  recentObjects,
  reportHistory,
  reportTemplates,
  searchHits,
  timelineEvents,
  traces,
  warnings,
} from './mock-data';

export interface ApiProvider {
  getCurrentCase(): Promise<CaseSummary | null>;
  getCaseMetrics(): Promise<CaseMetrics>;
  getRecentObjects(): Promise<RecentObject[]>;
  getRecentCases(): Promise<RecentCase[]>;
  getDataSources(): Promise<DataSourceSummary[]>;
  createCase(caseRoot: string, name: string, examiner?: string): Promise<CaseSummary>;
  openCase(caseRoot: string): Promise<CaseSummary>;
  closeCase(): Promise<void>;
  renameDataSource(dataSourceId: string, name: string): Promise<void>;
  importDataSource(sourcePath: string): Promise<string>;
  getFileTree(): Promise<FileTreeNode[]>;
  getFileChildren(parentId: string): Promise<FileTreeNode[]>;
  getFileRows(parentId?: string): Promise<FileEntryRow[]>;
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
  getSystemInfo(): Promise<AnalysisSystemInfo>;
  classifyFiles(sampleSize?: number): Promise<AnalysisFileClassification[]>;
  getEvidenceClassificationSummary(): Promise<EvidenceClassificationSummary>;
  runEvidenceClassification(categories?: string[]): Promise<EvidenceClassificationSummary>;
  generateAnalysisSummary(): Promise<string>;
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
  async getRecentCases() {
    return recentCases;
  },
  async getDataSources() {
    return dataSources;
  },
  async getFileTree() {
    return filesTree;
  },
  async getFileRows(parentId?: string) {
    if (!parentId) {
      return [];
    }
    if (parentId === 'tree-system32') {
      return fileRows;
    }
    return [];
  },
  async openFileHandle(fileId: string) {
    const file = fileRows.find((item) => item.id === fileId);
    return {
      handleId: `file:${fileId}`,
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
  async getSystemInfo() {
    return analysisSystemInfo;
  },
  async classifyFiles(sampleSize?: number) {
    const limit = sampleSize ?? analysisClassifications.length;
    let remaining = limit;
    return analysisClassifications
      .map((classification) => {
        const files = classification.files.slice(0, remaining);
        remaining = Math.max(remaining - files.length, 0);
        return {
          ...classification,
          files,
          totalSize: files.reduce((sum, file) => sum + file.size, 0),
        };
      })
      .filter((classification) => classification.files.length > 0);
  },
  async getEvidenceClassificationSummary() {
    return evidenceClassificationSummary;
  },
  async runEvidenceClassification(_categories?: string[]) {
    return {
      ...evidenceClassificationSummary,
      status: 'parsed',
      categories: evidenceClassificationSummary.categories.map((category) => (
        category.status === 'candidateFound'
          ? { ...category, status: 'parsed', artifactCount: Math.max(category.artifactCount, 1) }
          : category
      )),
    };
  },
  async generateAnalysisSummary() {
    return analysisSummary;
  },
  async createCase(_caseRoot: string, name: string, examiner?: string) {
    return { ...currentCase, name, number: 'MOCK-001', examiner };
  },
  async openCase(_caseRoot: string) {
    return currentCase;
  },
  async closeCase() {},
  async renameDataSource(_dataSourceId: string, _name: string) {},
  async importDataSource(_sourcePath: string) {
    return 'Mock import: 42 files, 3 dirs';
  },
  async getFileChildren(_parentId: string) {
    return [];
  },
};
