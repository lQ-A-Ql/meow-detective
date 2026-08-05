import { useTranslation } from 'react-i18next';
import { JobOutcomeBadges } from '@/features/jobs/components/JobOutcomeBadges';
import type { JobSnapshot } from '@/types/models';

interface JobStatusCardProps {
  job: JobSnapshot;
  tone: 'running' | 'completed' | 'failed' | 'warning' | 'cancelling' | 'cancelled';
}

export function JobStatusCard({ job, tone }: JobStatusCardProps) {
  const { t } = useTranslation();
  const className = cardClass(tone);

  return (
    <div className={`${className} min-w-0 max-w-full overflow-hidden`}>
      <div className="flex min-w-0 items-center justify-between gap-3">
        <span className={`min-w-0 truncate ${tone === 'running' || tone === 'cancelling' ? 'font-light text-forensics-text' : 'font-light'}`}>
          {job.name}
        </span>
        <span className="min-w-0 truncate text-right text-forensics-muted-light">{job.detail}</span>
      </div>
      <div className="mt-1 truncate opacity-80">{job.scope || (tone === 'failed' ? t('bottomDrawer.jobs.failedFallback') : '')}</div>
      <JobOutcomeBadges job={job} />
      {tone === 'running' && job.currentPartition ? <PartitionProgress job={job} /> : null}
      {tone === 'running' ? <OverallProgress job={job} /> : null}
    </div>
  );
}

function PartitionProgress({ job }: { job: JobSnapshot }) {
  const { t } = useTranslation();
  const progress = job.partitionProgress ?? 0;
  return (
    <div className="mt-2 border border-forensics-border-light bg-forensics-panel px-2 py-2">
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
        <div className="h-1.5 flex-1 overflow-hidden border border-forensics-border bg-forensics-surface">
          <div
            className="h-full transition-colors duration-300"
            style={{
              width: `${progress}%`,
              backgroundColor: progress >= 100 ? 'var(--forensics-success)' : 'var(--forensics-700)',
            }}
          />
        </div>
        <span className="w-8 text-right font-mono text-[10px] text-forensics-text-tertiary">{progress}%</span>
      </div>
      <div className="mt-1 break-words text-[11px] font-light text-forensics-text-secondary">{job.currentPartition}</div>
    </div>
  );
}

function OverallProgress({ job }: { job: JobSnapshot }) {
  return (
    <div className="mt-2 flex items-center gap-2">
      <div className="h-1 flex-1 overflow-hidden border border-forensics-border bg-forensics-200">
        <div className="h-full bg-forensics-text" style={{ width: `${job.progress}%` }} />
      </div>
      <span className="font-mono text-[10px] text-forensics-muted-light">{job.progress}%</span>
    </div>
  );
}

function cardClass(tone: JobStatusCardProps['tone']) {
  switch (tone) {
    case 'running':
      return 'border border-forensics-border bg-forensics-surface p-3 text-[11px]';
    case 'completed':
      return 'border-b border-forensics-border-light pb-2 text-[11px] text-forensics-text-tertiary';
    case 'failed':
      return 'border border-forensics-error-border bg-forensics-error-bg p-3 text-[11px] text-forensics-error-text';
    case 'warning':
      return 'border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text';
    case 'cancelling':
      return 'border border-forensics-border bg-forensics-surface p-3 text-[11px] text-forensics-text-tertiary';
    case 'cancelled':
      return 'border border-forensics-border-light bg-forensics-panel p-3 text-[11px] text-forensics-text-tertiary';
  }
}
