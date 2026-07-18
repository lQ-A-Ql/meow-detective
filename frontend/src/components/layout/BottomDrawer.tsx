import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { Terminal, AlertCircle, ChevronUp, ChevronDown, Clock3 } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { useResizableHeight } from '@/hooks/use-resizable-height';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
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
import type {
  ApiErrorDto,
  DataSourceSummary,
  ImportPhaseProgress,
  JobSnapshot,
  WarningItem,
} from '@/types/models';

type DrawerIssueSeverity = 'error' | 'warning';

interface DrawerIssueMeta {
  label: string;
  value: string;
}

interface DrawerIssue {
  id: string;
  severity: DrawerIssueSeverity;
  title: string;
  detail: string;
  meta: DrawerIssueMeta[];
  details?: string;
  suggestion?: string;
}

export function BottomDrawer() {
  const { t } = useTranslation();
  const jobsQuery = useJobsSnapshot();
  const warningsQuery = useWarnings();
  const dataSourcesQuery = useDataSources();
  const traceQuery = useTraceItems();
  const jobs = jobsQuery.data;
  const warnings = warningsQuery.data;
  const dataSources = dataSourcesQuery.data;
  const trace = traceQuery.data;
  const drawerOpen = useUiStore((state) => state.drawerOpen);
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const importSignals = useImportEventState();

  // Drawer open/close is now fully manual via the toggle button; previous
  // auto-collapse on import completion was removed because jobs/events timing
  // was too unreliable to avoid premature or missed collapses.
  const runningJobs = jobs?.filter((job) => job.status === 'running') ?? [];
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];
  const failedJobs = jobs?.filter((job) => job.status === 'failed') ?? [];
  const warningJobs = jobs?.filter((job) => job.status === 'warning') ?? [];
  const cancellingJobs = jobs?.filter((job) => job.status === 'cancelling') ?? [];
  const cancelledJobs = jobs?.filter((job) => job.status === 'cancelled') ?? [];
  const partialJobs = jobs?.filter((job) => job.partial) ?? [];
  const jobSkippedCount = jobs?.reduce((sum, job) => sum + job.skippedCount, 0) ?? 0;
  const runningCount = runningJobs.length;
  const queryIssues = [
    buildQueryIssue('jobs', t('bottomDrawer.issues.sources.jobs'), jobsQuery.error, t),
    buildQueryIssue('warnings', t('bottomDrawer.issues.sources.warnings'), warningsQuery.error, t),
    buildQueryIssue('dataSources', t('bottomDrawer.issues.sources.dataSources'), dataSourcesQuery.error, t),
    buildQueryIssue('trace', t('bottomDrawer.issues.sources.trace'), traceQuery.error, t),
  ].filter((issue): issue is DrawerIssue => Boolean(issue));
  const failedJobIssues = failedJobs.map((job) => buildFailedJobIssue(job, t));
  const jobWarningIssues = buildJobWarningIssues(jobs ?? [], t);
  const processingIssues = buildDataSourceProcessingIssues(dataSources ?? [], t);
  const importPhaseIssue = buildImportPhaseIssue(importSignals.latestPhase, t);
  const errorIssues = [
    ...queryIssues,
    ...failedJobIssues,
    ...processingIssues.filter((issue) => issue.severity === 'error'),
    ...(importPhaseIssue ? [importPhaseIssue] : []),
  ];
  const warningIssues = [
    ...buildWarningIssues(warnings ?? []),
    ...jobWarningIssues,
    ...processingIssues.filter((issue) => issue.severity === 'warning'),
  ];
  const issueCount = errorIssues.length;
  const warningSignalCount = warningIssues.length;
  const evidenceHashStatus = deriveEvidenceHashStatus(importSignals.partialResults, dataSources ?? []);
  const typedHeadline = importSignals.latestCancellation
    ? `${importSignals.latestCancellation.safeToClose ? t('bottomDrawer.labels.safeToClose') : getCacheStateLabel(importSignals.latestCancellation.state)} · ${importSignals.latestCancellation.detail}`
    : importSignals.latestPhase
      ? `${getImportPhaseLabel(importSignals.latestPhase.phase)} ${importSignals.latestPhase.percent}% · ${importSignals.latestPhase.detail}`
      : undefined;
  const headline =
    typedHeadline ||
    runningJobs[0]?.detail ||
    warningJobs[0]?.detail ||
    failedJobs[0]?.detail ||
    cancellingJobs[0]?.detail ||
    completedJobs[0]?.detail ||
    t('bottomDrawer.headline.waiting');

  const { height: drawerHeight, isResizing: isResizingDrawer, onResizeStart: onDrawerResizeStart } = useResizableHeight({
    defaultHeight: 224,
    minHeight: 128,
    maxHeight: 600,
    storageKey: 'bottomDrawerHeight',
  });

  return (
    <div
      className={`shrink-0 border-t border-forensics-border bg-forensics-panel z-10 transition-[height] duration-150 ${drawerOpen ? 'flex flex-col' : 'h-8 overflow-hidden'}`}
      style={drawerOpen ? { height: `${drawerHeight}px` } : undefined}
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
              <span className={issueCount > 0 ? 'text-red-600' : 'text-forensics-text'}>{issueCount}</span> {t('bottomDrawer.issues.errors')}
            </span>
            <span>
              <span className="text-forensics-text">{warningSignalCount}</span> {t('bottomDrawer.jobs.warnings')}
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
          <Button
            type="button"
            variant="forensicsSurface"
            size="compact"
            onClick={toggleDrawer}
            className="gap-1"
          >
            <span>{drawerOpen ? t('bottomDrawer.toggle.collapse') : t('bottomDrawer.toggle.expand')}</span>
            {drawerOpen ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
          </Button>
          <div className="hidden xl:flex border-l border-forensics-border pl-4">
            {t('bottomDrawer.status.recent')} <span className="text-forensics-text">{runningJobs[0]?.scope || warningJobs[0]?.scope || cancellingJobs[0]?.scope || failedJobs[0]?.scope || completedJobs[0]?.scope || t('bottomDrawer.status.idle')}</span>
          </div>
        </div>
      </div>
      {drawerOpen ? (
        <>
          <div
            className={`shrink-0 h-1 cursor-row-resize transition-colors ${
              isResizingDrawer ? 'bg-blue-400' : 'hover:bg-blue-200'
            }`}
            onMouseDown={onDrawerResizeStart}
            title="拖拽调整抽屉高度"
          />
          <div className="grid flex-1 min-h-0 grid-cols-3 overflow-hidden border-t border-forensics-border">
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
                    {importSignals.latestPhase && importSignals.latestPhase.metrics.warnings > 0 ? (
                      <DrawerChip
                        tone="warning"
                        label={t('bottomDrawer.labels.warnings')}
                        detail={importSignals.latestPhase.metrics.warnings.toString()}
                      />
                    ) : null}
                    {importSignals.latestPhase && importSignals.latestPhase.metrics.failed > 0 ? (
                      <DrawerChip
                        tone="failed"
                        label={t('bottomDrawer.labels.failed')}
                        detail={importSignals.latestPhase.metrics.failed.toString()}
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
              {warningJobs.map((job) => (
                <div key={job.id} className="border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{job.name}</span>
                    <span className="truncate">{job.detail}</span>
                  </div>
                  <div className="mt-1 truncate opacity-80">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                </div>
              ))}
              {cancellingJobs.map((job) => (
                <div key={job.id} className="border border-forensics-border bg-forensics-surface p-3 text-[11px] text-forensics-text-tertiary">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium text-forensics-text">{job.name}</span>
                    <span className="truncate">{job.detail}</span>
                  </div>
                  <div className="mt-1 truncate">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                </div>
              ))}
              {cancelledJobs.map((job) => (
                <div key={job.id} className="border border-forensics-border-light bg-forensics-panel p-3 text-[11px] text-forensics-text-tertiary">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{job.name}</span>
                    <span className="truncate">{job.detail}</span>
                  </div>
                  <div className="mt-1 truncate">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                </div>
              ))}
            </div>
          </div>
          <div className="overflow-auto border-r border-forensics-border p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-forensics-text-tertiary">
              <span>{t('bottomDrawer.issues.title')}</span>
              <span className="font-mono text-forensics-muted-light">
                {issueCount} {t('bottomDrawer.issues.errors')} / {warningSignalCount} {t('bottomDrawer.jobs.warnings')}
              </span>
            </div>
            <div className="space-y-2">
              {errorIssues.map((issue) => (
                <DrawerIssueCard key={issue.id} issue={issue} />
              ))}
              {warningIssues.map((issue) => (
                <DrawerIssueCard key={issue.id} issue={issue} />
              ))}
              {errorIssues.length === 0 && warningIssues.length === 0 ? (
                <div className="border border-forensics-border-light bg-forensics-surface p-3 text-[11px] text-forensics-muted">
                  {t('bottomDrawer.issues.empty')}
                </div>
              ) : null}
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
        </>
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

function buildQueryIssue(source: string, title: string, error: unknown, t: TFunction): DrawerIssue | undefined {
  if (!error) {
    return undefined;
  }

  const apiError = isApiErrorDto(error) ? error : undefined;

  return {
    id: `query-${source}`,
    severity: 'error',
    title,
    detail: errorMessage(error),
    meta: buildApiErrorMeta(apiError, source, t),
    details: formatErrorDetails(apiError?.details),
    suggestion: apiError?.suggestion,
  };
}

function buildApiErrorMeta(apiError: ApiErrorDto | undefined, source: string, t: TFunction): DrawerIssueMeta[] {
  const meta: DrawerIssueMeta[] = [
    { label: t('bottomDrawer.issues.meta.source'), value: source },
  ];

  if (!apiError) {
    return meta;
  }

  meta.push({ label: t('bottomDrawer.issues.meta.code'), value: apiError.code });

  if (apiError.category) {
    meta.push({ label: t('bottomDrawer.issues.meta.category'), value: apiError.category });
  }

  if (apiError.recoverable !== undefined) {
    meta.push({
      label: t('bottomDrawer.issues.meta.recoverable'),
      value: apiError.recoverable
        ? t('bottomDrawer.issues.recoverable.yes')
        : t('bottomDrawer.issues.recoverable.no'),
    });
  }

  return meta;
}

function buildFailedJobIssue(job: JobSnapshot, t: TFunction): DrawerIssue {
  return {
    id: `job-${job.id}`,
    severity: 'error',
    title: job.name,
    detail: job.detail || t('bottomDrawer.jobs.failedFallback'),
    meta: [
      { label: t('bottomDrawer.issues.meta.jobId'), value: job.id },
      { label: t('bottomDrawer.issues.meta.status'), value: job.status },
      { label: t('bottomDrawer.issues.meta.progress'), value: `${job.progress}%` },
      ...(job.scope ? [{ label: t('bottomDrawer.issues.meta.scope'), value: job.scope }] : []),
      { label: t('bottomDrawer.labels.failed'), value: job.failedCount.toString() },
      { label: t('bottomDrawer.labels.warnings'), value: job.warningCount.toString() },
      { label: t('bottomDrawer.labels.skipped'), value: job.skippedCount.toString() },
      ...(job.currentPartition ? [{ label: t('bottomDrawer.partitionProgress.title'), value: job.currentPartition }] : []),
    ],
  };
}

function buildImportPhaseIssue(phase: ImportPhaseProgress | undefined, t: TFunction): DrawerIssue | undefined {
  const failedCount = phase?.metrics.failed ?? 0;

  if (!phase || (phase.state !== 'failed' && failedCount === 0)) {
    return undefined;
  }

  return {
    id: `import-phase-${phase.jobId}-${phase.phase}`,
    severity: 'error',
    title: t('bottomDrawer.issues.importPhaseTitle'),
    detail: phase.detail || t('bottomDrawer.jobs.failedFallback'),
    meta: [
      { label: t('bottomDrawer.issues.meta.jobId'), value: phase.jobId },
      { label: t('bottomDrawer.issues.meta.phase'), value: getImportPhaseLabel(phase.phase) },
      { label: t('bottomDrawer.issues.meta.status'), value: getImportPhaseStateLabel(phase.state) },
      { label: t('bottomDrawer.issues.meta.progress'), value: `${phase.percent}%` },
      { label: t('bottomDrawer.labels.failed'), value: failedCount.toString() },
      { label: t('bottomDrawer.importSignals.processed'), value: phase.metrics.rowsProcessed.toString() },
    ],
  };
}

function buildWarningIssues(warnings: WarningItem[]): DrawerIssue[] {
  return warnings.map((warning) => ({
    id: `warning-${warning.id}`,
    severity: 'warning',
    title: warning.title,
    detail: warning.detail,
    meta: [],
  }));
}

function buildJobWarningIssues(jobs: JobSnapshot[], t: TFunction): DrawerIssue[] {
  return jobs
    .filter((job) => job.status !== 'failed')
    .filter((job) => job.status === 'warning' || job.partial || job.warningCount > 0 || job.skippedCount > 0)
    .map((job) => ({
      id: `job-warning-${job.id}`,
      severity: 'warning',
      title: job.name,
      detail: job.detail || job.scope || t('bottomDrawer.issues.jobWarningFallback'),
      meta: [
        { label: t('bottomDrawer.issues.meta.jobId'), value: job.id },
        { label: t('bottomDrawer.issues.meta.status'), value: job.status },
        { label: t('bottomDrawer.issues.meta.progress'), value: `${job.progress}%` },
        ...(job.scope ? [{ label: t('bottomDrawer.issues.meta.scope'), value: job.scope }] : []),
        { label: t('bottomDrawer.labels.warnings'), value: job.warningCount.toString() },
        { label: t('bottomDrawer.labels.skipped'), value: job.skippedCount.toString() },
        { label: t('bottomDrawer.labels.failed'), value: job.failedCount.toString() },
      ],
    }));
}

function buildDataSourceProcessingIssues(
  dataSources: DataSourceSummary[],
  t: TFunction,
): DrawerIssue[] {
  return dataSources.flatMap((source) =>
    source.processing?.phases.flatMap((phase) => {
      const warningDetails = stringArray(phase.stats.warningDetails);
      const warningCount = numericValue(phase.stats.warningCount, warningDetails.length);
      const issues: DrawerIssue[] = [];

      if (phase.state === 'failed' || phase.state === 'deferred') {
        issues.push({
          id: `source-phase-${source.id}-${phase.phase}-${phase.state}`,
          severity: phase.state === 'failed' ? 'error' : 'warning',
          title: `${source.name} / ${phase.phase}`,
          detail: phase.lastError || `${phase.phase} ${phase.state}`,
          meta: [
            { label: t('bottomDrawer.issues.meta.source'), value: source.id },
            { label: t('bottomDrawer.issues.meta.phase'), value: phase.phase },
            { label: t('bottomDrawer.issues.meta.status'), value: phase.state },
          ],
        });
      }

      if (warningCount > 0) {
        const truncated = phase.stats.warningDetailsTruncated === true;
        issues.push({
          id: `source-phase-${source.id}-${phase.phase}-warnings`,
          severity: 'warning',
          title: `${source.name} / ${phase.phase}`,
          detail: warningDetails[0] || `${warningCount} processing warning(s)`,
          meta: [
            { label: t('bottomDrawer.issues.meta.source'), value: source.id },
            { label: t('bottomDrawer.issues.meta.phase'), value: phase.phase },
            { label: t('bottomDrawer.labels.warnings'), value: warningCount.toString() },
          ],
          details: warningDetails.length > 0
            ? `${warningDetails.join('\n')}${truncated ? '\n...' : ''}`
            : undefined,
        });
      }

      return issues;
    }) ?? [],
  );
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : [];
}

function numericValue(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function formatErrorDetails(details: unknown) {
  if (details === undefined || details === null) {
    return undefined;
  }

  if (typeof details === 'string') {
    return details;
  }

  try {
    return JSON.stringify(details, null, 2);
  } catch {
    return String(details);
  }
}

function DrawerIssueCard({ issue }: { issue: DrawerIssue }) {
  const { t } = useTranslation();
  const isError = issue.severity === 'error';

  return (
    <div className={`border p-3 text-[11px] ${isError ? 'border-red-200 bg-red-50 text-red-700' : 'border-forensics-warning-border bg-forensics-surface text-forensics-text'}`}>
      <div className="flex items-start gap-2">
        <AlertCircle size={12} className={`mt-0.5 shrink-0 ${isError ? 'text-red-600' : 'text-forensics-warning'}`} />
        <div className="min-w-0 flex-1">
          <div className="font-medium break-words">{issue.title}</div>
          <div className={`mt-1 whitespace-pre-wrap break-words ${isError ? 'text-red-700/90' : 'text-forensics-muted'}`}>
            {issue.detail}
          </div>
        </div>
      </div>
      {issue.meta.length > 0 ? (
        <div className="mt-2 grid grid-cols-2 gap-1.5">
          {issue.meta.map((item) => (
            <div key={`${item.label}-${item.value}`} className={`border px-1.5 py-1 ${isError ? 'border-red-200 bg-white/60' : 'border-forensics-border-light bg-forensics-panel'}`}>
              <span className={isError ? 'text-red-500/80' : 'text-forensics-muted-light'}>{item.label}: </span>
              <span className="font-mono break-all">{item.value}</span>
            </div>
          ))}
        </div>
      ) : null}
      {issue.suggestion ? (
        <div className={`mt-2 border px-2 py-1.5 ${isError ? 'border-red-200 bg-white/60' : 'border-forensics-border-light bg-forensics-panel'}`}>
          <span className="font-semibold">{t('bottomDrawer.issues.suggestion')}: </span>
          <span className="break-words">{issue.suggestion}</span>
        </div>
      ) : null}
      {issue.details ? (
        <pre className={`mt-2 max-h-28 overflow-auto whitespace-pre-wrap break-words border px-2 py-1.5 font-mono text-[10px] ${isError ? 'border-red-200 bg-white/70 text-red-800' : 'border-forensics-border-light bg-forensics-panel text-forensics-text-tertiary'}`}>
          {issue.details}
        </pre>
      ) : null}
    </div>
  );
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
