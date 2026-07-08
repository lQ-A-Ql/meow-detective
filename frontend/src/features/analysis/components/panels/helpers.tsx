import {
  Archive,
  Clock,
  Database,
  FileText,
  Globe,
  HardDrive,
  Image,
  Mail,
  Monitor,
  Shield,
} from 'lucide-react';
import { Progress } from '@/app/components/ui/progress';
import { EmptyState, MetricCard, StatGrid } from '@/components/data-display';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  AnalysisFieldProvenance,
  AnalysisProvenance,
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

export const CATEGORY_ICONS: Record<string, typeof Monitor> = {
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

export const CATEGORY_COLORS: Record<string, string> = {
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

export function ExtractionTableSection({
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

export function SummaryStrip({ items }: { items: Array<[string, string]> }) {
  return (
    <StatGrid>
      {items.map(([label, value]) => (
        <StatCard key={label} label={label} value={value} />
      ))}
    </StatGrid>
  );
}

export function TableBlock({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <div className="mb-2 text-[12px] font-semibold text-[#111]">{title}</div>
      {children}
    </section>
  );
}

export function DenseTableFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col min-h-0 overflow-hidden rounded border border-[#e0e0e0] bg-white">
      {children}
    </div>
  );
}

export function ProvenancePanel({
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

export function FieldProvenancePanel({ fieldProvenance }: { fieldProvenance: AnalysisFieldProvenance[] }) {
  const columns: DenseColumn<AnalysisFieldProvenance>[] = [
    {
      key: 'field',
      title: '字段',
      className: 'w-[160px]',
      render: (item) => item.field,
    },
    {
      key: 'hive',
      title: 'Hive',
      className: 'min-w-[220px]',
      render: (item) => item.hivePath || '-',
    },
    {
      key: 'key',
      title: 'Key',
      className: 'min-w-[300px]',
      render: (item) => item.keyPath || '-',
    },
    {
      key: 'value',
      title: 'Value',
      className: 'w-[140px]',
      render: (item) => item.valueName || '-',
    },
    {
      key: 'parser',
      title: 'Parser',
      className: 'w-[160px]',
      render: (item) => item.parser || '-',
    },
  ];

  return (
    <section>
      <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <Database size={16} />
        字段级来源
      </h3>
      {fieldProvenance.length > 0 ? (
        <div className="h-[360px] min-h-0">
          <DenseTableFrame>
            <DenseDataTable
              rows={fieldProvenance}
              columns={columns}
              getRowKey={(item) => `${item.field}-${item.hivePath}-${item.keyPath}-${item.valueName}`}
              emptyTitle="暂无字段级来源"
              emptyDescription="字段级 Registry provenance 暂不可用。"
            />
          </DenseTableFrame>
        </div>
      ) : (
        <EmptyLine text="字段级 Registry provenance 暂不可用。" />
      )}
    </section>
  );
}

export function formatProvenanceSummary(provenance: AnalysisProvenance) {
  return `${provenance.parser || '-'} · ${provenance.artifactPath || '-'} · ${statusLabel(provenance.status)}`;
}

export function RunMetric({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} size="sm" className="rounded-none border-y-0 border-l-0 border-r border-[#e0e0e0] last:border-r-0" />;
}

export function InfoCard({ label, value }: { label: string; value?: string }) {
  return <MetricCard label={label} value={value || '未解析'} size="md" className="bg-[#f8f8f8]" />;
}

export function StatCard({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} mono={false} align="center" size="lg" className="bg-[#f8f8f8]" />;
}

export function Metric({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} size="sm" />;
}

export function StatusPill({ status }: { status: string }) {
  return (
    <span className="rounded bg-[#f0f0f0] px-2 py-0.5 font-mono text-[10px] text-[#555]">
      {statusLabel(status)}
    </span>
  );
}

export function WarningList({ warnings }: { warnings: string[] }) {
  return (
    <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] leading-5 text-amber-800">
      {warnings.map((warning) => (
        <div key={warning}>{warning}</div>
      ))}
    </div>
  );
}

export function EmptyLine({ text }: { text: string }) {
  return <EmptyState className="px-3 py-2 text-left text-[#777]">{text}</EmptyState>;
}
