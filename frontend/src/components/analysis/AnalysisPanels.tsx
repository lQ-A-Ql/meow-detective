import {
  Archive,
  Clock,
  Database,
  Download,
  FileText,
  Globe,
  HardDrive,
  Image,
  Mail,
  Monitor,
  Network,
  RefreshCw,
  Shield,
  Usb,
  Wifi,
} from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Progress } from '@/app/components/ui/progress';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  AnalysisExtractionRun,
  AnalysisFieldProvenance,
  AnalysisFileClassification,
  AnalysisProvenance,
  AnalysisSystemInfo,
  BrowserDownload,
  BrowserHistorySummary,
  BrowserVisit,
  EmailExtractionSummary,
  EmailMessage,
  EvidenceClassificationSummary,
  InstalledSoftware,
  NetworkProfileEntry,
  RegistryExtractionSummary,
  RegistryHiveOverview,
  RegistryStructuredSummary,
  RegistryValue,
  SamUserAccount,
  UsbDeviceHistory,
  UserAssistEntry,
} from '@/types/models';

export type AnalysisExtractionProgressState = 'idle' | 'running' | 'success' | 'partial' | 'failed';

export interface AnalysisExtractionProgressInfo {
  label: string;
  status: AnalysisExtractionProgressState;
  scannedCount: number;
  artifactCount: number;
  timelineEventCount: number;
  warnings: string[];
  error?: string;
}

const CATEGORY_ICONS: Record<string, typeof Monitor> = {
  Executables: Shield,
  Documents: FileText,
  Images: Image,
  Archives: Archive,
  Databases: Database,
  System: HardDrive,
  Forensics: Monitor,
  Logs: FileText,
  Registry: Database,
  BrowserHistory: Globe,
  Email: Mail,
  Prefetch: Clock,
  Shortcuts: FileText,
  SystemInformation: Monitor,
  EventLogs: FileText,
  ProgramExecution: Shield,
  UserActivity: Clock,
  RecycleBin: Archive,
  Thumbnails: Image,
  ResourceUsage: Database,
  BrowserData: Globe,
  FileTypeInventory: FileText,
  Other: FileText,
};

const CATEGORY_COLORS: Record<string, string> = {
  Executables: '#b42318',
  Documents: '#175cd3',
  Images: '#027a48',
  Archives: '#b54708',
  Databases: '#6941c6',
  System: '#475467',
  Forensics: '#0e9384',
  Logs: '#344054',
  Registry: '#7a5af8',
  BrowserHistory: '#026aa2',
  Email: '#b54708',
  Prefetch: '#9a6700',
  Shortcuts: '#026aa2',
  SystemInformation: '#344054',
  EventLogs: '#175cd3',
  ProgramExecution: '#b42318',
  UserActivity: '#9a6700',
  RecycleBin: '#b54708',
  Thumbnails: '#027a48',
  ResourceUsage: '#6941c6',
  BrowserData: '#026aa2',
  FileTypeInventory: '#667085',
  Other: '#667085',
};

export function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function statusLabel(status: string) {
  switch (status) {
    case 'parsed':
      return '已解析';
    case 'notParsed':
      return '未解析';
    case 'unavailable':
      return '不可用';
    case 'partial':
      return '部分解析';
    case 'candidateFound':
      return '已发现候选';
    case 'notFound':
      return '未发现';
    case 'failed':
      return '解析失败';
    default:
      return status;
  }
}

function extractionProgressLabel(status: AnalysisExtractionProgressState) {
  switch (status) {
    case 'running':
      return '运行中';
    case 'success':
      return '已完成';
    case 'partial':
      return '部分完成';
    case 'failed':
      return '失败';
    case 'idle':
    default:
      return '等待';
  }
}

export function AnalysisExtractionProgress({
  progress,
}: {
  progress?: AnalysisExtractionProgressInfo;
}) {
  if (!progress) {
    return null;
  }
  const value = progress.status === 'running'
    ? 50
    : progress.status === 'idle'
      ? 0
      : 100;
  const tone = progress.status === 'failed'
    ? 'border-red-200 bg-red-50 text-red-700'
    : progress.status === 'partial'
      ? 'border-amber-200 bg-amber-50 text-amber-800'
      : 'border-[#e0e0e0] bg-[#fcfcfc] text-[#555]';
  return (
    <div className={`rounded border px-3 py-2 ${tone}`}>
      <div className="mb-2 flex items-center justify-between gap-3 text-[11px]">
        <span className="font-semibold text-[#111]">{progress.label}</span>
        <span className="font-mono">{extractionProgressLabel(progress.status)}</span>
      </div>
      <Progress value={value} className="h-1.5 rounded-none bg-white" />
      <div className="mt-2 flex flex-wrap gap-3 font-mono text-[10px]">
        <span>scanned={progress.scannedCount}</span>
        <span>artifacts={progress.artifactCount}</span>
        <span>timeline={progress.timelineEventCount}</span>
      </div>
      {progress.error ? (
        <div className="mt-1 text-[11px] text-red-700">{progress.error}</div>
      ) : null}
      {progress.warnings.length > 0 ? (
        <div className="mt-1 text-[11px]">{progress.warnings.slice(0, 2).join('；')}</div>
      ) : null}
    </div>
  );
}

