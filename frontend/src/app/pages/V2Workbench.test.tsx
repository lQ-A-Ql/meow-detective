import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { V2Workbench } from './V2Workbench';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  useCreateAnalysisDemoCase: vi.fn(),
  useV2GovernanceSnapshot: vi.fn(),
  useCorrelationSnapshot: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
  useCreateAnalysisDemoCase: mocks.useCreateAnalysisDemoCase,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useV2GovernanceSnapshot: mocks.useV2GovernanceSnapshot,
  useCorrelationSnapshot: mocks.useCorrelationSnapshot,
}));

const selectionState = {
  setSelectedFileId: vi.fn(),
  setSelectedArtifactId: vi.fn(),
  setSelectedTimelineId: vi.fn(),
};

vi.mock('@/stores/selection-store', () => ({
  useSelectionStore: vi.fn((selector) => selector(selectionState)),
}));

vi.mock('react-router', () => ({
  useNavigate: () => vi.fn(),
}));

function mutationState() {
  return {
    mutateAsync: vi.fn(),
    isPending: false,
    error: null,
  };
}

function queryState(data: unknown) {
  return {
    data,
    error: null,
    isLoading: false,
    isSuccess: true,
    isFetching: false,
    refetch: vi.fn(),
  };
}

describe('V2Workbench', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useCreateAnalysisDemoCase.mockReturnValue(mutationState());
    mocks.useCurrentCase.mockReturnValue(queryState({ id: 'case-1' }));
    mocks.useV2GovernanceSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      factSources: [
        {
          area: 'verification',
          factFile: 'testdata/governance/v2-verification-catalog.json',
          factKind: 'catalog',
          derivedOutputs: ['verificationChains', 'supportMatrixEntries', 'supportMatrix'],
          lastVerifiedAt: '2026-06-12T00:00:00Z',
        },
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
          {
            checkId: 'security-baseline',
            title: '安全基线',
            status: 'passed',
            evidence: 'latestSecurityRun=2026-06-13T00:00:00Z; pathGuard=true',
            detail: '导出路径、防覆盖、MCP 白名单、媒体句柄与错误脱敏基线均已通过最近一次安全回归',
            checkedAt: '2026-06-13T00:00:00Z',
            subChecks: [
              {
                checkId: 'export-path-guard',
                title: '导出路径防护',
                status: 'passed',
                evidence: 'pathGuard=true overwriteDefault=false',
                detail: '默认不覆盖且路径规范化防护开启',
              },
            ],
          },
        ],
      },
      verificationChains: [
        {
          chain: 'NTFS',
          displayName: 'NTFS 鏂囦欢绯荤粺',
          maturity: 'ga',
          guaranteeLevel: 'guaranteed',
          fixtureTier: 'public-small',
          expectedJsonVersion: 'v1',
          verifiedSampleCount: 4,
          result: 'passed',
          notes: ['覆盖 deleted/hidden銆?'],
        },
      ],
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
          verifiedSamples: ['tiny.raw', 'synthetic ntfs fixture'],
          baseline: 'fixture assertions / expected.json',
          guaranteeSummary: '枚举/读取稳定，复杂 deleted 恢复为 bestEffort',
          notes: ['复杂损坏样本仍不足。'],
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
        scenarios: [
          {
            datasetLevel: 'medium',
            scenario: '搜索热查询',
            p95Ms: 1500,
            memoryPeakMb: 1536,
            baselineVersion: '2026.06',
          },
        ],
        requiredChecks: [
          {
            datasetLevel: 'medium',
            scenario: '搜索热查询',
            thresholdP95Ms: 1500,
            measuredP95Ms: 1500,
            status: 'covered',
          },
          {
            datasetLevel: 'large',
            scenario: '文件树首展开',
            thresholdP95Ms: 2000,
            status: 'missing',
          },
        ],
        coveredRequiredCount: 1,
        missingRequiredCount: 1,
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
        auditEventCount: 4,
        sensitiveAuditEventCount: 3,
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
        notes: ['瀵煎嚭榛樿涓嶈鐩栥€?'],
      },
      errorTaxonomyEntries: [
        {
          category: 'security',
          severity: 'high',
          recoverable: false,
          examples: ['MCP policy block'],
          redactionRule: 'never expose credentials or raw absolute paths',
          notes: ['frontend only receives sanitized messages'],
        },
      ],
      releaseGates: [
        {
          gateId: 'benchmark-thresholds',
          title: 'Benchmark 阈值',
          status: 'warning',
          evidence: 'baselineVersion=2026.06, measuredScenarios=1, missingRequired=5, exceededRequired=0',
          detail: '浠嶇己灏?benchmark 蹇呴渶鍦烘櫙锛歮edium 文件树首展开銆乵edium 鏃堕棿绾跨瓫閫夈€乴arge 文件树首展开銆乴arge 鎼滅储鐑煡璇€乴arge 鏃堕棿绾跨瓫閫?',
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
            deductions: ['fixture coverage gap'],
          },
        ],
        blockers: ['仍需 private regression'],
        residualRisks: ['Browser 仍为 Beta'],
      },
      runtimeSignals: {
        dataSourceCount: 2,
        hashedDataSourceCount: 1,
        pendingHashDataSourceCount: 1,
        warningDataSourceCount: 1,
        runningJobCount: 0,
        partialJobCount: 1,
        failedJobCount: 0,
        reportCount: 2,
        correlationSnapshotAvailable: true,
        correlationLeadCount: 1,
        correlationHighConfidenceLeadCount: 1,
        correlationReviewLeadCount: 1,
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
    }));
    mocks.useCorrelationSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      nodeCount: 3,
      edgeCount: 3,
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
        {
          family: 'Prefetch',
          displayName: 'Prefetch',
          status: 'missing',
          leadCount: 0,
          highConfidenceLeadCount: 0,
          reviewLeadCount: 0,
          clusterCount: 0,
          sampleSignals: [],
        },
      ],
      nodes: [
        {
          id: 'file:file-1',
          kind: 'file',
          title: 'cmd.exe',
          subtitle: 'C:/Windows/System32/cmd.exe',
          sourceObjectId: 'file-1',
          relatedCount: 2,
          badges: ['deleted'],
          jumps: [{ route: '/files', targetId: 'file-1', label: '打开文件浏览' }],
        },
      ],
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
      clusters: [
        {
          id: 'cluster:file-1',
          title: 'cmd.exe',
          summary: '同一 source object 聚合 1 条痕迹记录与 1 条时间线事件。',
          confidence: 'direct',
          families: ['LNK'],
          primaryFileId: 'file-1',
          artifactCount: 1,
          timelineCount: 1,
          nodeIds: ['file:file-1', 'artifact:1', 'timeline:1'],
          edgeIds: ['edge-1'],
          provenance: [
            {
              sourceKind: 'artifact',
              sourceRecordId: 'artifact-1',
              sourceLabel: 'Prefetch',
              producer: 'prefetch',
              producerVersion: '1.0.0',
              guaranteeLevel: 'bestEffort',
              warningSummary: [],
            },
          ],
        },
      ],
      leads: [
        {
          id: 'lead:file-1',
          title: 'cmd.exe 形成关联线索',
          summary: 'LNK 目标路径命中文件路径銆?',
          confidence: 'direct',
          families: ['LNK'],
          primaryFileId: 'file-1',
          supportingNodeIds: ['artifact:1', 'timeline:1'],
          matchSignals: ['LNK 目标路径命中文件路径'],
          jumps: [{ route: '/timeline', targetId: 'timeline-1', label: 'Open Timeline' }],
          provenance: [
            {
              sourceKind: 'timeline',
              sourceRecordId: 'timeline-1',
              sourceLabel: 'FILE_MODIFIED',
              producer: 'timeline.macb',
              producerVersion: '1.0.0',
              guaranteeLevel: 'bestEffort',
              warningSummary: [],
            },
          ],
          caveats: ['时间线命中可能来自投影层，需回跳原始事件复核。'],
        },
      ],
    }));
  });

  it('renders governance snapshot sections', () => {
    render(<V2Workbench />);

    expect(screen.getByText('V2 治理工作台')).toBeDefined();
    expect(screen.getByText('治理事实源')).toBeDefined();
    expect(screen.getByText('testdata/governance/v2-verification-catalog.json')).toBeDefined();
    expect(screen.getByText('最近一次治理运行结果')).toBeDefined();
    expect(screen.getAllByText('Sub Checks').length).toBeGreaterThan(0);
    expect(screen.getByText('README 事实同步')).toBeDefined();
    expect(screen.getAllByText('导出路径防护').length).toBeGreaterThan(0);
    expect(screen.getAllByText('安全基线').length).toBeGreaterThan(0);
    expect(screen.getAllByText('可信验证').length).toBeGreaterThan(0);
    expect(screen.getByText('Benchmark 基线')).toBeDefined();
    expect(screen.getByText('Required Checks')).toBeDefined();
    expect(screen.getByText('必需项已覆盖')).toBeDefined();
    expect(screen.getAllByText('实测 p95').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Missing').length).toBeGreaterThan(0);
    expect(screen.getAllByText('安全治理').length).toBeGreaterThan(0);
    expect(screen.getByText('Recent Audit Entries')).toBeDefined();
    expect(screen.getByText('mcp.tool.call')).toBeDefined();
    expect(screen.getByText('支持矩阵明细')).toBeDefined();
    expect(screen.getByText('已知限制')).toBeDefined();
    expect(screen.getByText('Correlation Family Coverage')).toBeDefined();
    expect(screen.getAllByText('LNK').length).toBeGreaterThan(0);
    expect(screen.getByText('错误分类与脱敏')).toBeDefined();
    expect(screen.getByText('发布门禁')).toBeDefined();
    expect(screen.getByText('发布评分卡')).toBeDefined();
    expect(screen.getByText('总评')).toBeDefined();
    expect(screen.getByText('评分拆解')).toBeDefined();
    expect(screen.getByText('B')).toBeDefined();
    expect(screen.getAllByText('80').length).toBeGreaterThan(0);
    expect(screen.getByText('关联分析工作台')).toBeDefined();
    expect(screen.getByText('规则家族覆盖')).toBeDefined();
    expect(screen.getByTestId('correlation-family-coverage-panel')).toBeDefined();
    expect(screen.getAllByText('来源类别').length).toBeGreaterThan(0);
  });

  it('uses shared jump actions to drive file selection from correlation leads', () => {
    render(<V2Workbench />);

    fireEvent.click(screen.getAllByRole('button', { name: 'Open Timeline' })[0]);

    expect(selectionState.setSelectedTimelineId).toHaveBeenCalledWith('timeline-1');
  });

  it('shows lead detail drill-down for the selected correlation lead', () => {
    render(<V2Workbench />);

    expect(screen.getByTestId('selected-lead-panel')).toBeDefined();
    expect(screen.getByTestId('selected-lead-title').textContent).toContain('cmd.exe');
    expect(screen.getByText('主文件节点')).toBeDefined();
    expect(screen.getByText('相关边')).toBeDefined();
    expect(screen.getAllByText('Provenance').length).toBeGreaterThan(0);
  });

  it('filters correlation leads by focus mode and confidence', async () => {
    mocks.useCorrelationSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      nodeCount: 4,
      edgeCount: 2,
      clusterCount: 2,
      leadCount: 2,
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
        {
          family: 'BrowserDownload',
          displayName: 'Browser Download',
          status: 'review',
          leadCount: 1,
          highConfidenceLeadCount: 0,
          reviewLeadCount: 1,
          clusterCount: 1,
          sampleSignals: ['Browser download targetPath 命中文件路径'],
        },
      ],
      nodes: [
        {
          id: 'file:file-1',
          kind: 'file',
          title: 'cmd.exe',
          subtitle: 'C:/Windows/System32/cmd.exe',
          sourceObjectId: 'file-1',
          relatedCount: 2,
          badges: [],
          jumps: [{ route: '/files', targetId: 'file-1', label: '打开文件浏览' }],
        },
        {
          id: 'artifact:artifact-1',
          kind: 'artifact',
          title: 'cmd.lnk',
          subtitle: 'C:/Users/alice/Desktop/cmd.lnk',
          sourceObjectId: 'file-1',
          relatedCount: 1,
          badges: ['LNK'],
          jumps: [{ route: '/artifacts', targetId: 'artifact-1', label: '打开痕迹分析' }],
        },
        {
          id: 'file:file-2',
          kind: 'file',
          title: 'payload.exe',
          subtitle: 'C:/Temp/payload.exe',
          sourceObjectId: 'file-2',
          relatedCount: 1,
          badges: [],
          jumps: [{ route: '/files', targetId: 'file-2', label: '打开文件浏览' }],
        },
        {
          id: 'artifact:artifact-2',
          kind: 'artifact',
          title: 'Edge download',
          subtitle: 'targetPath=C:/Temp/payload.exe',
          sourceObjectId: 'file-2',
          relatedCount: 1,
          badges: ['BrowserDownload'],
          jumps: [{ route: '/artifacts', targetId: 'artifact-2', label: '打开痕迹分析' }],
        },
      ],
      edges: [
        {
          id: 'edge-1',
          kind: 'pathMatch',
          fromNodeId: 'artifact:artifact-1',
          toNodeId: 'file:file-1',
          summary: 'LNK 目标路径命中文件路径',
          confidence: 'direct',
        },
        {
          id: 'edge-2',
          kind: 'pathMatch',
          fromNodeId: 'artifact:artifact-2',
          toNodeId: 'file:file-2',
          summary: 'Browser download targetPath 命中文件路径',
          confidence: 'heuristic',
        },
      ],
      clusters: [
        {
          id: 'cluster:file-1',
          title: 'cmd.exe',
          summary: 'LNK 命中 cmd.exe',
          confidence: 'direct',
          families: ['LNK'],
          primaryFileId: 'file-1',
          artifactCount: 1,
          timelineCount: 0,
          nodeIds: ['file:file-1', 'artifact:artifact-1'],
          edgeIds: ['edge-1'],
          provenance: [],
        },
        {
          id: 'cluster:file-2',
          title: 'payload.exe',
          summary: 'Browser download 命中 payload.exe',
          confidence: 'heuristic',
          families: ['BrowserDownload'],
          primaryFileId: 'file-2',
          artifactCount: 1,
          timelineCount: 0,
          nodeIds: ['file:file-2', 'artifact:artifact-2'],
          edgeIds: ['edge-2'],
          provenance: [],
        },
      ],
      leads: [
        {
          id: 'lead:file-1',
          title: 'cmd.exe 形成关联线索',
          summary: 'LNK 命中 cmd.exe。',
          confidence: 'direct',
          families: ['LNK'],
          primaryFileId: 'file-1',
          supportingNodeIds: ['artifact:artifact-1'],
          matchSignals: ['LNK 目标路径命中文件路径'],
          jumps: [{ route: '/files', targetId: 'file-1', label: '打开文件浏览' }],
          provenance: [],
          caveats: [],
        },
        {
          id: 'lead:file-2',
          title: 'payload.exe 形成关联线索',
          summary: 'Browser download 命中 payload.exe。',
          confidence: 'heuristic',
          families: ['BrowserDownload'],
          primaryFileId: 'file-2',
          supportingNodeIds: ['artifact:artifact-2'],
          matchSignals: ['Browser download targetPath 命中文件路径'],
          jumps: [{ route: '/files', targetId: 'file-2', label: '打开文件浏览' }],
          provenance: [
            {
              sourceKind: 'artifact',
              sourceRecordId: 'artifact-2',
              sourceLabel: 'BrowserDownload',
              producer: 'browser',
              producerVersion: '1.0.0',
              guaranteeLevel: 'experimental',
              warningSummary: [],
            },
          ],
          caveats: ['需结合下载来源复核。'],
        },
      ],
    }));

    render(<V2Workbench />);

    fireEvent.click(screen.getByRole('radio', { name: '只看待复核线索' }));
    expect(screen.getAllByText('payload.exe 形成关联线索').length).toBeGreaterThan(0);

    fireEvent.keyDown(screen.getByTestId('correlation-confidence-filter'), {
      key: 'Enter',
      code: 'Enter',
    });
    fireEvent.click(await screen.findByRole('option', { name: 'Direct' }));
    expect(screen.queryByText('payload.exe 形成关联线索')).toBeNull();
  });

  it('switches lead detail when another lead card is selected', () => {
    mocks.useCorrelationSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      nodeCount: 4,
      edgeCount: 2,
      clusterCount: 2,
      leadCount: 2,
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
        {
          family: 'BrowserDownload',
          displayName: 'Browser Download',
          status: 'covered',
          leadCount: 1,
          highConfidenceLeadCount: 1,
          reviewLeadCount: 1,
          clusterCount: 1,
          sampleSignals: ['Browser download targetPath 命中文件路径'],
        },
      ],
      nodes: [
        {
          id: 'file:file-1',
          kind: 'file',
          title: 'cmd.exe',
          subtitle: 'C:/Windows/System32/cmd.exe',
          sourceObjectId: 'file-1',
          relatedCount: 2,
          badges: ['deleted'],
          jumps: [{ route: '/files', targetId: 'file-1', label: '打开文件浏览' }],
        },
        {
          id: 'artifact:artifact-1',
          kind: 'artifact',
          title: 'cmd.lnk',
          subtitle: 'C:/Users/alice/Desktop/cmd.lnk',
          sourceObjectId: 'file-1',
          relatedCount: 2,
          badges: ['LNK'],
          jumps: [{ route: '/artifacts', targetId: 'artifact-1', label: '打开痕迹分析' }],
        },
        {
          id: 'file:file-2',
          kind: 'file',
          title: 'payload.exe',
          subtitle: 'C:/Temp/payload.exe',
          sourceObjectId: 'file-2',
          relatedCount: 1,
          badges: [],
          jumps: [{ route: '/files', targetId: 'file-2', label: '打开文件浏览' }],
        },
        {
          id: 'artifact:artifact-2',
          kind: 'artifact',
          title: 'Edge download',
          subtitle: 'targetPath=C:/Temp/payload.exe',
          sourceObjectId: 'file-2',
          relatedCount: 1,
          badges: ['BrowserDownload'],
          jumps: [{ route: '/artifacts', targetId: 'artifact-2', label: '打开痕迹分析' }],
        },
      ],
      edges: [
        {
          id: 'edge-1',
          kind: 'pathMatch',
          fromNodeId: 'artifact:artifact-1',
          toNodeId: 'file:file-1',
          summary: 'LNK 目标路径命中文件路径',
          confidence: 'direct',
        },
        {
          id: 'edge-2',
          kind: 'pathMatch',
          fromNodeId: 'artifact:artifact-2',
          toNodeId: 'file:file-2',
          summary: 'Browser download targetPath 命中文件路径',
          confidence: 'strong',
        },
      ],
      clusters: [
        {
          id: 'cluster:file-1',
          title: 'cmd.exe',
          summary: 'LNK 命中 cmd.exe',
          confidence: 'direct',
          families: ['LNK'],
          primaryFileId: 'file-1',
          artifactCount: 1,
          timelineCount: 0,
          nodeIds: ['file:file-1', 'artifact:artifact-1'],
          edgeIds: ['edge-1'],
          provenance: [],
        },
        {
          id: 'cluster:file-2',
          title: 'payload.exe',
          summary: 'Browser download 命中 payload.exe',
          confidence: 'strong',
          families: ['BrowserDownload'],
          primaryFileId: 'file-2',
          artifactCount: 1,
          timelineCount: 0,
          nodeIds: ['file:file-2', 'artifact:artifact-2'],
          edgeIds: ['edge-2'],
          provenance: [],
        },
      ],
      leads: [
        {
          id: 'lead:file-1',
          title: 'cmd.exe 形成关联线索',
          summary: 'LNK 命中 cmd.exe。',
          confidence: 'direct',
          families: ['LNK'],
          primaryFileId: 'file-1',
          supportingNodeIds: ['artifact:artifact-1'],
          matchSignals: ['LNK 目标路径命中文件路径'],
          jumps: [{ route: '/files', targetId: 'file-1', label: '打开文件浏览' }],
          provenance: [],
          caveats: [],
        },
        {
          id: 'lead:file-2',
          title: 'payload.exe 形成关联线索',
          summary: 'Browser download 命中 payload.exe。',
          confidence: 'strong',
          families: ['BrowserDownload'],
          primaryFileId: 'file-2',
          supportingNodeIds: ['artifact:artifact-2'],
          matchSignals: ['Browser download targetPath 命中文件路径'],
          jumps: [{ route: '/files', targetId: 'file-2', label: '打开文件浏览' }],
          provenance: [],
          caveats: ['需结合下载来源复核。'],
        },
      ],
    }));

    render(<V2Workbench />);

    fireEvent.click(screen.getByTestId('lead-card-lead:file-2'));

    expect(screen.getByTestId('selected-lead-title').textContent).toContain('payload.exe');
    const panel = screen.getByTestId('selected-lead-panel');
    expect(within(panel).getAllByText('Browser download targetPath 命中文件路径').length).toBeGreaterThan(0);
    expect(within(panel).getAllByText('需结合下载来源复核。').length).toBeGreaterThan(0);
  });

  it('renders correlation family coverage from the correlation snapshot payload', () => {
    render(<V2Workbench />);

    const panel = screen.getByTestId('correlation-family-coverage-panel');
    expect(within(screen.getByTestId('correlation-family-LNK')).getAllByText('LNK').length).toBeGreaterThan(0);
    expect(within(screen.getByTestId('correlation-family-Prefetch')).getAllByText('Prefetch').length).toBeGreaterThan(0);
    expect(within(panel).getAllByText('Covered').length).toBeGreaterThan(0);
    expect(within(panel).getByText('LNK 目标路径命中文件路径')).toBeDefined();
  });
});
