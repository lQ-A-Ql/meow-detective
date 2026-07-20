import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DataAnalysis } from './DataAnalysis';
import { useAnalysisStore } from '@/stores/analysis-store';
import type { AnalysisExtractionSectionRun } from '@/types/analysis';
import type { DataSourceSummary } from '@/types/models';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  systemInfo: vi.fn(),
  evidenceSummary: vi.fn(),
  evidenceScan: vi.fn(),
  extractionRun: vi.fn(),
  registrySummary: vi.fn(),
  registryStructured: vi.fn(),
  browserSummary: vi.fn(),
  emailSummary: vi.fn(),
  eventLogSummary: vi.fn(),
  linuxSummary: vi.fn(),
  classifications: vi.fn(),
  summaryMutation: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useDataSources: mocks.dataSources,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useAnalysisSystemInfo: mocks.systemInfo,
  useEvidenceClassificationSummary: mocks.evidenceSummary,
  useRunEvidenceClassification: mocks.evidenceScan,
  useRunAnalysisExtraction: mocks.extractionRun,
  useRegistryExtractionSummary: mocks.registrySummary,
  useRegistryStructuredSummary: mocks.registryStructured,
  useBrowserHistorySummary: mocks.browserSummary,
  useEmailExtractionSummary: mocks.emailSummary,
  useEvtxEventSummary: mocks.eventLogSummary,
  useLinuxArtifactSummary: mocks.linuxSummary,
  useAnalysisClassifications: mocks.classifications,
  useGenerateAnalysisSummary: mocks.summaryMutation,
}));

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(DataAnalysis),
    ),
  );
}

function queryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function extractionSection(
  key: string,
  artifactCount: number,
  scannedCount = 8,
  timelineEventCount = 3,
): AnalysisExtractionSectionRun {
  return {
    key,
    label: key,
    status: 'parsed',
    scannedCount,
    artifactCount,
    timelineEventCount,
    warnings: [],
  };
}

const windowsDataSource: DataSourceSummary = {
  id: 'ds-win',
  name: 'Windows Evidence',
  kind: 'e01',
  sourcePath: 'E:\\cases\\windows.E01',
  importedAt: '2026-06-01T10:00:00Z',
  platform: 'windows',
  importState: 'ready',
  partitions: [
    {
      index: 0,
      name: 'C:',
      kindLabel: 'Basic data partition',
      status: 'supported',
      offset: 0,
      length: 1024,
      filesystem: 'NTFS',
    },
  ],
};

const linuxDataSource: DataSourceSummary = {
  id: 'ds-linux',
  name: 'Linux Server',
  kind: 'e01',
  sourcePath: 'E:\\cases\\linux.E01',
  importedAt: '2026-06-02T10:00:00Z',
  platform: 'linux',
  importState: 'ready',
  partitions: [
    {
      index: 1,
      name: 'root',
      kindLabel: 'Linux LVM root',
      status: 'supported',
      offset: 0,
      length: 2048,
      filesystem: 'XFS',
    },
  ],
};