export function AnalysisHeader({
  loading,
  hasCase,
  demoPending,
  extractionPending,
  extractionRun,
  onLoadDemoCase,
  onRefresh,
  onRunExtraction,
}: {
  loading: boolean;
  hasCase: boolean;
  demoPending: boolean;
  extractionPending: boolean;
  extractionRun?: AnalysisExtractionRun;
  onLoadDemoCase: () => void;
  onRefresh: () => void;
  onRunExtraction: () => void;
}) {
  return (
    <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] p-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="font-serif text-xl tracking-tight text-[#111]">数据源分析</div>
          <div className="mt-1 font-mono text-[11px] text-[#666]">
            证据分类 · 注册表提取 · 浏览器记录 · 邮件信息
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            onClick={onLoadDemoCase}
            disabled={loading}
            className="h-8 rounded border border-[#111] bg-[#111] px-3 text-[12px] text-white hover:bg-[#333]"
          >
            {demoPending ? <RefreshCw size={14} className="animate-spin" /> : <Database size={14} />}
            加载演示案件
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={onRefresh}
            disabled={!hasCase || loading}
            className="h-8 rounded border-[#ddd] bg-white px-3 text-[12px] hover:bg-[#f5f5f5]"
          >
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            刷新
          </Button>
          <Button
            type="button"
            onClick={onRunExtraction}
            disabled={!hasCase || extractionPending}
            className="h-8 rounded border border-[#111] bg-[#111] px-3 text-[12px] text-white hover:bg-[#333]"
          >
            {extractionPending ? <RefreshCw size={14} className="animate-spin" /> : <Shield size={14} />}
            {extractionPending ? '提取中...' : '运行提取'}
          </Button>
        </div>
      </div>
      <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-[1fr_auto]">
        <div className="rounded border border-[#e0e0e0] bg-white px-3 py-2 text-[11px] leading-5 text-[#666]">
          数据源分析保持只读读取，结构化结果通过 Tauri commands 获取；mock mode 会展示 Registry、Chrome/Edge/Firefox、EML/EMLX 示例。
        </div>
        {extractionRun ? (
          <div className="grid min-w-[360px] grid-cols-3 rounded border border-[#e0e0e0] bg-white text-center">
            <RunMetric label="扫描" value={extractionRun.scannedCount.toString()} />
            <RunMetric label="Artifact" value={extractionRun.artifactCount.toString()} />
            <RunMetric label="Timeline" value={extractionRun.timelineEventCount.toString()} />
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function AnalysisEmptyState({
  demoPending,
  onLoadDemoCase,
}: {
  demoPending: boolean;
  onLoadDemoCase: () => void;
}) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-md text-center">
        <Monitor size={40} className="mx-auto mb-4 text-[#bbb]" />
        <div className="text-[15px] font-semibold text-[#111]">请先创建或打开案件</div>
        <div className="mt-2 text-[12px] leading-6 text-[#666]">
          数据源分析依赖当前案件中的文件目录和数据源记录。未选择案件时不会发起分析请求。
        </div>
        <Button
          type="button"
          onClick={onLoadDemoCase}
          disabled={demoPending}
          className="mt-5 h-8 rounded bg-[#111] px-5 text-[12px] text-white hover:bg-[#333]"
        >
          {demoPending ? <RefreshCw size={14} className="animate-spin" /> : <Database size={14} />}
          加载演示案件
        </Button>
      </div>
    </div>
  );
}

export function AnalysisErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="mb-4 flex items-center justify-between gap-3 rounded border border-red-200 bg-red-50 p-3 text-[12px] text-red-700">
      <span>{message}</span>
      <Button
        type="button"
        variant="outline"
        onClick={onRetry}
        className="h-7 shrink-0 rounded border-red-200 bg-white px-3 text-[12px] text-red-700 hover:bg-red-100"
      >
        重试
      </Button>
    </div>
  );
}

export function AnalysisLoadingPanel({ text }: { text: string }) {
  return (
    <div className="flex h-64 items-center justify-center text-[#999]">
      <RefreshCw size={24} className="mr-2 animate-spin" />
      {text}
    </div>
  );
}

