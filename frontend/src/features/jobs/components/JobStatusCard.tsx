import { useTranslation } from 'react-i18next';
import { Progress } from '@/app/components/ui/progress';
import { Disclosure } from '@/app/components/ui/disclosure';
import { getImportPhaseLabel, getImportPhaseStateLabel } from '@/features/jobs/import-event-state';
import { ImportProgressMetrics } from '@/features/jobs/components/ImportProgressMetrics';
import { JobOutcomeBadges } from '@/features/jobs/components/JobOutcomeBadges';
import { activeJobProgress } from '@/features/jobs/model/job-progress';
import type { ImportPhaseProgress, JobSnapshot } from '@/types/models';

interface JobStatusCardProps {
  job: JobSnapshot;
  phase?: ImportPhaseProgress;
  tone: 'pending' | 'running' | 'completed' | 'failed' | 'warning' | 'cancelling' | 'cancelled';
}

export function JobStatusCard({ job, tone, phase }: JobStatusCardProps) {
  const { t } = useTranslation();
  const className = cardClass(tone);
  const currentPhase = phase?.jobId === job.id ? phase : undefined;
  const stage = currentPhase ? getImportPhaseLabel(currentPhase.phase) : job.name;
  const active = tone === 'running' || tone === 'cancelling' || tone === 'pending';
  const stateLabel =
    tone === 'cancelling'
      ? t('taskProgress.cancelling')
      : currentPhase
        ? getImportPhaseStateLabel(currentPhase.state)
        : '';

  return (
    <div className={`${className} min-w-0 max-w-full overflow-hidden`} aria-busy={active}>
      <div className="flex min-w-0 items-center justify-between gap-3">
        <span
          className={`min-w-0 truncate ${tone === 'running' || tone === 'cancelling' ? 'font-light text-forensics-text' : 'font-light'}`}
        >
          {job.name}
        </span>
        {!currentPhase ? (
          <span className="min-w-0 truncate text-right text-forensics-muted-light">{job.detail}</span>
        ) : null}
      </div>
      <div className="mt-1 truncate opacity-80">
        {job.scope || (tone === 'failed' ? t('bottomDrawer.jobs.failedFallback') : '')}
      </div>
      <JobOutcomeBadges job={job} />
      {tone === 'pending' ? <div role="status">{t('evidenceTask.states.queued')}</div> : null}
      {currentPhase && active ? (
        <>
          <div className="mt-2 text-forensics-text-secondary" role="status">
            {stage} · {stateLabel}
          </div>
          <ImportProgressMetrics metrics={currentPhase.metrics} />
        </>
      ) : null}
      {tone === 'running' && job.currentPartition ? <PartitionProgress job={job} /> : null}
      {active ? <OverallProgress job={job} stage={stage} value={activeJobProgress(job, currentPhase)} /> : null}
      {currentPhase?.detail ? (
        <Disclosure summary={t('taskProgress.diagnostics')}>{currentPhase.detail}</Disclosure>
      ) : null}
    </div>
  );
}

function PartitionProgress({ job }: { job: JobSnapshot }) {
  const { t } = useTranslation();
  const progress = job.partitionProgress ?? null;
  return (
    <div className="mt-2 bg-transparent px-2 py-2">
      <div className="flex items-center justify-between gap-3 text-[10px] uppercase tracking-wider text-forensics-muted">
        <span>{t('bottomDrawer.partitionProgress.title')}</span>
        <span className="font-mono text-forensics-text">
          {t('bottomDrawer.partitionProgress.completed', {
            completed: job.completedPartitions ?? 0,
            total: job.totalPartitions ?? '?',
          })}
        </span>
      </div>
      <div className="mt-1.5 flex items-center gap-2">
        <Progress
          className="h-1.5 flex-1"
          value={progress}
          indeterminate={progress === null}
          aria-label={job.currentPartition}
        />
        {progress === null ? null : (
          <span className="w-8 text-right font-mono text-[10px] text-forensics-text-tertiary">{progress}%</span>
        )}
      </div>
      <div className="mt-1 break-words text-[11px] font-light text-forensics-text-secondary">
        {job.currentPartition}
      </div>
    </div>
  );
}

function OverallProgress({ job, value, stage }: { job: JobSnapshot; value: number | null; stage: string }) {
  const { t } = useTranslation();
  return (
    <div className="mt-2 flex items-center gap-2">
      <Progress
        className="flex-1"
        value={value}
        indeterminate={value === null}
        aria-label={job.name}
        aria-valuetext={value === null ? stage : t('taskProgress.value', { stage, percent: value })}
      />
      {value === null ? null : <span className="font-mono text-[10px] text-forensics-muted-light">{value}%</span>}
    </div>
  );
}

function cardClass(tone: JobStatusCardProps['tone']) {
  switch (tone) {
    case 'running':
    case 'pending':
      return 'bg-transparent p-3 text-[11px]';
    case 'completed':
      return 'border-b border-forensics-border-light pb-2 text-[11px] text-forensics-text-tertiary';
    case 'failed':
      return 'border border-forensics-error-border bg-forensics-error-bg p-3 text-[11px] text-forensics-error-text';
    case 'warning':
      return 'border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text';
    case 'cancelling':
      return 'bg-transparent p-3 text-[11px] text-forensics-text-tertiary';
    case 'cancelled':
      return 'bg-transparent p-3 text-[11px] text-forensics-text-tertiary';
  }
}
