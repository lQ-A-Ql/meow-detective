import { Activity, AlertTriangle, CheckCircle2, Clock, Database, FileText, Hash, PencilLine, Trash2, XCircle } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { InlineProgressRow } from '@/components/status/InlineProgressRow';
import { formatPartitionDisplayName, partitionDisplayLabel } from '@/lib/partition-display';
import type { DataSourceSummary, DataSourcePartition, JobSnapshot, RecentObject } from '@/types/models';
import type { EvidenceHashJobView } from '../types';

// ── Shared helper ──

const processingStateTone = {
  pending: 'border-forensics-border bg-forensics-panel text-forensics-muted',
  running: 'border-forensics-info-border bg-forensics-info-bg text-forensics-info-text',
  ready: 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text',
  failed: 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text',
  deferred: 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text',
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
  const { t } = useTranslation();
  return (
    <div className="border-b border-forensics-border shrink-0">
      <div className="grid grid-cols-4 divide-x divide-forensics-border">
        <MetricBlock icon={<Database size={12} />} title={t('caseHome.metrics.dataSources')} value={dataSourceCount} />
        <MetricBlock icon={<FileText size={12} />} title={t('caseHome.metrics.indexedFiles')} value={indexedFileCount} />
        <MetricBlock icon={<Clock size={12} />} title={t('caseHome.metrics.timelineEvents')} value={timelineEventCount} />
        <MetricBlock icon={<AlertTriangle size={12} />} title={t('caseHome.metrics.artifacts')} value={artifactCount} />
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
  const { t } = useTranslation();
  return (
    <div className="w-1/2 border-r border-forensics-border flex flex-col min-h-0 bg-forensics-surface">
      <div className="h-8 border-b border-forensics-border bg-forensics-panel flex items-center justify-between px-4 text-[11px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0">
        <span>{t('caseHome.recentTasks.title')}</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">
          {t('caseHome.recentTasks.summary', { completed: completedJobs.length, partial: partialJobCount, running: runningJob ? 1 : 0 })}
        </span>
      </div>
      <ScrollArea className="min-h-0 flex-1" viewportClassName="space-y-3 p-4">
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
                    {t('caseHome.jobStates.partial')}
                  </span>
                  <span className="border border-forensics-warning-border bg-forensics-surface px-1.5 py-0.5 text-forensics-warning-text">
                    {t('caseHome.jobStates.warnings', { count: job.warningCount })}
                  </span>
                  <span className="border border-forensics-border-strong bg-forensics-surface px-1.5 py-0.5 text-forensics-text-tertiary">
                    {t('caseHome.jobStates.skipped', { count: job.skippedCount })}
                  </span>
                  <span className="border border-forensics-error-border bg-forensics-surface px-1.5 py-0.5 text-forensics-error-text">
                    {t('caseHome.jobStates.failed', { count: job.failedCount })}
                  </span>
                </div>
              ) : null}
            </div>
          </div>
        ))}
      </ScrollArea>
    </div>
  );
}

// ── Data sources panel ──

export interface DataSourcesPanelProps {
  dataSources: DataSourceSummary[] | undefined;
  hashJobs?: EvidenceHashJobView[];
  editingDataSourceId: string | undefined;
  editingDataSourceName: string;
  setEditingDataSourceId: (id: string | undefined) => void;
  setEditingDataSourceName: (name: string) => void;
  onRename: (dataSourceId: string, name: string) => void;
  onRequestDelete: (source: DataSourceSummary) => void;
}