export function SystemInfoPanel({ systemInfo }: { systemInfo?: AnalysisSystemInfo }) {
  const info = systemInfo ?? {
    networkAdapters: [],
    bootHistory: [],
    status: 'unavailable' as const,
    warnings: ['系统信息暂不可用。'],
    provenance: [],
    fieldProvenance: [],
  };
  const parserFailures = info.provenance.filter(
    (item) => item.status !== 'parsed' && item.warnings.length > 0,
  );

  return (
    <div className="space-y-6">
      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Monitor size={16} />
          系统信息
        </h3>
        <div className="mb-3 flex items-center gap-2 text-[12px] text-[#666]">
          <StatusPill status={info.status} />
          {info.warnings[0] ? <span>{info.warnings[0]}</span> : null}
        </div>
        {parserFailures.length > 0 ? (
          <WarningList warnings={['已发现 Registry/EVTX 候选文件，但部分解析器失败；下方 provenance 已列出具体原因。']} />
        ) : null}
        {info.warnings.length > 1 ? <WarningList warnings={info.warnings.slice(0, 3)} /> : null}
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <InfoCard label="计算机名" value={info.computerName} />
          <InfoCard label="操作系统" value={info.osVersion} />
          <InfoCard label="Build 号" value={info.buildNumber} />
          <InfoCard label="注册用户" value={info.registeredOwner} />
          <InfoCard label="时区" value={info.timezone} />
          <InfoCard label="安装日期" value={info.installDate} />
        </div>
      </section>

      <ProvenancePanel
        title="解析来源"
        provenance={info.provenance}
        fallback="Registry/EVTX 解析来源暂不可用。"
      />

      <FieldProvenancePanel fieldProvenance={info.fieldProvenance} />

      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Network size={16} />
          网络适配器
        </h3>
        {info.networkAdapters.length > 0 ? (
          <div className="space-y-2">
            {info.networkAdapters.map((adapter) => (
              <div key={adapter.name} className="rounded border border-[#e0e0e0] bg-[#f8f8f8] p-3">
                <div className="text-[12px] font-medium">{adapter.name}</div>
                <div className="mt-1 font-mono text-[11px] text-[#666]">
                  MAC: {adapter.macAddress ?? '-'}
                </div>
                <div className="font-mono text-[11px] text-[#666]">
                  IP: {adapter.ipAddresses.join(', ') || '-'}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine text="未解析到网络适配器。" />
        )}
      </section>

      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Clock size={16} />
          开关机历史
        </h3>
        {info.bootHistory.length > 0 ? (
          <div className="space-y-1">
            {info.bootHistory.map((boot) => (
              <div key={`${boot.timestamp}-${boot.source}`} className="rounded border border-[#e0e0e0] bg-[#f8f8f8] p-3 text-[12px]">
                <div className="flex flex-wrap items-center gap-3">
                  <span className="font-mono text-[#666]">{boot.timestamp}</span>
                  <span className="rounded bg-[#f0f0f0] px-2 py-0.5 text-[10px]">
                    {boot.bootType}
                  </span>
                  {boot.eventId ? (
                    <span className="rounded bg-[#f0f0f0] px-2 py-0.5 font-mono text-[10px]">
                      EventID {boot.eventId}
                    </span>
                  ) : null}
                  <span className="text-[#999]">{boot.source}</span>
                </div>
                {boot.note ? <div className="mt-2 text-[11px] text-[#666]">{boot.note}</div> : null}
                <div className="mt-2 text-[11px] text-[#777]">
                  {formatProvenanceSummary(boot.provenance)}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine text="未解析到开关机历史。" />
        )}
      </section>
    </div>
  );
}

export function EvidenceClassificationPanel({
  summary,
  pending,
  onRun,
}: {
  summary?: EvidenceClassificationSummary;
  pending: boolean;
  onRun: () => void;
}) {
  const info = summary ?? {
    status: 'unavailable' as const,
    categories: [],
    generatedAt: '',
    warnings: ['证据分类摘要暂不可用。'],
    totals: {
      categoryCount: 0,
      candidateFileCount: 0,
      totalSize: 0,
      artifactCount: 0,
    },
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-[14px] font-semibold text-[#111]">证据语义分类</h3>
          <div className="mt-1 text-[11px] leading-5 text-[#666]">
            候选来自文件树 metadata；结构化提取会读取 Registry、浏览器记录与邮件候选并返回可审计来源。
          </div>
        </div>
        <Button
          type="button"
          onClick={onRun}
          disabled={pending}
          className="h-8 rounded border border-[#111] bg-[#111] px-4 text-[12px] text-white hover:bg-[#333]"
        >
          {pending ? <RefreshCw size={14} className="animate-spin" /> : <Shield size={14} />}
          {pending ? '分类中...' : '开始证据分类'}
        </Button>
      </div>

      <SummaryStrip
        items={[
          ['证据族', info.totals.categoryCount.toString()],
          ['候选文件', info.totals.candidateFileCount.toString()],
          ['候选总大小', formatSize(info.totals.totalSize)],
          ['已解析 Artifact', info.totals.artifactCount.toString()],
        ]}
      />

      {info.warnings.length > 0 ? <WarningList warnings={info.warnings} /> : null}

      {info.categories.length === 0 ? (
        <EmptyLine text="未发现证据语义分类数据。" />
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          {info.categories.map((category) => {
            const Icon = CATEGORY_ICONS[category.category] || Shield;
            const color = CATEGORY_COLORS[category.category] || '#344054';
            return (
              <section key={category.category} className="rounded border border-[#e0e0e0] bg-[#fcfcfc] p-4">
                <div className="mb-3 flex items-start justify-between gap-3">
                  <div className="flex items-center gap-2">
                    <Icon size={17} style={{ color }} />
                    <div>
                      <h4 className="text-[13px] font-semibold text-[#111]">{category.displayName}</h4>
                      <div className="font-mono text-[10px] text-[#888]">{category.category}</div>
                    </div>
                  </div>
                  <StatusPill status={category.status} />
                </div>

                <div className="mb-3 grid grid-cols-2 gap-2 text-[11px]">
                  <Metric label="候选" value={`${category.fileCount} 个`} />
                  <Metric label="大小" value={formatSize(category.totalSize)} />
                  <Metric label="Artifact" value={`${category.artifactCount} 条`} />
                  <Metric label="置信度" value={`${Math.round(category.confidence * 100)}%`} />
                </div>

                {category.warnings.length > 0 ? <WarningList warnings={category.warnings.slice(0, 2)} /> : null}

                {category.sources.length > 0 ? (
                  <div className="space-y-2">
                    {category.sources.slice(0, 4).map((source) => (
                      <div key={source.fileId} className="rounded border border-[#e5e5e5] bg-white px-3 py-2">
                        <div className="flex items-center justify-between gap-2">
                          <div className="truncate font-mono text-[11px] text-[#333]">{source.path}</div>
                          <span className="shrink-0 text-[10px] text-[#777]">{statusLabel(source.status)}</span>
                        </div>
                        <div className="mt-1 flex flex-wrap gap-2 font-mono text-[10px] text-[#888]">
                          <span>{source.evidenceKind}</span>
                          <span>{source.parser}</span>
                          <span>{formatSize(source.size)}</span>
                          <span>artifacts={source.artifactCount}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <EmptyLine text="该证据族暂无代表来源文件。" />
                )}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function RegistryExtractionPanel({
  summary,
  structured,
  progress,
}: {
  summary?: RegistryExtractionSummary;
  structured?: RegistryStructuredSummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const [activeTab, setActiveTab] = useState<'users' | 'activity' | 'network' | 'software' | 'usb' | 'raw'>('users');

  const info = summary ?? { status: 'unavailable' as const, total: 0, values: [], generatedAt: '', warnings: ['注册表提取结果暂不可用。'] };
  const s = structured;

  // Hive overview status badges
  const hiveOverviews: RegistryHiveOverview[] = s?.hiveOverviews ?? [];

  // SAM users columns
  const samColumns: DenseColumn<SamUserAccount>[] = [
    {
      key: 'username', title: '用户名', className: 'w-[130px] font-medium',
      render: (row) => (
        <span className={row.accountStatus === 'disabled' ? 'text-[#999]' : ''}>
          {row.username}
        </span>
      ),
    },
    { key: 'rid', title: 'RID', className: 'w-[60px] font-mono text-[10px]', render: (row) => row.ridHex },
    { key: 'sid', title: 'SID', className: 'min-w-[220px] font-mono text-[10px]', render: (row) => row.sid || '-' },
    {
      key: 'accountStatus', title: '状态', className: 'w-[60px]',
      render: (row) => (
        <span className={row.accountStatus === 'enabled' ? 'text-[#027a48]' : 'text-[#667085]'}>
          {row.accountStatus === 'enabled' ? '启用' : row.accountStatus === 'locked' ? '锁定' : '禁用'}
        </span>
      ),
    },
    { key: 'groups', title: '组', className: 'w-[180px]', render: (row) => row.groups.join(', ') },
    { key: 'loginCount', title: '登录次数', className: 'w-[80px] text-right', render: (row) => row.loginCount.toLocaleString() },
    { key: 'lastLogin', title: '最后登录', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastLogin ? row.lastLogin.replace('T', ' ').replace('Z', '') : '从未登录' },
    { key: 'profilePath', title: 'Profile 路径', className: 'min-w-[200px] font-mono text-[10px]', render: (row) => row.profilePath ?? '-' },
    {
      key: 'passwordHash', title: '密码哈希 (LM:NT)', className: 'min-w-[340px] font-mono text-[10px]',
      render: (row) => row.passwordHash
        ? <span className="select-all text-[#b42318]">{row.passwordHash}</span>
        : <span className="text-[#999]">—</span>,
    },
    { key: 'passwordHint', title: '密码提示', className: 'w-[120px]', render: (row) => row.passwordHint ?? '-' },
  ];

  // UserAssist columns
  const userAssistColumns: DenseColumn<UserAssistEntry>[] = [
    {
      key: 'programPath', title: '程序路径', className: 'min-w-[320px] font-mono text-[10px]',
      render: (row) => (
        <span className={row.isSuspicious ? 'text-[#b42318] font-semibold' : ''}>
          {row.isSuspicious ? '🚩 ' : ''}{row.programPath}
        </span>
      ),
    },
    { key: 'execCount', title: '执行次数', className: 'w-[80px] text-right', render: (row) => row.execCount.toLocaleString() },
    { key: 'lastExecTime', title: '最后执行', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastExecTime ? row.lastExecTime.replace('T', ' ').replace('Z', '') : '-' },
    { key: 'suspiciousReason', title: '备注', className: 'min-w-[200px]', render: (row) => row.suspiciousReason ?? '' },
  ];

  // Network profile columns (SOFTWARE\NetworkList)
  const networkColumns: DenseColumn<NetworkProfileEntry>[] = [
    { key: 'profileName', title: '配置文件名称', className: 'min-w-[180px]', render: (row) => row.profileName },
    { key: 'profileGuid', title: 'GUID', className: 'w-[220px] font-mono text-[10px]', render: (row) => row.profileGuid },
    { key: 'managed', title: '托管', className: 'w-[60px] text-center', render: (row) => (row.managed ? '是' : '否') },
    { key: 'firstNetwork', title: '首次网络', className: 'min-w-[160px]', render: (row) => row.firstNetwork ?? '-' },
    { key: 'defaultGatewayMacHex', title: '网关 MAC', className: 'w-[140px] font-mono text-[10px]', render: (row) => row.defaultGatewayMacHex ?? '-' },
    { key: 'dnsSuffix', title: 'DNS 后缀', className: 'w-[120px]', render: (row) => row.dnsSuffix ?? '-' },
    { key: 'dateCreated', title: '创建时间', className: 'w-[110px] font-mono text-[10px]', render: (row) => row.dateCreated?.slice(0, 10) ?? '-' },
    { key: 'dateLastConnected', title: '最后连接', className: 'w-[110px] font-mono text-[10px]', render: (row) => row.dateLastConnected?.slice(0, 10) ?? '-' },
    { key: 'description', title: '备注', className: 'w-[120px]', render: (row) => row.description ?? '-' },
  ];

  // Installed software columns
  const softwareColumns: DenseColumn<InstalledSoftware>[] = [
    {
      key: 'displayName', title: '软件名称', className: 'min-w-[220px]',
      render: (row) => (
        <span className={row.isSuspicious ? 'text-[#b42318] font-semibold' : ''}>
          {row.isSuspicious ? '🚩 ' : ''}{row.displayName}
        </span>
      ),
    },
    { key: 'version', title: '版本', className: 'w-[130px] font-mono text-[10px]', render: (row) => row.version },
    { key: 'publisher', title: '发布商', className: 'min-w-[200px]', render: (row) => row.publisher ?? <span className="text-[#b42318]">未知</span> },
    { key: 'installDate', title: '安装日期', className: 'w-[100px]', render: (row) => row.installDate ?? '-' },
    { key: 'estimatedSize', title: '大小', className: 'w-[80px] text-right', render: (row) => row.estimatedSize ?? '-' },
  ];

  // USB device columns
  const usbColumns: DenseColumn<UsbDeviceHistory>[] = [
    {
      key: 'deviceName', title: '设备名称', className: 'min-w-[200px]',
      render: (row) => (
        <span className={row.isSuspicious ? 'text-[#b42318] font-semibold' : ''}>
          {row.isSuspicious ? '🚩 ' : ''}{row.deviceName}
        </span>
      ),
    },
    { key: 'serialNumber', title: '序列号', className: 'w-[180px] font-mono text-[10px]', render: (row) => row.serialNumber },
    { key: 'driveLetter', title: '盘符', className: 'w-[60px]', render: (row) => row.driveLetter ?? '-' },
    { key: 'fileSystem', title: '文件系统', className: 'w-[80px]', render: (row) => row.fileSystem ?? '-' },
    { key: 'capacity', title: '容量', className: 'w-[70px] text-right', render: (row) => row.capacity ?? '-' },
    { key: 'firstConnect', title: '首次连接', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.firstConnect ? row.firstConnect.replace('T', ' ').replace('Z', '') : '-' },
    { key: 'lastConnect', title: '最后连接', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastConnect ? row.lastConnect.replace('T', ' ').replace('Z', '') : '-' },
    { key: 'suspiciousReason', title: '备注', className: 'min-w-[220px]', render: (row) => row.suspiciousReason ?? '' },
  ];

  // Raw registry columns
  const rawColumns: DenseColumn<RegistryValue>[] = [
    { key: 'hivePath', title: 'Hive', className: 'w-[110px]', render: (row) => row.hivePath || '-' },
    { key: 'keyPath', title: 'Key', className: 'min-w-[260px]', render: (row) => row.keyPath || '-' },
    { key: 'valueName', title: 'Value', className: 'w-[180px]', render: (row) => row.valueName || '-' },
    { key: 'valueType', title: 'Type', className: 'w-[90px]', render: (row) => row.valueType || '-' },
    { key: 'data', title: 'Data', className: 'min-w-[220px]', render: (row) => row.data || '-' },
    { key: 'parser', title: 'Parser', className: 'w-[150px]', render: (row) => row.parser || '-' },
  ];

  const TABS: Array<{ key: typeof activeTab; label: string; icon: typeof Database }> = [
    { key: 'users', label: '用户账户', icon: Shield },
    { key: 'activity', label: '用户活动', icon: Clock },
    { key: 'network', label: '网络配置', icon: Wifi },
    { key: 'software', label: '软件列表', icon: Database },
    { key: 'usb', label: 'USB 设备', icon: Usb },
    { key: 'raw', label: '原始键值', icon: HardDrive },
  ];

  const hiveSummaryStats: Array<[string, string]> = hiveOverviews.map(
    (h) => [h.hiveName, `${h.keyValueCount} 条${h.txlogMerged ? ' · txlog✓' : ''}${h.deletedKeysFound > 0 ? ` · ⚠${h.deletedKeysFound}已删` : ''}`],
  );

  return (
    <ExtractionTableSection
      title="注册表提取"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={hiveSummaryStats.length > 0 ? hiveSummaryStats : [
        ['键值数', info.total.toString()],
        ['来源 Hive', new Set(info.values.map((v) => v.hivePath)).size.toString()],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />

      {/* Sub-tab bar */}
      <div className="flex gap-0 border-b border-[#e0e0e0] bg-[#fafafa]">
        {TABS.map(({ key, label, icon: Icon }) => (
          <button
            key={key}
            onClick={() => setActiveTab(key)}
            className={[
              'flex items-center gap-1.5 px-4 py-2 text-[11px] font-medium transition-colors',
              activeTab === key
                ? 'border-b-2 border-[#175cd3] text-[#175cd3] bg-white'
                : 'text-[#667085] hover:text-[#333]',
            ].join(' ')}
          >
            <Icon size={12} />
            {label}
          </button>
        ))}
      </div>

      <DenseTableFrame>
        {activeTab === 'users' && (
          <DenseDataTable
            rows={s?.samUsers ?? []}
            columns={samColumns}
            getRowKey={(row) => row.username}
            emptyTitle="暂无用户账户数据"
            emptyDescription="运行提取后将显示 SAM 账户信息。"
          />
        )}
        {activeTab === 'activity' && (
          <DenseDataTable
            rows={s?.userAssistEntries ?? []}
            columns={userAssistColumns}
            getRowKey={(row) => row.programPath}
            emptyTitle="暂无 UserAssist 数据"
            emptyDescription="从 NTUSER.DAT 提取程序执行记录。"
          />
        )}
        {activeTab === 'network' && (
          <DenseDataTable
            rows={s?.networkProfiles ?? []}
            columns={networkColumns}
            getRowKey={(row) => row.profileGuid}
            emptyTitle="暂无网络配置数据"
            emptyDescription="从 SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\NetworkList 提取网络配置文件及 Wi-Fi 连接历史。"
          />
        )}
        {activeTab === 'software' && (
          <DenseDataTable
            rows={s?.installedSoftware ?? []}
            columns={softwareColumns}
            getRowKey={(row) => `${row.displayName}-${row.version}`}
            emptyTitle="暂无已安装软件数据"
            emptyDescription="从 SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall 提取。"
          />
        )}
        {activeTab === 'usb' && (
          <DenseDataTable
            rows={s?.usbDevices ?? []}
            columns={usbColumns}
            getRowKey={(row) => row.serialNumber}
            emptyTitle="暂无 USB 设备历史"
            emptyDescription="从 SYSTEM hive USBSTOR 提取连接记录。"
          />
        )}
        {activeTab === 'raw' && (
          <DenseDataTable
            rows={info.values}
            columns={rawColumns}
            getRowKey={(row) => row.artifactId}
            emptyTitle="暂无原始键值数据"
            emptyDescription="运行提取后会显示关键 hive 的 key/value 摘要。"
          />
        )}
      </DenseTableFrame>
    </ExtractionTableSection>
  );
}

export function BrowserHistoryPanel({
  summary,
  progress,
}: {
  summary?: BrowserHistorySummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const info = summary ?? {
    status: 'unavailable' as const,
    visitTotal: 0,
    downloadTotal: 0,
    visits: [],
    downloads: [],
    generatedAt: '',
    warnings: ['浏览器记录暂不可用。'],
  };
  const visitColumns: DenseColumn<BrowserVisit>[] = [
    { key: 'visitTime', title: '时间', className: 'w-[170px]', render: (row) => row.visitTime ?? '-' },
    { key: 'browser', title: '浏览器', className: 'w-[90px]', render: (row) => row.browser },
    { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
    { key: 'title', title: '标题', className: 'min-w-[220px]', render: (row) => row.title || '-' },
    { key: 'url', title: 'URL', className: 'min-w-[300px]', render: (row) => row.url },
    { key: 'visitCount', title: '次数', className: 'w-[70px]', render: (row) => row.visitCount.toString() },
  ];
  const downloadColumns: DenseColumn<BrowserDownload>[] = [
    { key: 'startTime', title: '时间', className: 'w-[170px]', render: (row) => row.startTime ?? '-' },
    { key: 'browser', title: '浏览器', className: 'w-[90px]', render: (row) => row.browser },
    { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
    { key: 'targetPath', title: '目标路径', className: 'min-w-[260px]', render: (row) => row.targetPath || '-' },
    { key: 'url', title: 'URL', className: 'min-w-[260px]', render: (row) => row.url || '-' },
    { key: 'totalBytes', title: '大小', className: 'w-[110px]', render: (row) => formatSize(row.totalBytes) },
  ];

  return (
    <ExtractionTableSection
      title="浏览器记录"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['访问记录', info.visitTotal.toString()],
        ['下载记录', info.downloadTotal.toString()],
        ['浏览器', Array.from(new Set(info.visits.map((item) => item.browser))).join(' / ') || '-'],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />
      <div className="space-y-4">
        <TableBlock title="访问历史">
          <DenseTableFrame>
            <DenseDataTable
              rows={info.visits}
              columns={visitColumns}
              getRowKey={(row) => row.artifactId}
              emptyTitle="暂无浏览历史"
              emptyDescription="支持 Chrome、Edge History 与 Firefox places.sqlite。"
            />
          </DenseTableFrame>
        </TableBlock>
        <TableBlock title="下载记录">
          <DenseTableFrame>
            <DenseDataTable
              rows={info.downloads}
              columns={downloadColumns}
              getRowKey={(row) => row.artifactId}
              emptyTitle="暂无下载记录"
              emptyDescription="发现下载记录后会显示 URL、目标路径与大小。"
            />
          </DenseTableFrame>
        </TableBlock>
      </div>
    </ExtractionTableSection>
  );
}

export function EmailExtractionPanel({
  summary,
  progress,
}: {
  summary?: EmailExtractionSummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const info = summary ?? {
    status: 'unavailable' as const,
    total: 0,
    messages: [],
    generatedAt: '',
    warnings: ['邮件提取结果暂不可用。'],
  };
  const columns: DenseColumn<EmailMessage>[] = [
    { key: 'sentAt', title: '时间', className: 'w-[170px]', render: (row) => row.sentAt ?? '-' },
    { key: 'from', title: 'From', className: 'w-[180px]', render: (row) => row.from || '-' },
    { key: 'to', title: 'To', className: 'w-[200px]', render: (row) => row.to.join(', ') || '-' },
    { key: 'subject', title: 'Subject', className: 'min-w-[240px]', render: (row) => row.subject || '-' },
    { key: 'attachments', title: '附件', className: 'w-[180px]', render: (row) => row.attachments.join(', ') || '-' },
    { key: 'sourcePath', title: 'Source', className: 'min-w-[260px]', render: (row) => row.sourcePath || '-' },
  ];

  return (
    <ExtractionTableSection
      title="邮件信息"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['邮件数', info.total.toString()],
        ['当前页', info.messages.length.toString()],
        ['含附件', info.messages.filter((item) => item.attachments.length > 0).length.toString()],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />
      <DenseTableFrame>
        <DenseDataTable
          rows={info.messages}
          columns={columns}
          getRowKey={(row) => row.artifactId}
          emptyTitle="暂无邮件信息"
          emptyDescription="v1 支持 EML/EMLX 的头字段、附件名与正文预览。"
        />
      </DenseTableFrame>
      {info.messages[0] ? (
        <div className="mt-4 rounded border border-[#e0e0e0] bg-[#fcfcfc] p-3 text-[12px]">
          <div className="mb-1 font-semibold text-[#111]">正文预览</div>
          <div className="text-[#666]">{info.messages[0].bodyPreview}</div>
          <div className="mt-2 font-mono text-[10px] text-[#888]">
            Message-ID: {info.messages[0].messageId}
          </div>
        </div>
      ) : null}
    </ExtractionTableSection>
  );
}

export function FileClassificationPanel({
  classifications,
}: {
  classifications: AnalysisFileClassification[];
}) {
  const totalFiles = classifications.reduce((sum, category) => sum + category.fileCount, 0);
  const totalSize = classifications.reduce((sum, category) => sum + category.totalSize, 0);

  return (
    <div className="space-y-6">
      <SummaryStrip
        items={[
          ['文件总数', totalFiles.toString()],
          ['文件总大小', formatSize(totalSize)],
          ['分类数', classifications.length.toString()],
        ]}
      />
      <div className="rounded border border-[#e0e0e0] bg-[#fcfcfc] px-3 py-2 text-[11px] leading-5 text-[#666]">
        当前为 metadata-only 分类：总数/大小来自数据库全量聚合，文件列表仅展示抽样；不读取 E01/RAW 文件正文。
      </div>

      {classifications.length === 0 ? (
        <EmptyLine text="未发现可分类文件。" />
      ) : (
        <div className="space-y-4">
          {classifications.map((category) => {
            const Icon = CATEGORY_ICONS[category.category] || FileText;
            const color = CATEGORY_COLORS[category.category] || CATEGORY_COLORS.Other;
            return (
              <section key={category.category}>
                <div className="mb-2 flex items-center gap-2">
                  <Icon size={16} style={{ color }} />
                  <h3 className="text-[14px] font-semibold text-[#111]">{category.category}</h3>
                  <span className="text-[11px] text-[#999]">
                    总计 {category.fileCount} 个 · 抽样 {category.files.length} 个 · {formatSize(category.totalSize)} · {statusLabel(category.status)}
                  </span>
                </div>
                {category.warnings.length > 0 ? <WarningList warnings={category.warnings} /> : null}
                <ProvenancePanel
                  title="分类来源"
                  provenance={category.provenance}
                  compact
                  fallback="分类来源暂不可用。"
                />
                <div className="overflow-hidden rounded border border-[#e0e0e0] bg-[#f8f8f8]">
                  <table className="w-full text-[11px]">
                    <thead>
                      <tr className="bg-[#f0f0f0]">
                        <th className="px-3 py-2 text-left font-medium">文件名</th>
                        <th className="px-3 py-2 text-left font-medium">类型</th>
                        <th className="px-3 py-2 text-left font-medium">来源</th>
                        <th className="px-3 py-2 text-right font-medium">大小</th>
                      </tr>
                    </thead>
                    <tbody>
                      {category.files.slice(0, 20).map((file) => (
                        <tr key={file.fileId} className="border-t border-[#e0e0e0]">
                          <td className="max-w-[300px] truncate px-3 py-1.5 font-mono">{file.name}</td>
                          <td className="px-3 py-1.5 text-[#666]">{file.magicDescription}</td>
                          <td className="max-w-[260px] truncate px-3 py-1.5 text-[#666]">
                            {formatProvenanceSummary(file.provenance)}
                          </td>
                          <td className="px-3 py-1.5 text-right text-[#666]">{formatSize(file.size)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function AnalysisReportPanel({
  pending,
  onDownload,
}: {
  pending: boolean;
  onDownload: () => void;
}) {
  return (
    <div className="flex h-64 flex-col items-center justify-center gap-4">
      <FileText size={48} className="text-[#ccc]" />
      <div className="text-[14px] text-[#666]">生成分析报告</div>
      <Button
        type="button"
        onClick={onDownload}
        disabled={pending}
        className="h-9 rounded bg-[#111] px-6 text-[12px] text-white hover:bg-[#333]"
      >
        {pending ? <RefreshCw size={14} className="animate-spin" /> : <Download size={14} />}
        下载 Markdown 报告
      </Button>
    </div>
  );
}

function FieldProvenancePanel({ fieldProvenance }: { fieldProvenance: AnalysisFieldProvenance[] }) {
  return (
    <section>
      <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Database size={16} />
        字段级来源
      </h3>
      {fieldProvenance.length > 0 ? (
        <div className="overflow-hidden rounded border border-[#e0e0e0] bg-[#f8f8f8]">
          <table className="w-full text-[11px]">
            <thead>
              <tr className="bg-[#f0f0f0]">
                <th className="px-3 py-2 text-left font-medium">字段</th>
                <th className="px-3 py-2 text-left font-medium">Hive</th>
                <th className="px-3 py-2 text-left font-medium">Key</th>
                <th className="px-3 py-2 text-left font-medium">Value</th>
                <th className="px-3 py-2 text-left font-medium">Parser</th>
              </tr>
            </thead>
            <tbody>
              {fieldProvenance.map((item) => (
                <tr key={`${item.field}-${item.hivePath}-${item.keyPath}-${item.valueName}`} className="border-t border-[#e0e0e0]">
                  <td className="px-3 py-1.5 font-mono text-[#333]">{item.field}</td>
                  <td className="max-w-[240px] truncate px-3 py-1.5 font-mono text-[#666]">{item.hivePath || '-'}</td>
                  <td className="max-w-[360px] truncate px-3 py-1.5 font-mono text-[#666]">{item.keyPath || '-'}</td>
                  <td className="px-3 py-1.5 font-mono text-[#666]">{item.valueName || '-'}</td>
                  <td className="px-3 py-1.5 font-mono text-[#666]">{item.parser || '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <EmptyLine text="字段级 Registry provenance 暂不可用。" />
      )}
    </section>
  );
}

function ExtractionTableSection({
  title,
  status,
  generatedAt,
  warnings,
  stats,
  children,
}: {
  title: string;
  status: string;
  generatedAt: string;
  warnings: string[];
  stats: Array<[string, string]>;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-[14px] font-semibold text-[#111]">{title}</h3>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-[#666]">
            <StatusPill status={status} />
            <span>生成时间：{generatedAt || '-'}</span>
          </div>
        </div>
      </div>
      <SummaryStrip items={stats} />
      {warnings.length > 0 ? <WarningList warnings={warnings} /> : null}
      {children}
    </div>
  );
}

function SummaryStrip({ items }: { items: Array<[string, string]> }) {
  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-3 xl:grid-cols-4">
      {items.map(([label, value]) => (
        <StatCard key={label} label={label} value={value} />
      ))}
    </div>
  );
}

function TableBlock({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <div className="mb-2 text-[12px] font-semibold text-[#111]">{title}</div>
      {children}
    </section>
  );
}

function DenseTableFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-[340px] min-h-0 overflow-hidden rounded border border-[#e0e0e0] bg-white">
      {children}
    </div>
  );
}

function ProvenancePanel({
  title,
  provenance,
  fallback,
  compact = false,
}: {
  title: string;
  provenance: AnalysisProvenance[];
  fallback: string;
  compact?: boolean;
}) {
  return (
    <div className={compact ? 'mb-2' : 'space-y-2'}>
      <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-[#777]">{title}</div>
      {provenance.length > 0 ? (
        <div className="space-y-2">
          {provenance.map((item, index) => (
            <div key={`${item.parser}-${item.artifactPath}-${index}`} className="rounded border border-[#e0e0e0] bg-[#fcfcfc] px-3 py-2 text-[11px] text-[#666]">
              <div className="flex flex-wrap items-center gap-2">
                <StatusPill status={item.status} />
                <span className="font-mono text-[#333]">{item.parser || '-'}</span>
                <span className="font-mono text-[#777]">{item.artifactPath || '-'}</span>
              </div>
              <div className="mt-1 font-mono text-[10px] text-[#888]">
                dataSource={item.dataSourceId || '-'} · parsedAt={item.parsedAt || '-'}
              </div>
              {item.warnings.length > 0 ? <div className="mt-1 text-amber-800">{item.warnings.join('；')}</div> : null}
            </div>
          ))}
        </div>
      ) : (
        <EmptyLine text={fallback} />
      )}
    </div>
  );
}

function formatProvenanceSummary(provenance: AnalysisProvenance) {
  return `${provenance.parser || '-'} · ${provenance.artifactPath || '-'} · ${statusLabel(provenance.status)}`;
}

function RunMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="border-r border-[#e0e0e0] px-3 py-2 last:border-r-0">
      <div className="font-mono text-[15px] font-semibold text-[#111]">{value}</div>
      <div className="text-[10px] text-[#777]">{label}</div>
    </div>
  );
}

function InfoCard({ label, value }: { label: string; value?: string }) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-[#f8f8f8] p-3">
      <div className="mb-1 text-[10px] uppercase tracking-wider text-[#999]">{label}</div>
      <div className="font-mono text-[13px] text-[#111]">{value || '未解析'}</div>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-[#f8f8f8] p-4 text-center">
      <div className="break-words text-[24px] font-bold text-[#111]">{value}</div>
      <div className="mt-1 text-[11px] text-[#666]">{label}</div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-[#e8e8e8] bg-white px-2 py-1.5">
      <div className="text-[10px] text-[#999]">{label}</div>
      <div className="font-mono text-[11px] text-[#333]">{value}</div>
    </div>
  );
}

function StatusPill({ status }: { status: string }) {
  return (
    <span className="rounded bg-[#f0f0f0] px-2 py-0.5 font-mono text-[10px] text-[#555]">
      {statusLabel(status)}
    </span>
  );
}

function WarningList({ warnings }: { warnings: string[] }) {
  return (
    <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] leading-5 text-amber-800">
      {warnings.map((warning) => (
        <div key={warning}>{warning}</div>
      ))}
    </div>
  );
}

function EmptyLine({ text }: { text: string }) {
  return (
    <div className="rounded border border-dashed border-[#d8d8d8] bg-[#fcfcfc] px-3 py-2 text-[12px] text-[#777]">
      {text}
    </div>
  );
}
