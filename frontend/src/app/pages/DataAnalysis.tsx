import {
  Archive,
  Clock,
  Database,
  Download,
  FileText,
  HardDrive,
  Image,
  Monitor,
  Network,
  RefreshCw,
  Shield,
} from 'lucide-react';
import { useCurrentCase } from '@/features/case/hooks';
import {
  useAnalysisClassifications,
  useAnalysisSystemInfo,
  useGenerateAnalysisSummary,
} from '@/features/analysis/hooks';
import {
  AnalysisFileClassification,
  AnalysisSystemInfo,
} from '@/types/models';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/app/components/ui/tabs';
import { isApiErrorDto } from '@/lib/api/client';

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
  Prefetch: Clock,
  Shortcuts: FileText,
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
  Prefetch: '#9a6700',
  Shortcuts: '#026aa2',
  Other: '#667085',
};

const tabs = [
  { value: 'system', label: '系统信息', icon: Monitor },
  { value: 'files', label: '文件分类', icon: FileText },
  { value: 'report', label: '分析报告', icon: Download },
] as const;

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function errorMessage(error: unknown) {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function DataAnalysis() {
  const currentCase = useCurrentCase();
  const systemInfo = useAnalysisSystemInfo();
  const classifications = useAnalysisClassifications(1000);
  const summaryMutation = useGenerateAnalysisSummary();

  const hasCase = Boolean(currentCase.data);
  const loading = currentCase.isLoading || systemInfo.isLoading || classifications.isLoading;
  const error = currentCase.error ?? systemInfo.error ?? classifications.error ?? summaryMutation.error;
  const classes = classifications.data ?? [];
  const totalFiles = classes.reduce((sum, category) => sum + category.files.length, 0);
  const totalSize = classes.reduce((sum, category) => sum + category.totalSize, 0);

  async function refresh() {
    await Promise.all([systemInfo.refetch(), classifications.refetch()]);
  }

  async function downloadSummary() {
    const summary = await summaryMutation.mutateAsync();
    const blob = new Blob([summary], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'analysis-report.md';
    link.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-auto bg-white">
      <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] p-6">
        <div className="flex items-center justify-between gap-4">
          <div>
            <div className="font-serif text-xl tracking-tight text-[#111]">数据源分析</div>
            <div className="mt-1 font-mono text-[11px] text-[#666]">
              系统信息 · 文件分类 · 证据分析
            </div>
          </div>
          <button
            type="button"
            onClick={refresh}
            disabled={!hasCase || loading}
            className="flex items-center gap-2 rounded border border-[#ddd] bg-white px-4 py-2 text-[12px] hover:bg-[#f5f5f5] disabled:opacity-50"
          >
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            刷新
          </button>
        </div>
      </div>

      {!hasCase && currentCase.isSuccess ? (
        <div className="flex flex-1 items-center justify-center p-8">
          <div className="max-w-md text-center">
            <Monitor size={40} className="mx-auto mb-4 text-[#bbb]" />
            <div className="text-[15px] font-semibold text-[#111]">请先创建或打开案件</div>
            <div className="mt-2 text-[12px] leading-6 text-[#666]">
              数据源分析依赖当前案件中的文件目录和数据源记录。未选择案件时不会发起分析请求。
            </div>
          </div>
        </div>
      ) : (
        <Tabs defaultValue="system" className="min-h-0 flex-1 gap-0">
          <TabsList className="h-auto w-full justify-start rounded-none border-b border-[#e0e0e0] bg-[#fafafa] p-0">
            {tabs.map(({ value, label, icon: Icon }) => (
              <TabsTrigger
                key={value}
                value={value}
                className="h-auto flex-none rounded-none border-x-0 border-t-0 border-b-2 border-transparent bg-transparent px-6 py-3 text-[12px] data-[state=active]:border-[#111] data-[state=active]:bg-transparent"
              >
                <Icon size={14} />
                {label}
              </TabsTrigger>
            ))}
          </TabsList>

          <div className="min-h-0 flex-1 overflow-auto p-6">
            {error ? (
              <div className="mb-4 flex items-center justify-between gap-3 rounded border border-red-200 bg-red-50 p-3 text-[12px] text-red-700">
                <span>{errorMessage(error)}</span>
                <button
                  type="button"
                  onClick={refresh}
                  className="shrink-0 rounded border border-red-200 bg-white px-3 py-1 text-red-700 hover:bg-red-100"
                >
                  重试
                </button>
              </div>
            ) : null}

            {loading ? (
              <div className="flex h-64 items-center justify-center text-[#999]">
                <RefreshCw size={24} className="mr-2 animate-spin" />
                正在分析数据源...
              </div>
            ) : (
              <>
                <TabsContent value="system" forceMount className="m-0 data-[state=inactive]:hidden">
                  <SystemInfoTab systemInfo={systemInfo.data} />
                </TabsContent>
                <TabsContent value="files" forceMount className="m-0 data-[state=inactive]:hidden">
                  <FileClassificationTab
                    classifications={classes}
                    totalFiles={totalFiles}
                    totalSize={totalSize}
                  />
                </TabsContent>
                <TabsContent value="report" forceMount className="m-0 data-[state=inactive]:hidden">
                  <ReportTab
                    pending={summaryMutation.isPending}
                    onDownload={downloadSummary}
                  />
                </TabsContent>
              </>
            )}
          </div>
        </Tabs>
      )}
    </div>
  );
}

function SystemInfoTab({ systemInfo }: { systemInfo?: AnalysisSystemInfo }) {
  const info = systemInfo ?? {
    networkAdapters: [],
    bootHistory: [],
    status: 'unavailable' as const,
    warnings: ['系统信息暂不可用。'],
  };

  return (
    <div className="space-y-6">
      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Monitor size={16} />
          系统信息
        </h3>
        <div className="mb-3 flex items-center gap-2 text-[12px] text-[#666]">
          <span className="rounded bg-[#f0f0f0] px-2 py-0.5 font-mono text-[10px]">
            {statusLabel(info.status)}
          </span>
          {info.warnings[0] ? <span>{info.warnings[0]}</span> : null}
        </div>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <InfoCard label="计算机名" value={info.computerName} />
          <InfoCard label="操作系统" value={info.osVersion} />
          <InfoCard label="Build 号" value={info.buildNumber} />
          <InfoCard label="注册用户" value={info.registeredOwner} />
          <InfoCard label="时区" value={info.timezone} />
          <InfoCard label="安装日期" value={info.installDate} />
        </div>
      </section>

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
              <div key={`${boot.timestamp}-${boot.source}`} className="flex items-center gap-3 p-2 text-[12px]">
                <span className="font-mono text-[#666]">{boot.timestamp}</span>
                <span className="rounded bg-[#f0f0f0] px-2 py-0.5 text-[10px]">
                  {boot.bootType}
                </span>
                <span className="text-[#999]">{boot.source}</span>
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

function FileClassificationTab({
  classifications,
  totalFiles,
  totalSize,
}: {
  classifications: AnalysisFileClassification[];
  totalFiles: number;
  totalSize: number;
}) {
  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <StatCard label="样本文件数" value={totalFiles.toString()} />
        <StatCard label="样本总大小" value={formatSize(totalSize)} />
        <StatCard label="分类数" value={classifications.length.toString()} />
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
                    {category.files.length} 个文件 · {formatSize(category.totalSize)} · {statusLabel(category.status)}
                  </span>
                </div>
                {category.warnings.length > 0 ? (
                  <div className="mb-2 rounded border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] text-amber-800">
                    {category.warnings.join('；')}
                  </div>
                ) : null}
                <div className="overflow-hidden rounded border border-[#e0e0e0] bg-[#f8f8f8]">
                  <table className="w-full text-[11px]">
                    <thead>
                      <tr className="bg-[#f0f0f0]">
                        <th className="px-3 py-2 text-left font-medium">文件名</th>
                        <th className="px-3 py-2 text-left font-medium">类型</th>
                        <th className="px-3 py-2 text-right font-medium">大小</th>
                      </tr>
                    </thead>
                    <tbody>
                      {category.files.slice(0, 20).map((file) => (
                        <tr key={file.fileId} className="border-t border-[#e0e0e0]">
                          <td className="max-w-[300px] truncate px-3 py-1.5 font-mono">
                            {file.name}
                          </td>
                          <td className="px-3 py-1.5 text-[#666]">{file.magicDescription}</td>
                          <td className="px-3 py-1.5 text-right text-[#666]">
                            {formatSize(file.size)}
                          </td>
                        </tr>
                      ))}
                      {category.files.length > 20 ? (
                        <tr className="border-t border-[#e0e0e0]">
                          <td colSpan={3} className="px-3 py-1.5 text-center text-[#999]">
                            还有 {category.files.length - 20} 个文件...
                          </td>
                        </tr>
                      ) : null}
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

function ReportTab({
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
      <button
        type="button"
        onClick={onDownload}
        disabled={pending}
        className="flex items-center gap-2 rounded bg-[#111] px-6 py-2 text-[12px] text-white hover:bg-[#333] disabled:opacity-50"
      >
        {pending ? <RefreshCw size={14} className="animate-spin" /> : <Download size={14} />}
        下载 Markdown 报告
      </button>
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
      <div className="text-[24px] font-bold text-[#111]">{value}</div>
      <div className="mt-1 text-[11px] text-[#666]">{label}</div>
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

function statusLabel(status: string) {
  switch (status) {
    case 'parsed':
      return '已解析';
    case 'notParsed':
      return '未解析';
    case 'unavailable':
      return '不可用';
    default:
      return status;
  }
}
