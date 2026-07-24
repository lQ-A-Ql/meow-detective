import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@/features/files/hooks', () => ({
  useFileHandle: () => ({ data: undefined }),
  useFileViewer: () => ({
    data: undefined,
    setJumpOffsetInput: vi.fn(),
    jumpToOffset: vi.fn(),
    loadNextRange: vi.fn(),
    loadPreviousRange: vi.fn(),
  }),
  useTextPreview: () => ({ data: null }),
  useImagePreview: () => ({ data: null }),
  useMediaUrl: () => ({ data: null }),
  useDocumentPreview: () => ({ data: null }),
}));
import {
  SystemInfoPanel,
  BrowserHistoryPanel,
  EmailExtractionPanel,
  RegistryExtractionPanel,
  EvidenceClassificationPanel,
  FileClassificationBoard,
  AnalysisReportPanel,
  AnalysisExtractionProgress,
  AnalysisEmptyState,
  AnalysisErrorBanner,
  AnalysisLoadingPanel,
  AnalysisHeader,
} from './panels';

describe('AnalysisPanels sub-components', () => {
  describe('SystemInfoPanel', () => {
    it('renders fallback when no data is provided', () => {
      render(createElement(SystemInfoPanel, { systemInfo: undefined }));
      expect(screen.getByText('系统信息暂不可用。')).toBeDefined();
      expect(screen.getAllByText('不可用').length).toBeGreaterThan(0);
    });

    it('renders parsed system info fields', () => {
      const systemInfo = {
        computerName: 'FORENSICS-PC',
        osVersion: 'Windows 11',
        buildNumber: '22631',
        registeredOwner: 'admin',
        timezone: 'UTC+8',
        installDate: '2025-01-15',
        networkAdapters: [{ name: 'Ethernet', macAddress: 'AA:BB:CC:DD:EE:FF', ipAddresses: ['192.168.1.1'] }],
        bootHistory: [],
        status: 'parsed' as const,
        warnings: [],
        provenance: [],
        fieldProvenance: [],
      };
      render(createElement(SystemInfoPanel, { systemInfo }));
      expect(screen.getByText('FORENSICS-PC')).toBeDefined();
      expect(screen.getByText('Windows 11')).toBeDefined();
      expect(screen.getByText('Ethernet')).toBeDefined();
      expect(screen.getByText(/AA:BB:CC:DD:EE:FF/)).toBeDefined();
    });
  });

  describe('BrowserHistoryPanel', () => {
    it('renders empty state when no summary is provided', () => {
      render(createElement(BrowserHistoryPanel, { summary: undefined }));
      expect(screen.getByText('浏览器记录暂不可用。')).toBeDefined();
    });

    it('renders visit data when summary is provided', () => {
      const summary = {
        status: 'parsed' as const,
        visitTotal: 1,
        downloadTotal: 0,
        cookieTotal: 0,
        sessionTotal: 0,
        passwordTotal: 0,
        generatedAt: '2026-06-01T10:00:00Z',
        warnings: [],
        visits: [{ artifactId: 'v1', fileId: 'f1', sourcePath: '/path', browser: 'Chrome', profile: 'Default', url: 'https://example.com', title: 'Example', visitTime: '2026-06-01T10:00:00Z', visitCount: 1 }],
        downloads: [],
        cookies: [],
        sessions: [],
        passwords: [],
      };
      render(createElement(BrowserHistoryPanel, { summary }));
      expect(screen.getByText('Example')).toBeDefined();
      expect(screen.getAllByText('Chrome').length).toBeGreaterThan(0);
    });
  });

  describe('EmailExtractionPanel', () => {
    it('renders empty state when no summary is provided', () => {
      render(createElement(EmailExtractionPanel, { summary: undefined }));
      expect(screen.getByText('邮件提取结果暂不可用。')).toBeDefined();
    });
  });

  describe('RegistryExtractionPanel', () => {
    it('renders empty state when no summary is provided', () => {
      render(createElement(RegistryExtractionPanel, { summary: undefined }));
      expect(screen.getByText('注册表提取结果暂不可用。')).toBeDefined();
    });

    it('renders sub-tabs', () => {
      render(createElement(RegistryExtractionPanel, { summary: undefined }));
      expect(screen.getByText('用户账户')).toBeDefined();
      expect(screen.getByText('用户活动')).toBeDefined();
      expect(screen.getByText('网络配置')).toBeDefined();
      expect(screen.getByText('软件列表')).toBeDefined();
      expect(screen.getByText('USB 设备')).toBeDefined();
      expect(screen.getByText('原始键值')).toBeDefined();
    });
  });

  describe('EvidenceClassificationPanel', () => {
    it('renders empty state when no summary is provided', () => {
      render(createElement(EvidenceClassificationPanel, { summary: undefined, pending: false, onRun: () => {} }));
      expect(screen.getByText('未发现证据语义分类数据。')).toBeDefined();
    });
  });

  describe('FileClassificationBoard', () => {
    it('renders empty state when no board data', () => {
      render(createElement(FileClassificationBoard, { board: undefined }));
      expect(screen.getByText('文件分类数据暂不可用。')).toBeDefined();
    });

    it('renders groups and subcategory rows', () => {
      render(createElement(FileClassificationBoard, {
        board: {
          status: 'parsed',
          generatedAt: '2026-07-24T00:00:00Z',
          totalFiles: 1,
          totalSize: 4,
          magicClassifiedCount: 1,
          metadataClassifiedCount: 0,
          warnings: [],
          groups: [{
            category: 'documents',
            displayName: '文档',
            fileCount: 1,
            totalSize: 4,
            subcategories: [{
              name: 'PDF 文档',
              fileCount: 1,
              totalSize: 4,
              truncated: false,
              files: [{ fileId: 'f1', name: 'doc.pdf', path: '[P3]/x/doc.pdf', size: 4, magicType: 'PDF', classificationSource: 'magic' }],
            }],
          }],
        },
      }));
      expect(screen.getByText('文档')).toBeDefined();
      expect(screen.getByText('PDF 文档')).toBeDefined();
      expect(screen.getByText('doc.pdf')).toBeDefined();
    });
  });

  describe('AnalysisReportPanel', () => {
    it('renders download button', () => {
      render(createElement(AnalysisReportPanel, { pending: false, onDownload: () => {} }));
      expect(screen.getByText('生成分析报告')).toBeDefined();
      expect(screen.getByRole('button', { name: /下载 Markdown 报告/ })).toBeDefined();
    });
  });

  describe('AnalysisExtractionProgress', () => {
    it('renders nothing when progress is undefined', () => {
      const { container } = render(createElement(AnalysisExtractionProgress, { progress: undefined }));
      expect(container.firstChild).toBeNull();
    });

    it('renders progress info when provided', () => {
      const progress = {
        label: 'Registry',
        status: 'success' as const,
        scannedCount: 10,
        artifactCount: 5,
        timelineEventCount: 3,
        warnings: [],
      };
      render(createElement(AnalysisExtractionProgress, { progress }));
      expect(screen.getByText('Registry')).toBeDefined();
      expect(screen.getByText('scanned=10')).toBeDefined();
      expect(screen.getByText('artifacts=5')).toBeDefined();
      expect(screen.getByText('timeline=3')).toBeDefined();
    });

    it('renders the real processed-to-total ratio instead of a fixed running value', () => {
      render(createElement(AnalysisExtractionProgress, {
        progress: {
          label: 'Linux journal',
          status: 'running',
          scannedCount: 414,
          artifactCount: 120,
          timelineEventCount: 42,
          warnings: [],
          totalCandidateCount: 749,
          processedCandidateCount: 414,
        },
      }));

      expect(screen.getByText('414/749 (55%)')).toBeDefined();
      expect(screen.queryByText('50%')).toBeNull();
    });
  });

  describe('AnalysisEmptyState', () => {
    it('renders empty state message', () => {
      render(createElement(AnalysisEmptyState));
      expect(screen.getByText('请先创建或打开案件')).toBeDefined();
      expect(screen.queryByRole('button', { name: /加载演示案件/ })).toBeNull();
    });
  });

  describe('AnalysisErrorBanner', () => {
    it('renders error message and retry button', () => {
      render(createElement(AnalysisErrorBanner, { message: 'Something failed', onRetry: () => {} }));
      expect(screen.getByText('Something failed')).toBeDefined();
      expect(screen.getByRole('button', { name: '重试' })).toBeDefined();
    });
  });

  describe('AnalysisLoadingPanel', () => {
    it('renders loading text', () => {
      render(createElement(AnalysisLoadingPanel, { text: '正在加载...' }));
      expect(screen.getByText('正在加载...')).toBeDefined();
    });
  });

  describe('AnalysisHeader', () => {
    it('renders header with buttons', () => {
      render(
        createElement(AnalysisHeader, {
          loading: false,
          hasCase: false,
          extractionPending: false,
          onRefresh: () => {},
          onRunExtraction: () => {},
        }),
      );
      expect(screen.getByText('数据源分析')).toBeDefined();
      expect(screen.queryByRole('button', { name: /加载演示案件/ })).toBeNull();
      expect(screen.getByRole('button', { name: /刷新/ })).toBeDefined();
      expect(screen.getByRole('button', { name: /运行提取/ })).toBeDefined();
    });

    it('asks for a data source before running source-bound analysis', () => {
      render(
        createElement(AnalysisHeader, {
          loading: false,
          hasCase: true,
          extractionPending: false,
          onRefresh: () => {},
          onRunExtraction: () => {},
        }),
      );
      expect(screen.getByText(/左侧数据源树选择一个来源/)).toBeDefined();
      expect(screen.getByRole('button', { name: /刷新/ }).hasAttribute('disabled')).toBe(true);
      expect(screen.getByRole('button', { name: /运行提取/ }).hasAttribute('disabled')).toBe(true);
    });

    it('shows source-bound analysis notice once a data source is selected', () => {
      render(
        createElement(AnalysisHeader, {
          loading: false,
          hasCase: true,
          extractionPending: false,
          onRefresh: () => {},
          onRunExtraction: () => {},
          selectedDataSourceId: 'ds-1',
        }),
      );
      expect(screen.getByText(/分析结果绑定当前数据源/)).toBeDefined();
      expect(screen.getByRole('button', { name: /刷新/ }).hasAttribute('disabled')).toBe(false);
      expect(screen.getByRole('button', { name: /运行提取/ }).hasAttribute('disabled')).toBe(false);
    });
  });
});
