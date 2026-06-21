import {
  AddCitationRequest,
  ArtifactRow,
  AnalysisExtractionPageRequest,
  BatchJob,
  BatchPlan,
  AnalysisExtractionRequest,
  AnalysisExtractionRun,
  AnalysisFileClassification,
  AnalysisSystemInfo,
  BrowserHistorySummary,
  CaseMetrics,
  CreateEntryRequest,
  DataSourceSummary,
  CaseSummary,
  CorrelationSnapshot,
  EmailExtractionSummary,
  FileEntryRow,
  FileJumpContext,
  GraphEdge,
  GraphNode,
  GraphQuery,
  GraphQueryResult,
  GraphSnapshot,
  EvidenceClassificationSummary,
  FileTreeNode,
  JobSnapshot,
  NotebookEntry,
  NotebookEntryListItem,
  RecentCase,
  RecentObject,
  ReportHistoryItem,
  ReportTemplate,
  RegistryExtractionSummary,
  RegistryStructuredSummary,
  RulePackSummary,
  RulePackValidationResult,
  SearchResultPage,
  TimelineEventDto,
  TraceItem,
  UpdateEntryRequest,
  V2GovernanceSnapshot,
  V3GovernanceSnapshot,
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
  registryStructuredSummary,
  searchHits,
  timelineEvents,
  traces,
  v2GovernanceSnapshot,
  v3GovernanceSnapshot,
  warnings,
  batchJobs,
  graphSnapshot,
  graphNodes,
  graphEdges,
  loadedRulePacks,
  rulePackValidationResult,
  notebookEntries,
  notebookEntryList,
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
      sortKey?: 'name' | 'size' | 'modifiedAt' | 'ext' | 'entryType';
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
  createBatchPlan(plan: BatchPlan): Promise<BatchJob>;
  startBatch(jobId: string): Promise<void>;
  pauseBatch(jobId: string): Promise<void>;
  resumeBatch(jobId: string): Promise<void>;
  cancelBatch(jobId: string): Promise<void>;
  getBatchJob(jobId: string): Promise<BatchJob | null>;
  listBatchJobs(): Promise<BatchJob[]>;
  getSystemInfo(): Promise<AnalysisSystemInfo>;
  classifyFiles(sampleSize?: number): Promise<AnalysisFileClassification[]>;
  getEvidenceClassificationSummary(): Promise<EvidenceClassificationSummary>;
  runEvidenceClassification(categories?: string[]): Promise<EvidenceClassificationSummary>;
  runAnalysisExtraction(request: AnalysisExtractionRequest): Promise<AnalysisExtractionRun>;
  getRegistryExtractionSummary(request?: AnalysisExtractionPageRequest): Promise<RegistryExtractionSummary>;
  getRegistryStructuredSummary(): Promise<RegistryStructuredSummary>;
  getBrowserHistorySummary(request?: AnalysisExtractionPageRequest): Promise<BrowserHistorySummary>;
  getEmailExtractionSummary(request?: AnalysisExtractionPageRequest): Promise<EmailExtractionSummary>;
  getV2GovernanceSnapshot(): Promise<V2GovernanceSnapshot>;
  getV3GovernanceSnapshot(): Promise<V3GovernanceSnapshot>;
  getCorrelationSnapshot(): Promise<CorrelationSnapshot>;
  generateAnalysisSummary(): Promise<string>;
  getGraphSnapshot(caseId: string): Promise<GraphSnapshot>;
  queryGraph(query: GraphQuery): Promise<GraphQueryResult>;
  getNodeNeighborhood(nodeId: string, depth?: number): Promise<GraphQueryResult>;
  listLoadedRulePacks(): Promise<RulePackSummary[]>;
  loadRulePack(path: string): Promise<RulePackSummary>;
  validateRulePack(packId: string): Promise<RulePackValidationResult>;
  listNotebookEntries(): Promise<NotebookEntryListItem[]>;
  getNotebookEntry(entryId: string): Promise<NotebookEntry | null>;
  createNotebookEntry(request: CreateEntryRequest): Promise<NotebookEntry>;
  updateNotebookEntry(request: UpdateEntryRequest): Promise<NotebookEntry>;
  addCitation(request: AddCitationRequest): Promise<NotebookEntry>;
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
  async createBatchPlan(plan: BatchPlan) {
    const job: BatchJob = {
      id: `batch-${Date.now()}`,
      name: plan.name,
      status: 'pending',
      progress: 0,
      phases: plan.phases.map((phase) => ({ phase, state: 'pending' as const, progress: 0, detail: '' })),
      plan: { ...plan, dataSourceCount: plan.dataSourceIds.length, phaseCount: plan.phases.length },
      createdAt: new Date().toISOString(),
      fileCount: 0,
      artifactCount: 0,
      logTail: [{ ts: new Date().toISOString(), level: 'info', message: 'Batch plan created.' }],
    };
    batchJobs.unshift(job);
    return job;
  },
  async startBatch(jobId: string) {
    const job = batchJobs.find((j) => j.id === jobId);
    if (job) {
      job.status = 'running';
      job.startedAt = new Date().toISOString();
      job.progress = 5;
      job.logTail.unshift({ ts: new Date().toISOString(), level: 'info', message: 'Batch job started.' });
    }
  },
  async pauseBatch(jobId: string) {
    const job = batchJobs.find((j) => j.id === jobId);
    if (job) {
      job.status = 'paused';
      job.logTail.unshift({ ts: new Date().toISOString(), level: 'info', message: 'Batch job paused.' });
    }
  },
  async resumeBatch(jobId: string) {
    const job = batchJobs.find((j) => j.id === jobId);
    if (job) {
      job.status = 'running';
      job.logTail.unshift({ ts: new Date().toISOString(), level: 'info', message: 'Batch job resumed.' });
    }
  },
  async cancelBatch(jobId: string) {
    const job = batchJobs.find((j) => j.id === jobId);
    if (job) {
      job.status = 'cancelled';
      job.completedAt = new Date().toISOString();
      job.logTail.unshift({ ts: new Date().toISOString(), level: 'warn', message: 'Batch job cancelled by user.' });
    }
  },
  async getBatchJob(jobId: string) {
    return batchJobs.find((j) => j.id === jobId) ?? null;
  },
  async listBatchJobs() {
    return batchJobs;
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
  async getRegistryStructuredSummary() {
    return registryStructuredSummary;
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
  async getV3GovernanceSnapshot() {
    return v3GovernanceSnapshot;
  },
  async getCorrelationSnapshot() {
    return correlationSnapshot;
  },
  async generateAnalysisSummary() {
    return analysisSummary;
  },
  async getGraphSnapshot(_caseId: string) {
    return graphSnapshot;
  },
  async queryGraph(query: GraphQuery) {
    const startSet = new Set(query.startIds);
    let candidateEdges = graphEdges;
    if (query.edgeTypes.length > 0) {
      const edgeTypeSet = new Set(query.edgeTypes);
      candidateEdges = graphEdges.filter((e) => edgeTypeSet.has(e.edgeType));
    }
    if (query.confidenceFloor != null) {
      candidateEdges = candidateEdges.filter((e) => (e.confidence ?? 1) >= query.confidenceFloor!);
    }
    const relevantEdges = candidateEdges.filter(
      (e) => startSet.has(e.sourceId) || startSet.has(e.targetId),
    );
    const nodeIds = new Set<string>(query.startIds);
    relevantEdges.forEach((e) => {
      nodeIds.add(e.sourceId);
      nodeIds.add(e.targetId);
    });
    const relevantNodes = graphNodes.filter((n) => nodeIds.has(n.id));
    const limit = query.limit ?? 100;
    return {
      nodes: relevantNodes.slice(0, limit),
      edges: relevantEdges.slice(0, limit),
      nodeCount: relevantNodes.length,
      edgeCount: relevantEdges.length,
    };
  },
  async getNodeNeighborhood(nodeId: string, _depth?: number) {
    const connectedEdges = graphEdges.filter(
      (e) => e.sourceId === nodeId || e.targetId === nodeId,
    );
    const nodeIds = new Set<string>([nodeId]);
    connectedEdges.forEach((e) => {
      nodeIds.add(e.sourceId);
      nodeIds.add(e.targetId);
    });
    const connectedNodes = graphNodes.filter((n) => nodeIds.has(n.id));
    return {
      nodes: connectedNodes,
      edges: connectedEdges,
      nodeCount: connectedNodes.length,
      edgeCount: connectedEdges.length,
    };
  },
  async listLoadedRulePacks() {
    return loadedRulePacks;
  },
  async loadRulePack(_path: string) {
    const newPack: RulePackSummary = {
      id: `rp-${Date.now()}`,
      name: 'Newly Loaded Rule Pack',
      version: '1.0.0',
      author: 'User',
      description: 'Custom rule pack loaded by user.',
      status: 'validating',
      ruleCount: 45,
      loadedAt: new Date().toISOString(),
      warnings: [],
      errors: [],
      coveredFamilies: ['Registry', 'Prefetch', 'LNK'],
    };
    loadedRulePacks.push(newPack);
    return newPack;
  },
  async validateRulePack(packId: string) {
    const pack = loadedRulePacks.find((p) => p.id === packId);
    if (!pack) {
      throw new Error('Rule pack not found');
    }
    return {
      packId,
      valid: pack.status !== 'error',
      errors: pack.errors,
      warnings: pack.warnings,
      coverage: {
        coveredFamilies: pack.coveredFamilies,
        uncoveredFamilies: ['RecycleBin', 'Thumbcache', 'SRU', 'Amcache', 'BAM', 'MFT'],
        coveragePercent: Math.round((pack.coveredFamilies.length / (pack.coveredFamilies.length + 6)) * 100),
      },
    };
  },
  async listNotebookEntries() {
    return notebookEntryList;
  },
  async getNotebookEntry(entryId: string) {
    return notebookEntries.find((e) => e.id === entryId) ?? null;
  },
  async createNotebookEntry(request: CreateEntryRequest) {
    const entry: NotebookEntry = {
      id: `nb-${Date.now()}`,
      caseId: 'case-2026-fx-091',
      parentId: request.parentId,
      title: request.title,
      content: request.content,
      entryType: request.entryType,
      status: 'draft',
      tags: request.tags ?? [],
      citationNodeIds: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    notebookEntries.unshift(entry);
    notebookEntryList.unshift({
      id: entry.id,
      parentId: entry.parentId,
      title: entry.title,
      entryType: entry.entryType,
      status: entry.status,
      tags: entry.tags,
      replyCount: 0,
      createdAt: entry.createdAt,
      updatedAt: entry.updatedAt,
    });
    if (request.parentId) {
      const parentListItem = notebookEntryList.find((e) => e.id === request.parentId);
      if (parentListItem) {
        (parentListItem as NotebookEntryListItem).replyCount += 1;
      }
    }
    return entry;
  },
  async updateNotebookEntry(request: UpdateEntryRequest) {
    const entry = notebookEntries.find((e) => e.id === request.entryId);
    if (!entry) {
      throw new Error('Notebook entry not found');
    }
    if (request.title !== undefined) entry.title = request.title;
    if (request.content !== undefined) entry.content = request.content;
    if (request.entryType !== undefined) entry.entryType = request.entryType;
    if (request.tags !== undefined) entry.tags = request.tags;
    if (request.status !== undefined) entry.status = request.status;
    entry.updatedAt = new Date().toISOString();
    const listItem = notebookEntryList.find((e) => e.id === request.entryId);
    if (listItem) {
      if (request.title !== undefined) listItem.title = request.title;
      if (request.entryType !== undefined) listItem.entryType = request.entryType;
      if (request.tags !== undefined) listItem.tags = request.tags;
      if (request.status !== undefined) listItem.status = request.status;
      listItem.updatedAt = entry.updatedAt;
    }
    return entry;
  },
  async addCitation(request: AddCitationRequest) {
    const entry = notebookEntries.find((e) => e.id === request.entryId);
    if (!entry) {
      throw new Error('Notebook entry not found');
    }
    const existing = new Set(entry.citationNodeIds);
    for (const nodeId of request.nodeIds) {
      existing.add(nodeId);
    }
    entry.citationNodeIds = Array.from(existing);
    entry.updatedAt = new Date().toISOString();
    return entry;
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
