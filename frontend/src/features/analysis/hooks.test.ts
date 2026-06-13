import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  getSystemInfo: vi.fn(),
  classifyFiles: vi.fn(),
  getEvidenceClassificationSummary: vi.fn(),
  runEvidenceClassification: vi.fn(),
  runAnalysisExtraction: vi.fn(),
  getRegistryExtractionSummary: vi.fn(),
  getBrowserHistorySummary: vi.fn(),
  getEmailExtractionSummary: vi.fn(),
  getV2GovernanceSnapshot: vi.fn(),
  getCorrelationSnapshot: vi.fn(),
  generateAnalysisSummary: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/analysis', () => ({
  getSystemInfo: mocks.getSystemInfo,
  classifyFiles: mocks.classifyFiles,
  getEvidenceClassificationSummary: mocks.getEvidenceClassificationSummary,
  runEvidenceClassification: mocks.runEvidenceClassification,
  runAnalysisExtraction: mocks.runAnalysisExtraction,
  getRegistryExtractionSummary: mocks.getRegistryExtractionSummary,
  getBrowserHistorySummary: mocks.getBrowserHistorySummary,
  getEmailExtractionSummary: mocks.getEmailExtractionSummary,
  getV2GovernanceSnapshot: mocks.getV2GovernanceSnapshot,
  getCorrelationSnapshot: mocks.getCorrelationSnapshot,
  generateAnalysisSummary: mocks.generateAnalysisSummary,
}));

