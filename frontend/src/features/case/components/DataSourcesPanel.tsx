import { CheckCircle2, PencilLine, Trash2, XCircle } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { InlineProgressRow } from '@/components/status/InlineProgressRow';
import { formatPartitionDisplayName, partitionDisplayLabel } from '@/lib/partition-display';
import type { DataSourceSummary, DataSourcePartition } from '@/types/models';
import type { EvidenceHashJobView } from '../types';
import type { DigestAlgorithm, EvidenceDigest } from '@/types/digest';
import { LinuxRootMounts } from './LinuxRootMounts';

const processingStateTone = {
  pending: 'border-forensics-border bg-forensics-panel text-forensics-muted',
  running: 'border-forensics-info-border bg-forensics-info-bg text-forensics-info-text',
  ready: 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text',
  failed: 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text',
  deferred: 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text',
} as const;

export interface DataSourcesPanelProps {
  dataSources: DataSourceSummary[] | undefined;
  hashJobs?: EvidenceHashJobView[];
  editingDataSourceId: string | undefined;
  editingDataSourceName: string;
  setEditingDataSourceId: (id: string | undefined) => void;
  setEditingDataSourceName: (name: string) => void;
  onRename: (dataSourceId: string, name: string) => void;
  onRequestDelete: (source: DataSourceSummary) => void;
  logicalDiskDigest?: EvidenceDigest;
  logicalDiskDigestTask?: ReturnType<typeof useEvidenceTask>;
  logicalDiskDigestSourceId?: string;
  logicalDiskDigestLoading?: boolean;
  logicalDiskDigestError?: string;
  onCalculateLogicalDiskDigest?: (dataSourceId: string, algorithm: DigestAlgorithm) => void;
  showHeader?: boolean;
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
  logicalDiskDigest,
  logicalDiskDigestTask,
  logicalDiskDigestSourceId,
  logicalDiskDigestLoading = false,
  logicalDiskDigestError,
  onCalculateLogicalDiskDigest,
  showHeader = true,
}: DataSourcesPanelProps) {
  const { t } = useTranslation();
  return (
    <>
      {showHeader ? <div className="flex h-10 shrink-0 items-center justify-between px-4 text-[10px] uppercase tracking-[0.08em] text-forensics-ink-3">
        <span>{t('caseHome.dataSources.title')}</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">{t('caseHome.count', { count: dataSources?.length ?? 0 })}</span>
      </div> : null}
      <ScrollArea className="min-h-0 flex-1">
        {dataSources?.length ? (
          dataSources.map((source) => {
            const isEditing = editingDataSourceId === source.id;
            const partitionCount = source.partitions?.length ?? 0;
            const hashJob = hashJobs.find((job) => job.dataSourceId === source.id);
            const hashStatus = source.hashStatus?.toLowerCase();
            return (
              <div key={source.id} className="border-b border-forensics-line px-4 py-3 last:border-b-0">
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
                        <Tooltip content={t('caseHome.actions.save')}><Button aria-label={t('caseHome.actions.save')}
                          type="button"
                          variant="forensicsOutline"
                          size="compact"
                          onClick={() => onRename(source.id, editingDataSourceName.trim())}
                        ><ActionIcon name="save" /></Button></Tooltip>
                        <Tooltip content={t('caseHome.actions.cancel')}><Button aria-label={t('caseHome.actions.cancel')}
                          type="button"
                          variant="forensicsGhost"
                          size="compact"
                          onClick={() => {
                            setEditingDataSourceId(undefined);
                            setEditingDataSourceName('');
                          }}
                        ><ActionIcon name="close" /></Button></Tooltip>
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
                    {source.gptDiskGuid ? (
                      <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted-light">
                        {t('caseHome.dataSources.gptDiskGuid', { value: source.gptDiskGuid })}
                      </div>
                    ) : null}
                    <LinuxRootMounts source={source} />
                    <div className="mt-2 py-2">
                      <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-forensics-muted-light">
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
                    {source.kind === 'local_disk' ? (
                      <div className="mt-2 border border-forensics-warning-border bg-forensics-warning-bg px-2.5 py-2" data-testid={`physical-disk-hash-${source.id}`}>
                        <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-forensics-warning-text">
                          <span>{t('caseHome.dataSources.physicalDiskHash')}</span>
                        </div>
                        <div className="mt-1 text-[10px] text-forensics-muted">{t('caseHome.dataSources.physicalDiskHashHint')}</div>
                        <div className="mt-2 grid grid-cols-2 gap-1">
                          {(['md5', 'sha1', 'sha256', 'sm3'] as DigestAlgorithm[]).map((algorithm) => (
                            <Button aria-label={algorithm.toUpperCase()} key={algorithm} type="button" size="xs" variant="forensicsOutline" disabled={logicalDiskDigestLoading} onClick={() => onCalculateLogicalDiskDigest?.(source.id, algorithm)}>
                              <ActionIcon name={`hash-${algorithm}`} />
                            </Button>
                          ))}
                        </div>
                        {logicalDiskDigestTask && logicalDiskDigestSourceId === source.id ? <EvidenceTaskStatus model={logicalDiskDigestTask} /> : null}
                        {logicalDiskDigestError && logicalDiskDigestSourceId === source.id ? <div className="mt-1.5 text-[10px] text-forensics-error-text">{logicalDiskDigestError}</div> : null}
                        {logicalDiskDigest && logicalDiskDigestSourceId === source.id ? <div className="mt-1.5 space-y-1 font-mono text-[10px]"><div>{logicalDiskDigest.algorithm.toUpperCase()} · {logicalDiskDigest.byteLength} bytes</div><div className="break-all text-forensics-text" data-testid={`physical-disk-digest-${source.id}`}>{logicalDiskDigest.value}</div></div> : null}
                      </div>
                    ) : null}
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
                                ? 'bg-forensics-base text-forensics-muted'
                                : partition.status === 'locked'
                                  ? 'bg-forensics-warning-bg text-forensics-warning-text'
                                  : 'bg-transparent text-forensics-muted';
                            const statusLabel =
                              partition.status === 'supported'
                                ? t('caseHome.partitions.supported')
                                : partition.status === 'locked'
                                  ? t('caseHome.partitions.locked')
                                  : t('caseHome.partitions.unsupported');

                            return (
                              <div key={`${source.id}-${partition.index}`} className={`border-l-2 border-forensics-line-2 px-3 py-2 text-[11px] ${statusTone}`}>
                                <div className="flex items-start justify-between gap-3">
                                  <div className="min-w-0">
                                    <div className="text-forensics-text font-light">
                                      {formatPartitionDisplayName(partition)}
                                    </div>
                                    <div className="mt-1 text-forensics-text-tertiary">{partition.name}</div>
                                    <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted">
                                      {t('caseHome.partitions.offsetLength', { offset: partition.offset, length: partitionLengthLabel(partition.length, t('caseHome.partitions.lengthUnknown')) })}
                                    </div>
                                    <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted-light">
                                      {t('caseHome.partitions.gptPartitionUuid', { value: partition.gptPartitionUuid ?? t('caseHome.partitions.uuidNotRecorded') })}
                                    </div>
                                    <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted-light">
                                      {t('caseHome.partitions.filesystemUuid', { value: partition.filesystemUuid ?? t('caseHome.partitions.uuidNotRecorded') })}
                                    </div>
                                    {partition.typeGuid ? (
                                      <div className="mt-1 font-mono text-[10px] break-all text-forensics-muted-light">
                                        {t('caseHome.partitions.typeGuid', { value: partition.typeGuid })}
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
import type { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import { EvidenceTaskStatus } from '@/features/jobs/components/EvidenceTaskStatus';
import { partitionLengthLabel } from '@/features/case/partition-length';
