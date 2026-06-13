import {
  ArtifactRow,
  AnalysisExtractionPageRequest,
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  AnalysisFileClassification,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  CaseMetrics,
  DataSourceSummary,
  CaseSummary,
  CorrelationSnapshot,
  EmailExtractionSummary,
  FileEntryRow,
  FileJumpContext,
  EvidenceClassificationSummary,
  FileTreeNode,
  JobSnapshot,
  RecentCase,
  RecentObject,
  ReportHistoryItem,
  ReportTemplate,
  RegistryExtractionSummary,
  SearchResultPage,
  TimelineEventDto,
  TraceItem,
  V2GovernanceSnapshot,
  ViewerHandle,
  ViewerRangeRequest,
  ViewerRangeResponse,
  WarningItem,
} from '@/types/models';
import {
  artifactFamilies,
  artifactRows,
  analysisClassifications,
  analysisExtractionRun,
  browserHistorySummary,
  emailExtractionSummary,
  evidenceClassificationSummary,
  analysisSummary,
  analysisSystemInfo,
  caseMetrics,
  currentCase,
  correlationSnapshot,
  dataSources,
  fileRows,
  filesTree,
  jobs,
  recentCases,
  recentObjects,
  reportHistory,
  reportTemplates,
  registryExtractionSummary,
  searchHits,
  timelineEvents,
  traces,
  v2GovernanceSnapshot,
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
  getFileTree(showHidden?: boolean): Promise<FileTreeNode[]>;
  getFileChildren(parentId: string, showHidden?: boolean): Promise<FileTreeNode[]>;
  getFileRows(parentId?: string, showHidden?: boolean): Promise<FileEntryRow[]>;
  getFileJumpContext(
    fileId: string,
    options?: {
      showHidden?: boolean;
      pageLimit?: number;
      sortKey?: 'name' | 'size' | 'modifiedAt' | 'ext';
      sortDirection?: 'asc' | 'desc';
    },
  ): Promise<FileJumpContext>;
  openFileHandle(fileId: string): Promise<ViewerHandle>;
  readFileRange(request: ViewerRangeRequest): Promise<ViewerRangeResponse>;
  searchFiles(query: string): Promise<SearchResultPage>;
  getTimelineEvents(): Promise<TimelineEventDto[]>;
  getTimelineEventById(eventId: string): Promise<TimelineEventDto | null>;
  getArtifactFamilies(): Promise<string[]>;
  getArtifactRows(family?: string): Promise<ArtifactRow[]>;
  getArtifactById(artifactId: string): Promise<ArtifactRow | null>;
  getReportTemplates(): Promise<ReportTemplate[]>;
  getReportHistory(): Promise<ReportHistoryItem[]>;
  getJobsSnapshot(): Promise<JobSnapshot[]>;
  getWarnings(): Promise<WarningItem[]>;
  getTraceItems(): Promise<TraceItem[]>;
  getSystemInfo(): Promise<AnalysisSystemInfo>;
  classifyFiles(sampleSize?: number): Promise<AnalysisFileClassification[]>;
  getEvidenceClassificationSummary(): Promise<EvidenceClassificationSummary>;
  runEvidenceClassification(categories?: string[]): Promise<EvidenceClassificationSummary>;
  runAnalysisExtraction(request: AnalysisExtractionRequest): Promise<AnalysisExtractionRun>;
  getRegistryExtractionSummary(request?: AnalysisExtractionPageRequest): Promise<RegistryExtractionSummary>;
  getBrowserHistorySummary(request?: AnalysisExtractionPageRequest): Promise<BrowserHistorySummary>;
  getEmailExtractionSummary(request?: AnalysisExtractionPageRequest): Promise<EmailExtractionSummary>;
  getV2GovernanceSnapshot(): Promise<V2GovernanceSnapshot>;
  getCorrelationSnapshot(): Promise<CorrelationSnapshot>;
  generateAnalysisSummary(): Promise<string>;
}

