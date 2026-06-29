import { useTranslation } from 'react-i18next';
import { Terminal, AlertCircle, ChevronUp, ChevronDown, Clock3 } from 'lucide-react';
import {
  deriveEvidenceHashStatus,
  getCacheStateLabel,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  getFreshnessLabel,
  getImportPhaseLabel,
  getImportPhaseStateLabel,
  getPartialKindLabel,
  useImportEventState,
} from '@/features/jobs/import-event-state';
import { useDataSources } from '@/features/case/hooks';
import { useJobsSnapshot, useTraceItems, useWarnings } from '@/features/jobs/hooks';
import { useUiStore } from '@/stores/ui-store';
import type { JobSnapshot } from '@/types/models';

export function BottomDrawer() {
  const { t } = useTranslation();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const { data: dataSources } = useDataSources();
  const { data: trace } = useTraceItems();
  const drawerOpen = useUiStore((state) => state.drawerOpen);
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const importSignals = useImportEventState();

  // Drawer open/close is now fully manual via the toggle button; previous
  // auto-collapse on import completion was removed because jobs/events timing
  // was too unreliable to avoid premature or missed collapses.
  const runningJobs = jobs?.filter((job) => job.status === 'running') ?? [];
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];
  const failedJobs = jobs?.filter((job) => job.status === 'failed') ?? [];
  const partialJobs = jobs?.filter((job) => job.partial) ?? [];
  const jobWarningCount = jobs?.reduce((sum, job) => sum + job.warningCount, 0) ?? 0;
  const jobSkippedCount = jobs?.reduce((sum, job) => sum + job.skippedCount, 0) ?? 0;
  const runningCount = runningJobs.length;
  const evidenceHashStatus = deriveEvidenceHashStatus(importSignals.partialResults, dataSources ?? []);
  const typedHeadline = importSignals.latestCancellation
    ? `${importSignals.latestCancellation.safeToClose ? t('bottomDrawer.labels.safeToClose') : getCacheStateLabel(importSignals.latestCancellation.state)} · ${importSignals.latestCancellation.detail}`
    : importSignals.latestPhase
      ? `${getImportPhaseLabel(importSignals.latestPhase.phase)} ${importSignals.latestPhase.percent}% · ${importSignals.latestPhase.detail}`
      : undefined;
  const headline =
    typedHeadline ||
    runningJobs[0]?.detail ||
    failedJobs[0]?.detail ||
    completedJobs[0]?.detail ||
    t('bottomDrawer.headline.waiting');

  return (
    <div
      className={`shrink-0 border-t border-forensics-border bg-forensics-panel z-10 transition-[height] duration-150 ${drawerOpen ? 'h-56' : 'h-8'}`}
    >
      <div className="h-8 flex items-center px-4 text-forensics-muted text-[11px] font-mono justify-between gap-4">
        <div className="flex items-center gap-4 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <Terminal size={12} className="text-forensics-muted-light shrink-0" />
            <span className="truncate">[{t('bottomDrawer.jobs.title')}] {headline}</span>
          </div>
          <div className="hidden lg:flex px-3 border-l border-forensics-border items-center gap-3 text-forensics-text-tertiary">
            <span>
              <span className="text-forensics-text">{runningCount}</span> {t('bottomDrawer.jobs.running')}
            </span>
            <span>
              <span className="text-forensics-text">{(warnings?.length ?? 0) + jobWarningCount}</span> {t('bottomDrawer.jobs.warnings')}
            </span>
            <span>
              <span className="text-forensics-text">{jobSkippedCount}</span> {t('bottomDrawer.jobs.skipped')}
            </span>
            <span>
              <span className="text-forensics-text">{trace?.length ?? 0}</span> {t('bottomDrawer.jobs.trace')}
            </span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-4">
          <button
            type="button"
            onClick={toggleDrawer}
            className="flex items-center gap-1 rounded border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[11px] text-forensics-text hover:bg-forensics-hover"
          >
            <span>{drawerOpen ? t('bottomDrawer.toggle.collapse') : t('bottomDrawer.toggle.expand')}</span>
            {drawerOpen ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
          </button>
          <div className="hidden xl:flex border-l border-forensics-border pl-4">
            {t('bottomDrawer.status.recent')} <span className="text-forensics-text">{runningJobs[0]?.scope || failedJobs[0]?.scope || completedJobs[0]?.scope || t('bottomDrawer.status.idle')}</span>
          </div>
        </div>
      </div>
      {drawerOpen ? (
        <div className="grid h-[calc(100%-2rem)] grid-cols-3 border-t border-forensics-border">
          <div className="overflow-auto border-r border-forensics-border p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-forensics-text-tertiary">
              <span>{t('bottomDrawer.jobs.title')}</span>
              <span className="font-mono text-forensics-muted-light">
                {t('bottomDrawer.jobs.stats', {
                  running: runningCount,
                  completed: completedJobs.length,
                  partial: partialJobs.length,
                  failed: failedJobs.length,
                })}
              </span>
            </div>
            <div className="space-y-3">
              {importSignals.latestPhase || importSignals.latestCancellation || importSignals.partialResults.length || importSignals.cacheStatuses.length || importSignals.latestReport ? (
                <div className="border border-forensics-350 bg-forensics-surface p-3 text-[11px]">
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <div className="text-[10px] font-semibold uppercase tracking-wider text-forensics-muted">{t('bottomDrawer.importSignals.title')}</div>
                      <div className="mt-1 text-forensics-text font-medium">
                        {importSignals.latestPhase
                          ? `${getImportPhaseLabel(importSignals.latestPhase.phase)} · ${getImportPhaseStateLabel(importSignals.latestPhase.state)}`
                          : importSignals.latestCancellation
                            ? `${t('bottomDrawer.labels.cancellation')} · ${getCacheStateLabel(importSignals.latestCancellation.state)}`
                            : t('bottomDrawer.importSignals.waiting')}
                      </div>
                      <div className="mt-1 text-forensics-muted">
                        {importSignals.latestCancellation?.detail || importSignals.latestPhase?.detail || t('bottomDrawer.importSignals.noStatus')}
                      </div>
                    </div>
                    {importSignals.latestPhase ? (
                      <div className="text-right">
                        <div className="font-mono text-forensics-text">{importSignals.latestPhase.percent}{t('bottomDrawer.importSignals.percent')}</div>
                        <div className="text-[10px] text-forensics-muted-light">{importSignals.lastUpdatedAt ?? '-'}</div>
                      </div>
                    ) : null}
                  </div>
                  {importSignals.latestPhase ? (
                    <div className="mt-2 flex items-center gap-2">
                      <div className="flex-1 h-1 overflow-hidden border border-forensics-border bg-forensics-200">
                        <div className="h-full bg-forensics-text" style={{ width: `${importSignals.latestPhase.percent}%` }} />
                      </div>
                      <span className="text-[10px] font-mono text-forensics-text-tertiary">
                        {importSignals.latestPhase.metrics.rowsProcessed}
                        {importSignals.latestPhase.metrics.rowsTotal ? `/${importSignals.latestPhase.metrics.rowsTotal}` : ''}
                      </span>
                    </div>
                  ) : null}
                  <div className="mt-3 flex flex-wrap gap-1.5">
                    {importSignals.latestCancellation ? (
                      <DrawerChip
                        tone={importSignals.latestCancellation.safeToClose ? 'ready' : 'warning'}
                        label={importSignals.latestCancellation.safeToClose ? t('bottomDrawer.labels.safeToClose') : getCacheStateLabel(importSignals.latestCancellation.state)}
                      />
                    ) : null}
                    {importSignals.partialResults.slice(0, 4).map((result) => (
                      <DrawerChip
                        key={`${result.kind}-${result.scopeId}-${result.queryKey}`}
                        tone={result.freshness}
                        label={`${getPartialKindLabel(result.kind)} ${getFreshnessLabel(result.freshness)}`}
                        detail={`${result.readyCount}${result.totalEstimate ? `/${result.totalEstimate}` : ''}`}
                      />
                    ))}
                    {importSignals.cacheStatuses.slice(0, 3).map((status) => (
                      <DrawerChip
                        key={status.cacheKey}
                        tone={status.state}
                        label={cacheKeyLabel(status.cacheKey, t)}
                        detail={getCacheStateLabel(status.state)}
                      />
                    ))}
                    {evidenceHashStatus ? (
                      <DrawerChip
                        tone={evidenceHashStatus}
                        label={`${t('bottomDrawer.labels.evidenceHash')} ${getEvidenceHashStatusLabel(evidenceHashStatus)}`}
                      />
                    ) : null}
                  </div>
                  {evidenceHashStatus && evidenceHashStatus !== 'ready' ? (
                    <div className="mt-2 border border-forensics-warning-border bg-forensics-warning-bg px-2 py-1.5 text-forensics-warning-text">
                      {getEvidenceHashCaveatText(evidenceHashStatus)}
                    </div>
                  ) : null}
                  {importSignals.latestReport ? (
                    <div className="mt-3 border-t border-forensics-border-light pt-2 text-forensics-text-tertiary">
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-[10px] font-semibold uppercase tracking-wider text-forensics-muted">{t('bottomDrawer.performance.title')}</span>
                        <span className="font-mono text-forensics-text">{importSignals.latestReport.summary.elapsedMs}ms</span>
                      </div>
                      <div className="mt-1 text-forensics-muted">{importSignals.latestReport.summary.summary}</div>
                    </div>
                  ) : null}
                </div>
              ) : null}
              {runningJobs.map((job) => (
                <div key={job.id} className="border border-forensics-border bg-forensics-surface p-3 text-[11px]">
                  <div className="flex items-center justify-between gap-3 text-forensics-text">
                    <span className="font-medium">{job.name}</span>
                    <span className="text-forensics-muted-light truncate">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-forensics-muted truncate">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                  {job.currentPartition ? (
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
                        <div className="flex-1 h-1.5 overflow-hidden border border-forensics-border bg-forensics-surface">
                          <div
                            className="h-full transition-all duration-300"
                            style={{
                              width: `${job.partitionProgress ?? 0}%`,
                              backgroundColor:
                                (job.partitionProgress ?? 0) >= 100
                                  ? 'var(--forensics-success)'
                                  : 'var(--forensics-700)',
                            }}
                          />
                        </div>
                        <span className="text-[10px] font-mono text-forensics-text-tertiary w-8 text-right">
                          {job.partitionProgress ?? 0}%
                        </span>
                      </div>
                      <div className="mt-1 text-[11px] text-forensics-text-secondary font-medium">
                        {job.currentPartition}
                      </div>
                    </div>
                  ) : null}
                  <div className="mt-2 flex items-center gap-2">
                    <div className="flex-1 h-1 overflow-hidden border border-forensics-border bg-forensics-200">
                      <div className="h-full bg-forensics-text" style={{ width: `${job.progress}%` }} />
                    </div>
                    <span className="text-[10px] font-mono text-forensics-muted-light">{job.progress}%</span>
                  </div>
                </div>
              ))}
              {completedJobs.map((job) => (
                <div key={job.id} className="border-b border-forensics-border-light pb-2 text-[11px] text-forensics-text-tertiary">
                  <div className="flex items-center justify-between gap-3">
                    <span className="flex items-center gap-2">
                      {job.name}
                      {job.partial ? (
                        <span className="border border-forensics-warning-border bg-forensics-warning-bg px-1.5 py-0.5 text-[9px] font-semibold text-forensics-warning-text-strong">
                          {t('bottomDrawer.labels.partial')}
                        </span>
                      ) : null}
                    </span>
                    <span className="text-forensics-muted-light truncate">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-forensics-muted-light truncate">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                </div>
              ))}
              {failedJobs.map((job) => (
                <div key={job.id} className="border border-red-200 bg-red-50 p-3 text-[11px] text-red-700">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{job.name}</span>
                    <span className="truncate">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-red-600/80 truncate">{job.scope || t('bottomDrawer.jobs.failedFallback')}</div>
                  <JobOutcomeBadges job={job} />
                </div>
              ))}
            </div>
          </div>
          <div className="overflow-auto border-r border-forensics-border p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-forensics-text-tertiary">
              <span>{t('bottomDrawer.warnings.title')}</span>
              <span className="font-mono text-forensics-muted-light">{warnings?.length ?? 0} {t('bottomDrawer.jobs.warnings')}</span>
            </div>
            <div className="space-y-2">
              {warnings?.map((warning) => (
                <div key={warning.id} className="border border-forensics-warning-border bg-forensics-surface p-3 text-[11px]">
                  <div className="flex items-start gap-2 text-forensics-text">
                    <AlertCircle size={12} className="mt-0.5 text-forensics-warning shrink-0" />
                    <div>
                      <div className="font-medium break-words">{warning.title}</div>
                      <div className="mt-1 text-forensics-muted line-clamp-2">{warning.detail}</div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
          <div className="overflow-auto p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-forensics-text-tertiary">
              <span>{t('bottomDrawer.trace.title')}</span>
              <span className="font-mono text-forensics-muted-light">{t('bottomDrawer.trace.recentStream')}</span>
            </div>
            <div className="space-y-2 text-[11px]">
              {trace?.map((item) => (
                <div key={item.id} className="border-b border-forensics-border-light pb-2 text-forensics-text-tertiary flex gap-2">
                  <Clock3 size={11} className="mt-0.5 shrink-0 text-forensics-muted-lighter" />
                  <div>
                    <div className="text-forensics-muted-light font-mono">{item.ts}</div>
                    <div>{item.message}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function cacheKeyLabel(cacheKey: string, t: (key: string) => string) {
  if (cacheKey.startsWith('timeline:')) {
    return t('bottomDrawer.labels.cache.timeline');
  }

  if (cacheKey.startsWith('artifacts:')) {
    return t('bottomDrawer.labels.cache.artifacts');
  }

  if (cacheKey.startsWith('search:')) {
    return t('bottomDrawer.labels.cache.search');
  }

  return cacheKey;
}

function DrawerChip({ label, detail, tone }: { label: string; detail?: string; tone: string }) {
  const toneClass = getToneClass(tone);

  return (
    <span className={`border px-1.5 py-0.5 text-[10px] font-medium ${toneClass}`}>
      {label}
      {detail ? <span className="ml-1 font-mono opacity-80">{detail}</span> : null}
    </span>
  );
}

function getToneClass(tone: string) {
  switch (tone) {
    case 'ready':
    case 'reused':
      return 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text';
    case 'pending':
    case 'partial':
    case 'warming':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    case 'unavailable':
    case 'deferred':
    case 'draining':
      return 'border-forensics-350 bg-forensics-surface text-forensics-text-tertiary';
    case 'failed':
    case 'stale':
    case 'invalidated':
    case 'cancelled':
      return 'border-red-200 bg-red-50 text-red-700';
    case 'warning':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    default:
      return 'border-forensics-350 bg-forensics-surface text-forensics-text-tertiary';
  }
}

function JobOutcomeBadges({ job }: { job: JobSnapshot }) {
  const { t } = useTranslation();

  if (!job.partial && job.warningCount === 0 && job.skippedCount === 0 && job.failedCount === 0) {
    return null;
  }

  return (
    <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[10px] font-mono">
      {job.partial ? (
        <span className="border border-forensics-warning-border bg-forensics-warning-bg px-1.5 py-0.5 font-semibold text-forensics-warning-text-strong">
          {t('bottomDrawer.labels.partial')}
        </span>
      ) : null}
      <span className="border border-forensics-warning-border bg-forensics-surface px-1.5 py-0.5 text-forensics-warning-text">
        {t('bottomDrawer.labels.warnings')} {job.warningCount}
      </span>
      <span className="border border-forensics-350 bg-forensics-surface px-1.5 py-0.5 text-forensics-text-tertiary">
        {t('bottomDrawer.labels.skipped')} {job.skippedCount}
      </span>
      <span className="border border-red-200 bg-forensics-surface px-1.5 py-0.5 text-red-700">
        {t('bottomDrawer.labels.failed')} {job.failedCount}
      </span>
    </div>
  );
}