export function DataSourcesPanel({
  dataSources,
  hashJobs = [],
  editingDataSourceId,
  editingDataSourceName,
  setEditingDataSourceId,
  setEditingDataSourceName,
  onRename,
  onRequestDelete,
}: DataSourcesPanelProps) {
  const { t } = useTranslation();
  return (
    <>
      <div className="h-8 border-b border-forensics-border bg-forensics-panel flex items-center justify-between px-4 text-[11px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0">
        <span>{t('caseHome.dataSources.title')}</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">{t('caseHome.count', { count: dataSources?.length ?? 0 })}</span>
      </div>
      <ScrollArea className="max-h-64 border-b border-forensics-border bg-forensics-surface">
        {dataSources?.length ? (
          dataSources.map((source) => {
            const isEditing = editingDataSourceId === source.id;
            const partitionCount = source.partitions?.length ?? 0;
            const hashJob = hashJobs.find((job) => job.dataSourceId === source.id);
            const hashStatus = source.hashStatus?.toLowerCase();
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
                          {t('caseHome.actions.save')}
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
                          {t('caseHome.actions.cancel')}
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
                          onClick={() => onRequestDelete(source)}
                          aria-label={t('caseHome.actions.deleteDataSource')}
                        >
                          <Trash2 size={12} />
                        </Button>
                      </div>
                    )}
                    <div className="mt-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">{source.kind}</div>
                    <div className="mt-1 text-[11px] text-forensics-muted font-mono break-all">{source.sourcePath}</div>
                    <div className="mt-2 border border-forensics-border-light bg-forensics-panel px-2.5 py-2">
                      <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-forensics-muted-light">
                        <Hash size={11} />
                        <span>{t('caseHome.dataSources.sha256')}</span>
                      </div>
                      {hashJob ? (
                        <div className="mt-2">
                          <InlineProgressRow
                            title={hashJob.status === 'pending' ? t('caseHome.hash.waiting') : t('caseHome.hash.running')}
                            subtitle={t('caseHome.hash.auto')}
                            detail={`${hashJob.progress}%`}
                            progress={hashJob.progress}
                          />
                        </div>
                      ) : ['hashed', 'recorded', 'ready'].includes(hashStatus ?? '') && source.sourceHash ? (
                        <div className="mt-1.5">
                          <div className="flex items-center gap-1 text-[10px] text-forensics-success-text">
                            <CheckCircle2 size={11} />
                            <span>{t('caseHome.hash.completed')}</span>
                          </div>
                          <div className="mt-1 break-all font-mono text-[10px] leading-4 text-forensics-text" data-testid={`source-hash-${source.id}`}>
                            {source.sourceHash}
                          </div>
                        </div>
                      ) : hashStatus === 'failed' ? (
                        <div className="mt-1.5 flex items-center gap-1 text-[10px] text-forensics-error-text">
                          <XCircle size={11} />
                          <span>{t('caseHome.hash.failed')}</span>
                        </div>
                      ) : hashStatus === 'unavailable' ? (
                        <div className="mt-1.5 text-[10px] text-forensics-muted">{t('caseHome.hash.unavailable')}</div>
                      ) : (
                        <div className="mt-1.5 text-[10px] text-forensics-muted">{t('caseHome.hash.waiting')}</div>
                      )}
                    </div>
                    {source.processing ? (
                      <div className="mt-2 flex flex-wrap items-center gap-2 text-[10px]">
                        <span
                          className={`border px-2 py-0.5 font-light ${processingStateTone[source.processing.state]}`}
                          title={source.processing.lastError}
                        >
                          {t(`caseHome.processing.${source.processing.state}`)}
                        </span>
                        <span className="font-mono text-forensics-muted">
                          {t('caseHome.processing.phase', { ready: source.processing.readyCount, total: source.processing.totalCount })}
                        </span>
                        {source.processing.failedCount > 0 ? (
                          <span className="font-mono text-forensics-error-text">
                            {t('caseHome.jobStates.failed', { count: source.processing.failedCount })}
                          </span>
                        ) : null}
                        {source.processing.deferredCount > 0 ? (
                          <span className="font-mono text-forensics-warning-text">
                            {t('caseHome.processing.deferredCount', { count: source.processing.deferredCount })}
                          </span>
                        ) : null}
                      </div>
                    ) : null}
                    {partitionCount > 0 ? (
                      <div className="mt-3 space-y-2">
                        <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-forensics-muted">
                          <span>{t('caseHome.dataSources.partitions')}</span>
                          <span className="font-mono">{t('caseHome.count', { count: partitionCount })}</span>
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
                                ? t('caseHome.partitions.supported')
                                : partition.status === 'locked'
                                  ? t('caseHome.partitions.locked')
                                  : t('caseHome.partitions.unsupported');

                            return (
                              <div key={`${source.id}-${partition.index}`} className={`border px-3 py-2 text-[11px] ${statusTone}`}>
                                <div className="flex items-start justify-between gap-3">
                                  <div className="min-w-0">
                                    <div className="text-forensics-text font-light">
                                      {formatPartitionDisplayName(partition)}
                                    </div>
                                    <div className="mt-1 text-forensics-text-tertiary">{partition.name}</div>
                                    <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted">
                                      {t('caseHome.partitions.offsetLength', { offset: partition.offset, length: partition.length })}
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
                    <div className="text-[10px] text-forensics-muted-light">{t('caseHome.dataSources.objects')}</div>
                  </div>
                </div>
              </div>
            );
          })
        ) : (
          <div className="px-4 py-6 text-[12px] text-forensics-muted">{t('caseHome.dataSources.empty')}</div>
        )}
      </ScrollArea>
    </>
  );
}

// ── Recent objects panel ──

export function RecentObjectsPanel({ recentObjects }: { recentObjects: RecentObject[] | undefined }) {
  const { t } = useTranslation();
  return (
    <>
      <div className="h-8 border-b border-forensics-border bg-forensics-panel flex items-center justify-between px-4 text-[11px] font-light uppercase text-forensics-text-tertiary tracking-wider shrink-0">
        <span>{t('caseHome.recentObjects.title')}</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">{t('caseHome.recentObjects.count', { count: recentObjects?.length ?? 0 })}</span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
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
            <div className="px-4 py-6 text-[12px] text-forensics-muted">{t('caseHome.recentObjects.empty')}</div>
          )}
        </div>
      </ScrollArea>
    </>
  );
}
