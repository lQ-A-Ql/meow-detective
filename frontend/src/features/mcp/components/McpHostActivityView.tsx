import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import type { JobSnapshot } from '@/types/jobs';
import type { McpHostMetrics } from '@/types/mcpHost';
import type { TFunction } from 'i18next';

interface Props {
  t: TFunction;
  enabled: boolean;
  loading: boolean;
  error: boolean;
  metrics?: McpHostMetrics;
  jobs?: JobSnapshot[];
  onRefresh: () => void;
}

export function McpHostActivityView({ t, enabled, loading, error, metrics, jobs, onRefresh }: Props) {
  return (
    <section className="space-y-2" aria-label={t('mcpUi.activity.title')}>
      <div className="flex items-center gap-2">
        <h3 className="text-[12px] font-medium text-forensics-ink">{t('mcpUi.activity.title')}</h3>
        <IconButton label={t('mcpUi.activity.refresh')} disabled={!enabled || loading} onClick={onRefresh}>
          <ActionIcon name="refresh" />
        </IconButton>
      </div>
      {error ? <p role="alert" className="text-[11px] text-forensics-error-text">{t('mcpUi.activity.unavailable')}</p> : null}
      {metrics ? <div className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-forensics-muted">
        <span>{t('mcpUi.activity.active', { count: metrics.activeRequests })}</span>
        <span>{t('mcpUi.activity.queued', { count: metrics.queuedRequests })}</span>
        <span>{t('mcpUi.activity.calls', { count: metrics.totalCalls })}</span>
        <span>{t('mcpUi.activity.failed', { count: metrics.failedCalls })}</span>
        <span>{t('mcpUi.activity.average', { value: metrics.averageDurationMs })}</span>
        <span>{t('mcpUi.activity.rss', { current: metrics.currentRssMb, peak: metrics.peakRssMb })}</span>
      </div> : null}
      {jobs && jobs.length > 0 ? <ul className="space-y-1 text-[11px] text-forensics-muted">
        {jobs.map((job) => <li key={job.id} className="flex flex-wrap gap-x-3">
          <span>{job.name}</span><span>{job.status}</span><span>{job.progress}%</span>
        </li>)}
      </ul> : jobs ? <p className="text-[11px] text-forensics-muted">{t('mcpUi.activity.empty')}</p> : null}
    </section>
  );
}
