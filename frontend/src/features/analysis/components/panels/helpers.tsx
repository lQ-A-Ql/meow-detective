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
  totalCandidateCount?: number;
  processedCandidateCount?: number;
  structuredCandidateCount?: number;
  unsupportedCandidateCount?: number;
  textFallbackCandidateCount?: number;
  warningCandidateCount?: number;
  checkpointHitCount?: number;
  phase?: string;
  currentPath?: string;
  detail?: string;
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
  Executables: 'var(--forensics-error-text)',
  Documents: 'var(--forensics-info-text)',
  Images: 'var(--forensics-success-text)',
  Archives: 'var(--forensics-warning-text)',
  Databases: 'var(--forensics-primary-blue)',
  System: 'var(--forensics-text-secondary)',
  Forensics: 'var(--forensics-info-text)',
  Logs: 'var(--forensics-text-secondary)',
  Registry: 'var(--forensics-primary-blue)',
  BrowserHistory: 'var(--forensics-info-text)',
  Email: 'var(--forensics-warning-text)',
  Prefetch: 'var(--forensics-warning-text)',
  Shortcuts: 'var(--forensics-info-text)',
  SystemInformation: 'var(--forensics-text-secondary)',
  EventLogs: 'var(--forensics-info-text)',
  ProgramExecution: 'var(--forensics-error-text)',
  UserActivity: 'var(--forensics-warning-text)',
  RecycleBin: 'var(--forensics-warning-text)',
  Thumbnails: 'var(--forensics-success-text)',
  ResourceUsage: 'var(--forensics-primary-blue)',
  BrowserData: 'var(--forensics-info-text)',
  FileTypeInventory: 'var(--forensics-muted)',
  Other: 'var(--forensics-muted)',
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
  const totalCandidates = progress.totalCandidateCount ?? 0;
  const processedCandidates = progress.processedCandidateCount ?? progress.scannedCount;
  const value = totalCandidates > 0
    ? Math.min(100, Math.round((processedCandidates / totalCandidates) * 100))
    : progress.status === 'success'
      ? 100
      : 0;
  const tone = progress.status === 'failed'
    ? 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text'
    : progress.status === 'partial'
      ? 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text'
      : 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary';
  return (
    <div className={`rounded-none border px-3 py-2 ${tone}`}>
      <div className="mb-2 flex items-center justify-between gap-3 text-[11px]">
        <span className="font-light text-forensics-text">{progress.label}</span>
        <span className="font-mono">
          {totalCandidates > 0
            ? `${processedCandidates}/${totalCandidates} (${value}%)`
            : extractionProgressLabel(progress.status)}
        </span>
      </div>
      <Progress value={value} className="h-1.5 rounded-none bg-forensics-surface" />
      <div className="mt-2 flex flex-wrap gap-3 font-mono text-[10px]">
        <span>scanned={progress.scannedCount}</span>
        {totalCandidates > 0 ? <span>candidates={processedCandidates}/{totalCandidates}</span> : null}
        <span>artifacts={progress.artifactCount}</span>
        <span>timeline={progress.timelineEventCount}</span>
        {progress.unsupportedCandidateCount ? (
          <span>unsupported={progress.unsupportedCandidateCount}</span>
        ) : null}
        {progress.textFallbackCandidateCount ? (
          <span>fallback={progress.textFallbackCandidateCount}</span>
        ) : null}
      </div>
      {progress.detail ? <div className="mt-1 text-[11px] text-forensics-muted">{progress.detail}</div> : null}
      {progress.currentPath ? (
        <div className="mt-1 truncate font-mono text-[10px] text-forensics-muted" title={progress.currentPath}>
          {progress.currentPath}
        </div>
      ) : null}
      {progress.error ? (
        <div className="mt-1 text-[11px] text-forensics-error-text">{progress.error}</div>
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
          <h3 className="text-[14px] font-light text-forensics-text">{title}</h3>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-forensics-muted">
            <StatusPill status={status} />
            <span>生成时间：{generatedAt || '-'}</span>
          </div>
        </div>
      </div>
      {stats.length > 0 ? <SummaryStrip items={stats} /> : null}
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
      <div className="mb-2 text-[12px] font-light text-forensics-text">{title}</div>
      {children}
    </section>
  );
}

export function DenseTableFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col min-h-0 overflow-hidden rounded-none border border-forensics-border bg-forensics-surface">
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
      <div className="mb-2 text-[11px] font-light uppercase tracking-wider text-forensics-muted">{title}</div>
      {provenance.length > 0 ? (
        <div className="space-y-2">
          {provenance.map((item, index) => (
            <div key={`${item.parser}-${item.artifactPath}-${index}`} className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-muted">
              <div className="flex flex-wrap items-center gap-2">
                <StatusPill status={item.status} />
                <span className="font-mono text-forensics-text-secondary">{item.parser || '-'}</span>
                <span className="font-mono text-forensics-muted">{item.artifactPath || '-'}</span>
              </div>
              <div className="mt-1 font-mono text-[10px] text-forensics-muted-light">
                dataSource={item.dataSourceId || '-'} · parsedAt={item.parsedAt || '-'}
              </div>
              {item.warnings.length > 0 ? <div className="mt-1 text-forensics-warning-text">{item.warnings.join('；')}</div> : null}
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
      <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
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

export function InfoCard({ label, value }: { label: string; value?: string }) {
  return <MetricCard label={label} value={value || '未解析'} size="md" className="bg-forensics-panel" />;
}

export function StatCard({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} mono={false} align="center" size="lg" className="bg-forensics-panel" />;
}

export function Metric({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} size="sm" />;
}

export function StatusPill({ status }: { status: string }) {
  return (
    <span className="rounded-none bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-tertiary">
      {statusLabel(status)}
    </span>
  );
}

export function WarningList({ warnings }: { warnings: string[] }) {
  return (
    <div className="rounded-none border border-forensics-warning-border bg-forensics-warning-bg px-3 py-2 text-[11px] leading-5 text-forensics-warning-text">
      {warnings.map((warning) => (
        <div key={warning}>{warning}</div>
      ))}
    </div>
  );
}

export function EmptyLine({ text }: { text: string }) {
  return <EmptyState className="px-3 py-2 text-left text-forensics-muted">{text}</EmptyState>;
}