import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useBrowserHistorySummary,
  useCorrelationSnapshot,
  useEmailExtractionSummary,
  useGenerateAnalysisSummary,
  useRegistryExtractionSummary,
  useRunAnalysisExtraction,
  useV2GovernanceSnapshot,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('analysis hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getSystemInfo.mockResolvedValue({
      networkAdapters: [],
      bootHistory: [],
      status: 'notParsed',
      warnings: [],
      provenance: [],
    });
    mocks.classifyFiles.mockResolvedValue([]);
    mocks.getEvidenceClassificationSummary.mockResolvedValue({
      status: 'notFound',
      categories: [],
      generatedAt: '2026-06-01T10:00:00Z',
      warnings: [],
      totals: {
        categoryCount: 0,
        candidateFileCount: 0,
        totalSize: 0,
        artifactCount: 0,
      },
    });
    mocks.runEvidenceClassification.mockResolvedValue({
      status: 'parsed',
      categories: [],
      generatedAt: '2026-06-01T10:00:00Z',
      warnings: [],
      totals: {
        categoryCount: 0,
        candidateFileCount: 0,
        totalSize: 0,
        artifactCount: 0,
      },
    });
    mocks.runAnalysisExtraction.mockResolvedValue({
      status: 'parsed',
      scannedCount: 3,
      artifactCount: 2,
      timelineEventCount: 1,
      generatedAt: '2026-06-01T10:15:00Z',
      warnings: [],
    });
    mocks.getRegistryExtractionSummary.mockResolvedValue({
      status: 'parsed',
      total: 1,
      values: [],
      generatedAt: '2026-06-01T10:10:00Z',
      warnings: [],
    });
    mocks.getBrowserHistorySummary.mockResolvedValue({
      status: 'parsed',
      visitTotal: 1,
      downloadTotal: 0,
      visits: [],
      downloads: [],
      generatedAt: '2026-06-01T10:12:00Z',
      warnings: [],
    });
    mocks.getEmailExtractionSummary.mockResolvedValue({
      status: 'parsed',
      total: 1,
      messages: [],
      generatedAt: '2026-06-01T10:14:00Z',
      warnings: [],
    });
    mocks.getV2GovernanceSnapshot.mockResolvedValue({
      generatedAt: '2026-06-12T00:00:00Z',
      factSources: [
        {
          area: 'knownLimitations',
          factFile: 'testdata/governance/v2-known-limitations.json',
          factKind: 'catalog',
          derivedOutputs: ['knownLimitations', 'supportMatrix.documentedLimitCount'],
          lastVerifiedAt: '2026-06-13T00:00:00Z',
        },
      ],
      runtimeResults: {
        checkedAt: '2026-06-13T00:00:00Z',
        checks: [
          {
            checkId: 'docs-drift',
            title: '文档防漂移',
            status: 'passed',
            evidence: 'scripts/check-doc-drift.ps1',
            detail: 'README / AGENTS / documentation-index / Mermaid 图块数量一致',
            checkedAt: '2026-06-13T00:00:00Z',
            subChecks: [
              {
                checkId: 'readme-fact-sync',
                title: 'README 事实同步',
                status: 'passed',
                evidence: 'crate/page/command counts match',
                detail: 'README 关键事实与仓库扫描结果一致',
              },
            ],
          },
        ],
      },
      verificationChains: [],
      supportMatrix: {
        gaCount: 1,
        betaCount: 7,
        experimentalCount: 6,
        unsupportedCount: 0,
        documentedLimitCount: 9,
      },
      supportMatrixEntries: [
        {
          chain: 'NTFS',
          maturity: 'beta',
          verifiedSamples: ['tiny.raw'],
          baseline: 'fixture assertions / expected.json',
          guaranteeSummary: '枚举/读取稳定，复杂 deleted 恢复为 bestEffort',
          notes: [],
        },
      ],
      knownLimitations: [
        {
          category: 'Recycle Bin',
          item: '全损坏恢复场景',
          status: 'notGuaranteed',
          summary: '当前以标准结构提取为主',
          affectedChains: ['RecycleBin'],
          sourceDoc: 'docs/known-unsupported-formats.md',
        },
        {
          category: 'Browser',
          item: '全浏览器全版本兼容',
          status: 'notGuaranteed',
          summary: 'Edge / Chrome / Firefox 仍需扩大样本',
          affectedChains: ['ChromeHistory', 'EdgeHistory', 'FirefoxHistory'],
          sourceDoc: 'docs/known-unsupported-formats.md',
        },
      ],
      benchmark: {
        hostProfile: 'Windows',
        baselineVersion: '2026.06',
        lastVerifiedAt: '2026-06-12T00:00:00Z',
        scenarios: [],
        requiredChecks: [
          {
            datasetLevel: 'medium',
            scenario: '搜索热查询',
            thresholdP95Ms: 1500,
            measuredP95Ms: 1500,
            status: 'covered',
          },
        ],
        coveredRequiredCount: 1,
        missingRequiredCount: 0,
        exceededRequiredCount: 0,
      },
      security: {
        exportOverwriteDefault: false,
        exportPathGuardEnabled: true,
        stdioCommandWhitelistEnforced: true,
        sseHttpsOnly: true,
        embeddedCredentialsBlocked: true,
        mediaHandleScoped: true,
        errorRedactionEnabled: true,
        auditLogRequired: true,
        auditEventCount: 1,
        sensitiveAuditEventCount: 1,
        recentAuditEntries: [
          {
            action: 'mcp.tool.call',
            resourceType: 'mcp',
            resourceId: 'fixture-catalog',
            createdAt: '2026-06-12T00:10:00Z',
            summary: 'status=ok; toolName=query_fixture_catalog',
            sensitive: true,
          },
        ],
        notes: [],
      },
      errorTaxonomyEntries: [
        {
          category: 'security',
          severity: 'high',
          recoverable: false,
          examples: ['MCP policy block'],
          redactionRule: 'never expose credentials or raw absolute paths',
          notes: [],
        },
      ],
      releaseGates: [
        {
          gateId: 'core-fixture-regression',
          title: '核心 fixture 回归',
          status: 'warning',
          evidence: 'coreChains=7, passed=6, partialOrPending=1, failed=0, maturity[ga=1, beta=7, experimental=6, unsupported=0]',
          detail: '仍有未全量通过链路：E01 镜像读取。',
        },
      ],
      releaseScorecard: {
        totalScore: 80,
        grade: 'B',
        verificationScore: 22,
        correlationScore: 21,
        performanceScore: 15,
        securityScore: 22,
        breakdown: [
          {
            dimension: 'verification',
            maxScore: 30,
            actualScore: 22,
            deductions: ['核心 fixture 未全量通过扣 4 分'],
          },
        ],
        blockers: [],
        residualRisks: [],
      },
      runtimeSignals: {
        dataSourceCount: 1,
        hashedDataSourceCount: 1,
        pendingHashDataSourceCount: 0,
        warningDataSourceCount: 0,
        runningJobCount: 0,
        partialJobCount: 0,
        failedJobCount: 0,
        reportCount: 0,
        correlationSnapshotAvailable: true,
        correlationLeadCount: 1,
        correlationHighConfidenceLeadCount: 1,
        correlationReviewLeadCount: 0,
        correlationClusterCount: 1,
        correlationRuleFamilyCount: 8,
        correlationCoveredFamilyCount: 1,
        correlationHighConfidenceFamilyCount: 1,
        correlationFamilyCoverage: [
          {
            family: 'LNK',
            displayName: 'LNK',
            status: 'covered',
            leadCount: 1,
            highConfidenceLeadCount: 1,
            reviewLeadCount: 0,
            clusterCount: 1,
            sampleSignals: ['LNK 目标路径命中文件路径'],
          },
        ],
      },
    });
    mocks.getCorrelationSnapshot.mockResolvedValue({
      generatedAt: '2026-06-12T00:00:00Z',
      nodeCount: 3,
      edgeCount: 2,
      clusterCount: 1,
      leadCount: 1,
      familyCoverage: [
        {
          family: 'LNK',
          displayName: 'LNK',
          status: 'covered',
          leadCount: 1,
          highConfidenceLeadCount: 1,
          reviewLeadCount: 0,
          clusterCount: 1,
          sampleSignals: ['LNK 目标路径命中文件路径'],
        },
      ],
      nodes: [],
      edges: [
        {
          id: 'edge-1',
          kind: 'pathMatch',
          fromNodeId: 'artifact:1',
          toNodeId: 'file:file-1',
          summary: 'LNK 目标路径命中文件路径',
          confidence: 'direct',
        },
      ],
      clusters: [],
      leads: [],
    });
    mocks.generateAnalysisSummary.mockResolvedValue('# 数据源分析报告');
  });

  it('does not call analysis APIs without an active case', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: null,
    });

    const { result } = renderHook(() => useAnalysisSystemInfo(), { wrapper: createWrapper() });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.getSystemInfo).not.toHaveBeenCalled();
  });

  it('loads system info when current case exists', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useAnalysisSystemInfo(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getSystemInfo).toHaveBeenCalledTimes(1);
  });

  it('passes sample size to classification API', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useAnalysisClassifications(250), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.classifyFiles).toHaveBeenCalledWith(250);
  });

  it('exposes summary download mutation', async () => {
    const { result } = renderHook(() => useGenerateAnalysisSummary(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync();
    expect(mocks.generateAnalysisSummary).toHaveBeenCalledTimes(1);
  });

  it('loads registry, browser and email extraction summaries with paging defaults', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const wrapper = createWrapper();
    const registry = renderHook(() => useRegistryExtractionSummary(), { wrapper });
    const browser = renderHook(() => useBrowserHistorySummary({ limit: 50 }), { wrapper });
    const email = renderHook(() => useEmailExtractionSummary({ offset: 10, limit: 25 }), { wrapper });

    await waitFor(() => expect(registry.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(browser.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(email.result.current.isSuccess).toBe(true));

    expect(mocks.getRegistryExtractionSummary).toHaveBeenCalledWith({ offset: 0, limit: 200 });
    expect(mocks.getBrowserHistorySummary).toHaveBeenCalledWith({ offset: 0, limit: 50 });
    expect(mocks.getEmailExtractionSummary).toHaveBeenCalledWith({ offset: 10, limit: 25 });
  });

  it('runs analysis extraction mutation with selected categories', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useRunAnalysisExtraction(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({ categories: ['Registry', 'BrowserHistory', 'Email'] });

    expect(mocks.runAnalysisExtraction).toHaveBeenCalledWith({
      categories: ['Registry', 'BrowserHistory', 'Email'],
    });
  });

  it('loads v2 governance snapshot when current case exists', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useV2GovernanceSnapshot(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getV2GovernanceSnapshot).toHaveBeenCalledTimes(1);
  });

  it('loads correlation snapshot when current case exists', async () => {
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });

    const { result } = renderHook(() => useCorrelationSnapshot(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getCorrelationSnapshot).toHaveBeenCalledTimes(1);
  });
});
