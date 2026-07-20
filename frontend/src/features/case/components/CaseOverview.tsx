import { Activity, AlertTriangle, CheckCircle2, Clock, Database, FileText, PencilLine, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { InlineProgressRow } from '@/components/status/InlineProgressRow';
import { formatPartitionDisplayName, partitionDisplayLabel } from '@/lib/partition-display';
import type { DataSourceSummary, DataSourcePartition, JobSnapshot, RecentObject } from '@/types/models';

// ── Shared helper ──

const processingStatePresentation = {
  pending: { label: '等待处理', tone: 'border-forensics-border bg-forensics-panel text-forensics-muted' },
  running: { label: '处理中', tone: 'border-forensics-info-border bg-forensics-info-bg text-forensics-info-text' },
  ready: { label: '处理完成', tone: 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text' },
  failed: { label: '处理失败', tone: 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text' },
  deferred: { label: '部分延后', tone: 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text' },
} as const;

export function MetricBlock({
  icon,
  title,
  value,
}: {
  icon: ReactNode;
  title: string;
  value: number;
}) {
  return (
    <div className="p-4 flex flex-col gap-3 bg-forensics-surface">
      <div className="text-forensics-muted-light text-[11px] uppercase tracking-wider flex items-center gap-1.5">
        {icon} {title}
      </div>
      <div className="text-2xl font-serif text-forensics-text">{value.toLocaleString()}</div>
    </div>
  );
}

// ── Metrics strip ──

export function CaseMetricsStrip({
  dataSourceCount,
  indexedFileCount,
  timelineEventCount,
  artifactCount,
}: {
  dataSourceCount: number;
  indexedFileCount: number;
  timelineEventCount: number;
  artifactCount: number;
}) {
  return (
    <div className="border-b border-forensics-border shrink-0">
      <div className="grid grid-cols-4 divide-x divide-forensics-border">
        <MetricBlock icon={<Database size={12} />} title="数据源" value={dataSourceCount} />
        <MetricBlock icon={<FileText size={12} />} title="已索引文件" value={indexedFileCount} />
        <MetricBlock icon={<Clock size={12} />} title="时间线事件" value={timelineEventCount} />
        <MetricBlock icon={<AlertTriangle size={12} />} title="提取痕迹" value={artifactCount} />
      </div>
    </div>
  );
}

// ── Recent tasks panel ──

export function RecentTasksPanel({
  runningJob,
  completedJobs,
  partialJobCount,
}: {
  runningJob: JobSnapshot | undefined;
  completedJobs: JobSnapshot[];
  partialJobCount: number;
}) {
  return (
    <div className="w-1/2 border-r border-forensics-border flex flex-col min-h-0 bg-forensics-surface">
      <div className="h-8 border-b border-forensics-border bg-forensics-panel flex items-center justify-between px-4 text-[11px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0">
        <span>最近任务</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">
          完成 {completedJobs.length} / 部分 {partialJobCount} / 运行 {runningJob ? 1 : 0}
        </span>
      </div>
      <div className="flex-1 overflow-auto p-4 space-y-3">
        {runningJob ? (
          <InlineProgressRow
            title={runningJob.name}
            subtitle={runningJob.scope}
            detail={runningJob.detail}
            progress={runningJob.progress}
          />
        ) : null}
        {completedJobs.map((job) => (
          <div key={job.id} className="border-t border-forensics-border-light pt-3 flex items-start gap-3">
            <CheckCircle2 size={14} className="text-forensics-muted-light mt-0.5" />
            <div className="flex-1">
              <div className="flex items-center justify-between gap-3">
                <div className="text-forensics-text-secondary text-[13px]">{job.name}</div>
                <div className="text-forensics-muted-light font-mono text-[10px]">{job.detail}</div>
              </div>
              <div className="text-forensics-muted text-[11px] mt-0.5">{job.scope}</div>
              {job.partial ? (
                <div className="mt-2 flex flex-wrap gap-1.5 text-[10px] font-mono">
                  <span className="border border-forensics-warning-border bg-forensics-warning-bg px-1.5 py-0.5 text-forensics-warning-text">
                    PARTIAL
                  </span>
                  <span className="border border-forensics-warning-border bg-forensics-surface px-1.5 py-0.5 text-forensics-warning-text">
                    warnings {job.warningCount}
                  </span>
                  <span className="border border-forensics-border-strong bg-forensics-surface px-1.5 py-0.5 text-forensics-text-tertiary">
                    skipped {job.skippedCount}
                  </span>
                  <span className="border border-forensics-error-border bg-forensics-surface px-1.5 py-0.5 text-forensics-error-text">
                    failed {job.failedCount}
                  </span>
                </div>
              ) : null}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Data sources panel ──

export interface DataSourcesPanelProps {
  dataSources: DataSourceSummary[] | undefined;
  editingDataSourceId: string | undefined;
  editingDataSourceName: string;
  setEditingDataSourceId: (id: string | undefined) => void;
  setEditingDataSourceName: (name: string) => void;
  onRename: (dataSourceId: string, name: string) => void;
  onDelete: (dataSourceId: string) => void;
}

export function DataSourcesPanel({
  dataSources,
  editingDataSourceId,
  editingDataSourceName,
  setEditingDataSourceId,
  setEditingDataSourceName,
  onRename,
  onDelete,
}: DataSourcesPanelProps) {
  return (
    <>
      <div className="h-8 border-b border-forensics-border bg-forensics-panel flex items-center justify-between px-4 text-[11px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0">
        <span>已有数据源</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">{dataSources?.length ?? 0} 个</span>
      </div>
      <div className="max-h-64 overflow-auto border-b border-forensics-border bg-forensics-surface">
        {dataSources?.length ? (
          dataSources.map((source) => {
            const isEditing = editingDataSourceId === source.id;
            const partitionCount = source.partitions?.length ?? 0;
            return (
              <div key={source.id} className="border-b border-forensics-border-light px-4 py-3 last:border-b-0">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    {isEditing ? (
                      <div className="flex items-center gap-2">
                        <Input
                          type="text"
                          value={editingDataSourceName}
                          onChange={(e) => setEditingDataSourceName(e.target.value)}
                          variant="forensics"
                          inputSize="compact"
                          className="flex-1"
                        />
                        <Button
                          type="button"
                          variant="forensicsOutline"
                          size="compact"
                          onClick={() => onRename(source.id, editingDataSourceName.trim())}
                        >
                          保存
                        </Button>
                        <Button
                          type="button"
                          variant="forensicsGhost"
                          size="compact"
                          onClick={() => {
                            setEditingDataSourceId(undefined);
                            setEditingDataSourceName('');
                          }}
                        >
                          取消
                        </Button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-2">
                        <div className="text-[13px] text-forensics-text font-light truncate">{source.name}</div>
                        <Button
                          type="button"
                          variant="forensicsGhost"
                          size="iconSm"
                          onClick={() => {
                            setEditingDataSourceId(source.id);
                            setEditingDataSourceName(source.name);
                          }}
                        >
                          <PencilLine size={12} />
                        </Button>
                        <Button
                          type="button"
                          variant="forensicsDangerGhost"
                          size="iconSm"
                          onClick={() => {
                            if (window.confirm(`确定删除数据源 "${source.name}"？\n\n该操作将级联删除其下的所有文件条目、时间线事件和提取痕迹，且不可撤销。`)) {
                              onDelete(source.id);
                            }
                          }}
                        >
                          <Trash2 size={12} />
                        </Button>
                      </div>
                    )}
                    <div className="mt-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{source.kind}</div>
                    <div className="mt-1 text-[11px] text-forensics-muted font-mono break-all">{source.sourcePath}</div>
                    {source.processing ? (
                      <div className="mt-2 flex flex-wrap items-center gap-2 text-[10px]">
                        <span
                          className={`border px-2 py-0.5 font-light ${processingStatePresentation[source.processing.state].tone}`}
                          title={source.processing.lastError}
                        >
                          {processingStatePresentation[source.processing.state].label}
                        </span>
                        <span className="font-mono text-forensics-muted">
                          phase {source.processing.readyCount}/{source.processing.totalCount}
                        </span>
                        {source.processing.failedCount > 0 ? (
                          <span className="font-mono text-forensics-error-text">
                            failed {source.processing.failedCount}
                          </span>
                        ) : null}
                        {source.processing.deferredCount > 0 ? (
                          <span className="font-mono text-forensics-warning-text">
                            deferred {source.processing.deferredCount}
                          </span>
                        ) : null}
                      </div>
                    ) : null}
                    {partitionCount > 0 ? (
                      <div className="mt-3 space-y-2">
                        <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-forensics-muted">
                          <span>分区结构</span>
                          <span className="font-mono">{partitionCount} 项</span>
                        </div>
                        <div className="space-y-2">
                          {source.partitions?.map((partition: DataSourcePartition) => {
                            const statusTone =
                              partition.status === 'supported'
                                ? 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text'
                                : partition.status === 'locked'
                                  ? 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text'
                                  : 'border-forensics-border bg-forensics-panel text-forensics-muted';
                            const statusLabel =
                              partition.status === 'supported'
                                ? '可浏览'
                                : partition.status === 'locked'
                                  ? '需要解锁'
                                  : '暂不支持';

                            return (
                              <div key={`${source.id}-${partition.index}`} className={`border px-3 py-2 text-[11px] ${statusTone}`}>
                                <div className="flex items-start justify-between gap-3">
                                  <div className="min-w-0">
                                    <div className="text-forensics-text font-light">
                                      {formatPartitionDisplayName(partition)}
                                    </div>
                                    <div className="mt-1 text-forensics-text-tertiary">{partition.name}</div>
                                    <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted">
                                      offset {partition.offset} / length {partition.length}
                                    </div>
                                    {partition.typeGuid ? (
                                      <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted-light">
                                        GUID {partition.typeGuid}
                                      </div>
                                    ) : null}
                                    {partition.unlockHint ? (
                                      <div className="mt-2 text-[10px] font-light text-forensics-warning-text">
                                        {partition.unlockHint}
                                      </div>
                                    ) : null}
                                  </div>
                                  <div className="shrink-0 text-right">
                                    <div className="text-[10px] uppercase tracking-wider font-light">
                                      {statusLabel}
                                    </div>
                                    <div className="mt-1 font-mono text-[10px] text-forensics-muted-light">
                                      {partitionDisplayLabel(partition)}
                                    </div>
                                  </div>
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    ) : null}
                  </div>
                  <div className="text-right shrink-0">
                    <div className="text-[11px] text-forensics-text font-mono">{source.fileCount ?? 0}</div>
                    <div className="text-[10px] text-forensics-muted-light">objects</div>
                  </div>
                </div>
              </div>
            );
          })
        ) : (
          <div className="px-4 py-6 text-[12px] text-forensics-muted">导入数据源后，这里会展示当前案件中的全部证据源，并允许重命名。</div>
        )}
      </div>
    </>
  );
}

// ── Recent objects panel ──

export function RecentObjectsPanel({ recentObjects }: { recentObjects: RecentObject[] | undefined }) {
  return (
    <>
      <div className="h-8 border-b border-forensics-border bg-forensics-panel flex items-center justify-between px-4 text-[11px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0">
        <span>高价值对象</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">最近发现 {recentObjects?.length ?? 0} 项</span>
      </div>
      <div className="flex-1 overflow-auto">
        <div className="flex flex-col border-b border-forensics-border">
          {recentObjects?.length ? (
            recentObjects.map((item) => (
              <div key={item.id} className="flex items-center px-4 py-2 border-b border-forensics-border-light hover:bg-forensics-panel-strong cursor-pointer bg-forensics-surface">
                <div className="w-8">
                  {item.kind === 'file' ? <FileText size={12} className="text-forensics-muted-light" /> : <Activity size={12} className="text-forensics-muted-light" />}
                </div>
                <div className="flex-1 font-mono text-[11px]">
                  <div className="text-forensics-text font-light">{item.title}</div>
                  <div className="text-forensics-muted text-[10px] mt-0.5 font-sans">{item.detail}</div>
                </div>
                <div className="text-forensics-muted-light text-[11px]">{item.time}</div>
              </div>
            ))
          ) : (
            <div className="px-4 py-6 text-[12px] text-forensics-muted">导入并完成初步解析后，这里会展示最近发现的高价值对象。</div>
          )}
        </div>
      </div>
    </>
  );
}