describe('DataAnalysis page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAnalysisStore.getState().reset();
    mocks.currentCase.mockReturnValue(queryState({
      data: { id: 'case-1', name: 'Case 1' },
    }));
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource],
    }));
    mocks.systemInfo.mockReturnValue(queryState({
      data: {
        computerName: 'BETA-LAB',
        osVersion: 'Windows Evidence Edition 24H2',
        buildNumber: '26000',
        networkAdapters: [],
        bootHistory: [
          {
            timestamp: '2026-06-01T08:15:00Z',
            bootType: 'eventLogStarted',
            source: 'Windows/System32/winevt/Logs/System.evtx',
            eventId: 6005,
            recordId: 42,
            note: 'EventLog 6005 candidate; indicates the Event Log service started, not a direct boot assertion.',
            provenance: {
              dataSourceId: 'ds-1',
              artifactPath: 'Windows/System32/winevt/Logs/System.evtx',
              parser: 'evtx.boot_shutdown',
              parsedAt: '2026-06-01T10:00:00Z',
              status: 'parsed',
              warnings: [],
            },
          },
        ],
        status: 'parsed',
        warnings: ['开关机历史来自 EVTX EventLog/User32 candidate events。'],
        provenance: [
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'parsed',
            warnings: [],
          },
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SOFTWARE',
            parser: 'registry.software',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'parsed',
            warnings: [],
          },
        ],
        fieldProvenance: [
          {
            field: 'computerName',
            valueName: 'ComputerName',
            keyPath: 'ControlSet001\\Control\\ComputerName\\ComputerName',
            hivePath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
          },
          {
            field: 'osVersion',
            valueName: 'ProductName',
            keyPath: 'Microsoft\\Windows NT\\CurrentVersion',
            hivePath: 'Windows/System32/config/SOFTWARE',
            parser: 'registry.software',
          },
        ],
      },
    }));
    mocks.evidenceSummary.mockReturnValue(queryState({
      data: {
        status: 'candidateFound',
        generatedAt: '2026-06-01T10:00:00Z',
        warnings: [],
        totals: {
          categoryCount: 3,
          candidateFileCount: 4,
          totalSize: 8192,
          artifactCount: 1,
        },
        categories: [
          {
            category: 'SystemInformation',
            displayName: '系统信息',
            status: 'parsed',
            fileCount: 2,
            totalSize: 4096,
            artifactCount: 1,
            confidence: 0.95,
            warnings: [],
            sources: [
              {
                fileId: 'system',
                path: 'Windows/System32/config/SYSTEM',
                size: 2048,
                evidenceKind: 'registry_hive',
                parser: 'registry.system_info',
                status: 'parsed',
                artifactCount: 1,
                warnings: [],
              },
            ],
            provenance: [],
          },
          {
            category: 'EventLogs',
            displayName: '事件日志',
            status: 'candidateFound',
            fileCount: 1,
            totalSize: 4096,
            artifactCount: 0,
            confidence: 0.65,
            warnings: ['已发现候选文件；尚未运行证据分类解析。'],
            sources: [
              {
                fileId: 'evtx',
                path: 'Windows/System32/winevt/Logs/System.evtx',
                size: 4096,
                evidenceKind: 'event_log',
                parser: 'evtx.boot_shutdown',
                status: 'candidateFound',
                artifactCount: 0,
                warnings: [],
              },
            ],
            provenance: [],
          },
        ],
      },
    }));
    mocks.evidenceScan.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync: vi.fn().mockResolvedValue({}),
      reset: vi.fn(),
    });
    mocks.extractionRun.mockReturnValue({
      data: undefined,
      error: null,
      isPending: false,
      mutateAsync: vi.fn().mockResolvedValue({
        status: 'parsed',
        scannedCount: 8,
        checkpointHitCount: 0,
        artifactCount: 7,
        timelineEventCount: 3,
        generatedAt: '2026-06-01T10:15:00Z',
        warnings: [],
      }),
      reset: vi.fn(),
    });
    mocks.registrySummary.mockReturnValue(queryState({
      data: {
        status: 'parsed',
        total: 2,
        generatedAt: '2026-06-01T10:10:00Z',
        warnings: [],
        values: [
          {
            artifactId: 'reg-1',
            fileId: 'file-system',
            sourcePath: 'Windows/System32/config/SYSTEM',
            hivePath: 'SYSTEM',
            keyPath: 'ControlSet001\\Control\\ComputerName\\ComputerName',
            valueName: 'ComputerName',
            valueType: 'REG_SZ',
            data: 'BETA-LAB',
            parser: 'registry.key_values',
            createdAt: '2026-06-01T10:10:00Z',
          },
          {
            artifactId: 'reg-2',
            fileId: 'file-software',
            sourcePath: 'Windows/System32/config/SOFTWARE',
            hivePath: 'SOFTWARE',
            keyPath: 'Microsoft\\Windows NT\\CurrentVersion',
            valueName: 'ProductName',
            valueType: 'REG_SZ',
            data: 'Windows Evidence Edition 24H2',
            parser: 'registry.key_values',
            createdAt: '2026-06-01T10:10:00Z',
          },
        ],
      },
    }));
    mocks.registryStructured.mockReturnValue(queryState());
    mocks.browserSummary.mockReturnValue(queryState({
      data: {
        status: 'parsed',
        visitTotal: 3,
        downloadTotal: 1,
        cookieTotal: 0,
        sessionTotal: 0,
        passwordTotal: 0,
        generatedAt: '2026-06-01T10:12:00Z',
        warnings: [],
        visits: [
          {
            artifactId: 'visit-1',
            fileId: 'file-chrome-history',
            sourcePath: 'Users/Admin/AppData/Local/Google/Chrome/User Data/Default/History',
            browser: 'Chrome',
            profile: 'Default',
            url: 'https://example.com/incident-playbook',
            title: 'Incident Response Playbook',
            visitTime: '2026-05-11T13:58:00Z',
            visitCount: 3,
          },
          {
            artifactId: 'visit-2',
            fileId: 'file-edge-history',
            sourcePath: 'Users/Admin/AppData/Local/Microsoft/Edge/User Data/Profile 1/History',
            browser: 'Edge',
            profile: 'Profile 1',
            url: 'https://login.microsoftonline.com/',
            title: 'Sign in to your account',
            visitTime: '2026-05-11T14:02:10Z',
            visitCount: 2,
          },
          {
            artifactId: 'visit-3',
            fileId: 'file-firefox-places',
            sourcePath: 'Users/Admin/AppData/Roaming/Mozilla/Firefox/Profiles/abcd.default/places.sqlite',
            browser: 'Firefox',
            profile: 'abcd.default',
            url: 'https://developer.mozilla.org/',
            title: 'MDN Web Docs',
            visitTime: '2026-05-11T14:08:40Z',
            visitCount: 1,
          },
        ],
        downloads: [
          {
            artifactId: 'download-1',
            fileId: 'file-chrome-history',
            sourcePath: 'Users/Admin/AppData/Local/Google/Chrome/User Data/Default/History',
            browser: 'Chrome',
            profile: 'Default',
            url: 'https://example.com/tools/triage.zip',
            targetPath: 'C:/Users/Admin/Downloads/triage.zip',
            startTime: '2026-05-11T14:18:00Z',
            totalBytes: 7340032,
          },
        ],
        cookies: [],
        sessions: [],
        passwords: [],
      },
    }));
    mocks.emailSummary.mockReturnValue(queryState({
      data: {
        status: 'parsed',
        total: 2,
        generatedAt: '2026-06-01T10:14:00Z',
        warnings: [],
        messages: [
          {
            artifactId: 'mail-1',
            fileId: 'file-mail-1',
            sourcePath: 'Users/Admin/Documents/incident-response.eml',
            sentAt: '2026-05-11T12:40:00Z',
            from: 'alice@example.com',
            to: ['dfir@example.com'],
            cc: ['lead@example.com'],
            bcc: ['hidden@example.com'],
            replyTo: 'reply@example.com',
            returnPath: '<alice@example.com>',
            subject: 'Initial triage notes',
            messageId: '<mock-incident-1@example.com>',
            inReplyTo: '<parent@example.com>',
            references: ['<parent@example.com>'],
            attachments: ['triage.csv'],
            attachmentDetails: [
              {
                fileName: 'triage.csv',
                size: 1024,
                mimeType: 'text/csv',
                contentId: '<att-1>',
              },
            ],
            headers: [
              { name: 'From', value: 'alice@example.com' },
              { name: 'Subject', value: 'Initial triage notes' },
            ],
            bodyPreview: 'Please review the initial triage notes.',
            bodyPlain: 'Please review the initial triage notes.',
            bodyHtml: '<p>Please review the initial triage notes.</p>',
            xMailer: 'TestMailer/1.0',
            xOriginatingIp: '192.168.1.1',
            attachmentCount: 1,
            isDeleted: false,
          },
          {
            artifactId: 'mail-2',
            fileId: 'file-mail-2',
            sourcePath: 'Users/Admin/Documents/forwarded-alert.emlx',
            sentAt: '2026-05-11T13:05:00Z',
            from: 'soc@example.com',
            to: ['admin@example.com'],
            cc: [],
            bcc: [],
            subject: 'Forwarded security alert',
            messageId: '<mock-alert-2@example.com>',
            references: [],
            attachments: [],
            attachmentDetails: [],
            headers: [],
            bodyPreview: 'Endpoint alert was forwarded from the SOC queue.',
            attachmentCount: 0,
            isDeleted: false,
          },
        ],
      },
    }));
    mocks.eventLogSummary.mockReturnValue(queryState({
      data: {
        status: 'parsed',
        bootShutdownCount: 1,
        logonLogoffCount: 0,
        privilegeEscalationCount: 0,
        processExecutionCount: 0,
        accountManagementCount: 0,
        scheduledTaskCount: 0,
        applicationCrashCount: 0,
        softwareInstallationCount: 0,
        otherCount: 0,
        totalCount: 1,
        bootEvents: [
          { eventId: 6005, kind: 'boot', timestamp: '2026-06-01T08:00:00Z', provider: 'EventLog', recordId: 1, sourcePath: 'System.evtx', note: 'System started' },
        ],
        securityEvents: [],
        applicationEvents: [],
        warnings: [],
        generatedAt: '2026-06-01T10:14:00Z',
      },
    }));
    mocks.linuxSummary.mockReturnValue(queryState({
      data: {
        status: 'notFound',
        journalCount: 0,
        loginCount: 0,
        bashCommandCount: 0,
        aptEventCount: 0,
        cronJobCount: 0,
        sudoEventCount: 0,
        systemConfigCount: 0,
        webSiteCount: 0,
        webAccessLogCount: 0,
        webErrorLogCount: 0,
        webFindingCount: 0,
        mysqlConfigCount: 0,
        mysqlLogCount: 0,
        mysqlFindingCount: 0,
        totalCount: 0,
        truncated: false,
        coverageRatio: 0,
        journalEntries: [],
        loginRecords: [],
        bashCommands: [],
        aptEvents: [],
        cronJobs: [],
        sudoEvents: [],
        systemConfigs: [],
        warnings: [],
        generatedAt: '2026-06-01T10:14:00Z',
      },
    }));
    mocks.classifications.mockReturnValue(queryState({
      data: [
        {
          category: 'Documents',
          fileCount: 1,
          totalSize: 4,
          status: 'parsed',
          warnings: [],
          files: [
            {
              fileId: 'file-1',
              path: 'doc.pdf',
              name: 'doc.pdf',
              size: 4,
              fileType: 'PDF',
              magicDescription: 'PDF Document',
              provenance: {
                dataSourceId: 'ds-1',
                artifactPath: 'doc.pdf',
                parser: 'metadata.extension_path',
                parsedAt: '2026-06-01T10:00:00Z',
                status: 'parsed',
                warnings: [],
              },
            },
          ],
          provenance: [
            {
              dataSourceId: 'ds-1',
              artifactPath: 'doc.pdf',
              parser: 'metadata.extension_path',
              parsedAt: '2026-06-01T10:00:00Z',
              status: 'parsed',
              warnings: [],
            },
          ],
        },
      ],
    }));
    mocks.summaryMutation.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync: vi.fn().mockResolvedValue('# 数据源分析报告'),
      reset: vi.fn(),
    });

    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:analysis'),
      revokeObjectURL: vi.fn(),
    });
  });

  it('shows empty state and does not render errors when no case is active', () => {
    mocks.currentCase.mockReturnValue(queryState({ data: null }));

    renderPage();

    expect(screen.getByText('请先创建或打开案件')).toBeDefined();
    expect(screen.queryByRole('button', { name: /加载演示案件/ })).toBeNull();
    expect(screen.queryByText('正在分析数据源...')).toBeNull();
  });

  it('only mounts the active tab content on initial render', () => {
    renderPage();

    expect(screen.getByLabelText('Windows Evidence / 系统信息')).toBeDefined();

    const panels = screen.queryAllByRole('tabpanel', { hidden: true });
    const activePanels = panels.filter((panel) => panel.getAttribute('data-state') === 'active');
    const inactivePanels = panels.filter((panel) => panel.getAttribute('data-state') === 'inactive');

    expect(activePanels.length).toBeGreaterThanOrEqual(1);
    expect(activePanels.some((panel) => panel.textContent?.includes('BETA-LAB'))).toBe(true);
    expect(inactivePanels.length).toBeGreaterThan(0);
    expect(inactivePanels.every((panel) => panel.textContent === '')).toBe(true);
  });

  it('renders the source tree and parsed registry facts with provenance', () => {
    renderPage();

    expect(screen.getByRole('complementary', { name: '数据源树' })).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 注册表')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 系统信息')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 证据分类')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 注册表')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 浏览器记录')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 邮件信息')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 事件日志')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 文件分类')).toBeDefined();
    expect(screen.getByLabelText('Windows Evidence / 分析报告')).toBeDefined();
    expect(screen.getAllByText('已解析').length).toBeGreaterThan(0);
    expect(screen.getAllByText('BETA-LAB').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Windows Evidence Edition 24H2').length).toBeGreaterThan(0);
    expect(screen.getAllByText('registry.system').length).toBeGreaterThan(0);
    expect(screen.getAllByText('registry.software').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Windows/System32/config/SYSTEM').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Windows/System32/config/SOFTWARE').length).toBeGreaterThan(0);
    expect(screen.getByText('字段级来源')).toBeDefined();
    expect(screen.getByText('computerName')).toBeDefined();
    expect(screen.getAllByText('ProductName').length).toBeGreaterThan(0);
    expect(screen.getByText('EventID 6005')).toBeDefined();
    expect(screen.getByText(/EventLog 6005 candidate/)).toBeDefined();
    expect(screen.getAllByText(/evtx\.boot_shutdown/).length).toBeGreaterThan(0);
    expect(screen.queryByText('FORENSICS-PC')).toBeNull();
    expect(screen.queryByText('Windows 10')).toBeNull();
  });

  it('collapses and expands an individual data source tree', () => {
    renderPage();

    const source = screen.getByLabelText('Windows Evidence');
    expect(source.getAttribute('aria-expanded')).toBe('true');
    fireEvent.click(source);
    expect(screen.queryByLabelText('Windows Evidence / 注册表')).toBeNull();
    expect(source.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(source);
    expect(screen.getByLabelText('Windows Evidence / 注册表')).toBeDefined();
    expect(source.getAttribute('aria-expanded')).toBe('true');
  });

  it('renders Linux artifacts as a separate platform view instead of a Windows tab', async () => {
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Linux Server'));

    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    expect(screen.getByLabelText('Linux Server / 概览')).toBeDefined();
    expect(screen.getByLabelText('Linux Server / 系统日志')).toBeDefined();
    expect(screen.getByLabelText('Linux Server / 登录记录')).toBeDefined();
    expect(screen.getByLabelText('Linux Server / 命令历史')).toBeDefined();
    expect(screen.getByLabelText('Linux Server / 软件包')).toBeDefined();
    expect(screen.getByLabelText('Linux Server / 定时任务')).toBeDefined();
    expect(screen.getByLabelText('Linux Server / Sudo')).toBeDefined();
    expect(screen.getByText('尚未从当前数据源发现或提取 Linux 痕迹。')).toBeDefined();

    fireEvent.click(screen.getByLabelText('Linux Server / 命令历史'));
    await waitFor(() => expect(screen.getByText('暂无 Shell 命令')).toBeDefined());
  });

  it('places Linux summary counts on their source tree nodes', async () => {
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));
    mocks.linuxSummary.mockReturnValue(queryState({
      data: {
        status: 'parsed',
        journalCount: 223,
        loginCount: 143,
        bashCommandCount: 50,
        aptEventCount: 0,
        cronJobCount: 0,
        sudoEventCount: 0,
        systemConfigCount: 2939,
        webSiteCount: 1,
        webAccessLogCount: 16,
        webErrorLogCount: 0,
        webFindingCount: 0,
        mysqlConfigCount: 1,
        mysqlLogCount: 474,
        mysqlFindingCount: 0,
        totalCount: 31232,
        truncated: false,
        coverageRatio: 1,
        journalEntries: [],
        loginRecords: [],
        bashCommands: [],
        aptEvents: [],
        cronJobs: [],
        sudoEvents: [],
        systemConfigs: [],
        webSites: [],
        webAccessLogs: [],
        webErrorLogs: [],
        webFindings: [],
        mysqlConfigs: [],
        mysqlLogs: [],
        mysqlFindings: [],
        warnings: [],
        generatedAt: '2026-07-20T03:09:47Z',
      },
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Linux Server'));

    await waitFor(() => expect(screen.getByText('概览(31232)')).toBeDefined());
    expect(screen.getByText('系统日志(223)')).toBeDefined();
    expect(screen.queryByText('31232')).toBeNull();
  });

  it('renders evidence semantic classification and can start targeted scan', async () => {
    const mutateAsync = vi.fn().mockResolvedValue({});
    mocks.evidenceScan.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });

    renderPage();
    fireEvent.click(screen.getByLabelText('Windows Evidence / 证据分类'));

    await waitFor(() => expect(screen.getByText('证据语义分类')).toBeDefined());
    expect(screen.getAllByText('系统信息').length).toBeGreaterThan(0);
    expect(screen.getAllByText('事件日志').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Windows/System32/winevt/Logs/System.evtx').length).toBeGreaterThan(0);
    expect(screen.getAllByText('已发现候选').length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole('button', { name: /开始证据分类/ }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledWith({
      dataSourceId: 'ds-win',
      categories: [],
    }));
  });

  it('runs only Windows extraction categories from the Windows view', async () => {
    const mutateAsync = vi.fn().mockResolvedValue({
      status: 'parsed',
      scannedCount: 8,
      checkpointHitCount: 0,
      artifactCount: 7,
      timelineEventCount: 3,
      warnings: [],
    });
    mocks.extractionRun.mockReturnValue({
      data: undefined,
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });

    renderPage();
    fireEvent.click(screen.getByRole('button', { name: /运行提取/ }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(4));
    expect(mutateAsync).toHaveBeenNthCalledWith(1, { dataSourceId: 'ds-win', categories: ['Registry'] });
    expect(mutateAsync).toHaveBeenNthCalledWith(2, { dataSourceId: 'ds-win', categories: ['BrowserHistory'] });
    expect(mutateAsync).toHaveBeenNthCalledWith(3, { dataSourceId: 'ds-win', categories: ['Email'] });
    expect(mutateAsync).toHaveBeenNthCalledWith(4, { dataSourceId: 'ds-win', categories: ['EventLogs'] });
    expect(mutateAsync).not.toHaveBeenCalledWith({ dataSourceId: 'ds-win', categories: ['LinuxArtifacts'] });
  });

  it('runs only Linux extraction categories from the Linux view', async () => {
    const mutateAsync = vi.fn().mockResolvedValue({
      status: 'parsed',
      scannedCount: 8,
      checkpointHitCount: 0,
      artifactCount: 7,
      timelineEventCount: 3,
      warnings: [],
    });
    mocks.extractionRun.mockReturnValue({
      data: undefined,
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });

    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Linux Server'));
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /运行提取/ }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(mutateAsync).toHaveBeenCalledWith({ dataSourceId: 'ds-linux', categories: ['LinuxArtifacts'] });
  });

  it('tracks Windows per-section extraction progress for the bottom drawer', async () => {
    const sectionArtifactCounts: Record<string, number> = {
      Registry: 1,
      BrowserHistory: 2,
      Email: 3,
      EventLogs: 4,
    };
    const mutateAsync = vi.fn().mockImplementation(async (request: { categories: string[] }) => {
      const key = request.categories[0];
      return {
        status: 'parsed',
        scannedCount: 8,
        checkpointHitCount: 0,
        artifactCount: 158,
        timelineEventCount: 3,
        sections: [extractionSection(key, sectionArtifactCounts[key])],
        generatedAt: '2026-06-01T10:15:00Z',
        warnings: [],
      };
    });
    mocks.extractionRun.mockReturnValue({
      data: undefined,
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });

    renderPage();

    fireEvent.click(screen.getByRole('button', { name: /运行提取/ }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(4));
    await waitFor(() => expect(useAnalysisStore.getState().extractionProgress.EventLogs.status).toBe('success'));
    expect(useAnalysisStore.getState().extractionProgress.Registry.artifactCount).toBe(1);
    expect(useAnalysisStore.getState().extractionProgress.BrowserHistory.artifactCount).toBe(2);
    expect(useAnalysisStore.getState().extractionProgress.Email.artifactCount).toBe(3);
    expect(useAnalysisStore.getState().extractionProgress.EventLogs.artifactCount).toBe(4);
    expect(screen.getByLabelText('Windows Evidence / 注册表').textContent).toContain('1');
  });

  it('tracks independent Linux extraction progress for the bottom drawer', async () => {
    const linuxSections = [
      extractionSection('LinuxJournal', 1),
      extractionSection('LinuxLogin', 2),
      extractionSection('LinuxCommands', 3),
      extractionSection('LinuxPackages', 4),
      extractionSection('LinuxCron', 5),
      extractionSection('LinuxSudo', 6),
      extractionSection('LinuxSystemConfig', 7),
      extractionSection('LinuxWebServices', 8),
      extractionSection('LinuxMysqlServices', 9),
      extractionSection('UnknownCapability', 999),
    ];
    const mutateAsync = vi.fn().mockResolvedValue({
      status: 'parsed',
      scannedCount: 72,
      checkpointHitCount: 0,
      artifactCount: 158,
      timelineEventCount: 27,
      sections: linuxSections,
      generatedAt: '2026-06-01T10:15:00Z',
      warnings: [],
    });
    mocks.extractionRun.mockReturnValue({
      data: undefined,
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Linux Server'));
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());

    fireEvent.click(screen.getByRole('button', { name: /运行提取/ }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useAnalysisStore.getState().extractionProgress.LinuxMysqlServices.status).toBe('success'));
    expect(useAnalysisStore.getState().extractionProgress.LinuxJournal.artifactCount).toBe(1);
    expect(useAnalysisStore.getState().extractionProgress.LinuxLogin.artifactCount).toBe(2);
    expect(useAnalysisStore.getState().extractionProgress.LinuxCommands.artifactCount).toBe(3);
    expect(useAnalysisStore.getState().extractionProgress.LinuxMysqlServices.artifactCount).toBe(9);
  });

  it('switches to the matching platform view when selecting a data source', async () => {
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Linux Server'));

    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    expect(screen.queryByRole('tab', { name: /系统信息/ })).toBeNull();
  });

  it('navigates to a source child from the sidebar tree', async () => {
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Linux Server / 系统日志'));

    await waitFor(() => expect(useAnalysisStore.getState().selectedDataSourceId).toBe('ds-linux'));
    expect(screen.getByLabelText('Linux Server / 系统日志').getAttribute('aria-current')).toBe('true');
  });

  it('only selects and displays ready data sources with a supported platform', async () => {
    const pendingSource = {
      ...windowsDataSource,
      id: 'ds-pending',
      name: 'Pending Windows',
      importState: 'pending',
    } satisfies DataSourceSummary;
    const failedSource = {
      ...linuxDataSource,
      id: 'ds-failed',
      name: 'Failed Linux',
      importState: 'failed',
    } satisfies DataSourceSummary;
    const unsupportedSource = {
      ...windowsDataSource,
      id: 'ds-unsupported',
      name: 'Unsupported Platform',
      platform: 'macos' as DataSourceSummary['platform'],
    } satisfies DataSourceSummary;
    mocks.dataSources.mockReturnValue(queryState({
      data: [pendingSource, failedSource, unsupportedSource, linuxDataSource],
    }));

    renderPage();

    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    expect(screen.getByLabelText('Linux Server')).toBeDefined();
    expect(screen.queryByLabelText('Pending Windows')).toBeNull();
    expect(screen.queryByLabelText('Failed Linux')).toBeNull();
    expect(screen.queryByLabelText('Unsupported Platform')).toBeNull();
    expect(useAnalysisStore.getState().selectedDataSourceId).toBe('ds-linux');
  });

  it.each(['evidence', 'extraction', 'summary'] as const)(
    'prevents data-source switching while the %s mutation is pending',
    async (mutation) => {
      const pendingMutation = {
        data: undefined,
        error: null,
        isPending: true,
        mutateAsync: vi.fn(),
        reset: vi.fn(),
      };
      if (mutation === 'evidence') {
        mocks.evidenceScan.mockReturnValue(pendingMutation);
      } else if (mutation === 'extraction') {
        mocks.extractionRun.mockReturnValue(pendingMutation);
      } else {
        mocks.summaryMutation.mockReturnValue(pendingMutation);
      }
      mocks.dataSources.mockReturnValue(queryState({
        data: [windowsDataSource, linuxDataSource],
      }));

      renderPage();
      await waitFor(() => expect(screen.getByLabelText('Windows Evidence')).toBeDefined());

      const linuxSelector = screen.getByLabelText('Linux Server');
      expect((linuxSelector as HTMLButtonElement).disabled).toBe(true);
      fireEvent.click(linuxSelector);
      expect(useAnalysisStore.getState().selectedDataSourceId).toBe('ds-win');
      expect(screen.queryByText('Linux 痕迹分析')).toBeNull();
    },
  );

  it('resets source-bound mutation state when switching data sources', async () => {
    const evidenceReset = vi.fn();
    const extractionReset = vi.fn();
    const summaryReset = vi.fn();
    mocks.evidenceScan.mockReturnValue({
      error: new Error('old evidence error'),
      isPending: false,
      mutateAsync: vi.fn(),
      reset: evidenceReset,
    });
    mocks.extractionRun.mockReturnValue({
      data: {
        status: 'failed',
        scannedCount: 0,
        checkpointHitCount: 0,
        artifactCount: 0,
        timelineEventCount: 0,
        generatedAt: '2026-06-01T10:15:00Z',
        warnings: [],
      },
      error: new Error('old extraction error'),
      isPending: false,
      mutateAsync: vi.fn(),
      reset: extractionReset,
    });
    mocks.summaryMutation.mockReturnValue({
      data: 'old report',
      error: new Error('old summary error'),
      isPending: false,
      mutateAsync: vi.fn(),
      reset: summaryReset,
    });
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    await waitFor(() => expect(screen.getByLabelText('Windows Evidence')).toBeDefined());
    evidenceReset.mockClear();
    extractionReset.mockClear();
    summaryReset.mockClear();

    fireEvent.click(screen.getByLabelText('Linux Server'));
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    expect(evidenceReset).toHaveBeenCalledTimes(1);
    expect(extractionReset).toHaveBeenCalledTimes(1);
    expect(summaryReset).toHaveBeenCalledTimes(1);
  });

  it('does not refetch the old evidence source after its generation changes', async () => {
    const scan = deferred<Record<string, never>>();
    const evidenceRefetch = vi.fn().mockResolvedValue(undefined);
    const mutateAsync = vi.fn(() => scan.promise);
    mocks.evidenceSummary.mockReturnValue(queryState({ refetch: evidenceRefetch }));
    mocks.evidenceScan.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Windows Evidence / 证据分类'));
    fireEvent.click(await screen.findByRole('button', { name: /开始证据分类/ }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));

    act(() => useAnalysisStore.getState().setSelectedDataSourceId('ds-linux'));
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    await act(async () => {
      scan.resolve({});
      await scan.promise;
    });

    expect(evidenceRefetch).not.toHaveBeenCalled();
  });

  it('does not apply stale extraction progress to a newly selected source', async () => {
    const run = deferred<{
      status: 'parsed';
      scannedCount: number;
      checkpointHitCount: number;
      artifactCount: number;
      timelineEventCount: number;
      generatedAt: string;
      warnings: string[];
      sections: AnalysisExtractionSectionRun[];
    }>();
    const mutateAsync = vi.fn(() => run.promise);
    mocks.extractionRun.mockReturnValue({
      data: undefined,
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    await waitFor(() => expect(screen.getByLabelText('Windows Evidence')).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /运行提取/ }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));

    act(() => useAnalysisStore.getState().setSelectedDataSourceId('ds-linux'));
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    await act(async () => {
      run.resolve({
        status: 'parsed',
        scannedCount: 99,
        checkpointHitCount: 0,
        artifactCount: 99,
        timelineEventCount: 99,
        generatedAt: '2026-06-03T10:00:00Z',
        warnings: [],
        sections: [extractionSection('Registry', 99)],
      });
      await run.promise;
    });

    expect(mutateAsync).toHaveBeenCalledTimes(1);
    expect(useAnalysisStore.getState().extractionProgress.Registry.artifactCount).toBe(0);
  });

  it('does not download a summary produced for an obsolete source generation', async () => {
    const summary = deferred<string>();
    const mutateAsync = vi.fn(() => summary.promise);
    mocks.summaryMutation.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });
    mocks.dataSources.mockReturnValue(queryState({
      data: [windowsDataSource, linuxDataSource],
    }));

    renderPage();
    fireEvent.click(screen.getByLabelText('Windows Evidence / 分析报告'));
    fireEvent.click(await screen.findByRole('button', { name: /下载 Markdown 报告/ }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));

    act(() => useAnalysisStore.getState().setSelectedDataSourceId('ds-linux'));
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    await act(async () => {
      summary.resolve('# obsolete report');
      await summary.promise;
    });

    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });

  it('refreshes only Windows queries for a persisted Windows source', async () => {
    const systemRefetch = vi.fn().mockResolvedValue(undefined);
    const evidenceRefetch = vi.fn().mockResolvedValue(undefined);
    const registryRefetch = vi.fn().mockResolvedValue(undefined);
    const registryStructuredRefetch = vi.fn().mockResolvedValue(undefined);
    const browserRefetch = vi.fn().mockResolvedValue(undefined);
    const emailRefetch = vi.fn().mockResolvedValue(undefined);
    const eventLogRefetch = vi.fn().mockResolvedValue(undefined);
    const classificationsRefetch = vi.fn().mockResolvedValue(undefined);
    const linuxRefetch = vi.fn().mockResolvedValue(undefined);
    mocks.systemInfo.mockReturnValue(queryState({ refetch: systemRefetch }));
    mocks.evidenceSummary.mockReturnValue(queryState({ refetch: evidenceRefetch }));
    mocks.registrySummary.mockReturnValue(queryState({ refetch: registryRefetch }));
    mocks.registryStructured.mockReturnValue(queryState({ refetch: registryStructuredRefetch }));
    mocks.browserSummary.mockReturnValue(queryState({ refetch: browserRefetch }));
    mocks.emailSummary.mockReturnValue(queryState({ refetch: emailRefetch }));
    mocks.eventLogSummary.mockReturnValue(queryState({ refetch: eventLogRefetch }));
    mocks.classifications.mockReturnValue(queryState({ refetch: classificationsRefetch }));
    mocks.linuxSummary.mockReturnValue(queryState({ refetch: linuxRefetch }));

    renderPage();
    await waitFor(() => expect(screen.getByLabelText('Windows Evidence')).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /刷新/ }));

    await waitFor(() => expect(systemRefetch).toHaveBeenCalledTimes(1));
    expect(evidenceRefetch).toHaveBeenCalledTimes(1);
    expect(registryRefetch).toHaveBeenCalledTimes(1);
    expect(registryStructuredRefetch).toHaveBeenCalledTimes(1);
    expect(browserRefetch).toHaveBeenCalledTimes(1);
    expect(emailRefetch).toHaveBeenCalledTimes(1);
    expect(eventLogRefetch).toHaveBeenCalledTimes(1);
    expect(classificationsRefetch).toHaveBeenCalledTimes(1);
    expect(linuxRefetch).not.toHaveBeenCalled();
  });

  it('refreshes and displays only Linux analysis for a persisted Linux source', async () => {
    const windowsRefetch = vi.fn().mockResolvedValue(undefined);
    const linuxRefetch = vi.fn().mockResolvedValue(undefined);
    mocks.dataSources.mockReturnValue(queryState({ data: [linuxDataSource] }));
    mocks.systemInfo.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.evidenceSummary.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.registrySummary.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.registryStructured.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.browserSummary.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.emailSummary.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.eventLogSummary.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.classifications.mockReturnValue(queryState({ refetch: windowsRefetch }));
    mocks.linuxSummary.mockReturnValue(queryState({ refetch: linuxRefetch }));

    renderPage();
    await waitFor(() => expect(screen.getByText('Linux 痕迹分析')).toBeDefined());
    expect(screen.queryByRole('tab', { name: /系统信息/ })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: /刷新/ }));
    await waitFor(() => expect(linuxRefetch).toHaveBeenCalledTimes(1));
    expect(windowsRefetch).not.toHaveBeenCalled();
  });

  it('renders registry, browser and email extraction tabs', async () => {
    renderPage();

    fireEvent.click(screen.getByLabelText('Windows Evidence / 注册表'));
    await waitFor(() => expect(screen.getAllByText('注册表提取').length).toBeGreaterThan(0));
    // registry panel renders sub-tabs for structured views
    expect(screen.getByText('用户账户')).toBeDefined();
    expect(screen.getByText('原始键值')).toBeDefined();

    fireEvent.click(screen.getByLabelText('Windows Evidence / 浏览器记录'));
    await waitFor(() => expect(screen.getAllByText('浏览器记录').length).toBeGreaterThan(0));
    expect(screen.getByText('Incident Response Playbook')).toBeDefined();
    expect(screen.getByText('Edge')).toBeDefined();
    expect(screen.getByText('Firefox')).toBeDefined();
    expect(screen.getByText('C:/Users/Admin/Downloads/triage.zip')).toBeDefined();

    fireEvent.click(screen.getByLabelText('Windows Evidence / 邮件信息'));
    await waitFor(() => expect(screen.getAllByText('邮件信息').length).toBeGreaterThan(0));
    expect(screen.getByText('Initial triage notes')).toBeDefined();
    expect(screen.getByText('alice@example.com')).toBeDefined();

    // Click the first email row to expand the detail card.
    fireEvent.click(screen.getByText('Initial triage notes'));
    await waitFor(() =>
      expect(screen.getByText('Message-ID:')).toBeDefined(),
    );
    expect(screen.getByText('<mock-incident-1@example.com>')).toBeDefined();
    expect(screen.getAllByText('lead@example.com').length).toBeGreaterThan(0);
    expect(screen.getAllByText('hidden@example.com').length).toBeGreaterThan(0);
    expect(screen.getAllByText('triage.csv').length).toBeGreaterThan(0);
    expect(screen.getByText('Reply-To:')).toBeDefined();
    expect(screen.getByText('reply@example.com')).toBeDefined();
    expect(screen.getByText('Bcc:')).toBeDefined();
    expect(screen.getByText('In-Reply-To:')).toBeDefined();
    expect(screen.getByText('X-Mailer:')).toBeDefined();
    expect(screen.getByText('192.168.1.1')).toBeDefined();
    expect(screen.getByText('纯文本')).toBeDefined();
    expect(screen.getByText('HTML')).toBeDefined();
  });

  it('renders notParsed system info without fake facts', () => {
    mocks.systemInfo.mockReturnValue(queryState({
      data: {
        networkAdapters: [],
        bootHistory: [],
        status: 'notParsed',
        warnings: ['Registry hive 缺失或损坏，系统字段未解析。'],
        provenance: [
          {
            dataSourceId: 'ds-1',
            artifactPath: 'Windows/System32/config/SYSTEM',
            parser: 'registry.system',
            parsedAt: '2026-06-01T10:00:00Z',
            status: 'notParsed',
            warnings: ['registry hive shorter than base block'],
          },
        ],
        fieldProvenance: [],
      },
    }));

    renderPage();

    expect(screen.getAllByText('未解析').length).toBeGreaterThan(0);
    expect(screen.getByText('registry hive shorter than base block')).toBeDefined();
    expect(screen.getByText('字段级 Registry provenance 暂不可用。')).toBeDefined();
    expect(screen.queryByText('FORENSICS-PC')).toBeNull();
    expect(screen.queryByText('Windows 10')).toBeNull();
  });

  it('renders file classifications from hook data', async () => {
    renderPage();
    fireEvent.click(screen.getByLabelText('Windows Evidence / 文件分类'));

    await waitFor(() => expect(screen.getByText('Documents')).toBeDefined());
    expect(screen.getAllByText('doc.pdf').length).toBeGreaterThan(0);
    expect(screen.getByText('PDF Document')).toBeDefined();
    expect(screen.getAllByText(/metadata\.extension_path/).length).toBeGreaterThan(0);
  });

  it('downloads markdown report through summary mutation', async () => {
    const click = vi.fn();
    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tagName) => {
      const element = originalCreateElement(tagName);
      if (tagName === 'a') {
        element.click = click;
      }
      return element;
    });
    const mutateAsync = vi.fn().mockResolvedValue('# 数据源分析报告');
    mocks.summaryMutation.mockReturnValue({
      error: null,
      isPending: false,
      mutateAsync,
      reset: vi.fn(),
    });

    renderPage();
    fireEvent.click(screen.getByLabelText('Windows Evidence / 分析报告'));
    await waitFor(() => expect(screen.getByRole('button', { name: /下载 Markdown 报告/ })).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /下载 Markdown 报告/ }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(URL.createObjectURL).toHaveBeenCalled();
    expect(click).toHaveBeenCalledTimes(1);
  });
});