function filterHidden<T extends { hidden?: boolean; system?: boolean }>(
  items: T[],
  showHidden: boolean,
): T[] {
  if (showHidden) {
    return items;
  }
  return items.filter((item) => !item.hidden && !item.system);
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
  async getFileTree(showHidden = false) {
    return filterHidden(filesTree, showHidden);
  },
  async getFileRows(parentId?: string, showHidden = false) {
    if (!parentId) {
      return [];
    }
    if (parentId === 'tree-system32') {
      return filterHidden(fileRows, showHidden);
    }
    return [];
  },
  async getFileJumpContext(fileId: string, options) {
    const allRows = await this.getFileRows('tree-system32', options?.showHidden ?? false);
    const target = allRows.find((item) => item.id === fileId) ?? fileRows.find((item) => item.id === fileId);
    if (!target) {
      throw new Error('file not found');
    }
    const directoryId = target.entryType === 'directory' ? target.id : target.parentId ?? 'tree-system32';
    const directoryRows = await this.getFileRows(directoryId, options?.showHidden ?? false);
    const sortedRows = directoryId === 'tree-system32' ? directoryRows : fileRows.filter((item) => item.parentId === directoryId);
    const rowIndex = Math.max(
      0,
      sortedRows.findIndex((item) => item.id === target.id),
    );
    const pageLimit = options?.pageLimit ?? 500;
    return {
      target,
      directory:
        fileRows.find((item) => item.id === directoryId) ?? {
          id: directoryId,
          parentId: undefined,
          path: '/',
          name: 'Root',
          entryType: 'directory',
          deleted: false,
          hidden: false,
          system: false,
        },
      ancestorDirectoryIds: ['tree-system32', directoryId].filter(
        (value, index, source) => source.indexOf(value) === index,
      ),
      rowOffset: Math.floor(rowIndex / pageLimit) * pageLimit,
      requiresShowHidden: Boolean(target.hidden || target.system),
    };
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
  async getTimelineEventById(eventId: string) {
    return timelineEvents.find((item) => item.id === eventId) ?? null;
  },
  async getArtifactFamilies() {
    return artifactFamilies;
  },
  async getArtifactRows(family?: string) {
    return artifactRows.filter((item) => !family || item.artifactType === family);
  },
  async getArtifactById(artifactId: string) {
    return artifactRows.find((item) => item.id === artifactId) ?? null;
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
  async runAnalysisExtraction(_request: AnalysisExtractionRequest) {
    return analysisExtractionRun;
  },
  async getRegistryExtractionSummary(request?: AnalysisExtractionPageRequest) {
    const offset = request?.offset ?? 0;
    const limit = request?.limit ?? registryExtractionSummary.values.length;
    return {
      ...registryExtractionSummary,
      values: registryExtractionSummary.values.slice(offset, offset + limit),
    };
  },
  async getBrowserHistorySummary(request?: AnalysisExtractionPageRequest) {
    const offset = request?.offset ?? 0;
    const limit = request?.limit ?? browserHistorySummary.visits.length;
    return {
      ...browserHistorySummary,
      visits: browserHistorySummary.visits.slice(offset, offset + limit),
      downloads: browserHistorySummary.downloads.slice(0, limit),
    };
  },
  async getEmailExtractionSummary(request?: AnalysisExtractionPageRequest) {
    const offset = request?.offset ?? 0;
    const limit = request?.limit ?? emailExtractionSummary.messages.length;
    return {
      ...emailExtractionSummary,
      messages: emailExtractionSummary.messages.slice(offset, offset + limit),
    };
  },
  async getV2GovernanceSnapshot() {
    return v2GovernanceSnapshot;
  },
  async getCorrelationSnapshot() {
    return correlationSnapshot;
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
  async getFileChildren(parentId: string, showHidden = false) {
    if (parentId === 'tree-system32') {
      return filterHidden(filesTree.filter((item) => item.depth === 1), showHidden);
    }
    if (parentId === 'tree-winevt') {
      return filterHidden(filesTree.filter((item) => item.depth === 2), showHidden);
    }
    return [];
  },
};
